use super::*;
use crate::{models::user::LoginInput, services::auth};
use argon2::{password_hash::PasswordHasher, Argon2};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{env, error::Error, fs, path::Path};

const QA_DIRECTORY_PREFIX: &str = "pos-tauri-v8-v13-qa-";
const V8_MIGRATIONS: [(&str, &[u8], &str); 8] = [
    (
        "001_create_users.sql",
        include_bytes!("../../../migrations/001_create_users.sql"),
        "847244dee2c0dde13120abfe8c2cd01ef81f8ce9e5937daaab311255d74a75d0",
    ),
    (
        "002_create_brands.sql",
        include_bytes!("../../../migrations/002_create_brands.sql"),
        "6aef237d6069e61d7e780db2bc169ea7ab1b7985dbf2b8fdaa93de55f8eecd99",
    ),
    (
        "003_create_categories.sql",
        include_bytes!("../../../migrations/003_create_categories.sql"),
        "c4f2a6c7e64dd802e4b5121944b7d8db3d6f4d068095f92e0dc7fadcb125923c",
    ),
    (
        "004_create_products.sql",
        include_bytes!("../../../migrations/004_create_products.sql"),
        "494786996905da6839409b62cfe720757e4913719be23175c996dd0616bdead6",
    ),
    (
        "005_create_sales.sql",
        include_bytes!("../../../migrations/005_create_sales.sql"),
        "d119172ab923ca8ad9f5f347f74b4f58ff4916773187270baff8521b3158e1f1",
    ),
    (
        "006_create_sale_details.sql",
        include_bytes!("../../../migrations/006_create_sale_details.sql"),
        "fd3c50a7d988aa1de49c73d9b9577c12f2b9ea0f79646217ca29e2ff4c47a8df",
    ),
    (
        "007_create_inventory_movements.sql",
        include_bytes!("../../../migrations/007_create_inventory_movements.sql"),
        "a589f03ddd9a5e9afc51dd1cc58a2915fc35f57490e61f23c6660cbad631bad2",
    ),
    (
        "008_create_indexes.sql",
        include_bytes!("../../../migrations/008_create_indexes.sql"),
        "e2cbba959454aeeba081f70044be4466714480da789a9db80c5b55afe1cc4444",
    ),
];
const IMMUTABLE_V9_V12_MIGRATIONS: [(&str, &[u8], &str); 4] = [
    (
        "009_create_product_presentations.sql",
        include_bytes!("../../../migrations/009_create_product_presentations.sql"),
        "2eaa90e11c39e58d64e6e4dfb9b8d91c3a90367a210c4fb5e1a898d10b03f786",
    ),
    (
        "010_rebuild_sale_details.sql",
        include_bytes!("../../../migrations/010_rebuild_sale_details.sql"),
        "ed7ae6cb9652cad3ea16ea9bce37b6d2f60e41b6001047bd7f6d94ec677ae1d5",
    ),
    (
        "011_rebuild_inventory_movements.sql",
        include_bytes!("../../../migrations/011_rebuild_inventory_movements.sql"),
        "6ab324ab5c4453880ea9d5852a99167efd8c51d9b71cee253e0a51b9ff117fc6",
    ),
    (
        "012_add_sale_pricing_and_discounts.sql",
        include_bytes!("../../../migrations/012_add_sale_pricing_and_discounts.sql"),
        "dfb071ef4f53a9bf923b5c9d5a10d9a10f9eec1c261e8b8ba9caa1a503225a1a",
    ),
];

fn assert_v8_migrations_match_e2ab2d2() {
    for (name, contents, expected) in V8_MIGRATIONS {
        assert_eq!(
            format!("{:x}", Sha256::digest(contents)),
            expected,
            "migration {name} differs from main e2ab2d2"
        );
    }
    for (name, contents, expected) in IMMUTABLE_V9_V12_MIGRATIONS {
        assert_eq!(
            format!("{:x}", Sha256::digest(contents)),
            expected,
            "migration {name} must remain immutable"
        );
    }
}

fn qa_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    let root = env::var_os("POS_MIGRATION_QA_DIR").ok_or("POS_MIGRATION_QA_DIR is required")?;
    let root = fs::canonicalize(root)?;
    let temporary = fs::canonicalize(env::temp_dir())?;
    if root.parent() != Some(temporary.as_path())
        || !root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(QA_DIRECTORY_PREFIX))
    {
        return Err(
            "QA directory must be a direct prefixed child of the system temp directory".into(),
        );
    }
    if fs::read_dir(&root)?.next().is_some() {
        return Err("QA directory must start empty".into());
    }
    Ok(root)
}

fn query_rows(
    connection: &Connection,
    sql: &str,
    columns: usize,
) -> rusqlite::Result<Vec<Vec<Value>>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement
        .query_map([], |row| {
            (0..columns)
                .map(|column| {
                    let value = row.get_ref(column)?;
                    Ok(match value {
                        rusqlite::types::ValueRef::Null => Value::Null,
                        rusqlite::types::ValueRef::Integer(value) => json!(value),
                        rusqlite::types::ValueRef::Real(value) => json!(value),
                        rusqlite::types::ValueRef::Text(value) => {
                            json!(String::from_utf8_lossy(value))
                        }
                        rusqlite::types::ValueRef::Blob(value) => json!(format!("{value:?}")),
                    })
                })
                .collect::<rusqlite::Result<Vec<_>>>()
        })?
        .collect();
    rows
}

fn integrity(connection: &Connection) -> rusqlite::Result<Value> {
    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    let foreign_key_violations: i64 =
        connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(json!({
        "user_version": version,
        "quick_check": quick_check,
        "foreign_key_violations": foreign_key_violations,
    }))
}

fn assert_v13_unit_domains(connection: &Connection) -> Result<(), Box<dyn Error>> {
    connection.execute_batch("SAVEPOINT centimeter_domain")?;
    for table in ["products", "sale_details", "inventory_movements"] {
        connection.execute(
            &format!("UPDATE {table} SET base_unit='centimeter' WHERE id=1"),
            [],
        )?;
        assert_eq!(
            connection.query_row(
                &format!("SELECT base_unit FROM {table} WHERE id=1"),
                [],
                |row| row.get::<_, String>(0),
            )?,
            "centimeter"
        );
    }
    connection.execute_batch("ROLLBACK TO centimeter_domain; RELEASE centimeter_domain")?;

    for table in ["products", "sale_details", "inventory_movements"] {
        assert!(
            connection
                .execute(
                    &format!("UPDATE {table} SET base_unit='unknown' WHERE id=1"),
                    [],
                )
                .is_err(),
            "{table} accepted an unknown base unit"
        );
    }
    Ok(())
}

fn assert_intermediate_versions_rejected(root: &Path) -> Result<(), Box<dyn Error>> {
    for version in 9..=11 {
        let runtime = root.join(format!("rejected-v{version}"));
        fs::create_dir(&runtime)?;
        let path = runtime.join("pos.db");
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "user_version", version)?;
        drop(connection);

        assert!(open_database_path(&runtime, &path).is_err());
        assert_eq!(
            Connection::open(&path)?
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            version
        );
        assert!(matching_files(&runtime, "pos-v").is_empty());
        assert!(!runtime.join(PENDING_MARKER_NAME).exists());
    }
    Ok(())
}

fn summary(connection: &Connection) -> rusqlite::Result<Value> {
    Ok(json!({
        "counts": query_rows(connection, "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM brands), (SELECT count(*) FROM categories), (SELECT count(*) FROM products), (SELECT count(*) FROM sales), (SELECT count(*) FROM sale_details), (SELECT count(*) FROM inventory_movements)", 7)?,
        "products": query_rows(connection, "SELECT id,name,price,stock,sku,created_at,updated_at FROM products ORDER BY id", 7)?,
        "sales": query_rows(connection, "SELECT id,user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,total,payment_method,cash_received,change_amount,created_at,updated_at FROM sales ORDER BY id", 13)?,
        "sale_details": query_rows(connection, "SELECT id,product_id,sale_id,product_name,sku,quantity,unit_price,created_at,updated_at FROM sale_details ORDER BY id", 9)?,
        "movements": query_rows(connection, "SELECT id,product_id,product_name,sku,sale_id,origin,reason,type,quantity,operation_token,intent_fingerprint,created_at,updated_at FROM inventory_movements ORDER BY id", 13)?,
        "sequences": query_rows(connection, "SELECT name,seq FROM sqlite_sequence WHERE name IN ('sales','sale_details','inventory_movements') ORDER BY name", 2)?,
    }))
}

fn create_v8_fixture(path: &Path) -> Result<Value, Box<dyn Error>> {
    let mut connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", true)?;
    migrations::run_to_version(&mut connection, 8)?;
    let password_hash = Argon2::default()
        .hash_password_with_salt(b"PosDemo123!", b"pos-tauri-qa-v8")?
        .to_string();
    connection.execute(
        "INSERT INTO users(id,first_name,last_name,email,password_hash,created_at,updated_at)
         VALUES(7,'Admin','POS','admin@pos.local',?1,'2025-01-01 08:00:00','2025-01-01 08:00:01')",
        [password_hash],
    )?;
    connection.execute_batch(
        "INSERT INTO brands(id,name,name_search_key,created_at,updated_at) VALUES
           (3,'Andina','andina','2025-01-02 08:00:00','2025-01-02 08:00:01'),
           (9,'Pacifico','pacifico','2025-01-02 08:01:00','2025-01-02 08:01:01');
         INSERT INTO categories(id,name,name_search_key,created_at,updated_at) VALUES
           (4,'Bebidas','bebidas','2025-01-02 09:00:00','2025-01-02 09:00:01'),
           (8,'Despensa','despensa','2025-01-02 09:01:00','2025-01-02 09:01:01');
         INSERT INTO products(id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,created_at,updated_at) VALUES
           (11,'Cafe actual','cafe actual',450,8,'CAFE-001','cafe-001',3,8,'2025-01-03 10:00:00','2025-02-03 10:00:00'),
           (14,'Agua actual','agua actual',700,0,'AGUA-002','agua-002',9,4,'2025-01-03 10:01:00','2025-02-03 10:01:00'),
           (20,'Arroz actual','arroz actual',320,25,'ARROZ-003','arroz-003',3,8,'2025-01-03 10:02:00','2025-02-03 10:02:00');
         INSERT INTO sales(id,user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,total,payment_method,cash_received,change_amount,created_at,updated_at) VALUES
           (21,7,'Admin Historico','admin@pos.local','checkout-card-2025','cart-v1|11:2','2025-03-10 11:12:13',900,'tarjeta',NULL,NULL,'2025-03-10 11:12:14','2025-03-10 11:12:15'),
           (27,7,'Admin Historico','admin@pos.local','checkout-cash-2025','cart-v1|14:1','2025-03-11 12:13:14',700,'efectivo',1000,300,'2025-03-11 12:13:15','2025-03-11 12:13:16');
         INSERT INTO sale_details(id,product_id,sale_id,product_name,sku,quantity,unit_price,created_at,updated_at) VALUES
           (31,11,21,'Cafe nombre historico','CAFE-HIST',2,450,'2025-03-10 11:12:16','2025-03-10 11:12:17'),
           (37,14,27,'Agua nombre historico','AGUA-HIST',1,700,'2025-03-11 12:13:17','2025-03-11 12:13:18');
         INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,sale_id,origin,reason,type,quantity,operation_token,intent_fingerprint,created_at,updated_at) VALUES
           (41,11,'Cafe venta historico','CAFE-HIST','cafe venta historico','cafe-hist',21,'sale','venta','salida',2,NULL,NULL,'2025-03-10 11:12:18','2025-03-10 11:12:19'),
           (47,14,'Agua venta historico','AGUA-HIST','agua venta historico','agua-hist',27,'sale','venta','salida',1,NULL,NULL,'2025-03-11 12:13:19','2025-03-11 12:13:20'),
           (53,20,'Arroz inicial historico','ARROZ-INICIAL','arroz inicial historico','arroz-inicial',NULL,'initial_stock','ajuste','entrada',30,NULL,NULL,'2025-01-03 10:02:01','2025-01-03 10:02:02'),
           (61,11,'Cafe manual historico','CAFE-MANUAL','cafe manual historico','cafe-manual',NULL,'manual','compra','entrada',5,'manual-buy-token','manual-buy-fingerprint','2025-02-10 09:00:00','2025-02-10 09:00:01'),
           (67,20,'Arroz ajuste historico','ARROZ-AJUSTE','arroz ajuste historico','arroz-ajuste',NULL,'manual','ajuste','salida',5,'manual-adjust-token','manual-adjust-fingerprint','2025-02-11 09:00:00','2025-02-11 09:00:01');
         INSERT INTO sales(id,user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,total,payment_method) VALUES
           (90,7,'Hueco','admin@pos.local','deleted-sale-token','deleted','2025-04-01',1,'tarjeta');
         DELETE FROM sales WHERE id=90;
         INSERT INTO sale_details(id,product_id,sale_id,product_name,sku,quantity,unit_price) VALUES
           (95,20,21,'Detalle eliminado','DELETED',1,1);
         DELETE FROM sale_details WHERE id=95;
         INSERT INTO inventory_movements(id,product_id,product_name,sku,product_search_key,sku_search_key,origin,reason,type,quantity,operation_token,intent_fingerprint) VALUES
           (110,20,'Movimiento eliminado','DELETED','movimiento eliminado','deleted','manual','ajuste','entrada',1,'deleted-movement-token','deleted-movement-fingerprint');
         DELETE FROM inventory_movements WHERE id=110;",
    )?;
    validate_database(&connection, 8, true, path)?;
    auth::login(
        &connection,
        LoginInput {
            email: "admin@pos.local".into(),
            password: "PosDemo123!".into(),
        },
    )?;
    let report = json!({"integrity": integrity(&connection)?, "summary": summary(&connection)?});
    connection.close().map_err(|(_, error)| error)?;
    Ok(report)
}

fn assert_v13_data(connection: &Connection, before: &Value) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        integrity(connection)?,
        json!({"user_version":13,"quick_check":"ok","foreign_key_violations":0})
    );
    let after_legacy = summary(connection)?;
    assert_eq!(after_legacy, before["summary"]);

    let product_backfills = query_rows(
        connection,
        "SELECT p.id,p.base_unit,p.inventory_cost_cents,pp.product_id,pp.name,pp.mode,pp.base_quantity,pp.price,pp.is_default,pp.is_active FROM products p JOIN product_presentations pp ON pp.product_id=p.id ORDER BY p.id",
        10,
    )?;
    assert_eq!(
        product_backfills,
        vec![
            vec![
                json!(11),
                json!("piece"),
                Value::Null,
                json!(11),
                json!("Pieza"),
                json!("fixed"),
                json!(1),
                json!(450),
                json!(1),
                json!(1)
            ],
            vec![
                json!(14),
                json!("piece"),
                json!(0),
                json!(14),
                json!("Pieza"),
                json!("fixed"),
                json!(1),
                json!(700),
                json!(1),
                json!(1)
            ],
            vec![
                json!(20),
                json!("piece"),
                Value::Null,
                json!(20),
                json!("Pieza"),
                json!("fixed"),
                json!(1),
                json!(320),
                json!(1),
                json!(1)
            ],
        ]
    );
    assert_eq!(
        query_rows(connection, "SELECT id,subtotal,discount_type,discount_value,discount_amount,total,cash_received,change_amount FROM sales ORDER BY id", 8)?,
        vec![
            vec![json!(21),json!(900),json!("none"),json!(0),json!(0),json!(900),Value::Null,Value::Null],
            vec![json!(27),json!(700),json!("none"),json!(0),json!(0),json!(700),json!(1000),json!(300)],
        ]
    );
    assert_eq!(
        query_rows(connection, "SELECT id,record_version,presentation_id,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total FROM sale_details ORDER BY id", 9)?,
        vec![
            vec![json!(31),json!(1),json!(1),json!(2),json!(450),json!(450),json!(2),json!(2),json!(900)],
            vec![json!(37),json!(1),json!(2),json!(1),json!(700),json!(700),json!(1),json!(1),json!(700)],
        ]
    );
    assert_eq!(
        query_rows(connection, "SELECT id,record_version,presentation_id,sale_detail_id,entered_quantity,base_quantity,stock_before,stock_after,operation_token,intent_fingerprint FROM inventory_movements ORDER BY id", 10)?,
        vec![
            vec![json!(41),json!(1),json!(1),json!(31),json!(2),json!(2),Value::Null,Value::Null,Value::Null,Value::Null],
            vec![json!(47),json!(1),json!(2),json!(37),json!(1),json!(1),Value::Null,Value::Null,Value::Null,Value::Null],
            vec![json!(53),json!(1),json!(3),Value::Null,json!(30),json!(30),Value::Null,Value::Null,Value::Null,Value::Null],
            vec![json!(61),json!(1),json!(1),Value::Null,json!(5),json!(5),Value::Null,Value::Null,json!("manual-buy-token"),json!("manual-buy-fingerprint")],
            vec![json!(67),json!(1),json!(3),Value::Null,json!(5),json!(5),Value::Null,Value::Null,json!("manual-adjust-token"),json!("manual-adjust-fingerprint")],
        ]
    );
    assert_eq!(
        query_rows(connection, "SELECT name,seq FROM sqlite_sequence WHERE name IN ('sales','sale_details','inventory_movements') ORDER BY name", 2)?,
        vec![
            vec![json!("inventory_movements"),json!(110)],
            vec![json!("sale_details"),json!(95)],
            vec![json!("sales"),json!(90)],
        ]
    );
    auth::login(
        connection,
        LoginInput {
            email: "admin@pos.local".into(),
            password: "PosDemo123!".into(),
        },
    )?;
    Ok(())
}

#[test]
fn v8_migration_sources_match_main_e2ab2d2() {
    assert_v8_migrations_match_e2ab2d2();
}

#[test]
#[ignore = "creates inspectable QA artifacts with POS_MIGRATION_QA_DIR"]
fn real_v8_to_v13_migration_qa() -> Result<(), Box<dyn Error>> {
    assert_v8_migrations_match_e2ab2d2();
    let root = qa_root()?;
    let before_path = root.join("before-v8.db");
    let runtime = root.join("runtime-v8-v13");
    fs::create_dir(&runtime)?;
    let database_path = runtime.join("pos.db");

    let before = create_v8_fixture(&before_path)?;
    fs::write(
        root.join("before-v8-report.json"),
        serde_json::to_vec_pretty(&before)?,
    )?;
    fs::copy(&before_path, &database_path)?;

    let connection = open_database_path(&runtime, &database_path)?;
    assert_v13_data(&connection, &before)?;
    let backup_files = matching_files(&runtime, "pos-v8-before-v13-");
    let completed_files = matching_files(&runtime, "pos-schema-v13-migration-completed-");
    assert_eq!(backup_files.len(), 1);
    assert_eq!(completed_files.len(), 1);
    assert!(!runtime.join(PENDING_MARKER_NAME).exists());
    let backup = Connection::open_with_flags(&backup_files[0], OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&backup, 8, true, &backup_files[0])?;
    assert_eq!(summary(&backup)?, before["summary"]);
    let v8_snapshot = migration_snapshot(&backup, &backup_files[0], 8)?;
    drop(backup);
    let manifest: MigrationManifest = serde_json::from_slice(&fs::read(&completed_files[0])?)?;
    assert_eq!(manifest.source_version, 8);
    assert_eq!(manifest.target_version, 13);
    assert_eq!(manifest.backup_path, backup_files[0]);
    assert!(manifest.created_at_unix_seconds > 0);
    assert_eq!(
        completed_files[0],
        runtime.join(format!(
            "pos-schema-v13-migration-completed-{}.json",
            manifest.attempt_id
        ))
    );
    drop(connection);

    let backup_bytes = fs::read(&backup_files[0])?;
    let manifest_bytes = fs::read(&completed_files[0])?;
    let reopened = open_database_path(&runtime, &database_path)?;
    assert_v13_data(&reopened, &before)?;
    assert_eq!(
        migration_snapshot(&reopened, &database_path, 8)?,
        v8_snapshot
    );
    assert_eq!(matching_files(&runtime, "pos-v8-before-v13-").len(), 1);
    assert_eq!(
        matching_files(&runtime, "pos-schema-v13-migration-completed-").len(),
        1
    );
    assert!(!runtime.join(PENDING_MARKER_NAME).exists());
    assert!(!runtime.join(RESTORE_MARKER_NAME).exists());
    assert_eq!(fs::read(&backup_files[0])?, backup_bytes);
    assert_eq!(fs::read(&completed_files[0])?, manifest_bytes);
    let v8_after_integrity = integrity(&reopened)?;
    let v8_after_summary = summary(&reopened)?;
    drop(reopened);

    let v8_restore_runtime = root.join("runtime-v8-restore");
    fs::create_dir(&v8_restore_runtime)?;
    let v8_restore_database = v8_restore_runtime.join("pos.db");
    fs::copy(&database_path, &v8_restore_database)?;
    let v8_restored =
        restore_migration_backup(&v8_restore_runtime, &v8_restore_database, &backup_files[0])?;
    assert_eq!(
        migration_snapshot(&v8_restored, &v8_restore_database, 8)?,
        v8_snapshot
    );
    assert_eq!(summary(&v8_restored)?, before["summary"]);
    assert_eq!(
        integrity(&v8_restored)?,
        json!({"user_version":8,"quick_check":"ok","foreign_key_violations":0})
    );
    drop(v8_restored);
    let v8_restore_completed =
        matching_files(&v8_restore_runtime, "pos-schema-v13-restore-completed-");
    assert_eq!(v8_restore_completed.len(), 1);
    assert!(!v8_restore_runtime.join(RESTORE_MARKER_NAME).exists());
    assert!(!v8_restore_runtime.join(PENDING_MARKER_NAME).exists());
    let v8_restore_manifest: RestoreManifest =
        serde_json::from_slice(&fs::read(&v8_restore_completed[0])?)?;
    assert_eq!(v8_restore_manifest.backup_path, backup_files[0]);
    assert_eq!(v8_restore_manifest.database_path, v8_restore_database);
    assert!(v8_restore_manifest
        .quarantined_database
        .as_ref()
        .is_some_and(|path| path.is_file()));
    assert_eq!(fs::read(&backup_files[0])?, backup_bytes);
    let v8_restored_reopened =
        Connection::open_with_flags(&v8_restore_database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&v8_restored_reopened, 8, true, &v8_restore_database)?;
    assert_eq!(
        migration_snapshot(&v8_restored_reopened, &v8_restore_database, 8)?,
        v8_snapshot
    );
    assert_eq!(summary(&v8_restored_reopened)?, before["summary"]);
    drop(v8_restored_reopened);

    let v8_result = json!({
        "status": "passed",
        "source_commit": "e2ab2d2",
        "migration_sha256": V8_MIGRATIONS.iter().chain(IMMUTABLE_V9_V12_MIGRATIONS.iter()).map(|(name, _, hash)| json!({"file":name,"sha256":hash})).collect::<Vec<_>>(),
        "credentials": {"email":"admin@pos.local","password":"PosDemo123!"},
        "database": database_path,
        "backup": backup_files[0],
        "completed_manifest": completed_files[0],
        "restore_runtime": v8_restore_runtime,
        "restore_completed_manifest": v8_restore_completed[0],
        "pending_marker_present": false,
        "backup_count_after_second_open": 1,
        "after": {"integrity": v8_after_integrity, "summary": v8_after_summary},
        "verified": [
            "v8 source identity", "v8 preflight", "productive open_database_path",
            "v13 integrity", "validated v8 backup", "completed manifest", "no pending marker",
            "idempotent second open", "credentials", "legacy values", "sequence holes", "001-012 migration hashes", "v9-v13 backfills",
            "complete v8 restore snapshot", "durable v8 restore manifest", "v8 restore reopen", "preserved v8 backup"
        ]
    });
    fs::write(
        root.join("result-v8-v13.json"),
        serde_json::to_vec_pretty(&v8_result)?,
    )?;

    let v12_before_path = root.join("before-v12.db");
    create_v12_database(&v12_before_path);
    let v12_before = Connection::open(&v12_before_path)?;
    let v12_snapshot = migration_snapshot(&v12_before, &v12_before_path, 12)?;
    let v12_financial_before = query_rows(
        &v12_before,
        "SELECT (SELECT group_concat(id||':'||quote(inventory_cost_cents),'|') FROM products ORDER BY id),(SELECT group_concat(id||':'||quote(movement_cost_cents),'|') FROM sale_details ORDER BY id),(SELECT group_concat(id||':'||quote(unit_price_cents)||':'||quote(movement_cost_cents)||':'||quote(inventory_cost_before_cents)||':'||quote(inventory_cost_after_cents),'|') FROM inventory_movements ORDER BY id)",
        3,
    )?;
    drop(v12_before);

    let v12_runtime = root.join("runtime-v12-v13");
    fs::create_dir(&v12_runtime)?;
    let v12_database_path = v12_runtime.join("pos.db");
    fs::copy(&v12_before_path, &v12_database_path)?;
    let v12_after = open_database_path(&v12_runtime, &v12_database_path)?;
    assert_eq!(
        migration_snapshot(&v12_after, &v12_database_path, 12)?,
        v12_snapshot
    );
    validate_database(&v12_after, 13, true, &v12_database_path)?;
    assert_v13_unit_domains(&v12_after)?;
    let v12_financial_after = query_rows(
        &v12_after,
        "SELECT (SELECT group_concat(id||':'||quote(inventory_cost_cents),'|') FROM products ORDER BY id),(SELECT group_concat(id||':'||quote(movement_cost_cents),'|') FROM sale_details ORDER BY id),(SELECT group_concat(id||':'||quote(unit_price_cents)||':'||quote(movement_cost_cents)||':'||quote(inventory_cost_before_cents)||':'||quote(inventory_cost_after_cents),'|') FROM inventory_movements ORDER BY id)",
        3,
    )?;
    assert_eq!(v12_financial_after, v12_financial_before);
    let v12_backups = matching_files(&v12_runtime, "pos-v12-before-v13-");
    let v12_manifests = matching_files(&v12_runtime, "pos-schema-v13-migration-completed-");
    assert_eq!(v12_backups.len(), 1);
    assert_eq!(v12_manifests.len(), 1);
    let v12_backup =
        Connection::open_with_flags(&v12_backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&v12_backup, 12, true, &v12_backups[0])?;
    assert_eq!(
        migration_snapshot(&v12_backup, &v12_backups[0], 12)?,
        v12_snapshot
    );
    drop(v12_backup);
    let v12_manifest: MigrationManifest = serde_json::from_slice(&fs::read(&v12_manifests[0])?)?;
    assert_eq!(
        (v12_manifest.source_version, v12_manifest.target_version),
        (12, 13)
    );
    assert_eq!(v12_manifest.backup_path, v12_backups[0]);
    assert!(v12_manifest.created_at_unix_seconds > 0);
    assert_eq!(
        v12_manifests[0],
        v12_runtime.join(format!(
            "pos-schema-v13-migration-completed-{}.json",
            v12_manifest.attempt_id
        ))
    );
    drop(v12_after);

    let v12_backup_bytes = fs::read(&v12_backups[0])?;
    let v12_manifest_bytes = fs::read(&v12_manifests[0])?;
    let v12_reopened = open_database_path(&v12_runtime, &v12_database_path)?;
    assert_eq!(
        migration_snapshot(&v12_reopened, &v12_database_path, 12)?,
        v12_snapshot
    );
    validate_database(&v12_reopened, 13, true, &v12_database_path)?;
    assert_v13_unit_domains(&v12_reopened)?;
    assert_eq!(
        query_rows(
            &v12_reopened,
            "SELECT (SELECT group_concat(id||':'||quote(inventory_cost_cents),'|') FROM products ORDER BY id),(SELECT group_concat(id||':'||quote(movement_cost_cents),'|') FROM sale_details ORDER BY id),(SELECT group_concat(id||':'||quote(unit_price_cents)||':'||quote(movement_cost_cents)||':'||quote(inventory_cost_before_cents)||':'||quote(inventory_cost_after_cents),'|') FROM inventory_movements ORDER BY id)",
            3,
        )?,
        v12_financial_before
    );
    assert_eq!(matching_files(&v12_runtime, "pos-v12-before-v13-").len(), 1);
    assert_eq!(
        matching_files(&v12_runtime, "pos-schema-v13-migration-completed-").len(),
        1
    );
    assert!(!v12_runtime.join(PENDING_MARKER_NAME).exists());
    assert!(!v12_runtime.join(RESTORE_MARKER_NAME).exists());
    assert_eq!(fs::read(&v12_backups[0])?, v12_backup_bytes);
    assert_eq!(fs::read(&v12_manifests[0])?, v12_manifest_bytes);
    let v12_integrity = integrity(&v12_reopened)?;
    drop(v12_reopened);

    let restore_runtime = root.join("runtime-v12-restore");
    fs::create_dir(&restore_runtime)?;
    let restore_database = restore_runtime.join("pos.db");
    fs::copy(&v12_database_path, &restore_database)?;
    let restored = restore_migration_backup(&restore_runtime, &restore_database, &v12_backups[0])?;
    assert_eq!(
        migration_snapshot(&restored, &restore_database, 12)?,
        v12_snapshot
    );
    assert_eq!(
        integrity(&restored)?,
        json!({"user_version":12,"quick_check":"ok","foreign_key_violations":0})
    );
    drop(restored);
    let restore_completed = matching_files(&restore_runtime, "pos-schema-v13-restore-completed-");
    assert_eq!(restore_completed.len(), 1);
    assert!(!restore_runtime.join(RESTORE_MARKER_NAME).exists());
    assert!(!restore_runtime.join(PENDING_MARKER_NAME).exists());
    let restore_manifest: RestoreManifest =
        serde_json::from_slice(&fs::read(&restore_completed[0])?)?;
    assert_eq!(restore_manifest.backup_path, v12_backups[0]);
    assert_eq!(restore_manifest.database_path, restore_database);
    assert_eq!(
        restore_completed[0],
        restore_runtime.join(format!(
            "pos-schema-v13-restore-completed-{}.json",
            restore_manifest.restore_id
        ))
    );
    assert!(!restore_manifest.temporary_path.exists());
    assert!(!restore_manifest.failed_database.exists());
    let quarantined_database = restore_manifest
        .quarantined_database
        .as_ref()
        .expect("the migrated destination must be quarantined");
    assert!(quarantined_database.is_file());
    let quarantined =
        Connection::open_with_flags(quarantined_database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&quarantined, 13, true, quarantined_database)?;
    assert_eq!(
        migration_snapshot(&quarantined, quarantined_database, 12)?,
        v12_snapshot
    );
    drop(quarantined);
    assert_eq!(fs::read(&v12_backups[0])?, v12_backup_bytes);

    let restored_reopened =
        Connection::open_with_flags(&restore_database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&restored_reopened, 12, true, &restore_database)?;
    assert_eq!(
        migration_snapshot(&restored_reopened, &restore_database, 12)?,
        v12_snapshot
    );
    assert_eq!(
        integrity(&restored_reopened)?,
        json!({"user_version":12,"quick_check":"ok","foreign_key_violations":0})
    );
    drop(restored_reopened);
    assert_intermediate_versions_rejected(&root)?;

    let v12_result = json!({
        "status": "passed",
        "route": "v12->v13",
        "database": v12_database_path,
        "backup": v12_backups[0],
        "completed_manifest": v12_manifests[0],
        "financial_values_before": v12_financial_before,
        "financial_values_after": v12_financial_after,
        "integrity": v12_integrity,
        "restore_runtime": restore_runtime,
        "restore_completed_manifest": restore_completed[0],
        "backup_count_after_second_open": 1,
        "verified": ["productive open_database_path", "canonical v12 preflight", "literal retained-field snapshot", "known and unknown cost preservation", "correction-ready purchase/adjustment/sale/valuation chronology", "centimeter in all persisted unit domains", "unknown unit rejection", "intermediate version rejection", "validated v12 backup", "completed migration manifest", "no pending marker", "idempotent second open", "complete v12 restore snapshot", "durable v12 restore manifest", "v12 restore reopen", "verified quarantine", "preserved v12 backup"]
    });
    fs::write(
        root.join("result-v12-v13.json"),
        serde_json::to_vec_pretty(&v12_result)?,
    )?;
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&json!({
            "status": "passed",
            "routes": {"v8->v13": v8_result, "v12->v13": v12_result}
        }))?,
    )?;
    fs::write(
        root.join("README.txt"),
        format!(
            "Linea POS migration QA: PASSED\n\nRoute v8 -> v13\nSource: main e2ab2d2 migrations 001-008\nBefore: {}\nRuntime: {}\nBackup: {}\nManifest: {}\nRestore runtime: {}\nReport: {}\nCredentials: admin@pos.local / PosDemo123!\n\nRoute v12 -> v13\nBefore: {}\nRuntime: {}\nBackup: {}\nManifest: {}\nRestore runtime: {}\nReport: {}\nCombined report: {}\n",
            before_path.display(),
            database_path.display(),
            backup_files[0].display(),
            completed_files[0].display(),
            v8_restore_runtime.display(),
            root.join("result-v8-v13.json").display(),
            v12_before_path.display(),
            v12_database_path.display(),
            v12_backups[0].display(),
            v12_manifests[0].display(),
            restore_runtime.display(),
            root.join("result-v12-v13.json").display(),
            root.join("result.json").display(),
        ),
    )?;
    println!("QA_ARTIFACT_DIR={}", root.display());
    Ok(())
}
