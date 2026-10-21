use crate::{db, process_lock};
use std::{ffi::OsString, fmt, fs, path::PathBuf};

const USAGE: &str = "Uso: pos_tauri maintenance-info | restore-migration-backup --database <ruta-absoluta>/pos.db --backup <ruta-absoluta>/pos-v8|v12-before-v13-ID.db";
const MAINTENANCE_INFO: &str = "Linea POS 0.2.1: mantenimiento local disponible";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaintenanceError {
    InvalidArguments,
    InvalidDatabasePath,
    InvalidBackupPath,
    SameFile,
    ApplicationRunning,
    LockUnavailable,
    RestoreFailed,
    CloseFailed,
}

impl fmt::Display for MaintenanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidArguments => USAGE,
            Self::InvalidDatabasePath => "La ruta de la base de datos no es valida",
            Self::InvalidBackupPath => "La ruta del backup no es valida",
            Self::SameFile => "La base de datos y el backup deben ser archivos distintos",
            Self::ApplicationRunning => {
                "Linea POS esta en ejecucion; cierre la aplicacion antes de restaurar"
            }
            Self::LockUnavailable => "No se pudo bloquear el directorio de datos",
            Self::RestoreFailed => "No se pudo completar la restauracion durable",
            Self::CloseFailed => "La restauracion termino, pero no se pudo cerrar SQLite",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for MaintenanceError {}

impl MaintenanceError {
    pub fn exit_code(self) -> u8 {
        match self {
            Self::InvalidArguments => 2,
            Self::InvalidDatabasePath | Self::InvalidBackupPath | Self::SameFile => 3,
            Self::ApplicationRunning | Self::LockUnavailable => 4,
            Self::RestoreFailed => 5,
            Self::CloseFailed => 6,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RestoreArguments {
    database: PathBuf,
    backup: PathBuf,
}

fn parse_arguments<I>(arguments: I) -> Result<RestoreArguments, MaintenanceError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut arguments = arguments.into_iter();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("restore-migration-backup")) {
        return Err(MaintenanceError::InvalidArguments);
    }

    let mut database = None;
    let mut backup = None;
    while let Some(argument) = arguments.next() {
        let target = match argument.to_str() {
            Some("--database") if database.is_none() => &mut database,
            Some("--backup") if backup.is_none() => &mut backup,
            _ => return Err(MaintenanceError::InvalidArguments),
        };
        let value = arguments
            .next()
            .filter(|value| !value.is_empty())
            .ok_or(MaintenanceError::InvalidArguments)?;
        *target = Some(PathBuf::from(value));
    }

    let result = RestoreArguments {
        database: database.ok_or(MaintenanceError::InvalidArguments)?,
        backup: backup.ok_or(MaintenanceError::InvalidArguments)?,
    };
    if !result.database.is_absolute() || !result.backup.is_absolute() {
        return Err(MaintenanceError::InvalidArguments);
    }
    Ok(result)
}

fn canonical_file(path: PathBuf, error: MaintenanceError) -> Result<PathBuf, MaintenanceError> {
    let original = fs::symlink_metadata(&path).map_err(|_| error)?;
    if original.file_type().is_symlink() {
        return Err(error);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if original.file_attributes() & 0x400 != 0 {
            return Err(error);
        }
    }
    let canonical = fs::canonicalize(path).map_err(|_| error)?;
    let metadata = fs::metadata(&canonical).map_err(|_| error)?;
    if !metadata.is_file() {
        return Err(error);
    }
    Ok(canonical)
}

pub fn run<I>(arguments: I) -> Result<&'static str, MaintenanceError>
where
    I: IntoIterator<Item = OsString>,
{
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    if arguments.as_slice() == [std::ffi::OsStr::new("maintenance-info")] {
        return Ok(MAINTENANCE_INFO);
    }
    let arguments = parse_arguments(arguments)?;
    let directory = arguments
        .database
        .parent()
        .ok_or(MaintenanceError::InvalidDatabasePath)?;
    let application_lock = process_lock::acquire(directory).map_err(|error| match error {
        process_lock::ApplicationLockError::Unavailable => MaintenanceError::ApplicationRunning,
        process_lock::ApplicationLockError::Io => MaintenanceError::LockUnavailable,
    })?;
    let database = canonical_file(arguments.database, MaintenanceError::InvalidDatabasePath)?;
    let backup = canonical_file(arguments.backup, MaintenanceError::InvalidBackupPath)?;
    if database == backup {
        return Err(MaintenanceError::SameFile);
    }
    if database.file_name().and_then(|name| name.to_str()) != Some("pos.db") {
        return Err(MaintenanceError::InvalidDatabasePath);
    }
    let directory = database
        .parent()
        .ok_or(MaintenanceError::InvalidDatabasePath)?;
    application_lock
        .validate()
        .map_err(|_| MaintenanceError::LockUnavailable)?;
    let mut lock_failed = false;
    let connection = db::restore_migration_backup_with_guard(directory, &database, &backup, || {
        application_lock.validate().map_err(|_| {
            lock_failed = true;
            std::io::Error::other("application lock replaced")
        })
    })
    .map_err(|error| {
        if lock_failed {
            MaintenanceError::LockUnavailable
        } else {
            match error {
                db::MaintenanceRestoreError::Locked => MaintenanceError::LockUnavailable,
                db::MaintenanceRestoreError::Failed => MaintenanceError::RestoreFailed,
            }
        }
    })?;
    connection
        .close()
        .map_err(|_| MaintenanceError::CloseFailed)?;
    Ok("Restauracion durable completada")
}

pub fn run_process_if_requested() -> bool {
    let mut arguments = std::env::args_os().skip(1);
    if !matches!(
        arguments.next().as_deref(),
        Some(command)
            if command == std::ffi::OsStr::new("restore-migration-backup")
                || command == std::ffi::OsStr::new("maintenance-info")
    ) {
        return false;
    }
    match run(std::env::args_os().skip(1)) {
        Ok(message) => {
            println!("{message}");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(error.exit_code().into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn os(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parser_requires_the_exact_command_and_both_absolute_paths() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("pos.db");
        let backup = temp.path().join("backup.db");
        assert_eq!(
            parse_arguments(vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.clone().into_os_string(),
                OsString::from("--backup"),
                backup.clone().into_os_string(),
            ]),
            Ok(RestoreArguments {
                database: database.clone(),
                backup: backup.clone(),
            })
        );
        let invalid = [
            os(&[]),
            os(&["restore-migration-backup", "--database", "pos.db"]),
            vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.clone().into_os_string(),
                OsString::from("--backup"),
                OsString::from("backup.db"),
            ],
            vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.clone().into_os_string(),
                OsString::from("--database"),
                temp.path().join("other.db").into_os_string(),
                OsString::from("--backup"),
                backup.clone().into_os_string(),
            ],
            vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.into_os_string(),
                OsString::from("--backup"),
                backup.into_os_string(),
                OsString::from("--force"),
            ],
        ];
        for invalid in invalid {
            assert_eq!(
                parse_arguments(invalid),
                Err(MaintenanceError::InvalidArguments)
            );
        }
    }

    #[test]
    fn maintenance_info_is_non_destructive_and_rejects_extra_arguments() {
        assert_eq!(run(os(&["maintenance-info"])), Ok(MAINTENANCE_INFO));
        assert_eq!(
            run(os(&["maintenance-info", "extra"])),
            Err(MaintenanceError::InvalidArguments)
        );
    }

    #[test]
    fn maintenance_exit_codes_are_stable() {
        assert_eq!(MaintenanceError::InvalidArguments.exit_code(), 2);
        assert_eq!(MaintenanceError::InvalidDatabasePath.exit_code(), 3);
        assert_eq!(MaintenanceError::InvalidBackupPath.exit_code(), 3);
        assert_eq!(MaintenanceError::SameFile.exit_code(), 3);
        assert_eq!(MaintenanceError::ApplicationRunning.exit_code(), 4);
        assert_eq!(MaintenanceError::LockUnavailable.exit_code(), 4);
        assert_eq!(MaintenanceError::RestoreFailed.exit_code(), 5);
        assert_eq!(MaintenanceError::CloseFailed.exit_code(), 6);
    }

    #[test]
    fn run_rejects_missing_and_identical_files() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("pos.db");
        fs::write(&database, b"not sqlite").unwrap();

        assert_eq!(
            run(vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.clone().into_os_string(),
                OsString::from("--backup"),
                temp.path().join("missing.db").into_os_string(),
            ]),
            Err(MaintenanceError::InvalidBackupPath)
        );
        assert_eq!(
            run(vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.clone().into_os_string(),
                OsString::from("--backup"),
                database.into_os_string(),
            ]),
            Err(MaintenanceError::SameFile)
        );
    }

    #[test]
    fn run_rejects_restore_while_application_lock_is_held() {
        let temp = TempDir::new().unwrap();
        let database = temp.path().join("pos.db");
        let backup = temp.path().join("backup.db");
        fs::write(&database, b"database placeholder").unwrap();
        fs::write(&backup, b"backup placeholder").unwrap();
        let _application_lock = process_lock::acquire(temp.path()).unwrap();

        assert_eq!(
            run(vec![
                OsString::from("restore-migration-backup"),
                OsString::from("--database"),
                database.into_os_string(),
                OsString::from("--backup"),
                backup.into_os_string(),
            ]),
            Err(MaintenanceError::ApplicationRunning)
        );
    }
}
