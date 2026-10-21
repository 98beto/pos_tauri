pub mod migrations;

use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fmt, fs,
    io::{Seek, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

use fs2::FileExt;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

const CURRENT_SCHEMA_VERSION: i64 = 13;
const LEGACY_MIGRATION_SOURCE_VERSION: i64 = 8;
const DIRECT_MIGRATION_SOURCE_VERSION: i64 = 12;
const PENDING_MARKER_NAME: &str = "pos-schema-v13-migration-pending.json";
const LEGACY_PENDING_MARKER_NAME: &str = "pos-v0.2.0-migration-pending.json";
const MIGRATION_LOCK_NAME: &str = "pos-schema-v13-migration.lock";
const RESTORE_MARKER_NAME: &str = "pos-schema-v13-restore-pending.json";
const LEGACY_RESTORE_MARKER_NAME: &str = "pos-v0.2.0-restore-pending.json";
const V8_REQUIRED_TABLES: &[&str] = &[
    "users",
    "brands",
    "categories",
    "products",
    "sales",
    "sale_details",
    "inventory_movements",
];
const V12_AND_V13_REQUIRED_TABLES: &[&str] = &[
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
    Locked(String),
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
            Self::Locked(message) => formatter.write_str(message),
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
            Self::General(_) | Self::Locked(_) => None,
        }
    }
}

pub(crate) enum MaintenanceRestoreError {
    Locked,
    Failed,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct MigrationManifest {
    attempt_id: String,
    source_version: i64,
    target_version: i64,
    created_at_unix_seconds: u64,
    database_path: PathBuf,
    backup_path: PathBuf,
    backup_size_bytes: u64,
    backup_sha256: String,
    backup_validation_completed: bool,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
struct CompletedMigrationManifest {
    #[serde(flatten)]
    migration: MigrationManifest,
    canonical_target_validation_completed: bool,
    source_specific_comparison_completed: bool,
    financial_and_valuation_removal_verified: bool,
    removed_valuation_row_count: usize,
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
    source_version: i64,
    target_version: i64,
    backup_size_bytes: u64,
    backup_sha256: String,
}

type SnapshotRows = Vec<Vec<String>>;

#[derive(Debug, PartialEq, Eq)]
struct MigrationSnapshot {
    users: SnapshotRows,
    brands: SnapshotRows,
    categories: SnapshotRows,
    products: SnapshotRows,
    presentations: SnapshotRows,
    sales: SnapshotRows,
    sale_details: SnapshotRows,
    movements: SnapshotRows,
    sequences: SnapshotRows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreStage {
    MarkerPersisted,
    NamespaceQuarantined,
    BeforeBackupPromotion,
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

pub(crate) fn acquire_application_lock(
    app: &tauri::App,
) -> OpenResult<crate::process_lock::ApplicationLock> {
    let app_data_dir = app.path().app_data_dir().map_err(|_| {
        DatabaseOpenError::General("No se pudo localizar el directorio de datos".into())
    })?;
    prepare_directory(&app_data_dir).map_err(|_| {
        DatabaseOpenError::General("No se pudo preparar el directorio de datos".into())
    })?;
    crate::process_lock::acquire(&app_data_dir)
        .map_err(|error| DatabaseOpenError::General(error.to_string()))
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
    let existed = database_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar la base de datos", database_path))?;
    let marker_paths = [
        directory.join(RESTORE_MARKER_NAME),
        directory.join(LEGACY_RESTORE_MARKER_NAME),
        directory.join(PENDING_MARKER_NAME),
        directory.join(LEGACY_PENDING_MARKER_NAME),
    ];
    let mut active_marker = None;
    for path in &marker_paths {
        if path
            .try_exists()
            .map_err(|_| safe_error("No se pudo comprobar un marcador de recuperacion", path))?
        {
            active_marker = Some(path);
            break;
        }
    }
    let recovery_backup = active_marker.and_then(|path| marker_backup_path(path));
    if existed && active_marker.is_none() {
        // The first look at an installed database must not create a lock, change
        // permissions, or recover SQLite when there is no durable work to reconcile.
        preflight_installed_database(database_path)?;
    }

    prepare_directory(directory)
        .map_err(|_| safe_error("No se pudo preparar el directorio de datos", directory))?;
    let _migration_lock = acquire_migration_lock(directory)?;
    if existed && active_marker.is_none() {
        // Repeat under the process lock so the approved source cannot race migration.
        preflight_installed_database(database_path)?;
    }

    for marker_name in [RESTORE_MARKER_NAME, LEGACY_RESTORE_MARKER_NAME] {
        let restore_marker = directory.join(marker_name);
        reconcile_restore_completion(directory, &restore_marker).map_err(|error| {
            marker_recovery_error(error, &restore_marker, recovery_backup.as_deref())
        })?;
        if restore_marker.try_exists().map_err(|_| {
            safe_error(
                "No se pudo comprobar el estado de restauracion",
                &restore_marker,
            )
        })? {
            return Err(pending_restore_error(&restore_marker));
        }
    }

    for marker_name in [PENDING_MARKER_NAME, LEGACY_PENDING_MARKER_NAME] {
        let marker = directory.join(marker_name);
        reconcile_migration_completion(directory, &marker)
            .map_err(|error| marker_recovery_error(error, &marker, recovery_backup.as_deref()))?;
        if marker
            .try_exists()
            .map_err(|_| safe_error("No se pudo comprobar el estado de migracion", &marker))?
        {
            return Err(pending_migration_error(&marker));
        }
    }
    let pending_path = directory.join(PENDING_MARKER_NAME);

    if !existed {
        create_database_file(database_path)
            .map_err(|_| safe_error("No se pudo crear la base de datos", database_path))?;
    } else {
        // Restore reconciliation can replace the installed file. Approve that source too
        // before SQLite is allowed to open it read-write.
        preflight_installed_database(database_path).map_err(|error| match &recovery_backup {
            Some(backup_path) => recovery_error(
                format!("La comprobacion posterior a la recuperacion fallo ({error})"),
                backup_path,
            ),
            None => error,
        })?;
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
        LEGACY_MIGRATION_SOURCE_VERSION | DIRECT_MIGRATION_SOURCE_VERSION => {
            validate_database(&connection, version, true, database_path)?;
            let before = migration_source_snapshot(&connection, database_path, version)?;
            let v8_sequences = if version == LEGACY_MIGRATION_SOURCE_VERSION {
                Some(sequence_state(&connection, database_path)?)
            } else {
                None
            };
            let valuation_rows = source_valuation_rows(&connection, database_path, version)?;
            let manifest = create_validated_backup(&connection, directory, database_path, version)?;
            write_pending_marker(&pending_path, &manifest).map_err(|_| {
                recovery_error(
                    "No se pudo registrar durablemente la migracion; conserve el backup",
                    &manifest.backup_path,
                )
            })?;

            migrations::run(&mut connection).map_err(|_| {
                recovery_error(
                    "La migracion fallo; restaure el backup indicado por el marcador",
                    &manifest.backup_path,
                )
            })?;
            if let Some(sequences) = &v8_sequences {
                restore_sequence_state(&connection, database_path, sequences).map_err(|_| {
                    recovery_error(
                        "No se pudieron restaurar las secuencias v8; restaure el backup",
                        &manifest.backup_path,
                    )
                })?;
            }
            let after =
                migration_target_snapshot(&connection, database_path, version).map_err(|_| {
                    recovery_error(
                        "No se pudo comparar la migracion; restaure el backup",
                        &manifest.backup_path,
                    )
                })?;
            if after != before {
                return Err(recovery_error(
                    "La migracion altero identidades, relaciones, stock o totales historicos; restaure el backup",
                    &manifest.backup_path,
                ));
            }
            if version == LEGACY_MIGRATION_SOURCE_VERSION {
                validate_v8_generated_values(&connection, database_path).map_err(|_| {
                    recovery_error(
                        "Los valores migrados desde v8 no son validos; restaure el backup",
                        &manifest.backup_path,
                    )
                })?;
            }
            validate_financial_retirement(&connection, database_path, valuation_rows.len())
                .map_err(|_| {
                    recovery_error(
                        "La validacion del retiro financiero fallo; restaure el backup",
                        &manifest.backup_path,
                    )
                })?;
            after_migration(&connection).map_err(|_| {
                recovery_error(
                    "La validacion posterior fallo; restaure el backup antes de continuar",
                    &manifest.backup_path,
                )
            })?;
            validate_database(&connection, CURRENT_SCHEMA_VERSION, true, database_path).map_err(
                |_| {
                    recovery_error(
                        "La validacion posterior fallo; restaure el backup antes de continuar",
                        &manifest.backup_path,
                    )
                },
            )?;
            complete_marker(directory, &pending_path, &manifest, valuation_rows.len()).map_err(
                |_| {
                    recovery_error(
                        "No se pudo completar la migracion; conserve y revise el backup",
                        &manifest.backup_path,
                    )
                },
            )?;
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

fn preflight_installed_database(database_path: &Path) -> OpenResult<()> {
    let connection = Connection::open_with_flags(
        database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| {
        safe_error(
            "No se pudo abrir la base de datos en solo lectura",
            database_path,
        )
    })?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|_| safe_error("No se pudo configurar la base de datos", database_path))?;
    let version = schema_version(&connection, database_path)?;
    let is_new = version == 0 && database_has_no_user_schema(&connection, database_path)?;
    match version {
        0 if is_new => validate_database(&connection, 0, false, database_path),
        LEGACY_MIGRATION_SOURCE_VERSION
        | DIRECT_MIGRATION_SOURCE_VERSION
        | CURRENT_SCHEMA_VERSION => validate_database(&connection, version, true, database_path),
        _ => {
            validate_database(&connection, version, false, database_path)?;
            Err(safe_error(
                "La version de la base de datos es incompatible",
                database_path,
            ))
        }
    }
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

fn marker_backup_path(marker_path: &Path) -> Option<PathBuf> {
    fs::read(marker_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.get("backup_path")?.as_str().map(PathBuf::from))
}

fn marker_recovery_error(
    error: DatabaseOpenError,
    marker_path: &Path,
    fallback_backup: Option<&Path>,
) -> DatabaseOpenError {
    let marker_backup = marker_backup_path(marker_path);
    match marker_backup.as_deref().or(fallback_backup) {
        Some(backup_path) => recovery_error(
            format!("No se pudo reconciliar el estado durable ({error})"),
            backup_path,
        ),
        None => error,
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
        .prepare("PRAGMA integrity_check")
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
            LEGACY_MIGRATION_SOURCE_VERSION => V8_REQUIRED_TABLES,
            DIRECT_MIGRATION_SOURCE_VERSION | CURRENT_SCHEMA_VERSION => V12_AND_V13_REQUIRED_TABLES,
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

    validate_schema_shape(connection, expected_version, path)?;

    Ok(())
}

fn validate_schema_shape(connection: &Connection, version: i64, path: &Path) -> OpenResult<()> {
    if version == LEGACY_MIGRATION_SOURCE_VERSION {
        validate_canonical_structure(connection, path, version)?;
    } else if version == DIRECT_MIGRATION_SOURCE_VERSION {
        validate_v12_structure(connection, path)?;
    } else if version == CURRENT_SCHEMA_VERSION {
        validate_v13_structure(connection, path)?;
    }
    let column_exists = |table: &str, column: &str| -> rusqlite::Result<bool> {
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
            rusqlite::params![table, column],
            |row| row.get(0),
        )
    };
    let valid = match version {
        LEGACY_MIGRATION_SOURCE_VERSION => true,
        DIRECT_MIGRATION_SOURCE_VERSION => column_exists("products", "inventory_cost_cents")
            .and_then(|product_cost| {
                column_exists("sale_details", "movement_cost_cents")
                    .map(|detail_cost| product_cost && detail_cost)
            })
            .and_then(|valid| {
                column_exists("inventory_movements", "movement_cost_cents")
                    .map(|movement_cost| valid && movement_cost)
            })
            .unwrap_or(false),
        CURRENT_SCHEMA_VERSION => {
            let retired = [
                ("products", "inventory_cost_cents"),
                ("sale_details", "movement_cost_cents"),
                ("inventory_movements", "unit_price_cents"),
                ("inventory_movements", "movement_cost_cents"),
                ("inventory_movements", "inventory_cost_before_cents"),
                ("inventory_movements", "inventory_cost_after_cents"),
            ];
            let required = [
                ("inventory_movements", "purchase_unit_price_cents"),
                ("inventory_movements", "purchase_total_cents"),
            ];
            retired
                .iter()
                .all(|(table, column)| !column_exists(table, column).unwrap_or(true))
                && required
                    .iter()
                    .all(|(table, column)| column_exists(table, column).unwrap_or(false))
        }
        _ => true,
    };
    if !valid {
        return Err(safe_error(
            "La estructura de la base no corresponde a su version",
            path,
        ));
    }
    Ok(())
}

fn validate_canonical_structure(
    connection: &Connection,
    path: &Path,
    version: i64,
) -> OpenResult<()> {
    let mut reference = Connection::open_in_memory()
        .map_err(|_| safe_error("No se pudo crear la referencia del esquema", path))?;
    match version {
        LEGACY_MIGRATION_SOURCE_VERSION => migrations::run_to_v8(&mut reference),
        DIRECT_MIGRATION_SOURCE_VERSION => migrations::run_to_v12(&mut reference),
        CURRENT_SCHEMA_VERSION => migrations::run_to_v13(&mut reference),
        _ => unreachable!("canonical validation only supports known schemas"),
    }
    .map_err(|_| safe_error("No se pudo construir la referencia del esquema", path))?;
    let expected = canonical_schema_objects(&reference)
        .map_err(|_| safe_error("No se pudo leer la referencia del esquema", path))?;
    let actual = canonical_schema_objects(connection)
        .map_err(|_| safe_error("No se pudo leer el esquema instalado", path))?;
    if actual != expected {
        return Err(safe_error(
            "El esquema no es canonico para su version",
            path,
        ));
    }
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    let characters = sql.chars().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character.is_whitespace() {
            index += 1;
        } else if matches!(character, '\'' | '"' | '`' | '[') {
            let closing = if character == '[' { ']' } else { character };
            let mut token = String::from(character);
            index += 1;
            while index < characters.len() {
                let current = characters[index];
                token.push(if character == '\'' {
                    current
                } else {
                    current.to_ascii_lowercase()
                });
                index += 1;
                if current == closing {
                    if index < characters.len() && characters[index] == closing {
                        token.push(characters[index]);
                        index += 1;
                    } else {
                        break;
                    }
                }
            }
            tokens.push(token);
        } else if character.is_alphanumeric() || matches!(character, '_' | '$') {
            let start = index;
            index += 1;
            while index < characters.len()
                && (characters[index].is_alphanumeric() || matches!(characters[index], '_' | '$'))
            {
                index += 1;
            }
            tokens.push(
                characters[start..index]
                    .iter()
                    .map(|value| value.to_ascii_lowercase())
                    .collect(),
            );
        } else {
            let mut token = String::from(character);
            index += 1;
            if index < characters.len()
                && matches!(
                    (character, characters[index]),
                    ('|', '|')
                        | ('<', '<' | '=' | '>')
                        | ('>', '>' | '=')
                        | ('!', '=')
                        | ('=', '=')
                        | ('-', '>')
                )
            {
                token.push(characters[index]);
                index += 1;
                if token == "->" && index < characters.len() && characters[index] == '>' {
                    token.push('>');
                    index += 1;
                }
            }
            tokens.push(token);
        }
    }
    tokens.join("\u{1f}")
}

fn canonical_schema_objects(
    connection: &Connection,
) -> rusqlite::Result<Vec<(String, String, String, String)>> {
    connection
        .prepare(
            "SELECT type,name,tbl_name,sql FROM sqlite_schema
             WHERE name NOT GLOB 'sqlite_*'
               AND NOT (type='index' AND sql IS NULL)
             ORDER BY type,name",
        )?
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                normalize_schema_sql(&row.get::<_, String>(3)?),
            ))
        })?
        .collect()
}

fn validate_v12_structure(connection: &Connection, path: &Path) -> OpenResult<()> {
    validate_canonical_structure(connection, path, DIRECT_MIGRATION_SOURCE_VERSION)
}

fn validate_v13_structure(connection: &Connection, path: &Path) -> OpenResult<()> {
    validate_canonical_structure(connection, path, CURRENT_SCHEMA_VERSION)
}

// Each side owns its SQL contract. Do not alias source/target projections: a
// retained-field change must be reviewed independently on both sides. Generated
// values and financial retirement are validated separately from literal equality.
struct SnapshotProjection {
    users: &'static str,
    brands: &'static str,
    categories: &'static str,
    products: &'static str,
    presentations: Option<&'static str>,
    sales: &'static str,
    sale_details: &'static str,
    movements: &'static str,
    sequences: &'static str,
}

const V8_BEFORE: SnapshotProjection = SnapshotProjection {
    users: "SELECT quote(id),quote(first_name),quote(last_name),quote(email),quote(password_hash),quote(created_at),quote(updated_at) FROM users ORDER BY id",
    brands: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM brands ORDER BY id",
    categories: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM categories ORDER BY id",
    products: "SELECT quote(id),quote(name),quote(name_search_key),quote(price),quote(stock),quote(sku),quote(sku_search_key),quote(brand_id),quote(category_id),quote(created_at),quote(updated_at) FROM products ORDER BY id",
    presentations: None,
    sales: "SELECT quote(id),quote(user_id),quote(cashier_name),quote(cashier_email),quote(checkout_token),quote(cart_fingerprint),quote(sale_date),quote(total),quote(payment_method),quote(cash_received),quote(change_amount),quote(created_at),quote(updated_at) FROM sales ORDER BY id",
    sale_details: "SELECT quote(id),quote(product_id),quote(sale_id),quote(product_name),quote(sku),quote(quantity),quote(unit_price),quote(created_at),quote(updated_at) FROM sale_details ORDER BY id",
    movements: "SELECT quote(id),quote(product_id),quote(product_name),quote(sku),quote(product_search_key),quote(sku_search_key),quote(sale_id),quote(origin),quote(reason),quote(type),quote(quantity),quote(operation_token),quote(intent_fingerprint),quote(created_at),quote(updated_at) FROM inventory_movements ORDER BY id",
    sequences: "WITH names(name) AS (VALUES('users'),('brands'),('categories'),('products'),('sales'),('sale_details'),('inventory_movements')) SELECT quote(names.name),quote(sqlite_sequence.name IS NOT NULL),quote(sqlite_sequence.seq) FROM names LEFT JOIN sqlite_sequence USING(name) ORDER BY names.name",
};

const V13_AFTER_V8: SnapshotProjection = SnapshotProjection {
    users: "SELECT quote(id),quote(first_name),quote(last_name),quote(email),quote(password_hash),quote(created_at),quote(updated_at) FROM users ORDER BY id",
    brands: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM brands ORDER BY id",
    categories: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM categories ORDER BY id",
    products: "SELECT quote(id),quote(name),quote(name_search_key),quote(price),quote(stock),quote(sku),quote(sku_search_key),quote(brand_id),quote(category_id),quote(created_at),quote(updated_at) FROM products ORDER BY id",
    presentations: None,
    sales: "SELECT quote(id),quote(user_id),quote(cashier_name),quote(cashier_email),quote(checkout_token),quote(cart_fingerprint),quote(sale_date),quote(total),quote(payment_method),quote(cash_received),quote(change_amount),quote(created_at),quote(updated_at) FROM sales ORDER BY id",
    sale_details: "SELECT quote(id),quote(product_id),quote(sale_id),quote(product_name),quote(sku),quote(quantity),quote(unit_price),quote(created_at),quote(updated_at) FROM sale_details ORDER BY id",
    movements: "SELECT quote(id),quote(product_id),quote(product_name),quote(sku),quote(product_search_key),quote(sku_search_key),quote(sale_id),quote(origin),quote(reason),quote(type),quote(quantity),quote(operation_token),quote(intent_fingerprint),quote(created_at),quote(updated_at) FROM inventory_movements ORDER BY id",
    sequences: "WITH names(name) AS (VALUES('users'),('brands'),('categories'),('products'),('sales'),('sale_details'),('inventory_movements')) SELECT quote(names.name),quote(sqlite_sequence.name IS NOT NULL),quote(sqlite_sequence.seq) FROM names LEFT JOIN sqlite_sequence USING(name) ORDER BY names.name",
};

const V12_BEFORE: SnapshotProjection = SnapshotProjection {
    users: "SELECT quote(id),quote(first_name),quote(last_name),quote(email),quote(password_hash),quote(created_at),quote(updated_at) FROM users ORDER BY id",
    brands: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM brands ORDER BY id",
    categories: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM categories ORDER BY id",
    products: "SELECT quote(id),quote(name),quote(name_search_key),quote(price),quote(stock),quote(sku),quote(sku_search_key),quote(brand_id),quote(category_id),quote(created_at),quote(updated_at),quote(base_unit) FROM products ORDER BY id",
    presentations: Some("SELECT quote(id),quote(product_id),quote(name),quote(mode),quote(base_quantity),quote(price),quote(barcode),quote(barcode_search_key),quote(is_default),quote(is_active),quote(created_at),quote(updated_at) FROM product_presentations ORDER BY id"),
    sales: "SELECT quote(id),quote(user_id),quote(cashier_name),quote(cashier_email),quote(checkout_token),quote(cart_fingerprint),quote(sale_date),quote(subtotal),quote(discount_type),quote(discount_value),quote(discount_amount),quote(total),quote(payment_method),quote(cash_received),quote(change_amount),quote(created_at),quote(updated_at) FROM sales ORDER BY id",
    sale_details: "SELECT quote(id),quote(product_id),quote(sale_id),quote(record_version),quote(presentation_id),quote(product_name),quote(sku),quote(presentation_name),quote(presentation_barcode),quote(base_unit),quote(presentation_mode),quote(presentation_base_quantity),quote(quantity),quote(catalog_unit_price),quote(unit_price),quote(entered_quantity),quote(base_quantity),quote(line_total),quote(created_at),quote(updated_at) FROM sale_details ORDER BY id",
    movements: "SELECT quote(id),quote(product_id),quote(product_name),quote(sku),quote(product_search_key),quote(sku_search_key),quote(sale_id),quote(record_version),quote(presentation_id),quote(sale_detail_id),quote(presentation_name),quote(presentation_barcode),quote(base_unit),quote(presentation_mode),quote(presentation_base_quantity),quote(origin),quote(reason),quote(type),quote(quantity),quote(entered_quantity),quote(base_quantity),quote(stock_before),quote(stock_after),quote(supplier),quote(provider_search_key),quote(document_reference),quote(document_reference_search_key),quote(note),quote(note_search_key),quote(operation_token),quote(intent_fingerprint),quote(created_at),quote(updated_at) FROM inventory_movements WHERE reason<>'valoracion' AND type<>'valoracion' ORDER BY id",
    sequences: "SELECT quote(name),quote(seq) FROM sqlite_sequence ORDER BY name",
};

const V13_AFTER_V12: SnapshotProjection = SnapshotProjection {
    users: "SELECT quote(id),quote(first_name),quote(last_name),quote(email),quote(password_hash),quote(created_at),quote(updated_at) FROM users ORDER BY id",
    brands: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM brands ORDER BY id",
    categories: "SELECT quote(id),quote(name),quote(name_search_key),quote(created_at),quote(updated_at) FROM categories ORDER BY id",
    products: "SELECT quote(id),quote(name),quote(name_search_key),quote(price),quote(stock),quote(sku),quote(sku_search_key),quote(brand_id),quote(category_id),quote(created_at),quote(updated_at),quote(base_unit) FROM products ORDER BY id",
    presentations: Some("SELECT quote(id),quote(product_id),quote(name),quote(mode),quote(base_quantity),quote(price),quote(barcode),quote(barcode_search_key),quote(is_default),quote(is_active),quote(created_at),quote(updated_at) FROM product_presentations ORDER BY id"),
    sales: "SELECT quote(id),quote(user_id),quote(cashier_name),quote(cashier_email),quote(checkout_token),quote(cart_fingerprint),quote(sale_date),quote(subtotal),quote(discount_type),quote(discount_value),quote(discount_amount),quote(total),quote(payment_method),quote(cash_received),quote(change_amount),quote(created_at),quote(updated_at) FROM sales ORDER BY id",
    sale_details: "SELECT quote(id),quote(product_id),quote(sale_id),quote(record_version),quote(presentation_id),quote(product_name),quote(sku),quote(presentation_name),quote(presentation_barcode),quote(base_unit),quote(presentation_mode),quote(presentation_base_quantity),quote(quantity),quote(catalog_unit_price),quote(unit_price),quote(entered_quantity),quote(base_quantity),quote(line_total),quote(created_at),quote(updated_at) FROM sale_details ORDER BY id",
    movements: "SELECT quote(id),quote(product_id),quote(product_name),quote(sku),quote(product_search_key),quote(sku_search_key),quote(sale_id),quote(record_version),quote(presentation_id),quote(sale_detail_id),quote(presentation_name),quote(presentation_barcode),quote(base_unit),quote(presentation_mode),quote(presentation_base_quantity),quote(origin),quote(reason),quote(type),quote(quantity),quote(entered_quantity),quote(base_quantity),quote(stock_before),quote(stock_after),quote(supplier),quote(provider_search_key),quote(document_reference),quote(document_reference_search_key),quote(note),quote(note_search_key),quote(operation_token),quote(intent_fingerprint),quote(created_at),quote(updated_at) FROM inventory_movements ORDER BY id",
    sequences: "SELECT quote(name),quote(seq) FROM sqlite_sequence ORDER BY name",
};

fn migration_source_snapshot(
    connection: &Connection,
    path: &Path,
    source_version: i64,
) -> OpenResult<MigrationSnapshot> {
    let projection = match source_version {
        LEGACY_MIGRATION_SOURCE_VERSION => &V8_BEFORE,
        DIRECT_MIGRATION_SOURCE_VERSION => &V12_BEFORE,
        _ => return Err(safe_error("Version de origen no admitida", path)),
    };
    migration_snapshot(connection, path, projection, source_version)
}

fn migration_target_snapshot(
    connection: &Connection,
    path: &Path,
    source_version: i64,
) -> OpenResult<MigrationSnapshot> {
    let projection = match source_version {
        LEGACY_MIGRATION_SOURCE_VERSION => &V13_AFTER_V8,
        DIRECT_MIGRATION_SOURCE_VERSION => &V13_AFTER_V12,
        _ => return Err(safe_error("Version de origen no admitida", path)),
    };
    migration_snapshot(connection, path, projection, CURRENT_SCHEMA_VERSION)
}

fn migration_snapshot(
    connection: &Connection,
    path: &Path,
    projection: &SnapshotProjection,
    expected_version: i64,
) -> OpenResult<MigrationSnapshot> {
    if schema_version(connection, path)? != expected_version {
        return Err(safe_error(
            "Version incorrecta para la proyeccion historica",
            path,
        ));
    }
    fn quoted_rows(connection: &Connection, sql: &str) -> rusqlite::Result<SnapshotRows> {
        let mut statement = connection.prepare(sql)?;
        let column_count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..column_count).map(|column| row.get(column)).collect()
            })?
            .collect();
        rows
    }

    let result = (|| -> rusqlite::Result<MigrationSnapshot> {
        Ok(MigrationSnapshot {
            users: quoted_rows(connection, projection.users)?,
            brands: quoted_rows(connection, projection.brands)?,
            categories: quoted_rows(connection, projection.categories)?,
            products: quoted_rows(connection, projection.products)?,
            presentations: projection
                .presentations
                .map(|sql| quoted_rows(connection, sql))
                .transpose()?
                .unwrap_or_default(),
            sales: quoted_rows(connection, projection.sales)?,
            sale_details: quoted_rows(connection, projection.sale_details)?,
            movements: quoted_rows(connection, projection.movements)?,
            sequences: quoted_rows(connection, projection.sequences)?,
        })
    })();
    result.map_err(|_| safe_error("No se pudo comparar la integridad historica", path))
}

fn sequence_state(connection: &Connection, path: &Path) -> OpenResult<Vec<(String, Option<i64>)>> {
    let mut statement = connection
        .prepare(
            "WITH names(name) AS (VALUES('users'),('brands'),('categories'),('products'),('sales'),('sale_details'),('inventory_movements'))
             SELECT names.name,sqlite_sequence.seq FROM names LEFT JOIN sqlite_sequence USING(name) ORDER BY names.name",
        )
        .map_err(|_| safe_error("No se pudieron fotografiar las secuencias v8", path))?;
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .and_then(|rows| rows.collect())
        .map_err(|_| safe_error("No se pudieron fotografiar las secuencias v8", path))
}

fn restore_sequence_state(
    connection: &Connection,
    path: &Path,
    sequences: &[(String, Option<i64>)],
) -> OpenResult<()> {
    for (name, sequence) in sequences {
        connection
            .execute("DELETE FROM sqlite_sequence WHERE name=?1", [name])
            .map_err(|_| safe_error("No se pudo restaurar una secuencia v8", path))?;
        if let Some(sequence) = sequence {
            connection
                .execute(
                    "INSERT INTO sqlite_sequence(name,seq) VALUES(?1,?2)",
                    rusqlite::params![name, sequence],
                )
                .map_err(|_| safe_error("No se pudo restaurar una secuencia v8", path))?;
        }
    }
    Ok(())
}

fn source_valuation_rows(
    connection: &Connection,
    path: &Path,
    source_version: i64,
) -> OpenResult<SnapshotRows> {
    if source_version == LEGACY_MIGRATION_SOURCE_VERSION {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            "SELECT quote(id),quote(product_id),quote(reason),quote(type),quote(quantity),quote(stock_before),quote(stock_after),quote(created_at),quote(updated_at) FROM inventory_movements WHERE reason='valoracion' OR type='valoracion' ORDER BY id",
        )
        .map_err(|_| safe_error("No se pudieron fotografiar las valoraciones historicas", path))?;
    let column_count = statement.column_count();
    statement
        .query_map([], |row| {
            (0..column_count).map(|column| row.get(column)).collect()
        })
        .and_then(|rows| rows.collect())
        .map_err(|_| {
            safe_error(
                "No se pudieron fotografiar las valoraciones historicas",
                path,
            )
        })
}

fn validate_financial_retirement(
    connection: &Connection,
    path: &Path,
    _removed_valuation_count: usize,
) -> OpenResult<()> {
    let retired_columns = [
        ("products", "inventory_cost_cents"),
        ("sale_details", "movement_cost_cents"),
        ("inventory_movements", "unit_price_cents"),
        ("inventory_movements", "movement_cost_cents"),
        ("inventory_movements", "inventory_cost_before_cents"),
        ("inventory_movements", "inventory_cost_after_cents"),
    ];
    for (table, column) in retired_columns {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
                rusqlite::params![table, column],
                |row| row.get(0),
            )
            .map_err(|_| safe_error("No se pudo verificar el retiro financiero", path))?;
        if exists {
            return Err(safe_error(
                "Una columna financiera retirada permanece en v13",
                path,
            ));
        }
    }
    let invalid_rows: i64 = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM inventory_movements WHERE reason='valoracion' OR type='valoracion')
              + (SELECT count(*) FROM inventory_movements WHERE record_version IN (1,2) AND (purchase_unit_price_cents IS NOT NULL OR purchase_total_cents IS NOT NULL))",
            [],
            |row| row.get(0),
        )
        .map_err(|_| safe_error("No se pudo verificar el retiro financiero", path))?;
    if invalid_rows != 0 {
        return Err(safe_error(
            "La migracion conservo valoraciones o copio costos historicos",
            path,
        ));
    }
    Ok(())
}

fn validate_v8_generated_values(connection: &Connection, path: &Path) -> OpenResult<()> {
    let invalid_rows: i64 = connection
        .query_row(
            "SELECT
                (SELECT count(*) FROM products WHERE base_unit<>'piece')
              + (SELECT count(*) FROM products AS p WHERE NOT EXISTS (
                    SELECT 1 FROM product_presentations AS pp
                    WHERE pp.product_id=p.id AND pp.name='Pieza' AND pp.mode='fixed'
                      AND pp.base_quantity=1 AND pp.price=p.price AND pp.is_default=1
                      AND pp.is_active=1 AND pp.created_at=p.created_at AND pp.updated_at=p.updated_at
                ))
              + abs((SELECT count(*) FROM product_presentations)-(SELECT count(*) FROM products))
              + (SELECT count(*) FROM sale_details AS d
                    LEFT JOIN product_presentations AS pp ON pp.id=d.presentation_id
                    LEFT JOIN products AS p ON p.id=d.product_id
                  WHERE d.record_version<>1 OR pp.id IS NULL OR p.id IS NULL
                     OR pp.product_id<>d.product_id
                     OR d.presentation_name<>pp.name OR d.base_unit<>p.base_unit
                     OR d.presentation_mode<>pp.mode OR d.presentation_base_quantity<>pp.base_quantity
                     OR d.entered_quantity<>d.quantity OR d.base_quantity<>d.quantity
                     OR d.catalog_unit_price<>d.unit_price
                     OR d.line_total IS NOT CASE WHEN d.quantity<=9007199254740991/d.unit_price THEN d.quantity*d.unit_price ELSE NULL END)
              + (SELECT count(*) FROM inventory_movements AS m
                    LEFT JOIN product_presentations AS pp ON pp.id=m.presentation_id
                    LEFT JOIN products AS p ON p.id=m.product_id
                  WHERE m.record_version<>1 OR pp.id IS NULL OR p.id IS NULL
                     OR pp.product_id<>m.product_id
                     OR m.presentation_name<>pp.name OR m.base_unit<>p.base_unit
                     OR m.presentation_mode<>pp.mode OR m.presentation_base_quantity<>pp.base_quantity
                     OR m.entered_quantity<>m.quantity OR m.base_quantity<>m.quantity
                     OR m.stock_before IS NOT NULL OR m.stock_after IS NOT NULL
                     OR m.purchase_unit_price_cents IS NOT NULL OR m.purchase_total_cents IS NOT NULL
                     OR (m.origin='sale' AND NOT EXISTS (
                         SELECT 1 FROM sale_details AS d
                         WHERE d.id=m.sale_detail_id AND d.sale_id=m.sale_id
                           AND d.product_id=m.product_id AND d.presentation_id=m.presentation_id
                     ))
                     OR (m.origin<>'sale' AND m.sale_detail_id IS NOT NULL))
              + (SELECT count(*) FROM sales
                  WHERE subtotal<>total OR discount_type<>'none'
                     OR discount_value<>0 OR discount_amount<>0)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| safe_error("No se pudieron validar los backfills de la fuente v8", path))?;
    if invalid_rows != 0 {
        return Err(safe_error(
            "Los valores generados durante la migracion v8 no son canonicos",
            path,
        ));
    }
    Ok(())
}

fn create_validated_backup(
    source: &Connection,
    directory: &Path,
    database_path: &Path,
    source_version: i64,
) -> OpenResult<MigrationManifest> {
    create_validated_backup_with(
        source,
        directory,
        database_path,
        source_version,
        copy_sqlite_database,
    )
}

fn create_validated_backup_with<F>(
    source: &Connection,
    directory: &Path,
    database_path: &Path,
    source_version: i64,
    copy_database: F,
) -> OpenResult<MigrationManifest>
where
    F: FnOnce(&Connection, &Path) -> OpenResult<()>,
{
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| safe_error("El reloj del sistema no permite crear el backup", directory))?;
    let attempt_id = format!("{}-{}", now.as_nanos(), std::process::id());
    let backup_path = directory.join(format!("pos-v{source_version}-before-v13-{attempt_id}.db"));

    create_database_file_new(&backup_path)
        .map_err(|_| safe_error("No se pudo crear un backup nuevo", &backup_path))?;
    let backup_result = (|| {
        copy_database(source, &backup_path)?;

        let validation = Connection::open_with_flags(
            &backup_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| safe_error("No se pudo reabrir el backup", &backup_path))?;
        validate_database(&validation, source_version, true, &backup_path)?;
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

    let backup_path = fs::canonicalize(&backup_path)
        .map_err(|_| safe_error("No se pudo resolver el backup creado", &backup_path))?;
    let database_path = fs::canonicalize(database_path)
        .map_err(|_| safe_error("No se pudo resolver la base instalada", database_path))?;
    let (backup_size_bytes, backup_sha256) = backup_identity(&backup_path)?;

    Ok(MigrationManifest {
        attempt_id,
        source_version,
        target_version: CURRENT_SCHEMA_VERSION,
        created_at_unix_seconds: now.as_secs(),
        database_path,
        backup_path,
        backup_size_bytes,
        backup_sha256,
        backup_validation_completed: true,
    })
}

fn backup_identity(path: &Path) -> OpenResult<(u64, String)> {
    reject_symlink(path).map_err(|_| safe_error("El backup no puede ser un enlace", path))?;
    let file = fs::File::open(path)
        .map_err(|_| safe_error("No se pudo abrir el backup para identificarlo", path))?;
    verify_named_file_handle(&file, path)
        .map_err(|_| safe_error("El backup cambio mientras se abria", path))?;
    backup_identity_from_file(&file, path)
}

fn backup_identity_from_file(file: &fs::File, path: &Path) -> OpenResult<(u64, String)> {
    let mut file = file;
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|_| safe_error("No se pudo leer la identidad del backup", path))?;
    let metadata = file
        .metadata()
        .map_err(|_| safe_error("No se pudo identificar el backup", path))?;
    if !metadata.is_file() {
        return Err(safe_error("El backup no es un archivo regular", path));
    }
    let mut digest = Sha256::new();
    let copied = std::io::copy(&mut file, &mut digest)
        .map_err(|_| safe_error("No se pudo calcular la identidad del backup", path))?;
    if copied != metadata.len() {
        return Err(safe_error(
            "El backup cambio mientras se identificaba",
            path,
        ));
    }
    let final_metadata = file
        .metadata()
        .map_err(|_| safe_error("No se pudo confirmar la identidad del backup", path))?;
    if final_metadata.len() != metadata.len() {
        return Err(safe_error(
            "El backup cambio mientras se identificaba",
            path,
        ));
    }
    Ok((metadata.len(), format!("{:x}", digest.finalize())))
}

fn verify_backup_identity(path: &Path, manifest: &MigrationManifest) -> OpenResult<()> {
    let (size, sha256) = backup_identity(path)?;
    if size != manifest.backup_size_bytes || sha256 != manifest.backup_sha256 {
        return Err(safe_error(
            "La identidad criptografica del backup no corresponde al manifiesto",
            path,
        ));
    }
    Ok(())
}

fn verify_opened_backup_identity(
    file: &fs::File,
    path: &Path,
    manifest: &MigrationManifest,
) -> OpenResult<()> {
    verify_named_file_handle(file, path)
        .map_err(|_| safe_error("El archivo identificado fue sustituido", path))?;
    let (size, sha256) = backup_identity_from_file(file, path)?;
    if size != manifest.backup_size_bytes || sha256 != manifest.backup_sha256 {
        return Err(safe_error(
            "La identidad criptografica del backup no corresponde al manifiesto",
            path,
        ));
    }
    verify_named_file_handle(file, path)
        .map_err(|_| safe_error("El archivo identificado fue sustituido", path))
}

fn copy_verified_backup_file(
    source_path: &Path,
    destination_path: &Path,
    manifest: &MigrationManifest,
) -> OpenResult<()> {
    reject_symlink(source_path)
        .map_err(|_| safe_error("El backup no puede ser un enlace", source_path))?;
    let mut source = fs::File::open(source_path)
        .map_err(|_| safe_error("No se pudo abrir el backup", source_path))?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(destination_path)
        .map_err(|_| {
            safe_error(
                "No se pudo abrir la restauracion temporal",
                destination_path,
            )
        })?;
    std::io::copy(&mut source, &mut destination)
        .and_then(|_| destination.sync_all())
        .map_err(|_| safe_error("No se pudo copiar el backup", destination_path))?;
    verify_backup_identity(destination_path, manifest)
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
#[cfg(test)]
pub fn restore_migration_backup(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
) -> Result<Connection, Box<dyn Error>> {
    restore_migration_backup_inner(directory, database_path, backup_path).map_err(Into::into)
}

pub(crate) fn restore_migration_backup_with_guard<F>(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
    mut validate_guard: F,
) -> Result<Connection, MaintenanceRestoreError>
where
    F: FnMut() -> std::io::Result<()>,
{
    restore_migration_backup_inner_with_hook(directory, database_path, backup_path, |_| {
        validate_guard()
    })
    .map_err(|error| match error {
        DatabaseOpenError::Locked(_) => MaintenanceRestoreError::Locked,
        DatabaseOpenError::General(_) | DatabaseOpenError::Recovery { .. } => {
            MaintenanceRestoreError::Failed
        }
    })
}

#[cfg(test)]
fn restore_migration_backup_inner(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
) -> OpenResult<Connection> {
    restore_migration_backup_inner_with_hook(directory, database_path, backup_path, |_| Ok(()))
}

fn restore_migration_backup_inner_with_hook<F>(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
    mut hook: F,
) -> OpenResult<Connection>
where
    F: FnMut(RestoreStage) -> std::io::Result<()>,
{
    let migration_lock = acquire_migration_lock(directory)?;
    let directory = canonical_regular_directory(directory)?;
    reject_symlink(database_path)
        .map_err(|_| safe_error("El destino no puede ser un enlace", database_path))?;
    reject_symlink(backup_path)
        .map_err(|_| safe_error("El backup no puede ser un enlace", backup_path))?;
    let database_path = fs::canonicalize(database_path)
        .map_err(|_| safe_error("No se pudo resolver la base instalada", database_path))?;
    let backup_path = fs::canonicalize(backup_path)
        .map_err(|_| safe_error("No se pudo resolver el backup", backup_path))?;
    if database_path.parent() != Some(directory.as_path())
        || database_path.file_name().and_then(|name| name.to_str()) != Some("pos.db")
    {
        return Err(safe_error(
            "El destino debe ser el pos.db canonico de esta instalacion",
            &database_path,
        ));
    }
    reconcile_migration_completion(&directory, &directory.join(PENDING_MARKER_NAME))?;
    let authority = migration_manifest_for_restore(&directory, &database_path, &backup_path)?;
    verify_backup_identity(&backup_path, &authority.manifest)?;
    let installed_identity = backup_identity(&database_path)?;
    let directory = directory.as_path();
    let database_path = database_path.as_path();
    let backup_path = backup_path.as_path();
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
    let source_version = schema_version(&source, backup_path)?;
    if !matches!(
        source_version,
        LEGACY_MIGRATION_SOURCE_VERSION | DIRECT_MIGRATION_SOURCE_VERSION
    ) || authority.manifest.source_version != source_version
    {
        return Err(safe_error(
            "El backup no corresponde a una fuente de migracion admitida",
            backup_path,
        ));
    }
    validate_database(&source, source_version, true, backup_path)?;

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
        copy_verified_backup_file(backup_path, &temporary_path, &authority.manifest)?;
        let validation = Connection::open_with_flags(
            &temporary_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| safe_error("No se pudo abrir la restauracion temporal", &temporary_path))?;
        validate_database(&validation, source_version, true, &temporary_path)?;
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
    let prepared_file = match (|| {
        let file = fs::File::open(&temporary_path).map_err(|_| {
            safe_error("No se pudo fijar la restauracion temporal", &temporary_path)
        })?;
        verify_opened_backup_identity(&file, &temporary_path, &authority.manifest)?;
        Ok(file)
    })() {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }
    };
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
        source_version,
        target_version: CURRENT_SCHEMA_VERSION,
        backup_size_bytes: authority.manifest.backup_size_bytes,
        backup_sha256: authority.manifest.backup_sha256.clone(),
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

    // This is the last read of the source before any installed SQLite file moves.
    // It closes the useful path-replacement window left by the earlier validation.
    if verify_backup_identity(backup_path, &authority.manifest).is_err() {
        remove_restore_marker(&restore_marker, directory)
            .map_err(|rollback| incomplete_recovery_error(backup_path, &rollback))?;
        let _ = fs::remove_file(&temporary_path);
        return Err(recovery_error(
            "El backup cambio antes de la mutacion; no se restauro",
            backup_path,
        ));
    }
    let namespace_is_unchanged = (|| {
        revalidate_named_file(
            &temporary_path,
            "La restauracion temporal cambio antes de la mutacion",
        )?;
        verify_backup_identity(&temporary_path, &authority.manifest)?;
        revalidate_named_file(
            database_path,
            "La base instalada cambio antes de la mutacion",
        )?;
        if backup_identity(database_path)? != installed_identity {
            return Err(safe_error(
                "La identidad de la base instalada cambio antes de la mutacion",
                database_path,
            ));
        }
        for sidecar in sqlite_sidecars(database_path) {
            if sidecar.try_exists().map_err(|_| {
                safe_error("No se pudo comprobar un archivo SQLite anterior", &sidecar)
            })? {
                revalidate_named_file(&sidecar, "Un sidecar SQLite no es un archivo regular")?;
            }
        }
        Ok(())
    })();
    if namespace_is_unchanged.is_err() {
        remove_restore_marker(&restore_marker, directory)
            .map_err(|rollback| incomplete_recovery_error(backup_path, &rollback))?;
        let _ = fs::remove_file(&temporary_path);
        return Err(recovery_error(
            "El destino cambio antes de la mutacion; no se restauro",
            backup_path,
        ));
    }
    migration_lock
        .validate()
        .map_err(|_| locked_error("El bloqueo de migracion fue sustituido"))?;

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

    if verify_opened_backup_identity(&prepared_file, &temporary_path, &authority.manifest).is_err()
        || hook(RestoreStage::BeforeBackupPromotion).is_err()
    {
        return Err(rollback_or_recovery_error(
            "La restauracion temporal cambio antes de promoverse",
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
    if migration_lock.validate().is_err() {
        let _ = rollback_restored_files(
            database_path,
            quarantined_database_ref,
            &quarantined_sidecars,
            &failed_database,
            database_directory,
            &restore_marker,
            false,
        );
        return Err(locked_error("El bloqueo de migracion fue sustituido"));
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
    let promoted_file = match fs::File::open(database_path) {
        Ok(file)
            if same_file_handle(&prepared_file, &file).unwrap_or(false)
                && verify_opened_backup_identity(&file, database_path, &authority.manifest)
                    .is_ok() =>
        {
            file
        }
        _ => {
            return Err(rollback_or_recovery_error(
                "La base promovida no corresponde exactamente al manifiesto",
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
    drop(prepared_file);
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
    if validate_database(&restored, source_version, true, database_path).is_err() {
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
    if verify_opened_backup_identity(&promoted_file, database_path, &authority.manifest).is_err() {
        drop(restored);
        return Err(rollback_or_recovery_error(
            "La base promovida cambio antes de completar la restauracion",
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

    if let Some(pending_path) = authority.pending_path {
        let manifest = authority.manifest;
        let archive_prefix = "pos-schema-v13-migration-restored";
        let archived_path =
            directory.join(format!("{archive_prefix}-{}.json", manifest.attempt_id));
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

    let completed_restore = directory.join(format!(
        "pos-schema-v13-restore-completed-{restore_id}.json"
    ));
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

struct RestoreAuthority {
    manifest: MigrationManifest,
    pending_path: Option<PathBuf>,
}

fn migration_manifest_for_restore(
    directory: &Path,
    database_path: &Path,
    backup_path: &Path,
) -> OpenResult<RestoreAuthority> {
    let pending_path = directory.join(PENDING_MARKER_NAME);
    let mut matches = Vec::new();
    if pending_path
        .try_exists()
        .map_err(|_| safe_error("No se pudo comprobar el marcador pendiente", &pending_path))?
    {
        reject_symlink(&pending_path).map_err(|_| {
            safe_error(
                "El marcador pendiente no puede ser un enlace",
                &pending_path,
            )
        })?;
        let manifest: MigrationManifest = read_marker(&pending_path, "migracion")?;
        matches.push((manifest, Some(pending_path)));
    }

    let entries = fs::read_dir(directory)
        .map_err(|_| safe_error("No se pudieron buscar manifiestos completados", directory))?;
    for entry in entries {
        let path = entry
            .map_err(|_| safe_error("No se pudo leer un manifiesto completado", directory))?
            .path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !name.starts_with("pos-schema-v13-migration-completed-") || !name.ends_with(".json") {
            continue;
        }
        reject_symlink(&path)
            .map_err(|_| safe_error("El manifiesto completado no puede ser un enlace", &path))?;
        let completed: CompletedMigrationManifest = read_marker(&path, "migracion completada")?;
        if !completed.migration.backup_validation_completed
            || !completed.canonical_target_validation_completed
            || !completed.source_specific_comparison_completed
            || !completed.financial_and_valuation_removal_verified
        {
            return Err(safe_error(
                "El manifiesto completado no contiene todas las atestaciones",
                &path,
            ));
        }
        matches.push((completed.migration, None));
    }

    matches.retain(|(manifest, _)| {
        manifest.backup_path == backup_path
            && manifest.database_path == database_path
            && backup_path.parent() == Some(directory)
            && manifest.backup_validation_completed
            && matches!(
                manifest.source_version,
                LEGACY_MIGRATION_SOURCE_VERSION | DIRECT_MIGRATION_SOURCE_VERSION
            )
            && manifest.target_version == CURRENT_SCHEMA_VERSION
            && manifest.backup_sha256.len() == 64
            && manifest
                .backup_sha256
                .bytes()
                .all(|value| value.is_ascii_hexdigit() && !value.is_ascii_uppercase())
    });
    if matches.len() != 1 {
        return Err(safe_error(
            "Se exige exactamente un manifiesto v13 pendiente o completado para este backup",
            directory,
        ));
    }
    let (manifest, pending_path) = matches.pop().unwrap();
    Ok(RestoreAuthority {
        manifest,
        pending_path,
    })
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

struct MigrationLock {
    file: fs::File,
    path: PathBuf,
}

impl MigrationLock {
    fn validate(&self) -> std::io::Result<()> {
        verify_named_file_handle(&self.file, &self.path)
    }
}

fn acquire_migration_lock(directory: &Path) -> OpenResult<MigrationLock> {
    let path = directory.join(MIGRATION_LOCK_NAME);
    let file = open_lock_file(&path)
        .map_err(|_| locked_error("No se pudo abrir el bloqueo de migracion"))?;
    secure_lock_file(&file)
        .map_err(|_| locked_error("No se pudo proteger el bloqueo de migracion"))?;
    FileExt::try_lock_exclusive(&file)
        .map_err(|_| locked_error("Otra instancia esta comprobando o migrando la base de datos"))?;
    verify_named_file_handle(&file, &path)
        .map_err(|_| locked_error("El archivo de bloqueo fue sustituido"))?;
    Ok(MigrationLock { file, path })
}

fn locked_error(message: &str) -> DatabaseOpenError {
    DatabaseOpenError::Locked(message.into())
}

#[cfg(unix)]
fn secure_lock_file(file: &fs::File) -> std::io::Result<()> {
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn secure_lock_file(_file: &fs::File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_lock_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(0x20000) // O_NOFOLLOW
        .open(path)
}

#[cfg(all(unix, not(target_os = "linux")))]
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
    #[cfg(windows)]
    use std::os::windows::fs::OpenOptionsExt;

    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(windows)]
    options.share_mode(
        windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
            | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE,
    );
    options.open(path)
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
    removed_valuation_row_count: usize,
) -> OpenResult<()> {
    let completed_path = directory.join(format!(
        "pos-schema-v13-migration-completed-{}.json",
        manifest.attempt_id
    ));
    let completed = CompletedMigrationManifest {
        migration: MigrationManifest {
            attempt_id: manifest.attempt_id.clone(),
            source_version: manifest.source_version,
            target_version: manifest.target_version,
            created_at_unix_seconds: manifest.created_at_unix_seconds,
            database_path: manifest.database_path.clone(),
            backup_path: manifest.backup_path.clone(),
            backup_size_bytes: manifest.backup_size_bytes,
            backup_sha256: manifest.backup_sha256.clone(),
            backup_validation_completed: manifest.backup_validation_completed,
        },
        canonical_target_validation_completed: true,
        source_specific_comparison_completed: true,
        financial_and_valuation_removal_verified: true,
        removed_valuation_row_count,
    };
    finalize_marker_with_hook(
        directory,
        pending_path,
        &completed_path,
        &completed,
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
    let prefix = if active_path.file_name().and_then(|name| name.to_str())
        == Some(LEGACY_PENDING_MARKER_NAME)
    {
        "pos-v0.2.0-migration-completed"
    } else {
        "pos-schema-v13-migration-completed"
    };
    let completed_path = directory.join(format!("{prefix}-{}.json", manifest.attempt_id));
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
    let prefix = if active_path.file_name().and_then(|name| name.to_str())
        == Some(LEGACY_RESTORE_MARKER_NAME)
    {
        "pos-v0.2.0-restore-completed"
    } else {
        "pos-schema-v13-restore-completed"
    };
    let completed_path = directory.join(format!("{prefix}-{}.json", manifest.restore_id));
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

fn canonical_regular_directory(directory: &Path) -> OpenResult<PathBuf> {
    reject_symlink(directory)
        .map_err(|_| safe_error("El directorio de datos no puede ser un enlace", directory))?;
    let canonical = fs::canonicalize(directory)
        .map_err(|_| safe_error("No se pudo resolver el directorio de datos", directory))?;
    if !fs::metadata(&canonical)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        return Err(safe_error("El directorio de datos no es valido", directory));
    }
    Ok(canonical)
}

fn revalidate_named_file(path: &Path, message: &str) -> OpenResult<()> {
    reject_symlink(path).map_err(|_| safe_error(message, path))?;
    let canonical = fs::canonicalize(path).map_err(|_| safe_error(message, path))?;
    if canonical != path || !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_file()) {
        return Err(safe_error(message, path));
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(std::io::Error::other("symbolic link rejected"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(std::io::Error::other("reparse point rejected"));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn verify_named_file_handle(file: &fs::File, path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let opened = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if named.file_type().is_symlink() || opened.dev() != named.dev() || opened.ino() != named.ino()
    {
        return Err(std::io::Error::other("lock path identity changed"));
    }
    Ok(())
}

#[cfg(windows)]
fn verify_named_file_handle(file: &fs::File, path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::MetadataExt;

    reject_symlink(path)?;
    let opened = file.metadata()?;
    let named = fs::metadata(path)?;
    if !opened.is_file()
        || !named.is_file()
        || opened.volume_serial_number() != named.volume_serial_number()
        || opened.file_index() != named.file_index()
    {
        return Err(std::io::Error::other("invalid lock file"));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn verify_named_file_handle(file: &fs::File, path: &Path) -> std::io::Result<()> {
    reject_symlink(path)?;
    if !file.metadata()?.is_file() || !fs::metadata(path)?.is_file() {
        return Err(std::io::Error::other("invalid lock file"));
    }
    Ok(())
}

#[cfg(unix)]
fn same_file_handle(left: &fs::File, right: &fs::File) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.dev() == right.dev() && left.ino() == right.ino())
}

#[cfg(windows)]
fn same_file_handle(left: &fs::File, right: &fs::File) -> std::io::Result<bool> {
    use std::os::windows::fs::MetadataExt;
    let left = left.metadata()?;
    let right = right.metadata()?;
    Ok(left.volume_serial_number() == right.volume_serial_number()
        && left.file_index() == right.file_index())
}

#[cfg(not(any(unix, windows)))]
fn same_file_handle(left: &fs::File, right: &fs::File) -> std::io::Result<bool> {
    Ok(left.metadata()?.len() == right.metadata()?.len())
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

    fn test_manifest(
        attempt_id: impl Into<String>,
        source_version: i64,
        target_version: i64,
        backup_path: PathBuf,
    ) -> MigrationManifest {
        let (backup_size_bytes, backup_sha256) =
            backup_identity(&backup_path).unwrap_or_else(|_| (0, "0".repeat(64)));
        MigrationManifest {
            attempt_id: attempt_id.into(),
            source_version,
            target_version,
            created_at_unix_seconds: 1,
            database_path: backup_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("pos.db"),
            backup_path,
            backup_size_bytes,
            backup_sha256,
            backup_validation_completed: true,
        }
    }

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

    fn create_v12_database(path: &Path) {
        let mut connection = Connection::open(path).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run_to_version(&mut connection, 12).unwrap();
        connection.execute_batch(
            "INSERT INTO users(id,first_name,last_name,email,password_hash,created_at,updated_at)
             VALUES(1,'Ada','Lovelace','ada@pos.local','hash','2025-01-01 08:00:00','2025-01-01 08:00:01');
             INSERT INTO brands(id,name,name_search_key,created_at,updated_at)
             VALUES(1,'Marca','marca','2025-01-01 08:01:00','2025-01-01 08:01:01');
             INSERT INTO categories(id,name,name_search_key,created_at,updated_at)
             VALUES(1,'Categoria','categoria','2025-01-01 08:02:00','2025-01-01 08:02:01');
             INSERT INTO products(id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
             VALUES(1,'Producto','producto',100,10,'SKU-1','sku-1',1,1,'piece',1000);
             INSERT INTO product_presentations(id,product_id,name,mode,base_quantity,price,is_default)
             VALUES(1,1,'Pieza','fixed',1,100,1);
             INSERT INTO sales(id,user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method,created_at,updated_at)
             VALUES(1,1,'Ada Lovelace','ada@pos.local','checkout','cart-v5:test','2025-01-03 10:00:00',360,'fixed',60,60,300,'tarjeta','2025-01-03 10:00:01','2025-01-03 10:00:02');
             INSERT INTO sale_details(id,product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total,movement_cost_cents)
             VALUES(1,1,1,3,1,'Producto historico','SKU-HIST','Pieza','piece','fixed',1,3,150,120,3,3,360,320);
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,supplier,provider_search_key,document_reference,document_reference_search_key,note,note_search_key,operation_token,intent_fingerprint,created_at,updated_at)
             VALUES(1,1,'Producto compra','SKU-COMPRA','producto compra','sku-compra',2,1,'Pieza','piece','fixed',1,100,'manual','compra','entrada',10,10,10,0,10,1000,0,1000,'Proveedor Uno','proveedor uno','FAC-001','fac-001','Compra corregible','compra corregible','purchase-token','purchase-fingerprint','2025-01-01 09:00:00','2025-01-01 09:00:01');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,operation_token,intent_fingerprint,created_at,updated_at)
             VALUES(2,1,'Producto ajuste','SKU-AJUSTE','producto ajuste','sku-ajuste',2,1,'Pieza','piece','fixed',1,120,'manual','ajuste','entrada',5,5,5,10,15,600,1000,1600,'adjust-in-token','adjust-in-fingerprint','2025-01-02 09:00:00','2025-01-02 09:00:01');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,sale_id,record_version,presentation_id,sale_detail_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,created_at,updated_at)
             VALUES(3,1,'Producto historico','SKU-HIST','producto historico','sku-hist',1,2,1,1,'Pieza','piece','fixed',1,120,'sale','venta','salida',3,3,3,15,12,320,1600,1280,'2025-01-03 10:00:03','2025-01-03 10:00:04');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,operation_token,intent_fingerprint,created_at,updated_at)
             VALUES(4,1,'Producto salida','SKU-SALIDA','producto salida','sku-salida',2,1,'Pieza','piece','fixed',1,107,'manual','ajuste','salida',2,2,2,12,10,214,1280,1066,'adjust-out-token','adjust-out-fingerprint','2025-01-04 09:00:00','2025-01-04 09:00:01');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,operation_token,intent_fingerprint,note,note_search_key,created_at,updated_at)
             VALUES(5,1,'Producto valorado','SKU-VAL','producto valorado','sku-val',2,'manual','valoracion','valoracion',0,0,0,10,10,1000,1066,1000,'valuation-token','valuation-fingerprint','Frontera de valoracion','frontera de valoracion','2025-01-05 09:00:00','2025-01-05 09:00:01');
             INSERT INTO products(id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
             VALUES(2,'Sin costo','sin costo',250,5,'SKU-2','sku-2',1,1,'piece',NULL);
             INSERT INTO product_presentations(id,product_id,name,mode,base_quantity,price,is_default)
             VALUES(2,2,'Pieza','fixed',1,250,1);
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,created_at,updated_at)
             VALUES(6,2,'Sin costo inicial','SKU-2-HIST','sin costo inicial','sku-2-hist',2,2,'Pieza','piece','fixed',1,'initial_stock','ajuste','entrada',5,5,5,0,5,'2025-01-01 11:00:00','2025-01-01 11:00:01');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,operation_token,intent_fingerprint,created_at,updated_at)
             VALUES(7,2,'Sin costo salida','SKU-2-HIST','sin costo salida','sku-2-hist',2,2,'Pieza','piece','fixed',1,'manual','ajuste','salida',1,1,1,5,4,'unknown-out-token','unknown-out-fingerprint','2025-01-02 11:00:00','2025-01-02 11:00:01');
             INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,operation_token,intent_fingerprint,created_at,updated_at)
             VALUES(8,2,'Sin costo compra','SKU-2-HIST','sin costo compra','sku-2-hist',2,2,'Pieza','piece','fixed',1,175,'manual','compra','entrada',1,1,1,4,5,175,'unknown-buy-token','unknown-buy-fingerprint','2025-01-03 11:00:00','2025-01-03 11:00:01');
              INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,operation_token,intent_fingerprint)
              VALUES(30,1,'Secuencia eliminada','SEQ','secuencia eliminada','seq',2,1,'Pieza','piece','fixed',1,1,'manual','ajuste','entrada',1,1,1,10,11,1,1000,1001,'deleted-sequence-token','deleted-sequence-fingerprint');
              DELETE FROM inventory_movements WHERE id=30;
              INSERT INTO products(id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
              VALUES(3,'Cafe por peso','cafe por peso',300,1000,'CAFE-KG','cafe-kg',1,1,'gram',150000000);
              INSERT INTO product_presentations(id,product_id,name,mode,base_quantity,price,is_default)
              VALUES(3,3,'Kilogramo','measured',1000,300,1);
              INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,supplier,provider_search_key,document_reference,document_reference_search_key,operation_token,intent_fingerprint)
              VALUES(31,3,'Cafe por peso','CAFE-KG','cafe por peso','cafe-kg',2,3,'Kilogramo','gram','measured',1000,150000,'manual','compra','entrada',1000,1000,1000,0,1000,150000000,0,150000000,'Proveedor kilo','proveedor kilo','KG-ERR','kg-err','kg-as-gram-token','kg-as-gram-fingerprint');
              INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,record_version,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,operation_token,intent_fingerprint)
              VALUES(40,3,'Valoracion mas alta','CAFE-KG','valoracion mas alta','cafe-kg',2,'manual','valoracion','valoracion',0,0,0,1000,1000,150000000,150000000,150000000,'highest-valuation-token','highest-valuation-fingerprint');",
        ).unwrap();
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
        migrations::run_to_version(&mut v12, 12).unwrap();
        validate_database(&v12, 12, true, &v12_path).unwrap();
        v12.pragma_update(None, "foreign_keys", false).unwrap();
        v12.execute("DROP TABLE product_presentations", []).unwrap();
        assert!(validate_database(&v12, 12, true, &v12_path).is_err());
    }

    #[test]
    fn canonical_v12_schema_passes_complete_comparison() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run_to_v12(&mut connection).unwrap();

        validate_database(&connection, 12, true, Path::new("canonical-v12.db")).unwrap();
    }

    #[test]
    fn canonical_v8_schema_passes_complete_comparison() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run_to_v8(&mut connection).unwrap();

        validate_database(&connection, 8, true, Path::new("canonical-v8.db")).unwrap();
    }

    #[test]
    fn v8_with_changed_columns_indexes_or_constraints_is_rejected() {
        for alteration in [
            "ALTER TABLE products ADD COLUMN unexpected TEXT",
            "DROP INDEX idx_products_sku_search_key",
            "CREATE INDEX unexpected_v8_index ON products(price)",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master SET sql=replace(sql,
               'FOREIGN KEY(category_id) REFERENCES categories(id)',
               'FOREIGN KEY(category_id) REFERENCES categories(id) ON DELETE CASCADE')
             WHERE type='table' AND name='products';
             PRAGMA schema_version=108;
             PRAGMA writable_schema=OFF",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master SET sql=replace(sql,
               'quantity INTEGER NOT NULL CHECK (typeof(quantity) = ''integer'' AND quantity BETWEEN 1 AND 9007199254740991)',
               'quantity INTEGER NOT NULL')
             WHERE type='table' AND name='sale_details';
             PRAGMA schema_version=109;
             PRAGMA writable_schema=OFF",
        ] {
            let mut connection = Connection::open_in_memory().unwrap();
            migrations::run_to_v8(&mut connection).unwrap();
            connection.execute_batch(alteration).unwrap();

            let error = validate_database(&connection, 8, true, Path::new("altered-v8.db"))
                .unwrap_err();
            assert!(error.to_string().contains("no es canonico"), "{alteration}");
        }
    }

    #[test]
    fn canonical_v13_schema_passes_complete_comparison() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run_to_v13(&mut connection).unwrap();

        validate_database(&connection, 13, true, Path::new("canonical-v13.db")).unwrap();
    }

    #[test]
    fn discarded_v13_shape_is_rejected_without_migration_or_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        let mut connection = Connection::open(&path).unwrap();
        migrations::run_to_v12(&mut connection).unwrap();
        connection.pragma_update(None, "user_version", 13).unwrap();
        drop(connection);

        let error = open_database_path(temp.path(), &path).unwrap_err();

        assert!(error.to_string().contains("no es canonico"));
        assert_eq!(version(&path), 13);
        assert!(matching_files(temp.path(), "pos-v12-before-v13-").is_empty());
        assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
    }

    #[test]
    fn v13_with_extra_or_missing_index_or_constraint_is_rejected() {
        for alteration in [
            "CREATE INDEX unexpected_v13_index ON products(price)",
            "DROP INDEX idx_inventory_movements_sale_detail_id",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master SET sql=replace(sql,
               'price INTEGER NOT NULL CHECK (typeof(price) = ''integer'' AND price BETWEEN 1 AND 9007199254740991),',
               'price INTEGER NOT NULL,')
             WHERE type='table' AND name='products';
             PRAGMA schema_version=101;
             PRAGMA writable_schema=OFF",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master SET sql=replace(sql,
               'stock INTEGER NOT NULL CHECK (typeof(stock) = ''integer'' AND stock BETWEEN 0 AND 9007199254740991),',
               'stock INTEGER NOT NULL CHECK (typeof(stock) = ''integer'' AND stock BETWEEN 0 AND 9007199254740991) CHECK (stock>=0),')
             WHERE type='table' AND name='products';
             PRAGMA schema_version=102;
             PRAGMA writable_schema=OFF",
        ] {
            let mut connection = Connection::open_in_memory().unwrap();
            migrations::run_to_v13(&mut connection).unwrap();
            connection.execute_batch(alteration).unwrap();

            let error = validate_database(&connection, 13, true, Path::new("altered-v13.db"))
                .unwrap_err();

            assert!(error.to_string().contains("no es canonico"), "{alteration}");
        }
    }

    #[test]
    fn v12_with_extra_trigger_or_view_is_rejected_before_backup_marker_or_lock() {
        for extra_object in [
            "CREATE TRIGGER unexpected_product_trigger AFTER INSERT ON products BEGIN SELECT NEW.id; END",
            "CREATE VIEW unexpected_product_view AS SELECT id,name FROM products",
        ] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            create_v12_database(&path);
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(extra_object).unwrap();
            drop(connection);

            let error = open_database_path(temp.path(), &path).unwrap_err();

            assert!(error.to_string().contains("no es canonico"), "{extra_object}");
            assert_eq!(version(&path), 12, "{extra_object}");
            assert!(
                matching_files(temp.path(), "pos-v12-before-v13-").is_empty(),
                "{extra_object}"
            );
            assert!(!temp.path().join(PENDING_MARKER_NAME).exists(), "{extra_object}");
            assert!(!temp.path().join(MIGRATION_LOCK_NAME).exists(), "{extra_object}");
        }
    }

    #[test]
    fn new_database_goes_directly_to_v13_without_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");

        let connection = open_database_path(temp.path(), &path).unwrap();

        assert_eq!(schema_version(&connection, &path).unwrap(), 13);
        assert_eq!(
            connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap()
                .to_lowercase(),
            "wal"
        );
        assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
    }

    #[test]
    fn migrates_v12_to_v13_with_financial_retirement_and_literal_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v12_database(&path);

        let connection = open_database_path(temp.path(), &path).unwrap();

        assert_eq!(schema_version(&connection, &path).unwrap(), 13);
        validate_database(&connection, 13, true, &path).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT subtotal,total FROM sales WHERE id=1", [], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })
                .unwrap(),
            (360, 300)
        );
        assert_eq!(
            connection
                .prepare("SELECT id,stock FROM products ORDER BY id")
                .unwrap()
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            vec![(1, 10), (2, 5), (3, 1000)]
        );
        for (table, column) in [
            ("products", "inventory_cost_cents"),
            ("sale_details", "movement_cost_cents"),
            ("inventory_movements", "unit_price_cents"),
            ("inventory_movements", "movement_cost_cents"),
            ("inventory_movements", "inventory_cost_before_cents"),
            ("inventory_movements", "inventory_cost_after_cents"),
        ] {
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name=?2",
                        params![table, column],
                        |row| row.get::<_, i64>(0)
                    )
                    .unwrap(),
                0
            );
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM inventory_movements WHERE reason='valoracion' OR type='valoracion'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT purchase_unit_price_cents,purchase_total_cents FROM inventory_movements WHERE id=31",
                    [],
                    |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .unwrap(),
            (None, None)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name='inventory_movements'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            40
        );
        let backups = matching_files(temp.path(), "pos-v12-before-v13-");
        assert_eq!(backups.len(), 1);
        let backup =
            Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        validate_database(&backup, 12, true, &backups[0]).unwrap();
        assert_eq!(
            backup
                .query_row(
                    "SELECT unit_price_cents,movement_cost_cents FROM inventory_movements WHERE id=31",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .unwrap(),
            (150000, 150000000)
        );
        assert_eq!(
            backup
                .query_row(
                    "SELECT count(*) FROM inventory_movements WHERE id=40 AND reason='valoracion'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        let manifest_path = &matching_files(temp.path(), "pos-schema-v13-migration-completed-")[0];
        let manifest: CompletedMigrationManifest =
            serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
        assert_eq!(
            (
                manifest.migration.source_version,
                manifest.migration.target_version
            ),
            (12, 13)
        );
        assert!(manifest.migration.backup_validation_completed);
        assert!(manifest.canonical_target_validation_completed);
        assert!(manifest.source_specific_comparison_completed);
        assert!(manifest.financial_and_valuation_removal_verified);
        assert_eq!(manifest.removed_valuation_row_count, 2);
    }

    #[test]
    fn source_and_target_snapshots_enforce_phase_and_detect_literal_changes() {
        for source_version in [8, 12] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            if source_version == 8 {
                create_v8_database(&path);
            } else {
                create_v12_database(&path);
            }
            let source = Connection::open(&path).unwrap();
            // SQL overlap must not permit accidentally using the target contract
            // on an unmigrated source (or the source contract after migration).
            assert!(migration_target_snapshot(&source, &path, source_version).is_err());
            let before = migration_source_snapshot(&source, &path, source_version).unwrap();
            drop(source);

            let target = open_database_path(temp.path(), &path).unwrap();
            assert!(migration_source_snapshot(&target, &path, source_version).is_err());
            assert!(migration_source_snapshot(&target, &path, 9).is_err());
            assert!(migration_target_snapshot(&target, &path, 9).is_err());
            assert_eq!(
                migration_target_snapshot(&target, &path, source_version).unwrap(),
                before
            );
            // Target projections must work with all retired financial columns
            // absent, while their removal remains a separate validation.
            validate_financial_retirement(&target, &path, 0).unwrap();

            if source_version == 8 {
                target
                    .execute(
                        "UPDATE product_presentations SET price=price+1 WHERE product_id=1",
                        [],
                    )
                    .unwrap();
                assert_eq!(
                    migration_target_snapshot(&target, &path, 8).unwrap(),
                    before
                );
                assert!(validate_v8_generated_values(&target, &path).is_err());
                target.execute("UPDATE product_presentations SET price=(SELECT price FROM products WHERE id=1) WHERE product_id=1", []).unwrap();
                validate_v8_generated_values(&target, &path).unwrap();
            }

            // Exercise literal quoting, including whitespace and apostrophes,
            // rather than accepting a normalized or recomputed equivalent.
            target
                .execute("UPDATE users SET first_name=?1 WHERE id=1", [" Ada's "])
                .unwrap();
            let after = migration_target_snapshot(&target, &path, source_version).unwrap();
            assert_eq!(after.users[0][1], "' Ada''s '");
            assert_ne!(after, before);
        }
    }

    #[test]
    fn v12_snapshot_covers_every_retained_field_and_sequence() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v12_database(&path);
        let mut connection = Connection::open(&path).unwrap();
        let before = migration_source_snapshot(&connection, &path, 12).unwrap();

        assert!(before.users.iter().all(|row| row.len() == 7));
        assert!(before.brands.iter().all(|row| row.len() == 5));
        assert!(before.categories.iter().all(|row| row.len() == 5));
        assert!(before.products.iter().all(|row| row.len() == 12));
        assert!(before.presentations.iter().all(|row| row.len() == 12));
        assert!(before.sales.iter().all(|row| row.len() == 17));
        assert!(before.sale_details.iter().all(|row| row.len() == 20));
        assert!(before.movements.iter().all(|row| row.len() == 33));
        assert_eq!(before.sequences.len(), 8);
        assert!(before
            .movements
            .iter()
            .flatten()
            .any(|value| value == "NULL"));

        migrations::run_to_version(&mut connection, 13).unwrap();
        assert_eq!(
            migration_target_snapshot(&connection, &path, 12).unwrap(),
            before
        );
    }

    #[test]
    fn historical_v12_presentation_combinations_do_not_block_migration() {
        for (base_unit, mode) in [("gram", "fixed"), ("piece", "measured")] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            create_v12_database(&path);
            let connection = Connection::open(&path).unwrap();
            connection
                .execute("UPDATE products SET base_unit=?1 WHERE id=1", [base_unit])
                .unwrap();
            connection
                .execute(
                    "UPDATE product_presentations SET mode=?1 WHERE id=1",
                    [mode],
                )
                .unwrap();
            drop(connection);
            let migrated = open_database_path(temp.path(), &path).unwrap();
            assert_eq!(version(&path), 13, "{base_unit}+{mode}");
            assert_eq!(
                migrated
                    .query_row(
                        "SELECT mode FROM product_presentations WHERE id=1",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                mode
            );
        }
    }

    #[test]
    fn adulterated_v12_schema_is_rejected_without_backup_or_marker() {
        for alteration in [
            "DROP INDEX idx_inventory_movements_sale_detail_id",
            "DROP INDEX idx_products_sku_search_key;
             CREATE INDEX idx_products_sku_search_key ON products(sku_search_key)",
            "DROP INDEX idx_sale_details_line_identity;
             CREATE INDEX idx_sale_details_line_identity ON sale_details(id)",
            "ALTER TABLE products RENAME COLUMN inventory_cost_cents TO retired_cost",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master
                SET sql=replace(sql, 'inventory_cost_cents INTEGER', 'inventory_cost_cents TEXT')
              WHERE type='table' AND name='products';
             PRAGMA schema_version=98;
             PRAGMA writable_schema=OFF",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master
                SET sql=replace(sql, 'REFERENCES users(id) ON DELETE RESTRICT', 'REFERENCES users(id) ON DELETE CASCADE')
              WHERE type='table' AND name='sales';
             PRAGMA schema_version=97;
             PRAGMA writable_schema=OFF",
            "PRAGMA writable_schema=ON;
             UPDATE sqlite_master
                SET sql=replace(sql, 'CHECK (base_unit IN (''piece'', ''gram'', ''milliliter''))', '')
              WHERE type='table' AND name='products';
             PRAGMA schema_version=99;
             PRAGMA writable_schema=OFF",
        ] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            create_v12_database(&path);
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch(alteration).unwrap();
            drop(connection);

            assert!(
                open_database_path(temp.path(), &path).is_err(),
                "{alteration}"
            );
            assert_eq!(version(&path), 12);
            assert!(matching_files(temp.path(), "pos-v12-before-v13-").is_empty());
            assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
            assert!(!temp.path().join(MIGRATION_LOCK_NAME).exists());
        }
    }

    #[test]
    fn adulterated_v12_sales_discount_coherence_is_rejected_before_backup() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v12_database(&path);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA writable_schema=ON;
                 UPDATE sqlite_master
                    SET sql=replace(sql,
                        'discount_type = ''fixed'' AND discount_value > 0 AND discount_amount = discount_value',
                        'discount_type = ''fixed'' AND discount_value > 0 AND discount_amount >= 0')
                  WHERE type='table' AND name='sales';
                 PRAGMA schema_version=101;
                 PRAGMA writable_schema=OFF",
            )
            .unwrap();
        drop(connection);

        assert!(open_database_path(temp.path(), &path).is_err());
        assert_eq!(version(&path), 12);
        assert!(matching_files(temp.path(), "pos-v12-before-v13-").is_empty());
        assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
        assert!(!temp.path().join(MIGRATION_LOCK_NAME).exists());
    }

    #[test]
    fn legacy_pending_marker_is_still_detected() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        let backup = temp.path().join("legacy-backup.db");
        let marker = test_manifest("legacy", 8, 12, backup.clone());
        write_pending_marker(&temp.path().join(LEGACY_PENDING_MARKER_NAME), &marker).unwrap();

        let error = open_database_path(temp.path(), &path).unwrap_err();

        assert!(error
            .to_string()
            .contains(backup.to_string_lossy().as_ref()));
        assert_eq!(version(&path), 8);
        assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
    }

    #[test]
    fn migrates_v8_with_one_valid_backup_and_completed_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);

        let connection = open_database_path(temp.path(), &path).unwrap();
        assert_eq!(schema_version(&connection, &path).unwrap(), 13);
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        drop(connection);

        let backups = matching_files(temp.path(), "pos-v8-before-v13-");
        let manifests = matching_files(temp.path(), "pos-schema-v13-migration-completed-");
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
    fn v13_reopens_without_replacing_backup_or_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        drop(open_database_path(temp.path(), &path).unwrap());
        let backup = matching_files(temp.path(), "pos-v8-before-v13-")[0].clone();
        let manifest =
            matching_files(temp.path(), "pos-schema-v13-migration-completed-")[0].clone();
        let backup_bytes = fs::read(&backup).unwrap();
        let manifest_bytes = fs::read(&manifest).unwrap();

        drop(open_database_path(temp.path(), &path).unwrap());

        assert_eq!(matching_files(temp.path(), "pos-v8-before-v13-").len(), 1);
        assert_eq!(fs::read(backup).unwrap(), backup_bytes);
        assert_eq!(fs::read(manifest).unwrap(), manifest_bytes);
    }

    #[test]
    fn rejects_intermediate_future_and_negative_versions() {
        for requested in [-1_i64, 1, 7, 9, 10, 11, 14] {
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
            assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
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
        assert!(matching_files(invalid.path(), "pos-v8-before-v13-").is_empty());
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
            let marker = test_manifest("blocked", 8, 13, backup.clone());
            write_pending_marker(&temp.path().join(PENDING_MARKER_NAME), &marker).unwrap();

            let error = open_database_path(temp.path(), &path).unwrap_err();
            let startup_message = error.recovery_startup_message().unwrap();
            assert!(startup_message
                .0
                .contains(backup.to_string_lossy().as_ref()));
        }
    }

    #[test]
    fn pending_marker_is_reported_before_strict_schema_preflight() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        let mut connection = Connection::open(&path).unwrap();
        migrations::run(&mut connection).unwrap();
        connection
            .execute("DROP INDEX idx_inventory_movements_sale_detail_id", [])
            .unwrap();
        drop(connection);
        let backup = temp.path().join("exact-backup.db");
        let marker = test_manifest("preflight-order", 12, 13, backup.clone());
        write_pending_marker(&temp.path().join(PENDING_MARKER_NAME), &marker).unwrap();

        let error = open_database_path(temp.path(), &path).unwrap_err();

        assert!(matches!(error, DatabaseOpenError::Recovery { .. }));
        assert!(error
            .to_string()
            .contains(backup.to_string_lossy().as_ref()));
    }

    #[test]
    fn reconciled_marker_keeps_backup_context_for_following_preflight() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        let mut connection = Connection::open(&path).unwrap();
        migrations::run(&mut connection).unwrap();
        connection
            .execute("DROP INDEX idx_inventory_movements_sale_detail_id", [])
            .unwrap();
        drop(connection);
        let backup = temp.path().join("exact-backup.db");
        let marker = test_manifest("reconciled-preflight", 12, 13, backup.clone());
        let active = temp.path().join(PENDING_MARKER_NAME);
        let completed = temp
            .path()
            .join("pos-schema-v13-migration-completed-reconciled-preflight.json");
        write_pending_marker(&active, &marker).unwrap();
        write_json_marker(&completed, &marker, "reconciled-preflight").unwrap();

        let error = open_database_path(temp.path(), &path).unwrap_err();

        assert!(!active.exists());
        assert!(matches!(error, DatabaseOpenError::Recovery { .. }));
        assert!(error
            .to_string()
            .contains(backup.to_string_lossy().as_ref()));
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
                source_version: 8,
                target_version: 13,
                backup_size_bytes: 0,
                backup_sha256: "0".repeat(64),
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
    fn noncanonical_v8_is_rejected_before_backup_and_marker() {
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
        assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
        assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
    }

    #[test]
    fn failed_post_validation_leaves_v13_pending_and_blocks_reopen() {
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
        assert_eq!(version(&path), 13);
        assert!(temp.path().join(PENDING_MARKER_NAME).exists());
        assert!(open_database_path(temp.path(), &path).is_err());
    }

    #[test]
    fn every_hook_failure_after_marker_is_a_recovery_error() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);

        let error = open_database_path_with_hook(temp.path(), &path, |_| {
            Err(DatabaseOpenError::General("fallo inyectado".into()))
        })
        .unwrap_err();
        let marker: MigrationManifest =
            serde_json::from_slice(&fs::read(temp.path().join(PENDING_MARKER_NAME)).unwrap())
                .unwrap();

        assert!(matches!(error, DatabaseOpenError::Recovery { .. }));
        assert!(error
            .to_string()
            .contains(marker.backup_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn v8_sequence_snapshot_and_upgrade_preserve_absence_separately_from_zero() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        let mut connection = Connection::open(&path).unwrap();
        migrations::run_to_version(&mut connection, 8).unwrap();
        let absent = migration_source_snapshot(&connection, &path, 8).unwrap();
        assert!(absent
            .sequences
            .iter()
            .all(|row| row[1] == "0" && row[2] == "NULL"));
        connection
            .execute(
                "INSERT INTO sqlite_sequence(name,seq) VALUES('products',0)",
                [],
            )
            .unwrap();
        let zero = migration_source_snapshot(&connection, &path, 8).unwrap();
        assert_ne!(zero.sequences, absent.sequences);
        connection
            .execute("DELETE FROM sqlite_sequence WHERE name='products'", [])
            .unwrap();
        drop(connection);

        let migrated = open_database_path(temp.path(), &path).unwrap();
        assert_eq!(schema_version(&migrated, &path).unwrap(), 13);
        assert_eq!(
            migration_target_snapshot(&migrated, &path, 8).unwrap(),
            absent
        );
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
            let manifest = test_manifest(
                format!("completion-{failure_stage:?}"),
                8,
                13,
                temp.path().join("backup.db"),
            );
            write_pending_marker(&active, &manifest).unwrap();
            let completed = temp.path().join(format!(
                "pos-schema-v13-migration-completed-{}.json",
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
                source_version: 8,
                target_version: 13,
                backup_size_bytes: 0,
                backup_sha256: "0".repeat(64),
            };
            write_json_marker(&active, &manifest, &manifest.restore_id).unwrap();
            let completed = temp.path().join(format!(
                "pos-schema-v13-restore-completed-{}.json",
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
        let manifest = test_manifest("mismatch", 8, 13, temp.path().join("backup.db"));
        write_pending_marker(&active, &manifest).unwrap();
        let completed = temp
            .path()
            .join("pos-schema-v13-migration-completed-mismatch.json");
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

        let error =
            create_validated_backup_with(&source, temp.path(), &path, 8, |_, destination| {
                Err(safe_error("Fallo de backup inyectado", destination))
            })
            .unwrap_err();

        assert!(error.to_string().contains("Fallo de backup inyectado"));
        assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
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

        let result =
            create_validated_backup_with(&source, temp.path(), &path, 8, |_, destination| {
                fs::write(destination, b"invalid sqlite backup")
                    .map_err(|_| safe_error("Fallo de escritura inyectado", destination))
            });

        assert!(result.is_err());
        assert!(matching_files(temp.path(), "pos-v8-before-v13-").is_empty());
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

        let manifest = create_validated_backup(&source, temp.path(), &path, 8).unwrap();
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
        let backup = matching_files(temp.path(), "pos-v8-before-v13-")[0].clone();
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

        let restored = restore_migration_backup_inner(temp.path(), &path, &backup).unwrap();
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
    fn restore_rejects_a_canonical_v8_backup_without_its_installation_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
        create_v8_database(&path);
        drop(open_database_path(temp.path(), &path).unwrap());
        let backup = matching_files(temp.path(), "pos-v8-before-v13-")[0].clone();
        let manifest =
            matching_files(temp.path(), "pos-schema-v13-migration-completed-")[0].clone();
        fs::remove_file(manifest).unwrap();

        assert!(restore_migration_backup_inner(temp.path(), &path, &backup).is_err());
        assert_eq!(version(&path), 13);
    }

    #[test]
    fn restore_rejects_backup_identity_or_database_path_manifest_tampering() {
        for tamper_database_path in [false, true] {
            let temp = TempDir::new().unwrap();
            let path = temp.path().join("pos.db");
            create_v8_database(&path);
            drop(open_database_path(temp.path(), &path).unwrap());
            let backup = matching_files(temp.path(), "pos-v8-before-v13-")[0].clone();
            let manifest_path =
                matching_files(temp.path(), "pos-schema-v13-migration-completed-")[0].clone();

            if tamper_database_path {
                let mut completed: CompletedMigrationManifest =
                    serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
                completed.migration.database_path = temp.path().join("other.db");
                fs::write(
                    &manifest_path,
                    serde_json::to_vec_pretty(&completed).unwrap(),
                )
                .unwrap();
            } else {
                fs::OpenOptions::new()
                    .append(true)
                    .open(&backup)
                    .unwrap()
                    .write_all(b"tampered")
                    .unwrap();
            }

            assert!(restore_migration_backup_inner(temp.path(), &path, &backup).is_err());
            assert_eq!(version(&path), 13);
        }
    }

    #[test]
    fn durable_restore_uses_the_v12_source_declared_by_the_manifest() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("backup-v12.db");
        create_v12_database(&backup);
        let path = temp.path().join("pos.db");
        let mut current = Connection::open(&path).unwrap();
        migrations::run(&mut current).unwrap();
        drop(current);
        let manifest = test_manifest("restore-v12", 12, 13, backup.clone());
        write_pending_marker(&temp.path().join(PENDING_MARKER_NAME), &manifest).unwrap();

        let restored = restore_migration_backup_inner(temp.path(), &path, &backup).unwrap();

        assert_eq!(schema_version(&restored, &path).unwrap(), 12);
        validate_database(&restored, 12, true, &path).unwrap();
        assert!(!temp.path().join(PENDING_MARKER_NAME).exists());
        assert!(temp
            .path()
            .join("pos-schema-v13-migration-restored-restore-v12.json")
            .exists());
    }

    #[test]
    fn failed_restore_after_promotion_never_exposes_v8_with_old_sidecars_and_rolls_back() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("pos.db");
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
        let manifest = test_manifest("replace-failure", 8, 13, backup.clone());
        let pending = temp.path().join(PENDING_MARKER_NAME);
        write_pending_marker(&pending, &manifest).unwrap();

        let error =
            restore_migration_backup_inner_with_hook(temp.path(), &path, &backup, |stage| {
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
    fn substituted_temporary_is_never_accepted_and_previous_database_is_restored() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("backup.db");
        create_v8_database(&backup);
        let path = temp.path().join("pos.db");
        let mut current = Connection::open(&path).unwrap();
        migrations::run(&mut current).unwrap();
        drop(current);
        let before = fs::read(&path).unwrap();
        let pending = temp.path().join(PENDING_MARKER_NAME);
        write_pending_marker(
            &pending,
            &test_manifest("temporary-substitution", 8, 13, backup.clone()),
        )
        .unwrap();

        let result =
            restore_migration_backup_inner_with_hook(temp.path(), &path, &backup, |stage| {
                if stage == RestoreStage::BeforeBackupPromotion {
                    let temporary = matching_files(temp.path(), ".pos-restore-");
                    assert_eq!(temporary.len(), 1);
                    fs::remove_file(&temporary[0])?;
                    fs::write(&temporary[0], b"substituted after validation")?;
                }
                Ok(())
            });

        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(pending.exists());
        assert!(!temp.path().join(RESTORE_MARKER_NAME).exists());
        assert_eq!(matching_files(temp.path(), ".pos-restore-failed-").len(), 1);
        assert!(matching_files(temp.path(), ".pos-quarantine-").is_empty());
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
        let manifest = test_manifest("successful-restore", 8, 13, backup.clone());
        let pending = temp.path().join(PENDING_MARKER_NAME);
        write_pending_marker(&pending, &manifest).unwrap();

        let restored = restore_migration_backup_inner(temp.path(), &path, &backup).unwrap();

        assert_eq!(schema_version(&restored, &path).unwrap(), 8);
        assert!(!pending.exists());
        let archived = temp
            .path()
            .join("pos-schema-v13-migration-restored-successful-restore.json");
        assert_eq!(
            fs::read(archived).unwrap(),
            serde_json::to_vec_pretty(&manifest).unwrap()
        );
        assert!(sqlite_sidecars(&path)
            .into_iter()
            .all(|sidecar| !sidecar.exists()));
        assert_eq!(matching_files(temp.path(), ".pos-quarantine-").len(), 3);
        assert_eq!(
            matching_files(temp.path(), "pos-schema-v13-restore-completed-").len(),
            1
        );
        assert!(!temp.path().join(RESTORE_MARKER_NAME).exists());
    }

    #[test]
    fn injected_restore_failures_rollback_each_durable_stage() {
        for failure_stage in [
            RestoreStage::MarkerPersisted,
            RestoreStage::NamespaceQuarantined,
            RestoreStage::BeforeBackupPromotion,
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
                &test_manifest(format!("failure-{failure_stage:?}"), 8, 13, backup.clone()),
            )
            .unwrap();

            let result =
                restore_migration_backup_inner_with_hook(temp.path(), &path, &backup, |stage| {
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
            let manifest = test_manifest(
                format!("windows-{failure_stage:?}"),
                8,
                13,
                temp.path().join("backup.db"),
            );
            write_pending_marker(&active, &manifest).unwrap();
            let completed = temp.path().join(format!(
                "pos-schema-v13-migration-completed-{}.json",
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
        assert_eq!(schema_version(&connection, &path).unwrap(), 13);
    }

    #[cfg(unix)]
    #[test]
    fn replacing_the_migration_lock_invalidates_the_guard() {
        let temp = TempDir::new().unwrap();
        let lock = acquire_migration_lock(temp.path()).unwrap();
        let path = temp.path().join(MIGRATION_LOCK_NAME);
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement").unwrap();

        assert!(lock.validate().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn maintenance_restore_classifies_a_replaced_migration_lock_as_locked() {
        let temp = TempDir::new().unwrap();
        let backup = temp.path().join("backup.db");
        create_v8_database(&backup);
        let database = temp.path().join("pos.db");
        let mut current = Connection::open(&database).unwrap();
        migrations::run(&mut current).unwrap();
        drop(current);
        write_pending_marker(
            &temp.path().join(PENDING_MARKER_NAME),
            &test_manifest("replaced-lock", 8, 13, backup.clone()),
        )
        .unwrap();
        let lock_path = temp.path().join(MIGRATION_LOCK_NAME);
        let mut replaced = false;

        let result = restore_migration_backup_with_guard(temp.path(), &database, &backup, || {
            if !replaced {
                fs::remove_file(&lock_path)?;
                fs::write(&lock_path, b"replacement")?;
                replaced = true;
            }
            Ok(())
        });

        assert!(matches!(result, Err(MaintenanceRestoreError::Locked)));
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

        let backup = matching_files(temp.path(), "pos-v8-before-v13-")[0].clone();
        let manifest =
            matching_files(temp.path(), "pos-schema-v13-migration-completed-")[0].clone();
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
