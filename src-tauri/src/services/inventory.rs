use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

use crate::models::inventory_movement::{
    CreateInventoryMovementInput, InventoryMovement, InventoryView, InventoryViewFilters,
    InventoryViewRow, InventoryViewStats,
};

use super::{
    checked_total, normalize_search, search_key, validate_id, ServiceError, ServiceResult,
    JS_MAX_SAFE_INTEGER,
};

const OPERATION_TOKEN_MAX_LENGTH: usize = 128;

fn map_movement(row: &Row<'_>) -> rusqlite::Result<InventoryMovement> {
    Ok(InventoryMovement {
        id: row.get(0)?,
        product_id: row.get(1)?,
        product_name: row.get(2)?,
        sku: row.get(3)?,
        sale_id: row.get(4)?,
        reason: row.get(5)?,
        movement_type: row.get(6)?,
        quantity: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<InventoryMovement>> {
    let mut statement = connection.prepare(
        "SELECT id, product_id, product_name, sku, sale_id, reason, type, quantity,
                created_at, updated_at
         FROM inventory_movements ORDER BY id DESC",
    )?;
    let rows = statement.query_map([], map_movement)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn view(
    connection: &Connection,
    filters: InventoryViewFilters,
) -> ServiceResult<InventoryView> {
    let search = normalize_search(filters.search)?;
    let reason = normalize_filter(
        filters.reason,
        &["venta", "compra", "ajuste"],
        "INVALID_REASON",
    )?;
    let movement_type = normalize_filter(
        filters.movement_type,
        &["entrada", "salida"],
        "INVALID_MOVEMENT_TYPE",
    )?;
    let mut statement = connection.prepare(
        "SELECT m.id, m.product_id, m.product_name, m.sku, m.sale_id,
                 CASE WHEN m.sale_id IS NULL THEN NULL ELSE printf('POS-%08d', m.sale_id) END,
                 m.reason, m.type, m.quantity, m.created_at, m.updated_at
         FROM inventory_movements m
         WHERE (?1 IS NULL OR m.reason = ?1)
           AND (?2 IS NULL OR m.type = ?2)
           AND (?3 = ''
                OR instr(m.product_search_key, ?3) > 0
                OR instr(m.sku_search_key, ?3) > 0)
         ORDER BY m.created_at DESC, m.id DESC",
    )?;
    let items = statement
        .query_map(params![reason, movement_type, search], |row| {
            Ok(InventoryViewRow {
                id: row.get(0)?,
                product_id: row.get(1)?,
                product_name: row.get(2)?,
                sku: row.get(3)?,
                sale_id: row.get(4)?,
                ticket_reference: row.get(5)?,
                reason: row.get(6)?,
                movement_type: row.get(7)?,
                quantity: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut global_total_entries = 0;
    let mut global_total_exits = 0;
    let mut movements = connection.prepare("SELECT type, quantity FROM inventory_movements")?;
    for movement in movements.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (movement_type, quantity) = movement?;
        if movement_type == "entrada" {
            global_total_entries =
                checked_total(global_total_entries, quantity, "INVENTORY_STATS_OVERFLOW")?;
        } else {
            global_total_exits =
                checked_total(global_total_exits, quantity, "INVENTORY_STATS_OVERFLOW")?;
        }
    }

    let mut global_current_stock = 0;
    let mut global_out_of_stock_products = 0;
    let mut stocks = connection.prepare("SELECT stock FROM products ORDER BY id")?;
    for stock in stocks.query_map([], |row| row.get::<_, i64>(0))? {
        let stock = stock?;
        global_current_stock =
            checked_total(global_current_stock, stock, "INVENTORY_STATS_OVERFLOW")?;
        if stock == 0 {
            global_out_of_stock_products =
                checked_total(global_out_of_stock_products, 1, "INVENTORY_STATS_OVERFLOW")?;
        }
    }
    Ok(InventoryView {
        items,
        stats: InventoryViewStats {
            global_total_entries,
            global_total_exits,
            global_current_stock,
            global_out_of_stock_products,
        },
    })
}

fn normalize_filter(
    value: Option<String>,
    allowed: &[&str],
    code: &'static str,
) -> ServiceResult<Option<String>> {
    let value = value.map(|value| search_key(&value));
    match value {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if allowed.contains(&value.as_str()) => Ok(Some(value)),
        Some(_) => Err(ServiceError::validation(
            code,
            "El filtro seleccionado no es valido",
        )),
        None => Ok(None),
    }
}

pub fn create(
    connection: &mut Connection,
    input: CreateInventoryMovementInput,
) -> ServiceResult<InventoryMovement> {
    validate_id(input.product_id)?;
    let quantity = parse_quantity(&input.quantity)?;
    let operation_token = input.operation_token.trim();
    if operation_token.is_empty() || operation_token.chars().count() > OPERATION_TOKEN_MAX_LENGTH {
        return Err(ServiceError::validation(
            "INVALID_INVENTORY_OPERATION_TOKEN",
            "El token de operacion no es valido",
        ));
    }
    match (input.reason.as_str(), input.movement_type.as_str()) {
        ("compra", "entrada") | ("ajuste", "entrada") | ("ajuste", "salida") => {}
        ("venta", _) => {
            return Err(ServiceError::validation(
                "INVALID_SALE_MOVEMENT",
                "Los movimientos de venta solo se crean desde el servicio de ventas",
            ));
        }
        _ => {
            return Err(ServiceError::validation(
                "INVALID_MOVEMENT_TYPE",
                "Combinacion de motivo y tipo de movimiento invalida",
            ));
        }
    }
    let intent_fingerprint = format!(
        "inventory-manual-v1|{}|{}:{}|{}:{}|{}",
        input.product_id,
        input.reason.len(),
        input.reason,
        input.movement_type.len(),
        input.movement_type,
        quantity
    );

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((movement, persisted_fingerprint)) =
        movement_by_token(&transaction, operation_token)?
    {
        if persisted_fingerprint == intent_fingerprint {
            return Ok(movement);
        }
        return Err(ServiceError::conflict(
            "INVENTORY_OPERATION_TOKEN_REUSED",
            "El token de operacion ya fue usado con otra intencion",
        ));
    }

    let product: Option<(String, String, i64)> = transaction
        .query_row(
            "SELECT name, sku, stock FROM products WHERE id = ?1",
            [input.product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((product_name, sku, current_stock)) = product else {
        return Err(ServiceError::not_found(
            "PRODUCT_NOT_FOUND",
            "Producto no encontrado",
        ));
    };
    let product_search_key = search_key(&product_name);
    let sku_search_key = search_key(&sku);
    let new_stock = if input.movement_type == "entrada" {
        current_stock
            .checked_add(quantity)
            .filter(|stock| *stock <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(|| {
                ServiceError::conflict("STOCK_OVERFLOW", "El stock excede el limite permitido")
            })?
    } else {
        current_stock
            .checked_sub(quantity)
            .filter(|stock| *stock >= 0)
            .ok_or_else(|| {
                ServiceError::conflict(
                    "INSUFFICIENT_STOCK",
                    "Stock insuficiente para completar el movimiento",
                )
            })?
    };
    transaction.execute(
        "UPDATE products SET stock = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![new_stock, input.product_id],
    )?;

    transaction.execute(
        "INSERT INTO inventory_movements (
             product_id, product_name, sku, product_search_key, sku_search_key,
             sale_id, origin, reason, type, quantity, operation_token, intent_fingerprint
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'manual', ?6, ?7, ?8, ?9, ?10)",
        params![
            input.product_id,
            product_name,
            sku,
            product_search_key,
            sku_search_key,
            input.reason,
            input.movement_type,
            quantity,
            operation_token,
            intent_fingerprint
        ],
    )?;
    let movement_id = transaction.last_insert_rowid();
    let movement = transaction.query_row(
        "SELECT id, product_id, product_name, sku, sale_id, reason, type, quantity,
                created_at, updated_at
         FROM inventory_movements WHERE id = ?1",
        [movement_id],
        map_movement,
    )?;
    transaction.commit()?;
    Ok(movement)
}

fn parse_quantity(value: &str) -> ServiceResult<i64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ServiceError::validation_field(
            "INVALID_MOVEMENT_QUANTITY",
            "La cantidad debe ser un entero entre 1 y 9007199254740991",
            "quantity",
        ));
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|quantity| (1..=JS_MAX_SAFE_INTEGER).contains(quantity))
        .ok_or_else(|| {
            ServiceError::validation_field(
                "INVALID_MOVEMENT_QUANTITY",
                "La cantidad debe ser un entero entre 1 y 9007199254740991",
                "quantity",
            )
        })
}

fn movement_by_token(
    connection: &Connection,
    operation_token: &str,
) -> ServiceResult<Option<(InventoryMovement, String)>> {
    connection
        .query_row(
            "SELECT id, product_id, product_name, sku, sale_id, reason, type, quantity,
                    created_at, updated_at, intent_fingerprint
             FROM inventory_movements WHERE operation_token = ?1",
            [operation_token],
            |row| Ok((map_movement(row)?, row.get(10)?)),
        )
        .optional()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{Arc, Barrier},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use rusqlite::Connection;

    use crate::{
        db::migrations,
        models::{
            inventory_movement::{CreateInventoryMovementInput, InventoryViewFilters},
            product::{CreateProductInput, UpdateProductInput},
        },
    };

    fn movement_database(stock: i64) -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
                 INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, ?1, 'SKU-1', 'sku-1', 1, 1)",
                [stock],
            )
            .unwrap();
        connection
    }

    fn movement(reason: &str, movement_type: &str, quantity: i64) -> CreateInventoryMovementInput {
        CreateInventoryMovementInput {
            product_id: 1,
            reason: reason.into(),
            movement_type: movement_type.into(),
            quantity: quantity.to_string(),
            operation_token: format!("test-{reason}-{movement_type}-{quantity}"),
        }
    }

    fn stock_and_movement_count(connection: &Connection) -> (i64, i64) {
        (
            connection
                .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .unwrap(),
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| {
                    row.get(0)
                })
                .unwrap(),
        )
    }

    #[test]
    fn creates_each_supported_manual_movement_and_updates_stock() {
        for (reason, movement_type, quantity, expected_stock) in [
            ("compra", "entrada", 3, 8),
            ("ajuste", "entrada", 2, 7),
            ("ajuste", "salida", 4, 1),
        ] {
            let mut connection = movement_database(5);

            let created =
                super::create(&mut connection, movement(reason, movement_type, quantity)).unwrap();

            assert_eq!(created.reason, reason);
            assert_eq!(created.movement_type, movement_type);
            assert_eq!(created.quantity, quantity);
            assert_eq!(created.sale_id, None);
            assert_eq!(stock_and_movement_count(&connection), (expected_stock, 1));
        }
    }

    #[test]
    fn repeated_operation_token_is_idempotent_and_rejects_a_different_intent() {
        let mut connection = movement_database(5);
        let input = movement("compra", "entrada", 3);

        let first = super::create(&mut connection, input.clone()).unwrap();
        let repeated = super::create(&mut connection, input.clone()).unwrap();

        assert_eq!(repeated.id, first.id);
        assert_eq!(stock_and_movement_count(&connection), (8, 1));
        let fingerprint: String = connection
            .query_row(
                "SELECT intent_fingerprint FROM inventory_movements WHERE id = ?1",
                [first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fingerprint, "inventory-manual-v1|1|6:compra|7:entrada|3");

        let mut equivalent = input.clone();
        equivalent.quantity = "03".into();
        assert_eq!(
            super::create(&mut connection, equivalent).unwrap().id,
            first.id
        );

        let mut different = input;
        different.quantity = "4".into();
        assert_eq!(
            super::create(&mut connection, different).unwrap_err().code,
            "INVENTORY_OPERATION_TOKEN_REUSED"
        );
        assert_eq!(stock_and_movement_count(&connection), (8, 1));
    }

    #[test]
    fn concurrent_connections_apply_the_same_intent_once() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pos-tauri-manual-concurrency-{}-{unique}.db",
            std::process::id()
        ));
        {
            let mut connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "foreign_keys", true)
                .unwrap();
            migrations::run(&mut connection).unwrap();
            connection.execute_batch(
                "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
                 INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');
                 INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, 5, 'SKU-1', 'sku-1', 1, 1);"
            ).unwrap();
        }

        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["2", "02"]
            .into_iter()
            .map(|quantity| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let mut connection = Connection::open(path).unwrap();
                    connection.busy_timeout(Duration::from_secs(5)).unwrap();
                    barrier.wait();
                    let mut input = movement("compra", "entrada", 2);
                    input.operation_token = "concurrent-token".into();
                    input.quantity = quantity.into();
                    super::create(&mut connection, input).map(|movement| movement.id)
                })
            })
            .collect();
        let ids: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect();

        let mut connection = Connection::open(&path).unwrap();
        assert_eq!(ids[0], ids[1]);
        assert_eq!(stock_and_movement_count(&connection), (7, 1));
        let mut different = movement("compra", "entrada", 3);
        different.operation_token = "concurrent-token".into();
        assert_eq!(
            super::create(&mut connection, different).unwrap_err().code,
            "INVENTORY_OPERATION_TOKEN_REUSED"
        );
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_transaction_does_not_consume_the_operation_token() {
        let mut connection = movement_database(5);
        connection
            .execute_batch(
                "CREATE TRIGGER fail_manual_movement BEFORE INSERT ON inventory_movements
                 BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
            )
            .unwrap();
        let input = movement("compra", "entrada", 2);

        assert!(super::create(&mut connection, input.clone()).is_err());
        assert_eq!(stock_and_movement_count(&connection), (5, 0));
        connection
            .execute_batch("DROP TRIGGER fail_manual_movement")
            .unwrap();

        super::create(&mut connection, input).unwrap();
        assert_eq!(stock_and_movement_count(&connection), (7, 1));
    }

    #[test]
    fn quantity_text_is_validated_without_floating_point() {
        for value in [
            "",
            "0",
            "-1",
            "1.0",
            "+1",
            " 1",
            "9007199254740992",
            "999999999999999999999999999999999999",
        ] {
            let mut connection = movement_database(5);
            let mut input = movement("compra", "entrada", 1);
            input.quantity = value.into();
            let error = super::create(&mut connection, input).unwrap_err();
            assert_eq!(error.code, "INVALID_MOVEMENT_QUANTITY", "accepted {value}");
            assert_eq!(error.field, Some("quantity"));
            assert_eq!(stock_and_movement_count(&connection), (5, 0));
        }

        let mut connection = movement_database(0);
        let mut input = movement("compra", "entrada", 1);
        input.quantity = crate::services::JS_MAX_SAFE_INTEGER.to_string();
        super::create(&mut connection, input).unwrap();
        assert_eq!(
            stock_and_movement_count(&connection),
            (crate::services::JS_MAX_SAFE_INTEGER, 1)
        );
    }

    #[test]
    fn rejects_invalid_inputs_and_missing_products_without_changes() {
        for (input, code) in [
            (
                movement("compra", "entrada", 0),
                "INVALID_MOVEMENT_QUANTITY",
            ),
            (
                movement("compra", "entrada", -1),
                "INVALID_MOVEMENT_QUANTITY",
            ),
            (
                movement(
                    "compra",
                    "entrada",
                    crate::services::JS_MAX_SAFE_INTEGER + 1,
                ),
                "INVALID_MOVEMENT_QUANTITY",
            ),
            (movement("compra", "salida", 1), "INVALID_MOVEMENT_TYPE"),
            (movement("ajuste", "otro", 1), "INVALID_MOVEMENT_TYPE"),
            (movement("venta", "salida", 1), "INVALID_SALE_MOVEMENT"),
        ] {
            let mut connection = movement_database(5);
            assert_eq!(
                super::create(&mut connection, input).unwrap_err().code,
                code
            );
            assert_eq!(stock_and_movement_count(&connection), (5, 0));
        }

        let mut connection = movement_database(5);
        let mut input = movement("ajuste", "entrada", 1);
        input.product_id = 999;
        assert_eq!(
            super::create(&mut connection, input).unwrap_err().code,
            "PRODUCT_NOT_FOUND"
        );
        assert_eq!(stock_and_movement_count(&connection), (5, 0));
    }

    #[test]
    fn insufficient_stock_and_database_failure_are_atomic() {
        let mut connection = movement_database(5);
        assert_eq!(
            super::create(&mut connection, movement("ajuste", "salida", 6))
                .unwrap_err()
                .code,
            "INSUFFICIENT_STOCK"
        );
        assert_eq!(stock_and_movement_count(&connection), (5, 0));

        connection
            .execute_batch(
                "CREATE TRIGGER fail_manual_movement BEFORE INSERT ON inventory_movements
                 BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
            )
            .unwrap();
        assert!(super::create(&mut connection, movement("compra", "entrada", 2)).is_err());
        assert_eq!(stock_and_movement_count(&connection), (5, 0));
    }

    #[test]
    fn rejects_stock_overflow_without_creating_a_movement() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(&mut connection).unwrap();
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
        connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, ?1, 'SKU-1', 'sku-1', 1, 1)",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();

        let error = super::create(
            &mut connection,
            CreateInventoryMovementInput {
                product_id: 1,
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "1".into(),
                operation_token: "overflow-test".into(),
            },
        )
        .unwrap_err();
        let movement_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(error.code, "STOCK_OVERFLOW");
        assert_eq!(movement_count, 0);
        let stock: i64 = connection
            .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stock, crate::services::JS_MAX_SAFE_INTEGER);
    }

    #[test]
    fn inventory_view_combines_filters_resolves_tickets_and_calculates_stats() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(
            "INSERT INTO users (first_name, last_name, email, password_hash)
             VALUES ('Ana', 'Admin', 'ana@example.com', 'hash');
             INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
             INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');
             INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES
               ('Te Verde', 'te verde', 100, 3, 'TE-01', 'te-01', 1, 1),
               ('Agotado', 'agotado', 100, 0, 'OUT-01', 'out-01', 1, 1);
             INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, total, payment_method)
             VALUES (1, 'Ana Admin', 'ana@example.com', 'token', 'cart-v1|1:2', CURRENT_TIMESTAMP, 200, 'tarjeta');
              INSERT INTO inventory_movements
                (product_id, product_name, sku, product_search_key, sku_search_key, sale_id, origin, reason, type, quantity)
               VALUES (1, 'Te Verde', 'TE-01', 'te verde', 'te-01', 1, 'sale', 'venta', 'salida', 2);
              INSERT INTO inventory_movements
                (product_id, product_name, sku, product_search_key, sku_search_key, origin, reason, type, quantity, operation_token, intent_fingerprint)
               VALUES (1, 'Te Verde', 'TE-01', 'te verde', 'te-01', 'manual', 'compra', 'entrada', 5, 'view-manual', 'view-fingerprint');"
        ).unwrap();

        let all = super::view(&connection, Default::default()).unwrap();
        assert_eq!(all.stats.global_total_entries, 5);
        assert_eq!(all.stats.global_total_exits, 2);
        assert_eq!(all.stats.global_current_stock, 3);
        assert_eq!(all.stats.global_out_of_stock_products, 1);

        let filtered = super::view(
            &connection,
            crate::models::inventory_movement::InventoryViewFilters {
                search: Some(" tE-01 ".into()),
                reason: Some("VENTA".into()),
                movement_type: Some("SALIDA".into()),
            },
        )
        .unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].product_name, "Te Verde");
        assert_eq!(
            filtered.items[0].ticket_reference.as_deref(),
            Some("POS-00000001")
        );
        assert_eq!(filtered.stats.global_total_entries, 5);
        assert_eq!(filtered.stats.global_total_exits, 2);
        assert_eq!(filtered.stats.global_current_stock, 3);
        assert_eq!(filtered.stats.global_out_of_stock_products, 1);
        assert_eq!(
            super::view(
                &connection,
                crate::models::inventory_movement::InventoryViewFilters {
                    reason: Some("otro".into()),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
            "INVALID_REASON"
        );
    }

    #[test]
    fn inventory_search_is_unicode_case_insensitive_and_nfc_aware() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(&mut connection).unwrap();
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
        crate::services::products::create(
            &mut connection,
            CreateProductInput {
                name: "Straße ς Café".into(),
                price: "1.00".into(),
                stock: 1,
                sku: "ARBOL-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();

        for search in ["STRASSE", "Σ", "Cafe\u{301}"] {
            let view = super::view(
                &connection,
                InventoryViewFilters {
                    search: Some(search.into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(view.items.len(), 1, "search did not match: {search}");
            assert_eq!(view.items[0].product_name, "Straße ς Café");
        }

        let persisted_key: String = connection
            .query_row(
                "SELECT product_search_key FROM inventory_movements WHERE product_id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(persisted_key, crate::services::search_key("Straße ς Café"));
    }

    #[test]
    fn inventory_view_empty_and_max_safe_overflow_are_explicit() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        let empty = super::view(&connection, Default::default()).unwrap();
        assert!(empty.items.is_empty());
        assert_eq!(empty.stats.global_current_stock, 0);

        connection
            .execute_batch(
                "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
             INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
             VALUES ('Grande', 'grande', 1, ?1, 'BIG', 'big', 1, 1)",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
             VALUES ('Uno', 'uno', 1, 1, 'ONE', 'one', 1, 1)",
                [],
            )
            .unwrap();
        assert_eq!(
            super::view(&connection, Default::default())
                .unwrap_err()
                .code,
            "INVENTORY_STATS_OVERFLOW"
        );
    }

    #[test]
    fn initial_and_manual_movements_keep_snapshots_after_edit_and_reopen() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pos-tauri-inventory-{}-{unique}.db",
            std::process::id()
        ));
        {
            let mut connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "foreign_keys", true)
                .unwrap();
            migrations::run(&mut connection).unwrap();
            connection
                .execute(
                    "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca')",
                    [],
                )
                .unwrap();
            connection
                .execute("INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria')", [])
                .unwrap();
            let product = crate::services::products::create(
                &mut connection,
                CreateProductInput {
                    name: "Cafe\u{301}".into(),
                    price: "1.00".into(),
                    stock: 2,
                    sku: "cafe\u{301}-1".into(),
                    brand_id: 1,
                    category_id: 1,
                },
            )
            .unwrap();
            super::create(
                &mut connection,
                CreateInventoryMovementInput {
                    product_id: product.id,
                    reason: "compra".into(),
                    movement_type: "entrada".into(),
                    quantity: "3".into(),
                    operation_token: "snapshot-test".into(),
                },
            )
            .unwrap();
            crate::services::products::update(
                &connection,
                product.id,
                UpdateProductInput {
                    name: "Renombrado".into(),
                    price: "1.00".into(),
                    sku: "NEW-SKU".into(),
                    brand_id: 1,
                    category_id: 1,
                },
            )
            .unwrap();
        }

        let reopened = Connection::open(&path).unwrap();
        let view = super::view(
            &reopened,
            InventoryViewFilters {
                search: Some("cafe\u{301}-1".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(view.items.len(), 2);
        assert!(view
            .items
            .iter()
            .all(|item| item.product_name == "Café" && item.sku == "CAFÉ-1"));
        drop(reopened);
        fs::remove_file(path).unwrap();
    }
}
