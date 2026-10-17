use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

const MIGRATIONS_SLICE: &[M<'_>] = &[
    M::up(include_str!("../../migrations/001_create_users.sql")),
    M::up(include_str!("../../migrations/002_create_brands.sql")),
    M::up(include_str!("../../migrations/003_create_categories.sql")),
    M::up(include_str!("../../migrations/004_create_products.sql")),
    M::up(include_str!("../../migrations/005_create_sales.sql")),
    M::up(include_str!("../../migrations/006_create_sale_details.sql")),
    M::up(include_str!(
        "../../migrations/007_create_inventory_movements.sql"
    )),
    M::up(include_str!("../../migrations/008_create_indexes.sql")),
    M::up(include_str!(
        "../../migrations/009_create_product_presentations.sql"
    )),
    M::up(include_str!(
        "../../migrations/010_rebuild_sale_details.sql"
    )),
    M::up(include_str!(
        "../../migrations/011_rebuild_inventory_movements.sql"
    )),
    M::up(include_str!(
        "../../migrations/012_add_sale_pricing_and_discounts.sql"
    )),
    M::up(include_str!(
        "../../migrations/013_measured_presentations_centimeters.sql"
    )),
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

pub fn run(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_latest(connection)
}

pub(crate) fn run_to_v12(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_version(connection, 12)
}

#[cfg(test)]
pub(crate) fn run_to_v13(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_version(connection, 13)
}

#[cfg(test)]
pub(crate) fn run_to_version(
    connection: &mut Connection,
    version: usize,
) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_version(connection, version)
}

#[cfg(test)]
mod tests {
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
    use rusqlite::{params, Connection};

    fn version(connection: &Connection) -> i64 {
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    fn create_v8_fixture(connection: &mut Connection) {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::MIGRATIONS.to_version(connection, 8).unwrap();
        connection
            .execute_batch(
                "INSERT INTO users (id, first_name, last_name, email, password_hash, created_at, updated_at)
                 VALUES (1, 'Ada', 'Lovelace', 'ada@pos.local', 'hash', '2025-01-01 08:00:00', '2025-01-01 08:00:00');
                 INSERT INTO brands (id, name, name_search_key) VALUES (1, 'Marca', 'marca');
                 INSERT INTO categories (id, name, name_search_key) VALUES (1, 'Categoria', 'categoria');
                 INSERT INTO products
                    (id, name, name_search_key, price, stock, sku, sku_search_key,
                     brand_id, category_id, created_at, updated_at)
                 VALUES
                    (11, 'Nombre actual', 'nombre actual', 250, 25, 'SKU-ACTUAL', 'sku-actual',
                     1, 1, '2025-01-02 09:00:00', '2025-02-02 09:00:00'),
                    (12, 'Sin stock', 'sin stock', 100, 0, 'SKU-CERO', 'sku-cero',
                     1, 1, '2025-01-03 09:00:00', '2025-01-03 09:00:00');
                 INSERT INTO sales
                    (id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                     sale_date, total, payment_method, created_at, updated_at)
                 VALUES
                    (21, 1, 'Ada Histórica', 'ada@pos.local', 'checkout-known', 'cart-v1|11:2',
                     '2025-01-10 10:00:00', 400, 'tarjeta',
                     '2025-01-10 10:00:01', '2025-01-10 10:00:02');
                 INSERT INTO sale_details
                    (id, product_id, sale_id, product_name, sku, quantity, unit_price,
                     created_at, updated_at)
                 VALUES
                    (31, 11, 21, 'Nombre histórico', 'SKU-HIST', 2, 200,
                     '2025-01-10 10:00:03', '2025-01-10 10:00:04');
                 INSERT INTO inventory_movements
                    (id, product_id, product_name, sku, product_search_key, sku_search_key,
                     sale_id, origin, reason, type, quantity, operation_token,
                     intent_fingerprint, created_at, updated_at)
                 VALUES
                    (41, 11, 'Nombre histórico', 'SKU-HIST', 'nombre historico', 'sku-hist',
                     21, 'sale', 'venta', 'salida', 2, NULL, NULL,
                     '2025-01-10 10:00:05', '2025-01-10 10:00:06'),
                    (42, 11, 'Manual histórico', 'SKU-MANUAL', 'manual historico', 'sku-manual',
                     NULL, 'manual', 'ajuste', 'entrada', 3, 'manual-known', 'fingerprint-known',
                     '2025-01-11 11:00:00', '2025-01-11 11:00:01'),
                    (43, 11, 'Inicial histórico', 'SKU-INICIAL', 'inicial historico', 'sku-inicial',
                     NULL, 'initial_stock', 'ajuste', 'entrada', 30, NULL, NULL,
                     '2025-01-02 09:00:01', '2025-01-02 09:00:02');",
            )
            .unwrap();
    }

    #[test]
    fn applies_all_migrations() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();

        super::run(&mut connection).unwrap();

        let version = version(&connection);
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN ('users', 'brands', 'categories', 'products', 'product_presentations',
                                'sales', 'sale_details', 'inventory_movements')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 13);
        assert_eq!(table_count, 8);
    }

    #[test]
    fn v13_adds_centimeter_without_replacing_cost_columns() {
        let mut connection = Connection::open_in_memory().unwrap();
        super::run(&mut connection).unwrap();

        connection.execute_batch(
            "INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
             INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
             INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
             VALUES('Cable','cable',100,0,'CABLE','cable',1,1,'centimeter',0);
             INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)
             VALUES(1,'Metro','measured',100,100,1);
             INSERT INTO users(first_name,last_name,email,password_hash)
             VALUES('Ada','Lovelace','ada@pos.local','hash');
             INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method)
             VALUES(1,'Ada Lovelace','ada@pos.local','checkout','cart',CURRENT_TIMESTAMP,100,'none',0,0,100,'tarjeta');
             INSERT INTO sale_details(product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,unit_price,entered_quantity,base_quantity,line_total)
             VALUES(1,1,2,1,'Cable','CABLE','Metro','centimeter','measured',100,1,100,1,1,1);
             INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,record_version,base_unit,origin,reason,type,quantity)
             VALUES(1,'Cable','CABLE','cable','cable',1,'centimeter','initial_stock','ajuste','entrada',1);",
        ).unwrap();
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
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                1
            );
        }
        for statement in [
            "UPDATE products SET base_unit='unknown' WHERE id=1",
            "UPDATE sale_details SET base_unit='unknown' WHERE id=1",
            "UPDATE inventory_movements SET base_unit='unknown' WHERE id=1",
        ] {
            assert!(connection.execute(statement, []).is_err(), "{statement}");
        }
    }

    #[test]
    fn inventory_v2_enforces_price_search_key_and_fingerprint_constraints() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();

        let insert = |unit_price: Option<i64>,
                      supplier: Option<&str>,
                      provider_key: Option<&str>,
                      fingerprint: &str| {
            connection.execute(
                "INSERT INTO inventory_movements
                    (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                     unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                     stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                     inventory_cost_after_cents,supplier,provider_search_key,operation_token,
                     intent_fingerprint)
                 VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,?1,'manual','compra',
                        'entrada',1,1,1,25,26,?1,NULL,NULL,?2,?3,?4,?5)",
                params![
                    unit_price,
                    supplier,
                    provider_key,
                    format!("token-{}", fingerprint.len()),
                    fingerprint
                ],
            )
        };

        assert!(insert(None, None, None, "missing-price").is_err());
        assert!(insert(Some(100), Some("Proveedor"), None, "missing-key").is_err());
        assert!(insert(Some(100), None, Some("proveedor"), "orphan-key").is_err());
        assert!(insert(Some(100), None, None, &"x".repeat(2049)).is_err());
        assert!(insert(Some(100), Some("Proveedor"), Some("proveedor"), "valid").is_ok());

        assert!(connection.execute(
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                 stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                 inventory_cost_after_cents,operation_token,intent_fingerprint)
             VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,100,'manual','ajuste',
                    'entrada',1,1,1,25,25,100,500,600,'bad-entry-transition','bad-entry-transition')",
            [],
        ).is_err());
        assert!(connection
            .execute(
                "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                 stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                 inventory_cost_after_cents,operation_token,intent_fingerprint)
             VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,100,'manual','ajuste',
                    'salida',1,1,1,25,25,100,500,400,'bad-exit-transition','bad-exit-transition')",
                [],
            )
            .is_err());
        assert!(connection.execute(
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,
                 movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,
                 operation_token,intent_fingerprint)
             VALUES(12,'Sin stock','SKU-EMPTY','sin stock','sku-empty',2,'manual','valoracion',
                    'valoracion',0,0,0,0,0,1,0,1,'positive-empty','positive-empty')",
            [],
        ).is_err());
        assert!(connection.execute(
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,
                 movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,
                 operation_token,intent_fingerprint)
             VALUES(12,'Sin stock','SKU-EMPTY','sin stock','sku-empty',2,'manual','valoracion',
                    'valoracion',0,0,0,0,0,0,0,0,'zero-empty','zero-empty')",
            [],
        ).is_ok());

        for statement in [
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                 stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                 inventory_cost_after_cents,operation_token,intent_fingerprint)
             VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,100,'manual','ajuste',
                    'entrada',1,2,2,25,27,200,500,700,'bad-entry-quantity','bad-entry-quantity')",
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                 stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                 inventory_cost_after_cents,operation_token,intent_fingerprint)
             VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,20,'manual','ajuste',
                    'salida',1,2,2,25,23,40,500,460,'bad-exit-quantity','bad-exit-quantity')",
            "INSERT INTO inventory_movements
                (product_id,product_name,sku,product_search_key,sku_search_key,record_version,
                 origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,
                 movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents,
                 operation_token,intent_fingerprint)
             VALUES(11,'Actual','SKU-ACTUAL','actual','sku-actual',2,'manual','valoracion',
                    'valoracion',1,0,0,25,25,500,500,500,'bad-valuation-quantity','bad-valuation-quantity')",
        ] {
            assert!(connection.execute(statement, []).is_err());
        }
    }

    #[test]
    fn migrates_v8_fixture_to_v11_without_losing_history() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);

        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();

        assert_eq!(version(&connection), 12);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );

        let products: Vec<(i64, i64, Option<i64>, String)> = connection
            .prepare("SELECT id, stock, inventory_cost_cents, base_unit FROM products ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            products,
            vec![
                (11, 25, None, "piece".into()),
                (12, 0, Some(0), "piece".into())
            ]
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM product_presentations
                     WHERE name = 'Pieza' AND mode = 'fixed' AND base_quantity = 1
                       AND is_active = 1 AND is_default = 1",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );

        let detail = connection
            .query_row(
                "SELECT id, product_id, sale_id, record_version, product_name, sku,
                        quantity, unit_price, entered_quantity, base_quantity, line_total,
                        created_at, updated_at, presentation_id
                 FROM sale_details WHERE id = 31",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, i64>(13)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(detail.0, 31);
        assert_eq!((detail.1, detail.2, detail.3), (11, 21, 1));
        assert_eq!(
            (detail.4.as_str(), detail.5.as_str()),
            ("Nombre histórico", "SKU-HIST")
        );
        assert_eq!(
            (detail.6, detail.7, detail.8, detail.9, detail.10),
            (2, 200, 2, 2, 400)
        );
        assert_eq!(
            (detail.11.as_str(), detail.12.as_str()),
            ("2025-01-10 10:00:03", "2025-01-10 10:00:04")
        );

        let sale_movement = connection
            .query_row(
                "SELECT record_version, presentation_id, sale_detail_id, product_name, sku,
                        quantity, entered_quantity, base_quantity, stock_before, stock_after,
                        movement_cost_cents, created_at, updated_at
                 FROM inventory_movements WHERE id = 41",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!((sale_movement.0, sale_movement.2), (1, 31));
        assert_eq!(sale_movement.1, detail.13);
        assert_eq!(
            (sale_movement.3.as_str(), sale_movement.4.as_str()),
            ("Nombre histórico", "SKU-HIST")
        );
        assert_eq!(
            (sale_movement.5, sale_movement.6, sale_movement.7),
            (2, 2, 2)
        );
        assert_eq!(
            (sale_movement.8, sale_movement.9, sale_movement.10),
            (None, None, None)
        );
        assert_eq!(
            (sale_movement.11.as_str(), sale_movement.12.as_str()),
            ("2025-01-10 10:00:05", "2025-01-10 10:00:06")
        );

        let manual: (Option<String>, Option<String>, String, String) = connection
            .query_row(
                "SELECT operation_token, intent_fingerprint, created_at, updated_at
                 FROM inventory_movements WHERE id = 42",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(manual.0.as_deref(), Some("manual-known"));
        assert_eq!(manual.1.as_deref(), Some("fingerprint-known"));
        assert_eq!(
            (manual.2.as_str(), manual.3.as_str()),
            ("2025-01-11 11:00:00", "2025-01-11 11:00:01")
        );
        assert_eq!(
            connection
                .query_row("SELECT total FROM sales WHERE id = 21", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            400
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            3
        );
        let initial_movement: (i64, String, String, i64, i64, i64) = connection
            .query_row(
                "SELECT id, product_name, sku, quantity, entered_quantity, base_quantity
                 FROM inventory_movements WHERE origin = 'initial_stock'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            initial_movement,
            (
                43,
                "Inicial histórico".into(),
                "SKU-INICIAL".into(),
                30,
                30,
                30
            )
        );

        connection
            .execute(
                "INSERT INTO product_presentations
                    (product_id, name, mode, base_quantity, price, barcode, barcode_search_key)
                 VALUES (11, 'Bolsa 10', 'fixed', 10, 180, 'BOLSA-10', 'bolsa-10')",
                [],
            )
            .unwrap();
        let second_presentation = connection.last_insert_rowid();
        connection
            .execute(
                "INSERT INTO sale_details
                    (product_id, sale_id, record_version, presentation_id, product_name, sku,
                     presentation_name, base_unit, presentation_mode, presentation_base_quantity,
                     quantity, unit_price, entered_quantity, base_quantity, line_total)
                 VALUES (11, 21, 2, ?1, 'Nombre actual', 'SKU-ACTUAL', 'Bolsa 10',
                         'piece', 'fixed', 10, 1, 180, 1, 10, 180)",
                [second_presentation],
            )
            .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(DISTINCT presentation_id) FROM sale_details
                     WHERE sale_id = 21 AND product_id = 11",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        assert!(connection
            .execute(
                "UPDATE product_presentations SET is_default = 1 WHERE id = ?1",
                [second_presentation]
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO product_presentations
                    (product_id, name, mode, base_quantity, price, barcode, barcode_search_key)
                 VALUES (12, 'Duplicada', 'fixed', 1, 100, 'otra', 'bolsa-10')",
                [],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     record_version, origin, reason, type, quantity, entered_quantity,
                     base_quantity, stock_before, stock_after, movement_cost_cents,
                     inventory_cost_before_cents, inventory_cost_after_cents,
                     operation_token, intent_fingerprint)
                 VALUES (11, 'Nombre actual', 'SKU-ACTUAL', 'nombre actual', 'sku-actual',
                         2, 'manual', 'valoracion', 'valoracion', 0, 0, 0, 25, 25,
                         500, NULL, 500, 'valuation-known', 'valuation-fingerprint')",
                [],
            )
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id = 11", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            25
        );
        assert!(connection
            .execute(
                "INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     record_version, origin, reason, type, quantity, entered_quantity,
                     base_quantity, stock_before, stock_after, movement_cost_cents,
                     inventory_cost_after_cents, operation_token, intent_fingerprint)
                 VALUES (11, 'Nombre actual', 'SKU-ACTUAL', 'nombre actual', 'sku-actual',
                         2, 'manual', 'valoracion', 'valoracion', 0, 0, 0, 25, 25,
                         -1, 500, 'valuation-negative', 'valuation-negative')",
                [],
            )
            .is_err());
    }

    #[test]
    fn rebuilds_preserve_sequences_with_deleted_highest_ids() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        connection
            .execute_batch(
                "INSERT INTO sales
                    (id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                     sale_date, total, payment_method)
                 VALUES (22, 1, 'Ada Lovelace', 'ada@pos.local', 'sequence-hole', 'sequence-hole',
                         '2025-02-01', 1, 'tarjeta');
                 INSERT INTO sale_details
                    (id, product_id, sale_id, product_name, sku, quantity, unit_price)
                 VALUES (90, 11, 22, 'Borrado', 'SKU-HOLE', 1, 1);
                 DELETE FROM sale_details WHERE id = 90;
                 INSERT INTO inventory_movements
                    (id, product_id, product_name, sku, product_search_key, sku_search_key,
                     origin, reason, type, quantity)
                 VALUES (100, 11, 'Borrado', 'SKU-HOLE', 'borrado', 'sku-hole',
                         'initial_stock', 'ajuste', 'entrada', 1);
                 DELETE FROM inventory_movements WHERE id = 100;",
            )
            .unwrap();

        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();

        connection
            .execute(
                "INSERT INTO sale_details
                    (product_id, sale_id, product_name, sku, quantity, unit_price)
                 VALUES (11, 22, 'Legado', 'SKU-LEGACY', 1, 1)",
                [],
            )
            .unwrap();
        assert_eq!(connection.last_insert_rowid(), 91);
        connection
            .execute(
                "INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     origin, reason, type, quantity)
                 VALUES (11, 'Inicial nuevo', 'SKU-NEW', 'inicial nuevo', 'sku-new',
                         'initial_stock', 'ajuste', 'entrada', 1)",
                [],
            )
            .unwrap();
        assert_eq!(connection.last_insert_rowid(), 101);
    }

    #[test]
    fn rebuilds_preserve_sequences_when_source_tables_are_empty() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        connection
            .execute_batch(
                "DELETE FROM inventory_movements;
                 DELETE FROM sale_details;",
            )
            .unwrap();

        super::run(&mut connection).unwrap();

        connection
            .execute(
                "INSERT INTO sale_details
                    (product_id, sale_id, product_name, sku, quantity, unit_price)
                 VALUES (11, 21, 'Legado vacío', 'SKU-EMPTY', 1, 1)",
                [],
            )
            .unwrap();
        assert_eq!(connection.last_insert_rowid(), 32);
        connection
            .execute(
                "INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     origin, reason, type, quantity)
                 VALUES (11, 'Inicial vacío', 'SKU-EMPTY', 'inicial vacio', 'sku-empty',
                         'initial_stock', 'ajuste', 'entrada', 1)",
                [],
            )
            .unwrap();
        assert_eq!(connection.last_insert_rowid(), 44);
    }

    #[test]
    fn v13_rebuild_preserves_historical_sequences_for_empty_tables() {
        let mut connection = Connection::open_in_memory().unwrap();
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();
        connection
            .execute_batch(
                "UPDATE sqlite_sequence SET seq=40 WHERE name='products';
                 INSERT INTO sqlite_sequence(name,seq) SELECT 'products',40
                   WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='products');
                 UPDATE sqlite_sequence SET seq=50 WHERE name='product_presentations';
                 INSERT INTO sqlite_sequence(name,seq) SELECT 'product_presentations',50
                   WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='product_presentations');
                 UPDATE sqlite_sequence SET seq=60 WHERE name='sale_details';
                 INSERT INTO sqlite_sequence(name,seq) SELECT 'sale_details',60
                   WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='sale_details');
                 UPDATE sqlite_sequence SET seq=70 WHERE name='inventory_movements';
                 INSERT INTO sqlite_sequence(name,seq) SELECT 'inventory_movements',70
                   WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name='inventory_movements');",
            )
            .unwrap();

        super::MIGRATIONS.to_version(&mut connection, 13).unwrap();

        let sequences = connection
            .prepare("SELECT name,seq FROM sqlite_sequence WHERE name IN ('products','product_presentations','sale_details','inventory_movements') ORDER BY name")
            .unwrap()
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            sequences,
            vec![
                ("inventory_movements".into(), 70),
                ("product_presentations".into(), 50),
                ("products".into(), 40),
                ("sale_details".into(), 60),
            ]
        );
    }

    #[test]
    fn v13_rebuild_preserves_sequence_presence_for_empty_tables() {
        let mut connection = Connection::open_in_memory().unwrap();
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();
        let before = connection
            .prepare("SELECT name,seq FROM sqlite_sequence ORDER BY name")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        super::MIGRATIONS.to_version(&mut connection, 13).unwrap();
        let after = connection
            .prepare("SELECT name,seq FROM sqlite_sequence ORDER BY name")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(after, before);
    }

    #[test]
    fn v13_rebuild_preserves_sequence_values_without_normalizing_to_max_id() {
        let mut connection = Connection::open_in_memory().unwrap();
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();
        connection
            .execute_batch(
                "INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
                 INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
                 INSERT INTO products(id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id)
                 VALUES(10,'Producto','producto',100,0,'SKU','sku',1,1);
                 UPDATE sqlite_sequence SET seq=3 WHERE name='products';",
            )
            .unwrap();

        super::MIGRATIONS.to_version(&mut connection, 13).unwrap();

        assert_eq!(
            connection
                .query_row(
                    "SELECT seq FROM sqlite_sequence WHERE name='products'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            3
        );
    }

    #[test]
    fn rejects_crossed_sale_line_relationships_and_accepts_legacy_details() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        super::run(&mut connection).unwrap();

        let presentation_11: i64 = connection
            .query_row(
                "SELECT id FROM product_presentations WHERE product_id = 11 AND is_default = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let presentation_12: i64 = connection
            .query_row(
                "SELECT id FROM product_presentations WHERE product_id = 12 AND is_default = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sales
                    (id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                     sale_date, subtotal, discount_type, discount_value, discount_amount, total, payment_method)
                 VALUES (22, 1, 'Ada Lovelace', 'ada@pos.local', 'crossed', 'crossed',
                          '2025-02-01', 1, 'none', 0, 0, 1, 'tarjeta')",
                [],
            )
            .unwrap();

        assert!(connection
            .execute(
                "INSERT INTO sale_details
                    (product_id, sale_id, record_version, presentation_id, product_name, sku,
                     presentation_name, base_unit, presentation_mode, presentation_base_quantity,
                     quantity, unit_price, entered_quantity, base_quantity, line_total)
                 VALUES (11, 22, 2, ?1, 'Cruzado', 'SKU-CROSS', 'Pieza', 'piece', 'fixed',
                         1, 1, 1, 1, 1, 1)",
                [presentation_12],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO sale_details
                    (product_id, sale_id, product_name, sku, quantity, unit_price)
                 VALUES (11, 22, 'Legado temporal', 'SKU-LEGACY', 1, 1)",
                [],
            )
            .unwrap();
        let legacy: (i64, Option<i64>) = connection
            .query_row(
                "SELECT record_version, presentation_id FROM sale_details WHERE id = last_insert_rowid()",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(legacy, (1, None));

        connection
            .execute(
                "INSERT INTO product_presentations
                    (product_id, name, mode, base_quantity, price)
                 VALUES (11, 'Caja', 'fixed', 2, 2)",
                [],
            )
            .unwrap();
        let other_presentation_11 = connection.last_insert_rowid();
        let insert_sale_movement = |sale_id, product_id, presentation_id| {
            connection.execute(
                "INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     sale_id, record_version, presentation_id, sale_detail_id,
                     presentation_name, base_unit, presentation_mode, presentation_base_quantity,
                     origin, reason, type, quantity, entered_quantity, base_quantity,
                     stock_before, stock_after)
                 VALUES (?1, 'Cruzado', 'SKU-CROSS', 'cruzado', 'sku-cross', ?2, 2, ?3, 31,
                         'Pieza', 'piece', 'fixed', 1, 'sale', 'venta', 'salida', 2, 2, 2, 10, 8)",
                params![product_id, sale_id, presentation_id],
            )
        };
        assert!(insert_sale_movement(22, 11, presentation_11).is_err());
        assert!(insert_sale_movement(21, 12, presentation_12).is_err());
        assert!(insert_sale_movement(21, 11, other_presentation_11).is_err());
    }

    #[test]
    fn legacy_line_total_overflow_migrates_as_null_and_history_uses_persisted_sale_total() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        super::MIGRATIONS.to_version(&mut connection, 9).unwrap();
        connection
            .execute(
                "INSERT INTO sales
                    (id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                     sale_date, total, payment_method)
                 VALUES (22, 1, 'Ada Lovelace', 'ada@pos.local', 'overflow', 'overflow',
                          '2025-02-01', 777, 'tarjeta')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO sale_details
                    (id, product_id, sale_id, product_name, sku, quantity, unit_price)
                 VALUES (32, 11, 22, 'Overflow histórico', 'SKU-HIST', ?1, 2)",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();

        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();
        assert_eq!(version(&connection), 12);
        let migrated: (i64, i64, Option<i64>) = connection
            .query_row(
                "SELECT quantity, unit_price, line_total FROM sale_details WHERE id = 32",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(migrated, (crate::services::JS_MAX_SAFE_INTEGER, 2, None));
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'sale_details_v8'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        let history = crate::services::sales::history_detail(&connection, 22).unwrap();
        assert_eq!(history.total, 777);
        assert_eq!(
            history.lines[0].quantity,
            crate::services::JS_MAX_SAFE_INTEGER
        );
        assert_eq!(history.lines[0].unit_price, 2);
        assert_eq!(history.lines[0].subtotal, None);

        let presentation_id: i64 = connection
            .query_row(
                "SELECT id FROM product_presentations WHERE product_id=11 AND is_default=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO sale_details
                    (product_id,sale_id,record_version,presentation_id,product_name,sku,
                     presentation_name,base_unit,presentation_mode,presentation_base_quantity,
                     quantity,unit_price,entered_quantity,base_quantity,line_total)
                 VALUES(11,22,2,?1,'V2','SKU-V2','Pieza','piece','fixed',1,1,1,1,1,NULL)",
                [presentation_id],
            )
            .is_err());
    }

    #[test]
    fn failed_inventory_rebuild_rolls_back_to_version_10() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        super::MIGRATIONS.to_version(&mut connection, 10).unwrap();
        connection
            .execute_batch(
                "DROP INDEX idx_inventory_movements_operation_token;
                 INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     origin, reason, type, quantity, operation_token, intent_fingerprint)
                 VALUES (11, 'Duplicado', 'SKU-DUP', 'duplicado', 'sku-dup',
                         'manual', 'ajuste', 'entrada', 1, 'manual-known', 'other');",
            )
            .unwrap();

        assert!(super::MIGRATIONS.to_version(&mut connection, 11).is_err());
        assert_eq!(version(&connection), 10);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM inventory_movements WHERE operation_token = 'manual-known'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'inventory_movements_v8'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn failure_in_pending_migrations_rolls_the_complete_v8_upgrade_back() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        connection
            .execute_batch(
                "DROP INDEX idx_inventory_movements_operation_token;
                 INSERT INTO inventory_movements
                    (product_id, product_name, sku, product_search_key, sku_search_key,
                     origin, reason, type, quantity, operation_token, intent_fingerprint)
                 VALUES (11, 'Duplicado', 'SKU-DUP', 'duplicado', 'sku-dup',
                         'manual', 'ajuste', 'entrada', 1, 'manual-known', 'other');",
            )
            .unwrap();

        assert!(super::run(&mut connection).is_err());
        assert_eq!(version(&connection), 8);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM inventory_movements WHERE operation_token='manual-known'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='product_presentations'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    fn assert_global_upgrade_rollback_when_create_table_is_denied(table_name: &'static str) {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        connection
            .authorizer(Some(move |context: AuthContext<'_>| match context.action {
                AuthAction::CreateTable { table_name: actual } if actual == table_name => {
                    Authorization::Deny
                }
                _ => Authorization::Allow,
            }))
            .unwrap();

        assert!(super::run(&mut connection).is_err());
        assert_eq!(version(&connection), 8);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='product_presentations'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT product_name FROM sale_details WHERE id=31",
                    [],
                    |row| { row.get::<_, String>(0) }
                )
                .unwrap(),
            "Nombre histórico"
        );
    }

    #[test]
    fn failure_in_migration_009_rolls_back_the_complete_upgrade() {
        assert_global_upgrade_rollback_when_create_table_is_denied("product_presentations");
    }

    #[test]
    fn failure_in_migration_012_rolls_back_the_complete_upgrade() {
        assert_global_upgrade_rollback_when_create_table_is_denied("sales");
    }

    #[test]
    fn failed_sale_detail_copy_rolls_back_migration_010_and_preserves_v8_data() {
        let mut connection = Connection::open_in_memory().unwrap();
        create_v8_fixture(&mut connection);
        super::MIGRATIONS.to_version(&mut connection, 9).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .execute("DELETE FROM products WHERE id = 11", [])
            .unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();

        assert!(super::MIGRATIONS.to_version(&mut connection, 10).is_err());
        assert_eq!(version(&connection), 9);
        assert_eq!(
            connection
                .query_row(
                    "SELECT product_name FROM sale_details WHERE id = 31",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "Nombre histórico"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'sale_details_v8'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn migrated_database_survives_close_and_reopen() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pos-tauri-migration-{}-{suffix}.sqlite3",
            std::process::id()
        ));
        {
            let mut connection = Connection::open(&path).unwrap();
            create_v8_fixture(&mut connection);
            super::run(&mut connection).unwrap();
        }
        {
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "foreign_keys", true)
                .unwrap();
            assert_eq!(version(&connection), 13);
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap(),
                0
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT product_name FROM sale_details WHERE id = 31",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                "Nombre histórico"
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn enforces_normalized_unique_keys_and_movement_coherence() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();

        connection
            .execute(
                "INSERT INTO users (first_name, last_name, email, password_hash)
                 VALUES ('Admin', 'POS', 'admin@pos.local', 'hash')",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO users (first_name, last_name, email, password_hash)
                 VALUES ('Otro', 'Usuario', 'ADMIN@POS.LOCAL', 'hash')",
                [],
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO brands (name, name_search_key) VALUES ('Straße', 'strasse')",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO brands (name, name_search_key) VALUES ('STRASSE', 'strasse')",
                [],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria')",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO categories (name, name_search_key) VALUES ('Otra', 'categoria')",
                [],
            )
            .is_err());

        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, 1, 'sku-1', 'sku-1', 1, 1)",
                [],
            )
            .is_err());
        connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, 1, 'SKU-1', 'sku-1', 1, 1)",
                [],
            )
            .unwrap();

        let over_safe_integer = crate::services::JS_MAX_SAFE_INTEGER + 1;
        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Caro', 'caro', ?1, 0, 'SKU-2', 'sku-2', 1, 1)",
                [over_safe_integer],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO brands (id, name, name_search_key) VALUES (?1, 'Fuera de rango', 'fuera de rango')",
                [over_safe_integer],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO sales (
                     user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                     sale_date, total, payment_method
                  ) VALUES (1, 'Admin POS', 'admin@pos.local', 'unsafe-total', 'cart-v1|1:1',
                            CURRENT_TIMESTAMP, ?1, 'tarjeta')",
                [over_safe_integer],
            )
            .is_err());

        assert!(connection
            .execute(
                 "INSERT INTO inventory_movements
                   (product_id, product_name, sku, product_search_key, sku_search_key,
                    origin, reason, type, quantity)
                 VALUES (1, 'Producto', 'SKU-1', 'producto', 'sku-1', 'sale', 'venta', 'salida', 1)",
                [],
            )
            .is_err());
    }

    #[test]
    fn enforces_exhaustive_inventory_movement_origins() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::MIGRATIONS.to_version(&mut connection, 12).unwrap();
        connection.execute_batch(
            "INSERT INTO users (first_name, last_name, email, password_hash)
             VALUES ('Admin', 'POS', 'admin@pos.local', 'hash');
             INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
             INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');
             INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
             VALUES ('Producto', 'producto', 100, 10, 'SKU-1', 'sku-1', 1, 1);
             INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, subtotal, discount_type, discount_value, discount_amount, total, payment_method)
             VALUES (1, 'Admin POS', 'admin@pos.local', 'sale-token', 'cart-v2', CURRENT_TIMESTAMP, 100, 'none', 0, 0, 100, 'tarjeta');"
        ).unwrap();

        let prefix = "INSERT INTO inventory_movements
            (product_id, product_name, sku, product_search_key, sku_search_key,
             sale_id, origin, reason, type, quantity, operation_token, intent_fingerprint)";
        for values in [
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', 1, 'sale', 'venta', 'salida', 1, NULL, NULL)",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'initial_stock', 'ajuste', 'entrada', 1, NULL, NULL)",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'manual', 'compra', 'entrada', 1, 'manual-1', 'fp-1')",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'manual', 'ajuste', 'salida', 1, 'manual-2', 'fp-2')",
        ] {
            connection
                .execute(&format!("{prefix} VALUES {values}"), [])
                .unwrap();
        }
        for values in [
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'sale', 'venta', 'salida', 1, NULL, NULL)",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', 1, 'initial_stock', 'ajuste', 'entrada', 1, NULL, NULL)",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'manual', 'venta', 'salida', 1, 'bad-1', 'fp')",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'manual', 'ajuste', 'entrada', 1, NULL, NULL)",
            "(1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'initial_stock', 'ajuste', 'entrada', 1, 'bad-2', 'fp')",
        ] {
            assert!(
                connection
                    .execute(&format!("{prefix} VALUES {values}"), [])
                    .is_err(),
                "accepted invalid origin combination: {values}"
            );
        }
        assert!(connection
            .execute(
                &format!(
                    "{prefix} VALUES (1, 'Producto', 'SKU-1', 'producto', 'sku-1', NULL, 'manual', 'compra', 'entrada', 1, 'manual-1', 'other')"
                ),
                [],
            )
            .is_err());
    }

    #[test]
    fn creates_operational_indexes() {
        let mut connection = Connection::open_in_memory().unwrap();
        super::run(&mut connection).unwrap();

        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index'
                   AND name IN (
                        'idx_sales_user_id',
                        'idx_sales_sale_date',
                        'idx_sales_payment_method_sale_date',
                         'idx_products_sku',
                         'idx_products_category_id',
                         'idx_products_brand_id',
                         'idx_products_name',
                         'idx_products_name_search_key',
                        'idx_products_sku_search_key',
                        'idx_brands_name_search_key',
                        'idx_categories_name_search_key',
                        'idx_inventory_movements_product_created_at',
                        'idx_inventory_movements_sale_id',
                        'idx_inventory_movements_origin',
                        'idx_inventory_movements_reason_type',
                         'idx_inventory_movements_product_search_key',
                         'idx_inventory_movements_sku_search_key',
                         'idx_inventory_movements_operation_token',
                         'idx_product_presentations_barcode_search_key',
                         'idx_product_presentations_one_default',
                         'idx_product_presentations_product_active',
                         'idx_product_presentations_identity',
                         'idx_sale_details_presentation_id',
                         'idx_sale_details_product_id',
                         'idx_sale_details_sale_id',
                         'idx_sale_details_sale_presentation_v2',
                         'idx_sale_details_line_identity',
                         'idx_inventory_movements_sale_detail_id',
                         'idx_inventory_movements_provider_search_key',
                         'idx_inventory_movements_document_reference_search_key',
                         'idx_inventory_movements_note_search_key'
                    )",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(index_count, 31);
        let explicit_index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_autoindex_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(explicit_index_count, 31);

        connection
            .execute_batch(
                "INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
                 INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
                 INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id)
                 VALUES('Uno','uno',1,0,'SKU-1','sku-compartido',1,1);",
            )
            .unwrap();
        assert!(connection.execute(
            "INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id)
             VALUES('Dos','dos',1,0,'SKU-2','sku-compartido',1,1)",
            [],
        ).is_err());
    }

    #[test]
    fn rejects_empty_oversized_and_non_integer_domain_values() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::run(&mut connection).unwrap();

        assert!(connection
            .execute(
                "INSERT INTO users (first_name, last_name, email, password_hash)
                 VALUES ('   ', 'POS', 'admin@pos.local', 'hash')",
                [],
            )
            .is_err());
        for email in ["a", "@local", "a@", "a@@local"] {
            assert!(connection
                .execute(
                    "INSERT INTO users (first_name, last_name, email, password_hash) VALUES ('Admin', 'POS', ?1, 'hash')",
                    [email],
                )
                .is_err(), "accepted {email}");
        }
        assert!(connection
            .execute(
                "INSERT INTO users (first_name, last_name, email, password_hash)
                 VALUES (?1, 'POS', 'admin@pos.local', 'hash')",
                ["\t\r\n"],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO brands (name, name_search_key) VALUES (?1, 'oversized')",
                ["a".repeat(101)],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO brands (name, name_search_key) VALUES (?1, 'whitespace')",
                ["\t\u{000b}\u{000c}"]
            )
            .is_err());

        connection
            .execute(
                "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria')",
                [],
            )
            .unwrap();
        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 1.5, 0, 'SKU-1', 'sku-1', 1, 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('   ', 'whitespace', 100, 0, 'SKU-2', 'sku-2', 1, 1)",
                [],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, 0, ?1, 'whitespace-sku', 1, 1)",
                ["\t\r\n"],
            )
            .is_err());
        assert!(connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Gratis', 'gratis', 0, 0, 'SKU-0', 'sku-0', 1, 1)",
                [],
            )
            .is_err());
    }

    #[test]
    fn accepts_the_same_local_email_for_user_and_sale_snapshot() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::run(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO users (first_name, last_name, email, password_hash)
             VALUES ('Admin', 'POS', 'owner@local', 'hash')",
                [],
            )
            .unwrap();
        connection.execute(
            "INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, subtotal, discount_type, discount_value, discount_amount, total, payment_method)
             VALUES (1, 'Admin POS', 'owner@local', 'token', 'cart-v2', CURRENT_TIMESTAMP, 1, 'none', 0, 0, 1, 'tarjeta')",
            [],
        ).unwrap();
    }
}
