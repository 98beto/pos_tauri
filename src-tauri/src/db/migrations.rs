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
];
const MIGRATIONS: Migrations<'_> = Migrations::from_slice(MIGRATIONS_SLICE);

pub fn run(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    MIGRATIONS.to_latest(connection)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn applies_all_migrations() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();

        super::run(&mut connection).unwrap();

        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table'
                   AND name IN ('users', 'brands', 'categories', 'products', 'sales',
                                'sale_details', 'inventory_movements')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 8);
        assert_eq!(table_count, 7);
    }

    #[test]
    fn enforces_normalized_unique_keys_and_movement_coherence() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        super::run(&mut connection).unwrap();

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
        super::run(&mut connection).unwrap();
        connection.execute_batch(
            "INSERT INTO users (first_name, last_name, email, password_hash)
             VALUES ('Admin', 'POS', 'admin@pos.local', 'hash');
             INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
             INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');
             INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
             VALUES ('Producto', 'producto', 100, 10, 'SKU-1', 'sku-1', 1, 1);
             INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, total, payment_method)
             VALUES (1, 'Admin POS', 'admin@pos.local', 'sale-token', 'cart-v2', CURRENT_TIMESTAMP, 100, 'tarjeta');"
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
                        'idx_inventory_movements_operation_token'
                   )",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(index_count, 15);
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
            "INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, total, payment_method)
             VALUES (1, 'Admin POS', 'owner@local', 'token', 'cart-v2', CURRENT_TIMESTAMP, 1, 'tarjeta')",
            [],
        ).unwrap();
    }
}
