//! All native work runs on the blocking pool. Never carry a session/SQL guard
//! through a dialog, file read, parser, export staging, or PDF preparation.
use super::{AuthSession, SessionState};
use crate::{
    db::Database,
    error::AppError,
    models::purchase_invoice::*,
    services::{
        purchase_invoice_files::{self as files, CapturedFile},
        purchase_invoice_parser::parse_purchase_invoice,
        purchase_invoices as service,
    },
};
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
type Result<T> = std::result::Result<T, AppError>;

#[derive(Clone)]
pub struct PreviewBuffer {
    preview: PurchaseInvoicePreview,
    xml: CapturedFile,
    pdf: Option<CapturedFile>,
    created: Instant,
}
impl PreviewBuffer {
    fn new(xml: CapturedFile) -> Result<Self> {
        let document = parse_purchase_invoice(&xml.bytes)?;
        Ok(Self {
            preview: PurchaseInvoicePreview {
                review_token: uuid::Uuid::new_v4().to_string(),
                document,
                xml: xml.metadata.clone(),
                pdf: None,
            },
            xml,
            pdf: None,
            created: Instant::now(),
        })
    }
}
fn stale() -> AppError {
    AppError::conflict("INVOICE_PREVIEW_STALE", "La revision ya no esta disponible")
}
fn lock(session: &AuthSession) -> Result<std::sync::MutexGuard<'_, SessionState>> {
    session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo comprobar la sesion"))
}
fn begin(session: &AuthSession) -> Result<u64> {
    let s = lock(session)?;
    s.authenticated_user()?;
    Ok(s.generation)
}
fn check(s: &SessionState, g: u64) -> Result<()> {
    s.authenticated_user()?;
    if s.generation != g {
        return Err(stale());
    }
    Ok(())
}
fn preview(s: &SessionState, token: &str) -> Result<PreviewBuffer> {
    s.authenticated_user()?;
    s.invoice_preview
        .as_ref()
        .filter(|p| {
            p.preview.review_token == token && p.created.elapsed() < Duration::from_secs(1800)
        })
        .cloned()
        .ok_or_else(stale)
}
fn db<T>(app: &AppHandle, op: impl FnOnce(&mut rusqlite::Connection) -> Result<T>) -> Result<T> {
    let state = app.state::<Database>();
    let mut c = state.0.lock().map_err(|_| AppError::database())?;
    op(&mut c)
}
fn authorized<T>(
    app: &AppHandle,
    g: u64,
    op: impl FnOnce(&mut rusqlite::Connection, &crate::models::user::User) -> Result<T>,
) -> Result<T> {
    let session = app.state::<AuthSession>();
    let s = lock(&session)?;
    check(&s, g)?;
    let user = s.authenticated_user()?.clone();
    drop(s);
    // Read-only operations do not delay logout while scanning SQLite. Never
    // deliver their captured data to a replacement authentication generation.
    let result = db(app, |c| op(c, &user));
    check(&*lock(&session)?, g)?;
    result
}
trait FileDialog {
    fn pick(&self, kind: InvoiceAttachmentKind) -> Result<Option<PathBuf>>;
    fn save(&self, name: &str) -> Result<Option<PathBuf>>;
}
struct NativeDialog(AppHandle);
impl FileDialog for NativeDialog {
    fn pick(&self, kind: InvoiceAttachmentKind) -> Result<Option<PathBuf>> {
        self.0
            .dialog()
            .file()
            .add_filter(service::kind(kind), &[service::kind(kind)])
            .blocking_pick_file()
            .map(|p| {
                p.into_path().map_err(|_| {
                    AppError::validation(
                        "INVOICE_FILE_UNREADABLE",
                        "No se pudo leer el archivo seleccionado",
                    )
                })
            })
            .transpose()
    }
    fn save(&self, name: &str) -> Result<Option<PathBuf>> {
        self.0
            .dialog()
            .file()
            .set_file_name(name)
            .blocking_save_file()
            .map(|p| {
                p.into_path()
                    .map_err(|_| files::failure("INVOICE_EXPORT_FAILED"))
            })
            .transpose()
    }
}
async fn blocking<T: Send + 'static>(op: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(op)
        .await
        .map_err(|_| {
            AppError::internal(
                "INVOICE_OPEN_FAILED",
                "No se pudo completar la operacion nativa",
            )
        })?
}
fn pick_xml(
    session: &AuthSession,
    dialog: &impl FileDialog,
    g: u64,
) -> Result<Option<PurchaseInvoicePreview>> {
    check(&*lock(session)?, g)?;
    let result = dialog
        .pick(InvoiceAttachmentKind::Xml)?
        .map(|p| files::read_selected(&p, InvoiceAttachmentKind::Xml).and_then(PreviewBuffer::new))
        .transpose();
    let mut s = lock(session)?;
    check(&s, g)?;
    if let Some(buffer) = result? {
        let result = buffer.preview.clone();
        s.invoice_preview = Some(buffer);
        Ok(Some(result))
    } else {
        Ok(None)
    }
}
#[tauri::command]
pub async fn pick_invoice_xml(app: AppHandle) -> Result<Option<PurchaseInvoicePreview>> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || pick_xml(&app.state::<AuthSession>(), &NativeDialog(app.clone()), g)).await
}
#[tauri::command]
pub async fn pick_invoice_preview_pdf(
    app: AppHandle,
    review_token: String,
) -> Result<Option<PurchaseInvoicePreview>> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || {
        pick_preview_pdf(
            &app.state::<AuthSession>(),
            &NativeDialog(app.clone()),
            g,
            &review_token,
        )
    })
    .await
}
fn pick_preview_pdf(
    session: &AuthSession,
    dialog: &impl FileDialog,
    g: u64,
    token: &str,
) -> Result<Option<PurchaseInvoicePreview>> {
    check(&*lock(session)?, g)?;
    let mut buffer = preview(&*lock(session)?, token)?;
    let selected = dialog
        .pick(InvoiceAttachmentKind::Pdf)?
        .map(|p| files::read_selected(&p, InvoiceAttachmentKind::Pdf))
        .transpose();
    let mut s = lock(session)?;
    check(&s, g)?;
    preview(&s, token)?;
    if let Some(pdf) = selected? {
        buffer.preview.pdf = Some(pdf.metadata.clone());
        buffer.pdf = Some(pdf);
        let result = buffer.preview.clone();
        s.invoice_preview = Some(buffer);
        Ok(Some(result))
    } else {
        Ok(None)
    }
}
#[tauri::command]
pub fn discard_invoice_preview(
    session: State<'_, AuthSession>,
    review_token: String,
) -> Result<()> {
    discard_preview(&session, &review_token)
}
fn discard_preview(session: &AuthSession, review_token: &str) -> Result<()> {
    let mut s = lock(session)?;
    s.authenticated_user()?;
    if s.invoice_preview
        .as_ref()
        .is_some_and(|p| p.preview.review_token == review_token)
    {
        s.invoice_preview = None;
    }
    Ok(())
}
#[tauri::command]
pub async fn register_invoice(
    app: AppHandle,
    review_token: String,
) -> Result<InvoiceRegistrationResult> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || {
        let session = app.state::<AuthSession>();
        check(&*lock(&session)?, g)?;
        let buffer = preview(&*lock(&session)?, &review_token)?;
        let document = parse_purchase_invoice(&buffer.xml.bytes)?;
        let s = lock(&session)?;
        check(&s, g)?;
        preview(&s, &review_token)?;
        db(&app, |c| {
            service::register(
                c,
                s.authenticated_user()?,
                &document,
                &buffer.xml,
                buffer.pdf.as_ref(),
            )
        })
    })
    .await
}
#[tauri::command]
pub async fn get_purchase_invoices_view(
    app: AppHandle,
    filters: PurchaseInvoicesViewFilters,
) -> Result<PurchaseInvoicesView> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || authorized(&app, g, |c, _| service::view(c, filters))).await
}
#[tauri::command]
pub async fn search_invoice_suppliers(
    app: AppHandle,
    search: Option<String>,
) -> Result<Vec<InvoiceSupplier>> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || authorized(&app, g, |c, _| service::suppliers(c, search))).await
}
#[tauri::command]
pub async fn get_purchase_invoice_detail(
    app: AppHandle,
    invoice_id: i64,
) -> Result<PurchaseInvoiceDetail> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || authorized(&app, g, |c, _| service::detail(c, invoice_id))).await
}
#[tauri::command]
pub async fn attach_invoice_pdf(
    app: AppHandle,
    invoice_id: i64,
) -> Result<Option<PurchaseInvoiceDetail>> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || {
        authorized(&app, g, |c, _| service::detail(c, invoice_id))?;
        let selected = NativeDialog(app.clone())
            .pick(InvoiceAttachmentKind::Pdf)?
            .map(|p| files::read_selected(&p, InvoiceAttachmentKind::Pdf))
            .transpose();
        let session = app.state::<AuthSession>();
        let s = lock(&session)?;
        check(&s, g)?;
        selected?
            .map(|pdf| db(&app, |c| service::attach(c, invoice_id, &pdf)))
            .transpose()
    })
    .await
}
#[tauri::command]
pub async fn export_invoice_attachment(
    app: AppHandle,
    invoice_id: i64,
    kind: InvoiceAttachmentKind,
) -> Result<bool> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || {
        let file = authorized(&app, g, |c, _| service::attachment(c, invoice_id, kind))?;
        export_file(
            &app.state::<AuthSession>(),
            &NativeDialog(app.clone()),
            g,
            &file,
        )
    })
    .await
}
fn export_file(
    session: &AuthSession,
    dialog: &impl FileDialog,
    g: u64,
    file: &CapturedFile,
) -> Result<bool> {
    check(&*lock(session)?, g)?;
    let destination = dialog.save(&file.metadata.original_name)?;
    check(&*lock(session)?, g)?;
    let Some(path) = destination else {
        return Ok(false);
    };
    let staged = files::stage_export(&path, &file.bytes)?;
    let s = lock(session)?;
    check(&s, g)?;
    // Only the atomic promotion is linearized against logout, not the write.
    staged
        .persist(&path)
        .map_err(|_| files::failure("INVOICE_EXPORT_FAILED"))?;
    Ok(true)
}
#[derive(Default)]
pub struct PdfCopies(pub Mutex<Vec<tempfile::TempDir>>);
#[tauri::command]
pub async fn open_invoice_pdf(app: AppHandle, invoice_id: i64) -> Result<()> {
    let g = begin(&app.state::<AuthSession>())?;
    blocking(move || {
        let file = authorized(&app, g, |c, _| {
            service::attachment(c, invoice_id, InvoiceAttachmentKind::Pdf)
        })?;
        let root = app
            .path()
            .app_cache_dir()
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        let root =
            files::pdf_cache_root(&root).map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        let dir = tempfile::Builder::new()
            .prefix("invoice-pdf-")
            .tempdir_in(root)
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        let path = dir.path().join("document.pdf");
        let staged = files::stage_export(&path, &file.bytes)
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        staged
            .persist(&path)
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        #[cfg(unix)]
        {
            let mut permissions = std::fs::metadata(&path)
                .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?
                .permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions)
                .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        }
        let session = app.state::<AuthSession>();
        let s = lock(&session)?;
        check(&s, g)?;
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        drop(s);
        let copies = app.state::<PdfCopies>();
        let mut copies = copies
            .0
            .lock()
            .map_err(|_| files::failure("INVOICE_OPEN_FAILED"))?;
        copies.push(dir);
        if copies.len() > 8 {
            copies.remove(0);
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::User;
    fn user() -> User {
        User {
            id: 1,
            first_name: "Ada".into(),
            last_name: "POS".into(),
            email: "a@b".into(),
            created_at: "".into(),
            updated_at: "".into(),
        }
    }
    fn session() -> AuthSession {
        let mut state = SessionState::default();
        state.set_current_user(user());
        AuthSession(Mutex::new(state))
    }
    struct FakeDialog<F: Fn() -> Result<Option<PathBuf>>>(F);
    impl<F: Fn() -> Result<Option<PathBuf>>> FileDialog for FakeDialog<F> {
        fn pick(&self, _: InvoiceAttachmentKind) -> Result<Option<PathBuf>> {
            (self.0)()
        }
        fn save(&self, _: &str) -> Result<Option<PathBuf>> {
            (self.0)()
        }
    }
    #[test]
    fn purchase_invoice_dialog_cancel_is_not_error_and_selection_is_bounded() {
        let session = session();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invoice.xml");
        std::fs::write(
            &path,
            include_bytes!("../../tests/fixtures/cfdi/income.xml"),
        )
        .unwrap();
        let g = begin(&session).unwrap();
        let dialog = FakeDialog(|| Ok(Some(path.clone())));
        let first = pick_xml(&session, &dialog, g).unwrap().unwrap();
        assert!(pick_xml(&session, &FakeDialog(|| Ok(None)), g)
            .unwrap()
            .is_none());
        assert_eq!(
            preview(&lock(&session).unwrap(), &first.review_token)
                .unwrap()
                .preview,
            first
        );
        let second = pick_xml(&session, &dialog, g).unwrap().unwrap();
        assert_ne!(second.review_token, first.review_token);
        assert_eq!(
            preview(&lock(&session).unwrap(), &first.review_token)
                .err()
                .unwrap()
                .code,
            "INVOICE_PREVIEW_STALE"
        );
        assert_eq!(
            pick_xml(
                &session,
                &FakeDialog(|| Err(AppError::validation("INVOICE_FILE_UNREADABLE", "safe"))),
                g
            )
            .unwrap_err()
            .code,
            "INVOICE_FILE_UNREADABLE"
        );
        let mut s = lock(&session).unwrap();
        s.clear();
        assert!(s.invoice_preview.is_none());
        s.set_current_user(user());
        assert_eq!(check(&s, g).unwrap_err().code, "INVOICE_PREVIEW_STALE");
    }
    #[test]
    fn purchase_invoice_slow_old_dialog_cannot_authorize_same_user_relogin() {
        let session = session();
        let g = begin(&session).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invoice.xml");
        std::fs::write(
            &path,
            include_bytes!("../../tests/fixtures/cfdi/income.xml"),
        )
        .unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let task = scope.spawn(|| {
                pick_xml(
                    &session,
                    &FakeDialog(|| {
                        barrier.wait();
                        barrier.wait();
                        Ok(Some(path.clone()))
                    }),
                    g,
                )
            });
            barrier.wait();
            {
                let mut s = lock(&session).unwrap();
                s.clear();
                s.set_current_user(user());
            }
            barrier.wait();
            assert_eq!(
                task.join().unwrap().unwrap_err().code,
                "INVOICE_PREVIEW_STALE"
            );
        });
        assert!(lock(&session).unwrap().invoice_preview.is_none());
        assert_eq!(
            pick_xml(
                &session,
                &FakeDialog(|| panic!("stale work must not open a dialog")),
                g
            )
            .unwrap_err()
            .code,
            "INVOICE_PREVIEW_STALE"
        );
    }
    #[test]
    fn purchase_invoice_preview_expiration_and_foreign_discard_do_not_evict_current() {
        let session = session();
        let mut s = lock(&session).unwrap();
        let xml = files::capture(
            include_bytes!("../../tests/fixtures/cfdi/income.xml").to_vec(),
            "a.xml",
            InvoiceAttachmentKind::Xml,
        )
        .unwrap();
        let mut buffer = PreviewBuffer::new(xml).unwrap();
        let token = buffer.preview.review_token.clone();
        buffer.created = Instant::now() - Duration::from_secs(1801);
        s.invoice_preview = Some(buffer);
        assert_eq!(
            preview(&s, &token).err().unwrap().code,
            "INVOICE_PREVIEW_STALE"
        );
        assert_eq!(
            preview(&s, "another-token").err().unwrap().code,
            "INVOICE_PREVIEW_STALE"
        );
        assert!(s.invoice_preview.is_some());
    }

    #[test]
    fn purchase_invoice_pdf_cancel_discard_and_old_save_dialog_are_safe() {
        let session = session();
        let g = begin(&session).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let xml_path = dir.path().join("a.xml");
        let pdf_path = dir.path().join("a.pdf");
        let output = dir.path().join("export.pdf");
        std::fs::write(
            &xml_path,
            include_bytes!("../../tests/fixtures/cfdi/income.xml"),
        )
        .unwrap();
        std::fs::write(&pdf_path, b"%PDF-exact\0").unwrap();
        let original = pick_xml(&session, &FakeDialog(|| Ok(Some(xml_path.clone()))), g)
            .unwrap()
            .unwrap();
        let updated = pick_preview_pdf(
            &session,
            &FakeDialog(|| Ok(Some(pdf_path.clone()))),
            g,
            &original.review_token,
        )
        .unwrap()
        .unwrap();
        assert!(updated.pdf.is_some());
        assert!(pick_preview_pdf(
            &session,
            &FakeDialog(|| Ok(None)),
            g,
            &original.review_token
        )
        .unwrap()
        .is_none());
        assert_eq!(
            preview(&lock(&session).unwrap(), &original.review_token)
                .unwrap()
                .preview,
            updated
        );
        discard_preview(&session, "foreign-token").unwrap();
        assert!(preview(&lock(&session).unwrap(), &original.review_token).is_ok());
        let file = files::read_selected(&pdf_path, InvoiceAttachmentKind::Pdf).unwrap();
        std::fs::write(&output, b"old").unwrap();
        assert!(!export_file(&session, &FakeDialog(|| Ok(None)), g, &file).unwrap());
        assert!(export_file(&session, &FakeDialog(|| Ok(Some(output.clone()))), g, &file).unwrap());
        assert_eq!(std::fs::read(&output).unwrap(), file.bytes.to_vec());
        let old = FakeDialog(|| {
            let mut s = lock(&session).unwrap();
            s.clear();
            s.set_current_user(user());
            Ok(Some(output.clone()))
        });
        std::fs::write(&output, b"must survive").unwrap();
        assert_eq!(
            export_file(&session, &old, g, &file).unwrap_err().code,
            "INVOICE_PREVIEW_STALE"
        );
        assert_eq!(std::fs::read(&output).unwrap(), b"must survive");
        discard_preview(&session, &original.review_token).unwrap();
        discard_preview(&session, &original.review_token).unwrap();
    }
}
