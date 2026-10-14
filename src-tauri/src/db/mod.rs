pub mod migrations;

use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

use fs2::FileExt;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

const CURRENT_SCHEMA_VERSION: i64 = 12;
const MIGRATION_SOURCE_VERSION: i64 = 8;
const PENDING_MARKER_NAME: &str = "pos-v0.2.0-migration-pending.json";
const MIGRATION_LOCK_NAME: &str = "pos-v0.2.0-migration.lock";
const RESTORE_MARKER_NAME: &str = "pos-v0.2.0-restore-pending.json";
const V8_REQUIRED_TABLES: &[&str] = &[
    "users",
    "brands",
    "categories",
    "products",
    "sales",
    "sale_details",
    "inventory_movements",
];
const V12_REQUIRED_TABLES: &[&str] = &[
    "users",
    "brands",
    "categories",
    "products",
    "product_presentations",
    "sales",
    "sale_details",
    "inventory_movements",
];

pub struct Database(pub Mutex<Connection>);

#[derive(Debug)]
pub(crate) enum DatabaseOpenError {
    General(String),
    Recovery {
        message: String,
        backup_path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecoveryStartupMessage(String);

impl RecoveryStartupMessage {
    #[cfg(not(debug_assertions))]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DatabaseOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::General(message) => formatter.write_str(message),
            Self::Recovery {
                message,
                backup_path,
            } => write!(formatter, "{message}: {}", backup_path.display()),
        }
    }
}

impl Error for DatabaseOpenError {}

type OpenResult<T> = Result<T, DatabaseOpenError>;

impl DatabaseOpenError {
    pub(crate) fn recovery_startup_message(&self) -> Option<RecoveryStartupMessage> {
        match self {
            Self::Recovery { .. } => Some(RecoveryStartupMessage(self.to_string())),
            Self::General(_) => None,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct MigrationManifest {
    attempt_id: String,
    source_version: i64,
    target_version: i64,
    created_at_unix_seconds: u64,
    backup_path: PathBuf,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct RestoreManifest {
    restore_id: String,
    backup_path: PathBuf,
    database_path: PathBuf,
    temporary_path: PathBuf,
    quarantined_database: Option<PathBuf>,
    quarantined_sidecars: Vec<(PathBuf, PathBuf)>,
    failed_database: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreStage {
    MarkerPersisted,
    NamespaceQuarantined,
    BackupPromoted,
    PromotionSynced,
    RestoreValidated,
    MigrationMarkerArchived,
    CompletedFileSync,
    CompletedRename,
    CompletedDirectorySync,
    ActiveMarkerRemoval,
    ActiveRemovalDirectorySync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerCompletionStage {
    CompletedFileSync,
    CompletedRename,
    CompletedDirectorySync,
    ActiveMarkerRemoval,
    ActiveRemovalDirectorySync,
}

#[derive(Debug)]
struct RollbackError {
    failures: Vec<String>,
}

impl fmt::Display for RollbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "fallaron estos pasos de rollback: {}",
            self.failures.join(", ")
        )
    }
}

impl Error for RollbackError {}

pub fn open_database(app: &tauri::App) -> OpenResult<Connection> {
    let app_data_dir = app.path().app_data_dir().map_err(|_| {
        DatabaseOpenError::General("No se pudo localizar el directorio de datos".into())
    })?;
    let database_path = app_data_dir.join("pos.db");
    open_database_path(&app_data_dir, &database_path)
}

fn open_database_path(directory: &Path, database_path: &Path) -> OpenResult<Connection> {
    open_database_path_with_hook(directory, database_path, |_| Ok(()))
}

fn open_database_path_with_hook<F>(
    directory: &Path,
    database_path: &Path,
    after_migration: F,
) -> OpenResult<Connection>
where
    F: FnOnce(&Connection) -> OpenResult<()>,
{
    prepare_directory(directory)
        .map_err(|_| safe_error("No se pudo preparar el directorio de datos", directory))?;
    let _migration_lock = acquire_migration_lock(directory)?;

    let restore_marker = directory.join(RESTORE_MARKER_NAME);
    reconcile_restore_completion(directory, &restore_marker)?;
    if restore_marker.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar el estado de restauracion",
            &restore_marker,
        )
    })? {
        return Err(pending_restore_error(&restore_marker));
    }

    let pending_path = directory.join(PENDING_MARKER_NAME);
    reconcile_migration_completion(directory, &pending_path)?;
    if pending_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar el estado de migracion", &pending_path))?
    {
        return Err(pending_migration_error(&pending_path));
    }

    let existed = database_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar la base de datos", database_path))?;
    if !existed {
        create_database_file(database_path)
            .map_err(|_| safe_error("No se pudo crear la base de datos", database_path))?;
    }

    let mut connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| safe_error("No se pudo abrir la base de datos", database_path))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| safe_error("No se pudo configurar la base de datos", database_path))?;

    let version = schema_version(&connection, database_path)?;
    let is_new = version == 0 && database_has_no_user_schema(&connection, database_path)?;

    match version {
        0 if is_new => {
            migrations::run(&mut connection).map_err(|_| {
                safe_error(
                    "No se pudo crear el esquema de la base de datos",
                    database_path,
                )
            })?;
            validate_database(&connection, CURRENT_SCHEMA_VERSION, true, database_path)?;
        }
        MIGRATION_SOURCE_VERSION => {
            validate_database(&connection, MIGRATION_SOURCE_VERSION, true, database_path)?;
            let manifest = create_validated_backup(&connection, directory)?;
            write_pending_marker(&pending_path, &manifest)?;

            migrations::run(&mut connection).map_err(|_| {
                recovery_error(
                    "La migracion fallo; restaure el backup indicado por el marcador",
                    &manifest.backup_path,
                )
            })?;
            after_migration(&connection)?;
            validate_database(&connection, CURRENT_SCHEMA_VERSION, true, database_path).map_err(
                |_| {
                    recovery_error(
                        "La validacion posterior fallo; restaure el backup antes de continuar",
                        &manifest.backup_path,
                    )
                },
            )?;
            complete_marker(directory, &pending_path, &manifest).map_err(|_| {
                recovery_error(
                    "No se pudo completar la migracion; conserve y revise el backup",
                    &manifest.backup_path,
                )
            })?;
        }
        CURRENT_SCHEMA_VERSION => {
            validate_database(&connection, CURRENT_SCHEMA_VERSION, true, database_path)?;
        }
        9..=11 => {
            return Err(safe_error(
                "La base tiene una version intermedia no admitida (9-11)",
                database_path,
            ));
        }
        _ => {
            return Err(safe_error(
                "La version de la base de datos es incompatible",
                database_path,
            ));
        }
    }

    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(|_| safe_error("No se pudieron activar las claves foraneas", database_path))?;
    connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))
        .map_err(|_| safe_error("No se pudo activar el journal WAL", database_path))?;
    secure_database_files(database_path)
        .map_err(|_| safe_error("No se pudieron proteger los archivos SQLite", database_path))?;

    Ok(connection)
}

fn safe_error(message: &str, path: &Path) -> DatabaseOpenError {
    DatabaseOpenError::General(format!("{message}: {}", path.display()))
}

fn recovery_error(message: impl Into<String>, backup_path: &Path) -> DatabaseOpenError {
    DatabaseOpenError::Recovery {
        message: message.into(),
        backup_path: backup_path.to_path_buf(),
    }
}

fn pending_migration_error(marker_path: &Path) -> DatabaseOpenError {
    let backup_path = fs::read(marker_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<MigrationManifest>(&bytes).ok())
        .map(|manifest| manifest.backup_path);

    match backup_path {
        Some(path) => recovery_error(
            "Hay una migracion pendiente; restaure o revise el backup",
            &path,
        ),
        None => safe_error(
            "Hay un marcador de migracion pendiente invalido; se requiere intervencion",
            marker_path,
        ),
    }
}

fn pending_restore_error(marker_path: &Path) -> DatabaseOpenError {
    let backup_path = fs::read(marker_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<RestoreManifest>(&bytes).ok())
        .map(|manifest| manifest.backup_path);

    match backup_path {
        Some(path) => recovery_error(
            "Hay una restauracion incompleta; no abra la base y revise la cuarentena",
            &path,
        ),
        None => safe_error(
            "Hay un marcador de restauracion invalido; se requiere intervencion",
            marker_path,
        ),
    }
}

fn schema_version(connection: &Connection, path: &Path) -> OpenResult<i64> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| safe_error("No se pudo leer la version de la base de datos", path))
}

fn database_has_no_user_schema(connection: &Connection, path: &Path) -> OpenResult<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count == 0)
        .map_err(|_| safe_error("No se pudo inspeccionar la base de datos nueva", path))
}

fn validate_database(
    connection: &Connection,
    expected_version: i64,
    require_tables: bool,
    path: &Path,
) -> OpenResult<()> {
    if schema_version(connection, path)? != expected_version {
        return Err(safe_error(
            "La base de datos no tiene la version esperada",
            path,
        ));
    }

    let mut quick_check = connection
        .prepare("PRAGMA quick_check")
        .map_err(|_| safe_error("No se pudo comprobar la integridad SQLite", path))?;
    let mut rows = quick_check
        .query([])
        .map_err(|_| safe_error("No se pudo comprobar la integridad SQLite", path))?;
    let mut saw_ok = false;
    while let Some(row) = rows
        .next()
        .map_err(|_| safe_error("No se pudo comprobar la integridad SQLite", path))?
    {
        let result: String = row
            .get(0)
            .map_err(|_| safe_error("La respuesta de integridad SQLite es invalida", path))?;
        if result != "ok" {
            return Err(safe_error("La comprobacion quick_check fallo", path));
        }
        saw_ok = true;
    }
    if !saw_ok {
        return Err(safe_error("La comprobacion quick_check no respondio", path));
    }

    let mut foreign_keys = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| safe_error("No se pudieron comprobar las claves foraneas", path))?;
    if foreign_keys
        .query([])
        .and_then(|mut rows| rows.next().map(|row| row.is_some()))
        .map_err(|_| safe_error("No se pudieron comprobar las claves foraneas", path))?
    {
        return Err(safe_error("La comprobacion de claves foraneas fallo", path));
    }

    if require_tables {
        let required_tables = match expected_version {
            MIGRATION_SOURCE_VERSION => V8_REQUIRED_TABLES,
            CURRENT_SCHEMA_VERSION => V12_REQUIRED_TABLES,
            _ => &[],
        };
        for table in required_tables {
            let exists: i64 = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .map_err(|_| safe_error("No se pudo validar el esquema SQLite", path))?;
            if exists != 1 {
                return Err(safe_error(
                    "Falta una tabla principal en la base de datos",
                    path,
                ));
            }
        }
    }

    Ok(())
}

fn create_validated_backup(source: &Connection, directory: &Path) -> OpenResult<MigrationManifest> {
    create_validated_backup_with(source, directory, copy_sqlite_database)
}

fn create_validated_backup_with<F>(
    source: &Connection,
    directory: &Path,
    copy_database: F,
) -> OpenResult<MigrationManifest>
where
    F: FnOnce(&Connection, &Path) -> OpenResult<()>,
{
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| safe_error("El reloj del sistema no permite crear el backup", directory))?;
    let attempt_id = format!("{}-{}", now.as_nanos(), std::process::id());
    let backup_path = directory.join(format!("pos-v8-before-v0.2.0-{attempt_id}.db"));

    create_database_file_new(&backup_path)
        .map_err(|_| safe_error("No se pudo crear un backup nuevo", &backup_path))?;
    let backup_result = (|| {
        copy_database(source, &backup_path)?;

        let validation = Connection::open_with_flags(
            &backup_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| safe_error("No se pudo reabrir el backup", &backup_path))?;
        validate_database(&validation, MIGRATION_SOURCE_VERSION, true, &backup_path)?;
        secure_file(&backup_path).map_err(|_| {
            safe_error(
                "No se pudieron proteger los permisos del backup",
                &backup_path,
            )
        })?;
        Ok(())
    })();

    if let Err(error) = backup_result {
        let _ = fs::remove_file(&backup_path);
        return Err(error);
    }

    Ok(MigrationManifest {
        attempt_id,
        source_version: MIGRATION_SOURCE_VERSION,
        target_version: CURRENT_SCHEMA_VERSION,
        created_at_unix_seconds: now.as_secs(),
        backup_path,
    })
}

fn copy_sqlite_database(source: &Connection, destination_path: &Path) -> OpenResult<()> {
    let mut destination = Connection::open_with_flags(
        destination_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| safe_error("No se pudo abrir el backup", destination_path))?;
    let backup = Backup::new(source, &mut destination)
        .map_err(|_| safe_error("No se pudo iniciar el backup SQLite", destination_path))?;
    backup
        .run_to_completion(128, Duration::from_millis(10), None)
        .map_err(|_| safe_error("No se pudo completar el backup SQLite", destination_path))?;
    drop(backup);
    destination
        .close()
        .map_err(|_| safe_error("No se pudo cerrar el backup SQLite", destination_path))
}

// Deliberately kept as a backend maintenance primitive, not a Tauri command.
#[allow(dead_code)]
pub fn restore_v8_backup(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
) -> Result<Connection, Box<dyn Error>> {
    restore_v8_backup_inner(directory, database_path, backup_path).map_err(Into::into)
}

fn restore_v8_backup_inner(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
) -> OpenResult<Connection> {
    restore_v8_backup_inner_with_hook(directory, database_path, backup_path, |_| Ok(()))
}

fn restore_v8_backup_inner_with_hook<F>(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
    mut hook: F,
) -> OpenResult<Connection>
where
    F: FnMut(RestoreStage) -> std::io::Result<()>,
{
    prepare_directory(directory).map_err(|_| {
        safe_error(
            "No se pudo preparar el directorio de restauracion",
            directory,
        )
    })?;
    let _migration_lock = acquire_migration_lock(directory)?;
    let pending_path = directory.join(PENDING_MARKER_NAME);
    let pending_manifest = pending_manifest_for_restore(&pending_path, backup_path)?;
    let restore_marker = directory.join(RESTORE_MARKER_NAME);
    reconcile_restore_completion(directory, &restore_marker)?;
    if restore_marker.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar el marcador de restauracion",
            &restore_marker,
        )
    })? {
        return Err(pending_restore_error(&restore_marker));
    }
    let source = Connection::open_with_flags(
        backup_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| safe_error("No se pudo abrir el backup para restaurarlo", backup_path))?;
    validate_database(&source, MIGRATION_SOURCE_VERSION, true, backup_path)?;

    let database_directory = database_path.parent().unwrap_or(directory);
    prepare_directory(database_directory).map_err(|_| {
        safe_error(
            "No se pudo preparar el destino de restauracion",
            database_directory,
        )
    })?;
    let restore_id = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| safe_error("El reloj impide iniciar la restauracion", directory))?
            .as_nanos()
    );
    let temporary_path = database_directory.join(format!(".pos-restore-{restore_id}.tmp"));
    let quarantined_database = database_directory.join(format!(".pos-quarantine-{restore_id}.db"));
    let failed_database = database_directory.join(format!(".pos-restore-failed-{restore_id}.db"));
    create_database_file_new(&temporary_path).map_err(|_| {
        safe_error(
            "No se pudo crear el archivo temporal de restauracion",
            &temporary_path,
        )
    })?;

    let prepare_result = (|| {
        copy_sqlite_database(&source, &temporary_path)?;
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&temporary_path)
            .and_then(|file| file.sync_all())
            .map_err(|_| {
                safe_error(
                    "No se pudo persistir la restauracion temporal",
                    &temporary_path,
                )
            })?;
        let validation = Connection::open_with_flags(
            &temporary_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| safe_error("No se pudo abrir la restauracion temporal", &temporary_path))?;
        validate_database(&validation, MIGRATION_SOURCE_VERSION, true, &temporary_path)?;
        secure_file(&temporary_path).map_err(|_| {
            safe_error(
                "No se pudo proteger la restauracion temporal",
                &temporary_path,
            )
        })
    })();
    if let Err(error) = prepare_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    drop(source);

    let destination_existed = database_path.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar la base de datos destino",
            database_path,
        )
    })?;
    let mut quarantined_sidecars = Vec::new();
    for (index, sidecar) in sqlite_sidecars(database_path).into_iter().enumerate() {
        if sidecar
            .try_exists()
            .map_err(|_| safe_error("No se pudo comprobar un archivo SQLite anterior", &sidecar))?
        {
            quarantined_sidecars.push((
                sidecar,
                database_directory.join(format!(".pos-quarantine-{restore_id}-sidecar-{index}")),
            ));
        }
    }
    let manifest = RestoreManifest {
        restore_id: restore_id.clone(),
        backup_path: backup_path.to_path_buf(),
        database_path: database_path.to_path_buf(),
        temporary_path: temporary_path.clone(),
        quarantined_database: destination_existed.then(|| quarantined_database.clone()),
        quarantined_sidecars: quarantined_sidecars.clone(),
        failed_database: failed_database.clone(),
    };
    write_json_marker(&restore_marker, &manifest, &restore_id).map_err(|_| {
        recovery_error(
            "No se pudo registrar durablemente la restauracion; conserve el backup",
            backup_path,
        )
    })?;
    if hook(RestoreStage::MarkerPersisted).is_err() {
        remove_restore_marker(&restore_marker, directory)
            .map_err(|rollback| incomplete_recovery_error(backup_path, &rollback))?;
        let _ = fs::remove_file(&temporary_path);
        return Err(recovery_error(
            "La restauracion se detuvo despues de registrar su estado; conserve el backup",
            backup_path,
        ));
    }

    let quarantined_database_ref = destination_existed.then_some(quarantined_database.as_path());
    let mut moved_database = false;
    let mut moved_sidecars = 0;
    let quarantine_result = (|| -> std::io::Result<()> {
        if destination_existed {
            durable_rename(database_path, &quarantined_database)?;
            moved_database = true;
            secure_file(&quarantined_database)?;
        }
        for (original, quarantine) in &quarantined_sidecars {
            durable_rename(original, quarantine)?;
            moved_sidecars += 1;
            secure_file(quarantine)?;
        }
        sync_directory(database_directory)?;
        hook(RestoreStage::NamespaceQuarantined)
    })();
    if quarantine_result.is_err() {
        let moved = &quarantined_sidecars[..moved_sidecars];
        return Err(rollback_or_recovery_error(
            "No se pudo poner en cuarentena el namespace SQLite; conserve el backup",
            backup_path,
            rollback_restored_files(
                database_path,
                moved_database.then_some(quarantined_database.as_path()),
                moved,
                &failed_database,
                database_directory,
                &restore_marker,
                false,
            ),
        ));
    }

    if durable_rename(&temporary_path, database_path).is_err() {
        return Err(rollback_or_recovery_error(
            "No se pudo promover la base restaurada; conserve el backup",
            backup_path,
            rollback_restored_files(
                database_path,
                quarantined_database_ref,
                &quarantined_sidecars,
                &failed_database,
                database_directory,
                &restore_marker,
                false,
            ),
        ));
    }
    if hook(RestoreStage::BackupPromoted).is_err() {
        return Err(rollback_or_recovery_error(
            "La restauracion se detuvo despues de promover el backup",
            backup_path,
            rollback_restored_files(
                database_path,
                quarantined_database_ref,
                &quarantined_sidecars,
                &failed_database,
                database_directory,
                &restore_marker,
                true,
            ),
        ));
    }
    if sync_directory(database_directory).is_err() || hook(RestoreStage::PromotionSynced).is_err() {
        return Err(rollback_or_recovery_error(
            "No se pudo persistir la promocion del backup",
            backup_path,
            rollback_restored_files(
                database_path,
                quarantined_database_ref,
                &quarantined_sidecars,
                &failed_database,
                database_directory,
                &restore_marker,
                true,
            ),
        ));
    }

    let restored = match Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(_) => {
            return Err(rollback_or_recovery_error(
                "No se pudo reabrir la base restaurada; conserve el backup",
                backup_path,
                rollback_restored_files(
                    database_path,
                    quarantined_database_ref,
                    &quarantined_sidecars,
                    &failed_database,
                    database_directory,
                    &restore_marker,
                    true,
                ),
            ));
        }
    };
    if validate_database(&restored, MIGRATION_SOURCE_VERSION, true, database_path).is_err() {
        drop(restored);
        return Err(rollback_or_recovery_error(
            "La base restaurada no es valida; conserve el backup",
            backup_path,
            rollback_restored_files(
                database_path,
                quarantined_database_ref,
                &quarantined_sidecars,
                &failed_database,
                database_directory,
                &restore_marker,
                true,
            ),
        ));
    }
    if hook(RestoreStage::RestoreValidated).is_err() {
        drop(restored);
        return Err(rollback_or_recovery_error(
            "La restauracion se detuvo despues de validarse",
            backup_path,
            rollback_restored_files(
                database_path,
                quarantined_database_ref,
                &quarantined_sidecars,
                &failed_database,
                database_directory,
                &restore_marker,
                true,
            ),
        ));
    }

    if let Some(manifest) = pending_manifest {
        let archived_path = directory.join(format!(
            "pos-v0.2.0-migration-restored-{}.json",
            manifest.attempt_id
        ));
        let archive_result = archived_path
            .try_exists()
            .ok()
            .filter(|exists| !exists)
            .ok_or(())
            .and_then(|_| durable_rename(&pending_path, &archived_path).map_err(|_| ()))
            .and_then(|_| sync_directory(directory).map_err(|_| ()))
            .and_then(|_| hook(RestoreStage::MigrationMarkerArchived).map_err(|_| ()));
        if archive_result.is_err() {
            drop(restored);
            if archived_path.exists()
                && !pending_path.exists()
                && (durable_rename(&archived_path, &pending_path).is_err()
                    || sync_directory(directory).is_err())
            {
                return Err(incomplete_recovery_error(
                    backup_path,
                    &RollbackError {
                        failures: vec!["restaurar marcador de migracion".into()],
                    },
                ));
            }
            return Err(rollback_or_recovery_error(
                "No se pudo archivar el marcador de migracion; conserve el backup",
                backup_path,
                rollback_restored_files(
                    database_path,
                    quarantined_database_ref,
                    &quarantined_sidecars,
                    &failed_database,
                    database_directory,
                    &restore_marker,
                    true,
                ),
            ));
        }
    }

    let completed_restore =
        directory.join(format!("pos-v0.2.0-restore-completed-{restore_id}.json"));
    finalize_marker_with_hook(
        directory,
        &restore_marker,
        &completed_restore,
        &manifest,
        &restore_id,
        |stage| {
            hook(match stage {
                MarkerCompletionStage::CompletedFileSync => RestoreStage::CompletedFileSync,
                MarkerCompletionStage::CompletedRename => RestoreStage::CompletedRename,
                MarkerCompletionStage::CompletedDirectorySync => {
                    RestoreStage::CompletedDirectorySync
                }
                MarkerCompletionStage::ActiveMarkerRemoval => RestoreStage::ActiveMarkerRemoval,
                MarkerCompletionStage::ActiveRemovalDirectorySync => {
                    RestoreStage::ActiveRemovalDirectorySync
                }
            })
        },
    )
    .map_err(|_| {
        recovery_error(
            "La base fue restaurada, pero no se pudo completar durablemente su marcador; no la abra",
            backup_path,
        )
    })?;
    Ok(restored)
}

fn sqlite_sidecars(database_path: &Path) -> [PathBuf; 2] {
    [
        sqlite_sidecar(database_path, "-wal"),
        sqlite_sidecar(database_path, "-shm"),
    ]
}

fn sqlite_sidecar(database_path: &Path, suffix: &str) -> PathBuf {
    let mut path = database_path.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}

fn pending_manifest_for_restore(
    pending_path: &Path,
    backup_path: &Path,
) -> OpenResult<Option<MigrationManifest>> {
    if !pending_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar el marcador pendiente", pending_path))?
    {
        return Ok(None);
    }
    let manifest: MigrationManifest = serde_json::from_slice(
        &fs::read(pending_path)
            .map_err(|_| safe_error("No se pudo leer el marcador pendiente", pending_path))?,
    )
    .map_err(|_| safe_error("El marcador pendiente no es valido", pending_path))?;
    if manifest.source_version != MIGRATION_SOURCE_VERSION
        || manifest.target_version != CURRENT_SCHEMA_VERSION
        || manifest.backup_path != backup_path
    {
        return Err(recovery_error(
            "El marcador pendiente no corresponde al backup v8 indicado",
            &manifest.backup_path,
        ));
    }
    Ok(Some(manifest))
}

fn rollback_or_recovery_error(
    message: &'static str,
    backup_path: &Path,
    rollback: Result<(), RollbackError>,
) -> DatabaseOpenError {
    match rollback {
        Ok(()) => recovery_error(message, backup_path),
        Err(error) => incomplete_recovery_error(backup_path, &error),
    }
}

fn incomplete_recovery_error(backup_path: &Path, error: &RollbackError) -> DatabaseOpenError {
    recovery_error(
        format!(
            "La recuperacion quedo incompleta; no abra la base y conserve backup, cuarentena y marcador ({error})"
        ),
        backup_path,
    )
}

fn rollback_restored_files(
    database_path: &Path,
    quarantined_database: Option<&Path>,
    quarantined_sidecars: &[(PathBuf, PathBuf)],
    failed_database: &Path,
    database_directory: &Path,
    restore_marker: &Path,
    promoted: bool,
) -> Result<(), RollbackError> {
    let mut failures = Vec::new();
    let mut promoted_moved = !promoted;
    if promoted {
        match database_path.try_exists() {
            Ok(true) => match durable_rename(database_path, failed_database) {
                Ok(()) => promoted_moved = true,
                Err(_) => failures.push("aislar base restaurada".into()),
            },
            Ok(false) => promoted_moved = true,
            Err(_) => failures.push("comprobar base restaurada".into()),
        }
    }

    for (original, quarantine) in quarantined_sidecars.iter().rev() {
        match quarantine.try_exists() {
            Ok(true) if durable_rename(quarantine, original).is_err() => {
                failures.push("restaurar sidecar SQLite".into())
            }
            Err(_) => failures.push("comprobar sidecar en cuarentena".into()),
            _ => {}
        }
    }

    if promoted_moved {
        if let Some(quarantine) = quarantined_database {
            match quarantine.try_exists() {
                Ok(true) if durable_rename(quarantine, database_path).is_err() => {
                    failures.push("restaurar base anterior".into())
                }
                Err(_) => failures.push("comprobar base anterior en cuarentena".into()),
                _ => {}
            }
        }
    } else if quarantined_database.is_some() {
        failures.push("base anterior bloqueada por restauracion no aislada".into());
    }

    if sync_directory(database_directory).is_err() {
        failures.push("sincronizar namespace restaurado".into());
    }
    if failures.is_empty() {
        if fs::remove_file(restore_marker).is_err() {
            failures.push("retirar marcador de restauracion".into());
        } else if sync_directory(restore_marker.parent().unwrap_or(database_directory)).is_err() {
            failures.push("sincronizar retiro del marcador".into());
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(RollbackError { failures })
    }
}

fn remove_restore_marker(path: &Path, directory: &Path) -> Result<(), RollbackError> {
    let mut failures = Vec::new();
    if fs::remove_file(path).is_err() {
        failures.push("retirar marcador de restauracion".into());
    }
    if sync_directory(directory).is_err() {
        failures.push("sincronizar retiro del marcador".into());
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(RollbackError { failures })
    }
}

fn acquire_migration_lock(directory: &Path) -> OpenResult<fs::File> {
    let path = directory.join(MIGRATION_LOCK_NAME);
    let file = open_lock_file(&path)
        .map_err(|_| safe_error("No se pudo abrir el bloqueo de migracion", &path))?;
    secure_file(&path).map_err(|_| safe_error("No se pudo proteger el bloqueo", &path))?;
    FileExt::try_lock_exclusive(&file).map_err(|_| {
        safe_error(
            "Otra instancia esta comprobando o migrando la base de datos",
            &path,
        )
    })?;
    Ok(file)
}

#[cfg(unix)]
fn open_lock_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_lock_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
}

fn write_pending_marker(path: &Path, manifest: &MigrationManifest) -> OpenResult<()> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|_| safe_error("No se pudo serializar el marcador de migracion", path))?;
    write_marker_bytes(path, &bytes, &manifest.attempt_id)
}

fn write_json_marker<T: Serialize>(path: &Path, value: &T, attempt_id: &str) -> OpenResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| safe_error("No se pudo serializar el marcador", path))?;
    write_marker_bytes(path, &bytes, attempt_id)
}

fn write_marker_bytes(path: &Path, bytes: &[u8], attempt_id: &str) -> OpenResult<()> {
    let temporary = path.with_extension(format!("tmp-{attempt_id}"));
    let result = (|| {
        let mut file = create_file_new(&temporary)
            .map_err(|_| safe_error("No se pudo crear el marcador temporal", &temporary))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| safe_error("No se pudo persistir el marcador de migracion", &temporary))?;
        durable_publish_new(&temporary, path)
            .map_err(|_| safe_error("Ya existe un marcador de migracion", path))?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn complete_marker(
    directory: &Path,
    pending_path: &Path,
    manifest: &MigrationManifest,
) -> OpenResult<()> {
    let completed_path = directory.join(format!(
        "pos-v0.2.0-migration-completed-{}.json",
        manifest.attempt_id
    ));
    finalize_marker_with_hook(
        directory,
        pending_path,
        &completed_path,
        manifest,
        &manifest.attempt_id,
        |_| Ok(()),
    )
}

fn finalize_marker_with_hook<T, F>(
    directory: &Path,
    active_path: &Path,
    completed_path: &Path,
    manifest: &T,
    attempt_id: &str,
    mut hook: F,
) -> OpenResult<()>
where
    T: Serialize,
    F: FnMut(MarkerCompletionStage) -> std::io::Result<()>,
{
    if completed_path.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar el manifiesto completado",
            completed_path,
        )
    })? {
        return Err(safe_error(
            "No se sobrescribira un manifiesto completado existente",
            completed_path,
        ));
    }

    let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| {
        safe_error(
            "No se pudo serializar el manifiesto completado",
            completed_path,
        )
    })?;
    let temporary = completed_path.with_extension(format!("tmp-{attempt_id}"));
    let result = (|| {
        let mut file = create_file_new(&temporary)
            .map_err(|_| safe_error("No se pudo crear el manifiesto temporal", &temporary))?;
        file.write_all(&bytes)
            .map_err(|_| safe_error("No se pudo escribir el manifiesto temporal", &temporary))?;
        hook(MarkerCompletionStage::CompletedFileSync)
            .and_then(|_| file.sync_all())
            .map_err(|_| safe_error("No se pudo persistir el manifiesto temporal", &temporary))?;
        drop(file);

        hook(MarkerCompletionStage::CompletedRename)
            .and_then(|_| durable_publish_new(&temporary, completed_path))
            .map_err(|_| {
                safe_error(
                    "No se pudo publicar el manifiesto completado",
                    completed_path,
                )
            })?;
        hook(MarkerCompletionStage::CompletedDirectorySync)
            .and_then(|_| sync_directory(directory))
            .map_err(|_| {
                safe_error(
                    "No se pudo persistir el manifiesto completado",
                    completed_path,
                )
            })?;

        hook(MarkerCompletionStage::ActiveMarkerRemoval)
            .and_then(|_| fs::remove_file(active_path))
            .map_err(|_| safe_error("No se pudo retirar el marcador activo", active_path))?;
        hook(MarkerCompletionStage::ActiveRemovalDirectorySync)
            .and_then(|_| sync_directory(directory))
            .map_err(|_| safe_error("No se pudo persistir el retiro del marcador", active_path))
    })();
    let _ = fs::remove_file(&temporary);
    result
}

fn reconcile_migration_completion(directory: &Path, active_path: &Path) -> OpenResult<()> {
    if !active_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar el marcador de migracion", active_path))?
    {
        return Ok(());
    }
    let manifest: MigrationManifest = read_marker(active_path, "migracion")?;
    let completed_path = directory.join(format!(
        "pos-v0.2.0-migration-completed-{}.json",
        manifest.attempt_id
    ));
    reconcile_completed_pair(directory, active_path, &completed_path, &manifest)
}

fn reconcile_restore_completion(directory: &Path, active_path: &Path) -> OpenResult<()> {
    if !active_path.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar el marcador de restauracion",
            active_path,
        )
    })? {
        return Ok(());
    }
    let manifest: RestoreManifest = read_marker(active_path, "restauracion")?;
    let completed_path = directory.join(format!(
        "pos-v0.2.0-restore-completed-{}.json",
        manifest.restore_id
    ));
    reconcile_completed_pair(directory, active_path, &completed_path, &manifest)
}

fn read_marker<T: for<'de> Deserialize<'de>>(path: &Path, kind: &str) -> OpenResult<T> {
    let bytes = fs::read(path)
        .map_err(|_| safe_error(&format!("No se pudo leer el marcador de {kind}"), path))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| safe_error(&format!("El marcador de {kind} no es valido"), path))
}

fn reconcile_completed_pair<T>(
    directory: &Path,
    active_path: &Path,
    completed_path: &Path,
    active_manifest: &T,
) -> OpenResult<()>
where
    T: for<'de> Deserialize<'de> + PartialEq,
{
    if !completed_path.try_exists().map_err(|_| {
        safe_error(
            "No se pudo comprobar el manifiesto completado",
            completed_path,
        )
    })? {
        return Ok(());
    }
    let completed_manifest: T = read_marker(completed_path, "finalizacion")?;
    if &completed_manifest != active_manifest {
        return Err(safe_error(
            "El marcador activo y el manifiesto completado no coinciden",
            completed_path,
        ));
    }

    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(completed_path)
        .and_then(|file| file.sync_all())
        .map_err(|_| {
            safe_error(
                "No se pudo persistir el manifiesto completado",
                completed_path,
            )
        })?;
    sync_directory(directory).map_err(|_| {
        safe_error(
            "No se pudo persistir el directorio del manifiesto completado",
            completed_path,
        )
    })?;
    fs::remove_file(active_path)
        .map_err(|_| safe_error("No se pudo retirar el marcador activo", active_path))?;
    sync_directory(directory)
        .map_err(|_| safe_error("No se pudo persistir el retiro del marcador", active_path))
}

#[cfg(unix)]
fn prepare_directory(directory: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn prepare_directory(directory: &Path) -> std::io::Result<()> {
    fs::create_dir_all(directory)
}

#[cfg(unix)]
fn create_file_new(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn create_file_new(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
}

fn create_database_file_new(path: &Path) -> std::io::Result<()> {
    create_file_new(path)?.sync_all()
}

fn create_database_file(path: &Path) -> std::io::Result<()> {
    create_database_file_new(path)
}

#[cfg(unix)]
fn secure_file(path: &Path) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    // Windows persists each critical namespace move with MOVEFILE_WRITE_THROUGH.
    // A residual active marker after deletion is safe and reconciled on startup.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "directory durability is not implemented for this platform",
    ))
}

#[cfg(windows)]
fn durable_rename(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    fn verbatim(path: &Path) -> std::io::Result<Vec<u16>> {
        let absolute = std::path::absolute(path)?;
        let wide: Vec<u16> = absolute.as_os_str().encode_wide().collect();
        let verbatim_prefix = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
        let device_prefix = [b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16];
        let mut result = if wide.starts_with(&verbatim_prefix) || wide.starts_with(&device_prefix) {
            wide
        } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
            let mut result: Vec<u16> = r"\\?\UNC\".encode_utf16().collect();
            result.extend_from_slice(&wide[2..]);
            result
        } else {
            let mut result: Vec<u16> = r"\\?\".encode_utf16().collect();
            result.extend_from_slice(&wide);
            result
        };
        result.push(0);
        Ok(result)
    }

    let source = verbatim(source)?;
    let destination = verbatim(destination)?;
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn durable_rename(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn durable_publish_new(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    durable_rename(temporary, destination)
}

#[cfg(not(windows))]
fn durable_publish_new(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    fs::hard_link(temporary, destination)?;
    sync_directory(destination.parent().unwrap_or_else(|| Path::new(".")))
}

#[cfg(unix)]
fn secure_database_files(database_path: &Path) -> std::io::Result<()> {
    for path in std::iter::once(database_path.to_path_buf()).chain(sqlite_sidecars(database_path)) {
        if path.try_exists()? {
            secure_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_database_files(_database_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    mod migration_qa;

    fn create_v8_database(path: &Path) {
        let mut connection = Connection::open(path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run_to_version(&mut connection, 8).unwrap();
        connection
            .execute_batch(
                "INSERT INTO users (id, first_name, last_name, email, password_hash)
                 VALUES (1, 'Ada', 'Lovelace', 'ada@pos.local', 'hash');
                 INSERT INTO brands (id, name, name_search_key) VALUES (1, 'Marca', 'marca');
                 INSERT INTO categories (id, name, name_search_key) VALUES (1, 'Categoria', 'categoria');
                 INSERT INTO products
                    (id, name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES (1, 'Producto', 'producto', 250, 3, 'SKU-1', 'sku-1', 1, 1);",
            )
            .unwrap();
    }

    fn version(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    fn matching_files(directory: &Path, prefix: &str) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(prefix)
            })
            .collect()
    }

    #[test]
    fn validates_required_tables_for_v8_and_v12() {
        let temp = TempDir::new().unwrap();
        let v8_path = temp.path().join("v8.db");
        create_v8_database(&v8_path);
        let v8 = Connection::open(&v8_path).unwrap();
        validate_database(&v8, 8, true, &v8_path).unwrap();
        assert_eq!(
            v8.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='product_presentations'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );

        let v12_path = temp.path().join("v12.db");
        let mut v12 = Connection::open(&v12_path).unwrap();
        migrations::run(&mut v12).unwrap();
        validate_database(&v12, 12, true, &v12_path).unwrap();
        v12.pragma_update(None, "foreign_keys", false).unwrap();
        v12.execute("DROP TABLE product_presentations", []).unwrap();
        assert!(validate_database(&v12, 12, true, &v12_path).is_err());
    }

    #[test]
    fn new_database_goes_directly_to_v12_without_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");

        let connection = open_database_path(temp.path(), &path).unwrap();

        assert_eq!(schema_version(&connection, &path).unwrap(), 12);
        assert_eq!(
            connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap()
                .to_lowercase(),
            "wal"
        );
        assert!(matching_files(temp.path(), "pos-v8-before-").is_empty());
    }

    #[test]
    fn migrates_v8_with_one_valid_backup_and_completed_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);

        let connection = open_database_path(temp.path(), &path).unwrap();
        assert_eq!(schema_version(&connection, &path).unwrap(), 12);
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        drop(connection);

        let backups = matching_files(temp.path(), "pos-v8-before-v0.2.0-");
        let manifests = matching_files(temp.path(), "pos-v0.2.0-migration-completed-");
        assert_eq!(backups.len(), 1);
        assert_eq!(manifests.len(), 1);
        assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
        let manifest: MigrationManifest =
            serde_json::from_slice(&fs::read(&manifests[0]).unwrap()).unwrap();
        assert_eq!(manifest.backup_path, backups[0]);
        let backup =
            Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        validate_database(&backup, 8, true, &backups[0]).unwrap();
    }

    #[test]
    fn v12_reopens_without_replacing_backup_or_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        drop(open_database_path(temp.path(), &path).unwrap());
        let backup = matching_files(temp.path(), "pos-v8-before-v0.2.0-")[0].clone();
        let manifest = matching_files(temp.path(), "pos-v0.2.0-migration-completed-")[0].clone();
        let backup_bytes = fs::read(&backup).unwrap();
        let manifest_bytes = fs::read(&manifest).unwrap();

        drop(open_database_path(temp.path(), &path).unwrap());

        assert_eq!(
            matching_files(temp.path(), "pos-v8-before-v0.2.0-").len(),
            1
        );
        assert_eq!(fs::read(backup).unwrap(), backup_bytes);
        assert_eq!(fs::read(manifest).unwrap(), manifest_bytes);
    }

    #[test]
    fn rejects_intermediate_future_and_negative_versions() {
        for requested in [-1_i64, 1, 7, 9, 10, 11, 13] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "user_version", requested)
                .unwrap();
            drop(connection);

            let error = open_database_path(temp.path(), &path).unwrap_err();
            assert!(error.to_string().contains(path.to_string_lossy().as_ref()));
            assert!(error.recovery_startup_message().is_none());
            assert_eq!(version(&path), requested);
            assert!(matching_files(temp.path(), "pos-v8-before-").is_empty());
        }
    }

    #[test]
    fn rejects_nonempty_v0_schema_and_invalid_sqlite_file() {
        let v0 = TempDir::new().unwrap();
        let v0_path = v0.path().join("pos.db");
        Connection::open(&v0_path)
            .unwrap()
            .execute("CREATE TABLE unexpected(id INTEGER)", [])
            .unwrap();
        assert!(open_database_path(v0.path(), &v0_path).is_err());
        assert_eq!(version(&v0_path), 0);

        let invalid = TempDir::new().unwrap();
        let invalid_path = invalid.path().join("pos.db");
        fs::write(&invalid_path, b"not a sqlite database").unwrap();
        let error = open_database_path(invalid.path(), &invalid_path)
            .unwrap_err()
            .to_string();
        assert!(error.contains(invalid_path.to_string_lossy().as_ref()));
        assert!(matching_files(invalid.path(), "pos-v8-before-").is_empty());
    }

    #[test]
    fn pending_marker_blocks_v8_and_v12_and_reports_exact_backup() {
        for migrate_first in [false, true] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            create_v8_database(&path);
            if migrate_first {
                let mut connection = Connection::open(&path).unwrap();
                migrations::run(&mut connection).unwrap();
            }
            let backup = temp.path().join("exact-backup.db");
            let marker = MigrationManifest {
                attempt_id: "blocked".into(),
                source_version: 8,
                target_version: 12,
                created_at_unix_seconds: 1,
                backup_path: backup.clone(),
            };
            write_pending_marker(&temp.path().join(PENDING_MARKER_NAME), &marker).unwrap();

            let error = open_database_path(temp.path(), &path).unwrap_err();
            let startup_message = error.recovery_startup_message().unwrap();
            assert!(startup_message
                .0
                .contains(backup.to_string_lossy().as_ref()));
        }
    }

    #[test]
    fn restore_marker_blocks_startup_after_an_interrupted_namespace_transition() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let backup = temp.path().join("exact-backup.db");
        let marker = temp.path().join(RESTORE_MARKER_NAME);
        write_json_marker(
            &marker,
            &RestoreManifest {
                restore_id: "interrupted".into(),
                backup_path: backup.clone(),
                database_path: path.clone(),
                temporary_path: temp.path().join("restore.tmp"),
                quarantined_database: Some(temp.path().join("quarantine.db")),
                quarantined_sidecars: Vec::new(),
                failed_database: temp.path().join("failed.db"),
            },
            "interrupted",
        )
        .unwrap();

        let error = open_database_path(temp.path(), &path).unwrap_err();

        assert!(error.to_string().contains("restauracion incompleta"));
        assert!(error
            .to_string()
            .contains(backup.to_string_lossy().as_ref()));
        assert_eq!(version(&path), 8);
    }

    #[test]
    fn migration_failure_rolls_back_to_v8_and_keeps_backup_and_marker() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "INSERT INTO inventory_movements
                    (product_id,product_name,sku,product_search_key,sku_search_key,
                     origin,reason,type,quantity,operation_token,intent_fingerprint)
                 VALUES(1,'A','A','a','a','manual','ajuste','entrada',1,'duplicate','one');
                 DROP INDEX idx_inventory_movements_operation_token;
                 INSERT INTO inventory_movements
                    (product_id,product_name,sku,product_search_key,sku_search_key,
                     origin,reason,type,quantity,operation_token,intent_fingerprint)
                 VALUES(1,'B','B','b','b','manual','ajuste','entrada',1,'duplicate','two');",
            )
            .unwrap();
        drop(connection);

        assert!(open_database_path(temp.path(), &path).is_err());
        assert_eq!(version(&path), 8);
        assert_eq!(
            matching_files(temp.path(), "pos-v8-before-v0.2.0-").len(),
            1
        );
        assert!(temp.path().join(PENDING_MARKER_NAME).exists());
        assert!(open_database_path(temp.path(), &path)
            .unwrap_err()
            .to_string()
            .contains("backup"));
    }

    #[test]
    fn failed_post_validation_leaves_v12_pending_and_blocks_reopen() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);

        let result = open_database_path_with_hook(temp.path(), &path, |connection| {
            connection
                .pragma_update(None, "foreign_keys", false)
                .unwrap();
            connection
                .execute(
                    "INSERT INTO product_presentations
                       (product_id,name,mode,base_quantity,price,is_default,is_active)
                     VALUES(999,'Huerfana','fixed',1,1,0,1)",
                    [],
                )
                .unwrap();
            Ok(())
        });

        let marker: MigrationManifest =
            serde_json::from_slice(&fs::read(temp.path().join(PENDING_MARKER_NAME)).unwrap())
                .unwrap();
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(marker.backup_path.to_string_lossy().as_ref()));
        assert_eq!(version(&path), 12);
        assert!(temp.path().join(PENDING_MARKER_NAME).exists());
        assert!(open_database_path(temp.path(), &path).is_err());
    }

    #[test]
    fn migration_completion_failures_always_leave_a_durable_guard() {
        for failure_stage in [
            MarkerCompletionStage::CompletedFileSync,
            MarkerCompletionStage::CompletedRename,
            MarkerCompletionStage::CompletedDirectorySync,
            MarkerCompletionStage::ActiveMarkerRemoval,
            MarkerCompletionStage::ActiveRemovalDirectorySync,
        ] {
            let temp = TempDir::new().unwrap();
            let active = temp.path().join(PENDING_MARKER_NAME);
            let manifest = MigrationManifest {
                attempt_id: format!("completion-{failure_stage:?}"),
                source_version: 8,
                target_version: 12,
                created_at_unix_seconds: 1,
                backup_path: temp.path().join("backup.db"),
            };
            write_pending_marker(&active, &manifest).unwrap();
            let completed = temp.path().join(format!(
                "pos-v0.2.0-migration-completed-{}.json",
                manifest.attempt_id
            ));

            let result = finalize_marker_with_hook(
                temp.path(),
                &active,
                &completed,
                &manifest,
                &manifest.attempt_id,
                |stage| {
                    if stage == failure_stage {
                        Err(std::io::Error::other("injected completion failure"))
                    } else {
                        Ok(())
                    }
                },
            );

            assert!(result.is_err(), "stage {failure_stage:?}");
            assert!(
                active.exists() || completed.exists(),
                "stage {failure_stage:?} removed every guard"
            );
            if completed.exists() {
                let persisted: MigrationManifest =
                    serde_json::from_slice(&fs::read(&completed).unwrap()).unwrap();
                assert_eq!(persisted, manifest, "stage {failure_stage:?}");
            }
            if active.exists() && completed.exists() {
                reconcile_migration_completion(temp.path(), &active).unwrap();
                assert!(!active.exists(), "stage {failure_stage:?}");
                assert!(completed.exists(), "stage {failure_stage:?}");
            }
        }
    }

    #[test]
    fn restore_completion_failures_always_leave_a_durable_guard() {
        for failure_stage in [
            MarkerCompletionStage::CompletedFileSync,
            MarkerCompletionStage::CompletedRename,
            MarkerCompletionStage::CompletedDirectorySync,
            MarkerCompletionStage::ActiveMarkerRemoval,
            MarkerCompletionStage::ActiveRemovalDirectorySync,
        ] {
            let temp = TempDir::new().unwrap();
            let active = temp.path().join(RESTORE_MARKER_NAME);
            let manifest = RestoreManifest {
                restore_id: format!("completion-{failure_stage:?}"),
                backup_path: temp.path().join("backup.db"),
                database_path: temp.path().join("pos.db"),
                temporary_path: temp.path().join("restore.tmp"),
                quarantined_database: Some(temp.path().join("quarantine.db")),
                quarantined_sidecars: Vec::new(),
                failed_database: temp.path().join("failed.db"),
            };
            write_json_marker(&active, &manifest, &manifest.restore_id).unwrap();
            let completed = temp.path().join(format!(
                "pos-v0.2.0-restore-completed-{}.json",
                manifest.restore_id
            ));

            let result = finalize_marker_with_hook(
                temp.path(),
                &active,
                &completed,
                &manifest,
                &manifest.restore_id,
                |stage| {
                    if stage == failure_stage {
                        Err(std::io::Error::other("injected completion failure"))
                    } else {
                        Ok(())
                    }
                },
            );

            assert!(result.is_err(), "stage {failure_stage:?}");
            assert!(
                active.exists() || completed.exists(),
                "stage {failure_stage:?} removed every guard"
            );
            if completed.exists() {
                let persisted: RestoreManifest =
                    serde_json::from_slice(&fs::read(&completed).unwrap()).unwrap();
                assert_eq!(persisted, manifest, "stage {failure_stage:?}");
            }
            if active.exists() && completed.exists() {
                reconcile_restore_completion(temp.path(), &active).unwrap();
                assert!(!active.exists(), "stage {failure_stage:?}");
                assert!(completed.exists(), "stage {failure_stage:?}");
            }
        }
    }

    #[test]
    fn mismatched_active_and_completed_markers_fail_closed() {
        let temp = TempDir::new().unwrap();
        let active = temp.path().join(PENDING_MARKER_NAME);
        let manifest = MigrationManifest {
            attempt_id: "mismatch".into(),
            source_version: 8,
            target_version: 12,
            created_at_unix_seconds: 1,
            backup_path: temp.path().join("backup.db"),
        };
        write_pending_marker(&active, &manifest).unwrap();
        let completed = temp
            .path()
            .join("pos-v0.2.0-migration-completed-mismatch.json");
        let different = MigrationManifest {
            backup_path: temp.path().join("different.db"),
            ..manifest
        };
        write_json_marker(&completed, &different, "different").unwrap();

        assert!(reconcile_migration_completion(temp.path(), &active).is_err());
        assert!(active.exists());
        assert!(completed.exists());
    }

    #[test]
    fn backup_creation_never_overwrites_an_existing_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("reserved.db");
        fs::write(&path, b"keep").unwrap();

        assert!(create_database_file_new(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"keep");
    }

    #[test]
    fn injected_backup_failure_removes_partial_file_and_preserves_source() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let source = Connection::open(&path).unwrap();

        let error = create_validated_backup_with(&source, temp.path(), |_, destination| {
            Err(safe_error("Fallo de backup inyectado", destination))
        })
        .unwrap_err();

        assert!(error.to_string().contains("Fallo de backup inyectado"));
        assert!(matching_files(temp.path(), "pos-v8-before-v0.2.0-").is_empty());
        assert_eq!(version(&path), 8);
        assert_eq!(
            source
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn invalid_injected_backup_is_rejected_and_removed() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let source = Connection::open(&path).unwrap();

        let result = create_validated_backup_with(&source, temp.path(), |_, destination| {
            fs::write(destination, b"invalid sqlite backup")
                .map_err(|_| safe_error("Fallo de escritura inyectado", destination))
        });

        assert!(result.is_err());
        assert!(matching_files(temp.path(), "pos-v8-before-v0.2.0-").is_empty());
        assert_eq!(version(&path), 8);
    }

    #[test]
    fn backup_includes_committed_wal_frames_without_checkpoint() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let source = Connection::open(&path).unwrap();
        source
            .pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))
            .unwrap();
        source.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        source
            .execute("UPDATE products SET stock=17 WHERE id=1", [])
            .unwrap();
        assert!(sqlite_sidecars(&path)[0].exists());

        let manifest = create_validated_backup(&source, temp.path()).unwrap();
        let backup = Connection::open_with_flags(
            &manifest.backup_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        assert_eq!(
            backup
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            17
        );
    }

    #[test]
    fn durable_restore_replaces_database_and_reopens_v8() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        drop(open_database_path(temp.path(), &path).unwrap());
        let backup = matching_files(temp.path(), "pos-v8-before-v0.2.0-")[0].clone();
        let current = Connection::open(&path).unwrap();
        current
            .pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))
            .unwrap();
        current
            .pragma_update(None, "wal_autocheckpoint", 0)
            .unwrap();
        current
            .execute("UPDATE products SET stock=99 WHERE id=1", [])
            .unwrap();
        assert!(sqlite_sidecars(&path)[0].exists());
        drop(current);

        let restored = restore_v8_backup_inner(temp.path(), &path, &backup).unwrap();
        assert_eq!(schema_version(&restored, &path).unwrap(), 8);
        assert_eq!(
            restored
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert!(sqlite_sidecars(&path)
            .into_iter()
            .all(|sidecar| !sidecar.exists()));
        assert!(matching_files(temp.path(), ".pos-restore-").is_empty());
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn failed_restore_after_promotion_never_exposes_v8_with_old_sidecars_and_rolls_back() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.sqlite");
        create_v8_database(&path);
        let backup = temp.path().join("backup.db");
        fs::copy(&path, &backup).unwrap();
        let mut current = Connection::open(&path).unwrap();
        migrations::run(&mut current).unwrap();
        drop(current);
        let before = fs::read(&path).unwrap();
        let [wal, shm] = sqlite_sidecars(&path);
        fs::write(&wal, b"old wal").unwrap();
        fs::write(&shm, b"old shm").unwrap();
        let manifest = MigrationManifest {
            attempt_id: "replace-failure".into(),
            source_version: 8,
            target_version: 12,
            created_at_unix_seconds: 1,
            backup_path: backup.clone(),
        };
        let pending = temp.path().join(PENDING_MARKER_NAME);
        write_pending_marker(&pending, &manifest).unwrap();

        let error = restore_v8_backup_inner_with_hook(temp.path(), &path, &backup, |stage| {
            if stage == RestoreStage::NamespaceQuarantined {
                assert!(!path.exists());
                assert!(!wal.exists());
                assert!(!shm.exists());
            }
            if stage == RestoreStage::BackupPromoted {
                assert_eq!(version(&path), 8);
                assert!(!wal.exists());
                assert!(!shm.exists());
                return Err(std::io::Error::other("injected post-promote failure"));
            }
            Ok(())
        })
        .unwrap_err();

        assert!(error
            .to_string()
            .contains(backup.to_string_lossy().as_ref()));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(wal).unwrap(), b"old wal");
        assert_eq!(fs::read(shm).unwrap(), b"old shm");
        assert!(pending.exists());
        assert_eq!(matching_files(temp.path(), ".pos-restore-failed-").len(), 1);
        assert!(matching_files(temp.path(), ".pos-quarantine-").is_empty());
        assert!(!temp.path().join(RESTORE_MARKER_NAME).exists());
    }

    #[test]
    fn successful_restore_archives_markers_and_preserves_quarantine_for_diagnostics() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("backup.db");
        create_v8_database(&backup);
        let path = temp.path().join("pos.db");
        let mut current = Connection::open(&path).unwrap();
        migrations::run(&mut current).unwrap();
        drop(current);
        for (sidecar, contents) in sqlite_sidecars(&path)
            .into_iter()
            .zip([b"old wal", b"old shm"])
        {
            fs::write(sidecar, contents).unwrap();
        }
        let manifest = MigrationManifest {
            attempt_id: "successful-restore".into(),
            source_version: 8,
            target_version: 12,
            created_at_unix_seconds: 1,
            backup_path: backup.clone(),
        };
        let pending = temp.path().join(PENDING_MARKER_NAME);
        write_pending_marker(&pending, &manifest).unwrap();

        let restored = restore_v8_backup_inner(temp.path(), &path, &backup).unwrap();

        assert_eq!(schema_version(&restored, &path).unwrap(), 8);
        assert!(!pending.exists());
        let archived = temp
            .path()
            .join("pos-v0.2.0-migration-restored-successful-restore.json");
        assert_eq!(
            fs::read(archived).unwrap(),
            serde_json::to_vec_pretty(&manifest).unwrap()
        );
        assert!(sqlite_sidecars(&path)
            .into_iter()
            .all(|sidecar| !sidecar.exists()));
        assert_eq!(matching_files(temp.path(), ".pos-quarantine-").len(), 3);
        assert_eq!(
            matching_files(temp.path(), "pos-v0.2.0-restore-completed-").len(),
            1
        );
        assert!(!temp.path().join(RESTORE_MARKER_NAME).exists());
    }

    #[test]
    fn injected_restore_failures_rollback_each_durable_stage() {
        for failure_stage in [
            RestoreStage::MarkerPersisted,
            RestoreStage::NamespaceQuarantined,
            RestoreStage::BackupPromoted,
            RestoreStage::PromotionSynced,
            RestoreStage::RestoreValidated,
            RestoreStage::MigrationMarkerArchived,
        ] {
            let temp = TempDir::new().unwrap();
            let backup = temp.path().join("backup.db");
            create_v8_database(&backup);
            let path = temp.path().join("pos.db");
            let mut current = Connection::open(&path).unwrap();
            migrations::run(&mut current).unwrap();
            drop(current);
            let before = fs::read(&path).unwrap();
            for (sidecar, contents) in sqlite_sidecars(&path)
                .into_iter()
                .zip([b"old wal", b"old shm"])
            {
                fs::write(sidecar, contents).unwrap();
            }
            let pending = temp.path().join(PENDING_MARKER_NAME);
            write_pending_marker(
                &pending,
                &MigrationManifest {
                    attempt_id: format!("failure-{failure_stage:?}"),
                    source_version: 8,
                    target_version: 12,
                    created_at_unix_seconds: 1,
                    backup_path: backup.clone(),
                },
            )
            .unwrap();

            let result = restore_v8_backup_inner_with_hook(temp.path(), &path, &backup, |stage| {
                if stage == failure_stage {
                    Err(std::io::Error::other("injected stage failure"))
                } else {
                    Ok(())
                }
            });

            assert!(result.is_err(), "stage {failure_stage:?}");
            assert_eq!(fs::read(&path).unwrap(), before, "stage {failure_stage:?}");
            let [wal, shm] = sqlite_sidecars(&path);
            assert_eq!(
                fs::read(wal).unwrap(),
                b"old wal",
                "stage {failure_stage:?}"
            );
            assert_eq!(
                fs::read(shm).unwrap(),
                b"old shm",
                "stage {failure_stage:?}"
            );
            assert!(pending.exists(), "stage {failure_stage:?}");
            assert!(!temp.path().join(RESTORE_MARKER_NAME).exists());
        }
    }

    #[test]
    fn rollback_reports_all_failed_recovery_steps_and_keeps_marker() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        fs::write(&path, b"promoted").unwrap();
        let failed = temp.path().join("missing-parent").join("failed.db");
        let marker = temp.path().join(RESTORE_MARKER_NAME);
        fs::write(&marker, b"state").unwrap();
        let quarantine = temp.path().join("old.db");
        fs::write(&quarantine, b"old").unwrap();

        let error = rollback_restored_files(
            &path,
            Some(&quarantine),
            &[],
            &failed,
            temp.path(),
            &marker,
            true,
        )
        .unwrap_err();

        assert!(error.failures.contains(&"aislar base restaurada".into()));
        assert!(error
            .failures
            .contains(&"base anterior bloqueada por restauracion no aislada".into()));
        assert!(marker.exists());
        assert_eq!(fs::read(path).unwrap(), b"promoted");
        assert_eq!(fs::read(quarantine).unwrap(), b"old");
    }

    #[cfg(windows)]
    #[test]
    fn windows_write_through_publish_is_supported_and_does_not_overwrite() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("durable-entry.tmp");
        let destination = temp.path().join("durable-entry");
        fs::write(&source, b"durable").unwrap();
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&source)
            .unwrap()
            .sync_all()
            .unwrap();

        durable_publish_new(&source, &destination)
            .expect("the Windows test volume must support write-through moves");
        assert!(!source.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"durable");

        let second = temp.path().join("second.tmp");
        fs::write(&second, b"replacement").unwrap();
        assert!(durable_publish_new(&second, &destination).is_err());
        assert_eq!(fs::read(destination).unwrap(), b"durable");

        let long_directory = temp.path().join("x".repeat(240));
        fs::create_dir(&long_directory).unwrap();
        let long_source = long_directory.join("source.tmp");
        let long_destination = long_directory.join("destination");
        fs::write(&long_source, b"long path").unwrap();
        durable_publish_new(&long_source, &long_destination).unwrap();
        assert_eq!(fs::read(long_destination).unwrap(), b"long path");
    }

    #[cfg(windows)]
    #[test]
    fn windows_completion_sync_failures_are_fail_closed() {
        for failure_stage in [
            MarkerCompletionStage::CompletedDirectorySync,
            MarkerCompletionStage::ActiveRemovalDirectorySync,
        ] {
            let temp = TempDir::new().unwrap();
            let active = temp.path().join(PENDING_MARKER_NAME);
            let manifest = MigrationManifest {
                attempt_id: format!("windows-{failure_stage:?}"),
                source_version: 8,
                target_version: 12,
                created_at_unix_seconds: 1,
                backup_path: temp.path().join("backup.db"),
            };
            write_pending_marker(&active, &manifest).unwrap();
            let completed = temp.path().join(format!(
                "pos-v0.2.0-migration-completed-{}.json",
                manifest.attempt_id
            ));

            assert!(finalize_marker_with_hook(
                temp.path(),
                &active,
                &completed,
                &manifest,
                &manifest.attempt_id,
                |stage| {
                    if stage == failure_stage {
                        Err(std::io::Error::other("injected Windows flush failure"))
                    } else {
                        Ok(())
                    }
                },
            )
            .is_err());

            assert!(active.exists() || completed.exists());
            if !active.exists() {
                let persisted: MigrationManifest =
                    serde_json::from_slice(&fs::read(completed).unwrap()).unwrap();
                assert_eq!(persisted, manifest);
            }
        }
    }

    #[test]
    fn exclusive_lock_blocks_concurrent_preflight_and_stale_file_is_safe() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let migration_lock = acquire_migration_lock(temp.path()).unwrap();

        let error = open_database_path(temp.path(), &path).unwrap_err();
        assert!(error.to_string().contains("Otra instancia"));
        assert_eq!(version(&path), 8);
        assert!(matching_files(temp.path(), "pos-v8-before-").is_empty());

        drop(migration_lock);
        fs::write(temp.path().join(MIGRATION_LOCK_NAME), b"stale metadata").unwrap();
        let connection = open_database_path(temp.path(), &path).unwrap();
        assert_eq!(schema_version(&connection, &path).unwrap(), 12);
    }

    #[cfg(unix)]
    #[test]
    fn sqlite_backup_marker_and_wal_files_are_private() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let connection = open_database_path(temp.path(), &path).unwrap();
        connection
            .execute(
                "INSERT INTO brands(name,name_search_key) VALUES('B','b')",
                [],
            )
            .unwrap();

        let backup = matching_files(temp.path(), "pos-v8-before-v0.2.0-")[0].clone();
        let manifest = matching_files(temp.path(), "pos-v0.2.0-migration-completed-")[0].clone();
        for file in [path, backup, manifest] {
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        for sidecar in sqlite_sidecars(&temp.path().join("pos.db")) {
            if sidecar.exists() {
                assert_eq!(
                    fs::metadata(sidecar).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
    }

    #[test]
    fn foreign_key_preflight_rejects_v8_without_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute(
                "INSERT INTO products
                   (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id)
                 VALUES('Huerfano','huerfano',1,0,'ORPHAN','orphan',999,1)",
                params![],
            )
            .unwrap();
        drop(connection);

        assert!(open_database_path(temp.path(), &path).is_err());
        assert_eq!(version(&path), 8);
        assert!(matching_files(temp.path(), "pos-v8-before-").is_empty());
    }
}
