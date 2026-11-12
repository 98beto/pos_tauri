//! Native-only file boundary. Selected symlinks are rejected (including on export).
use crate::{error::AppError, models::purchase_invoice::*};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

#[derive(Clone)]
pub struct CapturedFile {
    pub metadata: InvoiceAttachmentMetadata,
    pub bytes: Arc<[u8]>,
}

pub fn failure(code: &'static str) -> AppError {
    AppError::internal(code, "No se pudo completar la operacion de archivo")
}
fn unreadable() -> AppError {
    AppError::validation(
        "INVOICE_FILE_UNREADABLE",
        "No se pudo leer el archivo seleccionado",
    )
}
pub fn basename(name: &str) -> String {
    let name = name.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(180)
        .collect();
    let clean = clean.trim_matches([' ', '.']);
    if clean.is_empty() {
        "documento".into()
    } else {
        clean.into()
    }
}
pub fn capture(
    bytes: Vec<u8>,
    name: &str,
    kind: InvoiceAttachmentKind,
) -> Result<CapturedFile, AppError> {
    let limit = match kind {
        InvoiceAttachmentKind::Xml => 5 * 1024 * 1024,
        InvoiceAttachmentKind::Pdf => 15 * 1024 * 1024,
    };
    if bytes.len() > limit {
        return Err(AppError::validation(
            "INVOICE_FILE_LIMIT",
            "El archivo excede el limite permitido",
        ));
    }
    if kind == InvoiceAttachmentKind::Pdf && !bytes.starts_with(b"%PDF-") {
        return Err(AppError::validation(
            "INVOICE_FILE_UNSUPPORTED",
            "El archivo no tiene firma PDF",
        ));
    }
    let metadata = InvoiceAttachmentMetadata {
        kind,
        original_name: basename(name),
        mime_type: match kind {
            InvoiceAttachmentKind::Xml => "application/xml",
            InvoiceAttachmentKind::Pdf => "application/pdf",
        }
        .into(),
        size_bytes: bytes.len() as u32,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    Ok(CapturedFile {
        metadata,
        bytes: bytes.into(),
    })
}
pub fn read_selected(path: &Path, kind: InvoiceAttachmentKind) -> Result<CapturedFile, AppError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unreadable())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(unreadable());
    }
    let limit = match kind {
        InvoiceAttachmentKind::Xml => 5 * 1024 * 1024,
        InvoiceAttachmentKind::Pdf => 15 * 1024 * 1024,
    };
    if metadata.len() > limit {
        return Err(AppError::validation(
            "INVOICE_FILE_LIMIT",
            "El archivo excede el limite permitido",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| unreadable())?;
    if !file.metadata().map_err(|_| unreadable())?.is_file() {
        return Err(unreadable());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unreadable())?;
    capture(
        bytes,
        path.file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("documento"),
        kind,
    )
}

/// Stage in the destination filesystem, then replace atomically, never append.
pub fn stage_export(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile, AppError> {
    let result = (|| -> std::io::Result<_> {
        if let Ok(meta) = fs::symlink_metadata(path) {
            if !meta.is_file() || meta.file_type().is_symlink() {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
        }
        let mut file = tempfile::NamedTempFile::new_in(
            path.parent().ok_or(std::io::ErrorKind::InvalidInput)?,
        )?;
        file.write_all(bytes)?;
        file.as_file().sync_all()?;
        Ok(file)
    })();
    result.map_err(|_| failure("INVOICE_EXPORT_FAILED"))
}

/// Dedicated app-owned cache. Never traverse a substituted root or stale link.
pub fn pdf_cache_root(cache: &Path) -> std::io::Result<std::path::PathBuf> {
    let root = cache.join("purchase-invoice-pdfs");
    match fs::symlink_metadata(&root) {
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err(std::io::ErrorKind::InvalidInput.into())
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(cache)?;
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&root)?;
        }
        Err(e) => return Err(e),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(root)
}
pub fn clean_pdf_cache(cache: &Path) -> std::io::Result<()> {
    let root = pdf_cache_root(cache)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("invoice-pdf-")
        {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.file_type().is_symlink() || meta.is_file() {
            fs::remove_file(entry.path())?;
        } else if meta.is_dir() {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_files_and_original_hash() {
        let f = capture(
            b"%PDF-original\0".to_vec(),
            "../../a.pdf",
            InvoiceAttachmentKind::Pdf,
        )
        .unwrap();
        assert_eq!(f.metadata.original_name, "a.pdf");
        assert_eq!(
            f.metadata.sha256,
            format!("{:x}", Sha256::digest(&*f.bytes))
        );
        assert_eq!(
            capture(b"no pdf".to_vec(), "a", InvoiceAttachmentKind::Pdf)
                .err()
                .unwrap()
                .code,
            "INVOICE_FILE_UNSUPPORTED"
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_selected(dir.path(), InvoiceAttachmentKind::Xml)
                .err()
                .unwrap()
                .code,
            "INVOICE_FILE_UNREADABLE"
        );
    }

    #[test]
    fn bounded_native_file_metadata_and_atomic_export_preserve_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.xml");
        fs::File::create(&path)
            .unwrap()
            .set_len(5 * 1024 * 1024 + 1)
            .unwrap();
        assert_eq!(
            read_selected(&path, InvoiceAttachmentKind::Xml)
                .err()
                .unwrap()
                .code,
            "INVOICE_FILE_LIMIT"
        );
        fs::write(&path, b"original").unwrap();
        let staged = stage_export(&path, b"replacement\0bytes").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"original");
        staged.persist(&path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"replacement\0bytes");
        assert_eq!(basename("../../bad\nname.xml"), "badname.xml");
    }
    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_selected_exported_or_followed_during_cache_cleanup() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let sentinel = outside.path().join("keep.pdf");
        fs::write(&sentinel, b"%PDF-keep").unwrap();
        let link = dir.path().join("link.pdf");
        symlink(&sentinel, &link).unwrap();
        assert_eq!(
            read_selected(&link, InvoiceAttachmentKind::Pdf)
                .err()
                .unwrap()
                .code,
            "INVOICE_FILE_UNREADABLE"
        );
        assert_eq!(
            stage_export(&link, b"other").err().unwrap().code,
            "INVOICE_EXPORT_FAILED"
        );
        let root = pdf_cache_root(dir.path()).unwrap();
        symlink(outside.path(), root.join("invoice-pdf-stale")).unwrap();
        clean_pdf_cache(dir.path()).unwrap();
        assert_eq!(fs::read(&sentinel).unwrap(), b"%PDF-keep");
        assert!(!root.join("invoice-pdf-stale").exists());
        fs::remove_dir(&root).unwrap();
        symlink(outside.path(), &root).unwrap();
        assert!(clean_pdf_cache(dir.path()).is_err());
        assert!(sentinel.exists());
    }
}
