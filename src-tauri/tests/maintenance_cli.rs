use fs2::FileExt;
use std::{fs, process::Command};
use tempfile::TempDir;

#[test]
fn invalid_paths_exit_unsuccessfully_without_echoing_them() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("sensitive-database-name.db");
    let missing_backup = temp.path().join("sensitive-backup-name.db");
    fs::write(&database, b"database placeholder").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .args([
            "restore-migration-backup",
            "--database",
            database.to_str().unwrap(),
            "--backup",
            missing_backup.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stderr.trim_end(), "Error: La ruta del backup no es valida");
    assert!(!stderr.contains(temp.path().to_string_lossy().as_ref()));
}

#[test]
fn maintenance_mode_is_exposed_by_the_packaged_application_binary() {
    let output = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .arg("maintenance-info")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        "Linea POS 0.2.1: mantenimiento local disponible"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn syntax_and_rejected_restore_have_exact_exit_codes() {
    let syntax = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .args(["maintenance-info", "extra"])
        .output()
        .unwrap();
    assert_eq!(syntax.status.code(), Some(2));

    let temp = TempDir::new().unwrap();
    let database = temp.path().join("pos.db");
    let backup = temp.path().join("backup.db");
    fs::write(&database, b"database placeholder").unwrap();
    fs::write(&backup, b"backup placeholder").unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .args([
            "restore-migration-backup",
            "--database",
            database.to_str().unwrap(),
            "--backup",
            backup.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(rejected.status.code(), Some(5));
}

#[test]
fn another_process_lock_blocks_restore() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("pos.db");
    let backup = temp.path().join("pos-v12-before-v13-test.db");
    fs::write(&database, b"database placeholder").unwrap();
    fs::write(&backup, b"backup placeholder").unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(temp.path().join("pos-application.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .args([
            "restore-migration-backup",
            "--database",
            database.to_str().unwrap(),
            "--backup",
            backup.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("Linea POS esta en ejecucion"));
}

#[test]
fn migration_lock_contention_exits_as_locked() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("pos.db");
    let backup = temp.path().join("pos-v12-before-v13-test.db");
    fs::write(&database, b"database placeholder").unwrap();
    fs::write(&backup, b"backup placeholder").unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(temp.path().join("pos-schema-v13-migration.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_pos_tauri"))
        .args([
            "restore-migration-backup",
            "--database",
            database.to_str().unwrap(),
            "--backup",
            backup.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(4));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("No se pudo bloquear el directorio de datos"));
}
