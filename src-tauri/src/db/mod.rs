pub mod migrations;

use rusqlite::Connection;
use std::{error::Error, fs, path::Path, sync::Mutex, time::Duration};
use tauri::Manager;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};

pub struct Database(pub Mutex<Connection>);

pub fn open_database(app: &tauri::App) -> Result<Connection, Box<dyn Error>> {
    let app_data_dir = app.path().app_data_dir()?;
    let database_path = app_data_dir.join("pos.db");
    prepare_storage(&app_data_dir, &database_path)?;

    let mut connection = Connection::open(&database_path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
    migrations::run(&mut connection)?;
    secure_database_files(&database_path)?;

    Ok(connection)
}

#[cfg(unix)]
fn prepare_storage(directory: &Path, database_path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(database_path)?;
    fs::set_permissions(database_path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn prepare_storage(directory: &Path, _database_path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(directory)
}

#[cfg(unix)]
fn secure_database_files(database_path: &Path) -> std::io::Result<()> {
    for path in [
        database_path.to_path_buf(),
        database_path.with_extension("db-wal"),
        database_path.with_extension("db-shm"),
    ] {
        if path.try_exists()? {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_database_files(_database_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn secures_app_data_and_all_sqlite_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let parent = std::env::temp_dir().join(format!(
            "pos-tauri-permissions-{}-{unique}",
            std::process::id()
        ));
        let directory = parent.join("data");
        let database = directory.join("pos.db");

        super::prepare_storage(&directory, &database).unwrap();
        fs::write(database.with_extension("db-wal"), []).unwrap();
        fs::write(database.with_extension("db-shm"), []).unwrap();
        super::secure_database_files(&database).unwrap();

        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let wal = database.with_extension("db-wal");
        let shm = database.with_extension("db-shm");
        for path in [&database, &wal, &shm] {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(parent).unwrap();
    }
}
