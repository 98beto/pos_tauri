use super::*;
use crate::{models::user::LoginInput, services::auth};
use argon2::{password_hash::PasswordHasher, Argon2};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{env, error::Error, fs, path::Path};

const QA_DIRECTORY_PREFIX: &str = "pos-tauri-v8-v12-qa-";
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

fn assert_v8_migrations_match_e2ab2d2() {
    for (name, contents, expected) in V8_MIGRATIONS {
        assert_eq!(
            format!("{:x}", Sha256::digest(contents)),
            expected,
            "migration {name} differs from main e2ab2d2"
        );
    }
}

fn qa_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    let root = env::var_os("POS_MIGRATION_QA_DIR")
        .ok_or("POS_MIGRATION_QA_DIR is required; run pnpm qa:migration-v8-v12")?;
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

fn assert_v12_data(connection: &Connection, before: &Value) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        integrity(connection)?,
        json!({"user_version":12,"quick_check":"ok","foreign_key_violations":0})
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
#[ignore = "creates inspectable QA artifacts; run pnpm qa:migration-v8-v12"]
fn real_v8_to_v12_migration_qa() -> Result<(), Box<dyn Error>> {
    assert_v8_migrations_match_e2ab2d2();
    let root = qa_root()?;
    let before_path = root.join("before-v8.db");
    let runtime = root.join("runtime");
    fs::create_dir(&runtime)?;
    let database_path = runtime.join("pos.db");

    let before = create_v8_fixture(&before_path)?;
    fs::write(
        root.join("before-v8-report.json"),
        serde_json::to_vec_pretty(&before)?,
    )?;
    fs::copy(&before_path, &database_path)?;

    let connection = open_database_path(&runtime, &database_path)?;
    assert_v12_data(&connection, &before)?;
    let backup_files = matching_files(&runtime, "pos-v8-before-v0.2.0-");
    let completed_files = matching_files(&runtime, "pos-v0.2.0-migration-completed-");
    assert_eq!(backup_files.len(), 1);
    assert_eq!(completed_files.len(), 1);
    assert!(!runtime.join(PENDING_MARKER_NAME).exists());
    let backup = Connection::open_with_flags(&backup_files[0], OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    validate_database(&backup, 8, true, &backup_files[0])?;
    assert_eq!(summary(&backup)?, before["summary"]);
    drop(backup);
    let manifest: MigrationManifest = serde_json::from_slice(&fs::read(&completed_files[0])?)?;
    assert_eq!(manifest.source_version, 8);
    assert_eq!(manifest.target_version, 12);
    assert_eq!(manifest.backup_path, backup_files[0]);
    drop(connection);

    let backup_bytes = fs::read(&backup_files[0])?;
    let manifest_bytes = fs::read(&completed_files[0])?;
    let reopened = open_database_path(&runtime, &database_path)?;
    assert_v12_data(&reopened, &before)?;
    assert_eq!(matching_files(&runtime, "pos-v8-before-v0.2.0-").len(), 1);
    assert_eq!(
        matching_files(&runtime, "pos-v0.2.0-migration-completed-").len(),
        1
    );
    assert!(!runtime.join(PENDING_MARKER_NAME).exists());
    assert!(!runtime.join(RESTORE_MARKER_NAME).exists());
    assert_eq!(fs::read(&backup_files[0])?, backup_bytes);
    assert_eq!(fs::read(&completed_files[0])?, manifest_bytes);

    let result = json!({
        "status": "passed",
        "source_commit": "e2ab2d2",
        "migration_sha256": V8_MIGRATIONS.iter().map(|(name, _, hash)| json!({"file":name,"sha256":hash})).collect::<Vec<_>>(),
        "credentials": {"email":"admin@pos.local","password":"PosDemo123!"},
        "database": database_path,
        "backup": backup_files[0],
        "completed_manifest": completed_files[0],
        "pending_marker_present": false,
        "backup_count_after_second_open": 1,
        "after": {"integrity": integrity(&reopened)?, "summary": summary(&reopened)?},
        "verified": [
            "v8 source identity", "v8 preflight", "productive open_database_path",
            "v12 integrity", "validated v8 backup", "completed manifest", "no pending marker",
            "idempotent second open", "credentials", "legacy values", "sequence holes", "v9-v12 backfills"
        ]
    });
    fs::write(
        root.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    fs::write(
        root.join("README.txt"),
        format!(
            "Linea POS v8 -> v12 migration QA: PASSED\n\nSource: main e2ab2d2 migrations 001-008\nBefore: {}\nRuntime: {}\nBackup: {}\nManifest: {}\nReport: {}\nCredentials: admin@pos.local / PosDemo123!\n",
            before_path.display(),
            database_path.display(),
            backup_files[0].display(),
            completed_files[0].display(),
            root.join("result.json").display(),
        ),
    )?;
    println!("QA_ARTIFACT_DIR={}", root.display());
    Ok(())
}
