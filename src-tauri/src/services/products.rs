use rusqlite::{params, Connection, OptionalExtension, Row};

use std::collections::{BTreeMap, BTreeSet};

use crate::models::product::{
    CreateProductInput, Product, ProductSelectorFilters, ProductSelectorRow, ProductView,
    ProductViewFilters, ProductViewRow, ProductViewStats, UpdateProductInput,
};

use super::{
    checked_total, normalize_persisted, normalize_search, normalize_uppercase, search_key,
    validate_id, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER,
};

const NAME_MAX_LENGTH: usize = 200;
const SKU_MAX_LENGTH: usize = 64;

fn map_product(row: &Row<'_>) -> rusqlite::Result<Product> {
    Ok(Product {
        id: row.get(0)?,
        name: row.get(1)?,
        price: row.get(2)?,
        stock: row.get(3)?,
        sku: row.get(4)?,
        brand_id: row.get(5)?,
        category_id: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn validate(name: &str, sku: &str, price: &str) -> ServiceResult<(String, String, i64)> {
    let name = normalize_persisted(name);
    let sku = normalize_uppercase(sku);
    if name.is_empty() {
        return Err(ServiceError::validation(
            "PRODUCT_NAME_REQUIRED",
            "El nombre del producto es obligatorio",
        ));
    }
    if name.chars().count() > NAME_MAX_LENGTH {
        return Err(ServiceError::validation(
            "PRODUCT_NAME_TOO_LONG",
            "El nombre del producto debe tener como maximo 200 caracteres",
        ));
    }
    if sku.is_empty() {
        return Err(ServiceError::validation(
            "SKU_REQUIRED",
            "El SKU es obligatorio",
        ));
    }
    if sku.chars().count() > SKU_MAX_LENGTH {
        return Err(ServiceError::validation(
            "SKU_TOO_LONG",
            "El SKU debe tener como maximo 64 caracteres",
        ));
    }
    Ok((name, sku, parse_price(price)?))
}

// Product prices cross the IPC boundary as decimal text and never pass through a float.
fn parse_price(value: &str) -> ServiceResult<i64> {
    let value = value.trim();
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) if !whole.is_empty() && matches!(fraction.len(), 1 | 2) => {
            (whole, fraction)
        }
        Some(_) => return Err(invalid_price()),
        None if !value.is_empty() => (value, ""),
        None => return Err(invalid_price()),
    };
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_price());
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid_price())?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| invalid_price())? * 10,
        _ => fraction.parse::<i64>().map_err(|_| invalid_price())?,
    };
    whole
        .checked_mul(100)
        .and_then(|cents| cents.checked_add(fraction))
        .filter(|price| (1..=JS_MAX_SAFE_INTEGER).contains(price))
        .ok_or_else(invalid_price)
}

fn invalid_price() -> ServiceError {
    ServiceError::validation(
        "INVALID_PRODUCT_PRICE",
        "El precio debe ser un importe decimal valido de al menos 0.01 con hasta dos decimales",
    )
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Product>> {
    let mut statement = connection.prepare(
        "SELECT id, name, price, stock, sku, brand_id, category_id, created_at, updated_at
         FROM products ORDER BY name COLLATE NOCASE",
    )?;
    let rows = statement.query_map([], map_product)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn view(connection: &Connection, filters: ProductViewFilters) -> ServiceResult<ProductView> {
    let search = normalize_search(filters.search)?;
    if let Some(id) = filters.brand_id {
        validate_id(id)?;
    }
    if let Some(id) = filters.category_id {
        validate_id(id)?;
    }
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.price, p.stock, p.sku, p.brand_id, b.name,
                p.category_id, c.name, p.created_at, p.updated_at
         FROM products p
         JOIN brands b ON b.id = p.brand_id
         JOIN categories c ON c.id = p.category_id
         WHERE (?1 IS NULL OR p.brand_id = ?1)
           AND (?2 IS NULL OR p.category_id = ?2)
         ORDER BY p.name COLLATE NOCASE, p.id",
    )?;
    let mut items = statement
        .query_map(params![filters.brand_id, filters.category_id], |row| {
            Ok(ProductViewRow {
                id: row.get(0)?,
                name: row.get(1)?,
                price: row.get(2)?,
                stock: row.get(3)?,
                sku: row.get(4)?,
                brand_id: row.get(5)?,
                brand_name: row.get(6)?,
                category_id: row.get(7)?,
                category_name: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    items.retain(|item| {
        search.is_empty()
            || search_key(&item.name).contains(&search)
            || search_key(&item.sku).contains(&search)
    });
    let mut inventory_units = 0;
    let mut inventory_value = 0;
    let mut brands = BTreeSet::new();
    for item in &items {
        inventory_units = checked_total(inventory_units, item.stock, "PRODUCT_STATS_OVERFLOW")?;
        let value = item
            .price
            .checked_mul(item.stock)
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(|| {
                ServiceError::validation(
                    "PRODUCT_STATS_OVERFLOW",
                    "El valor del inventario excede el limite permitido por la aplicacion",
                )
            })?;
        inventory_value = checked_total(inventory_value, value, "PRODUCT_STATS_OVERFLOW")?;
        brands.insert(item.brand_id);
    }
    Ok(ProductView {
        stats: ProductViewStats {
            product_count: i64::try_from(items.len()).unwrap_or(JS_MAX_SAFE_INTEGER),
            inventory_units,
            brand_count: i64::try_from(brands.len()).unwrap_or(JS_MAX_SAFE_INTEGER),
            inventory_value,
        },
        items,
    })
}

pub fn selector(
    connection: &Connection,
    cart: &BTreeMap<i64, i64>,
    filters: ProductSelectorFilters,
) -> ServiceResult<Vec<ProductSelectorRow>> {
    let search = normalize_search(filters.search)?;
    if let Some(id) = filters.category_id {
        validate_id(id)?;
    }
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.sku, p.price, p.stock, p.brand_id, b.name,
                p.category_id, c.name
         FROM products p
         JOIN brands b ON b.id = p.brand_id
         JOIN categories c ON c.id = p.category_id
         WHERE (?1 IS NULL OR p.category_id = ?1)
         ORDER BY p.name COLLATE NOCASE, p.id",
    )?;
    let mut items = statement
        .query_map(params![filters.category_id], |row| {
            let id = row.get(0)?;
            let stock: i64 = row.get(4)?;
            let cart_quantity = cart.get(&id).copied().unwrap_or(0);
            Ok(ProductSelectorRow {
                id,
                name: row.get(1)?,
                sku: row.get(2)?,
                price: row.get(3)?,
                stock,
                cart_quantity,
                available_stock: stock.saturating_sub(cart_quantity).max(0),
                brand_id: row.get(5)?,
                brand_name: row.get(6)?,
                category_id: row.get(7)?,
                category_name: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    items.retain(|item| {
        search.is_empty()
            || search_key(&item.name).contains(&search)
            || search_key(&item.sku).contains(&search)
    });
    Ok(items)
}

pub fn find_by_sku(connection: &Connection, sku: &str) -> ServiceResult<Option<Product>> {
    let sku = search_key(sku);
    connection
        .query_row(
            "SELECT id, name, price, stock, sku, brand_id, category_id, created_at, updated_at
             FROM products WHERE sku_search_key = ?1",
            [sku],
            map_product,
        )
        .optional()
        .map_err(Into::into)
}

pub fn create(connection: &mut Connection, input: CreateProductInput) -> ServiceResult<Product> {
    let (name, sku, price) = validate(&input.name, &input.sku, &input.price)?;
    let name_search_key = search_key(&name);
    let sku_search_key = search_key(&sku);
    if !(0..=JS_MAX_SAFE_INTEGER).contains(&input.stock) {
        return Err(ServiceError::validation(
            "INVALID_INITIAL_STOCK",
            "El stock esta fuera del rango permitido",
        ));
    }
    ensure_unique_sku(connection, &sku, None)?;
    ensure_catalog_references(connection, input.brand_id, input.category_id)?;

    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
         VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7)",
        params![name, name_search_key, price, sku, sku_search_key, input.brand_id, input.category_id],
    )?;
    let product_id = transaction.last_insert_rowid();

    if input.stock > 0 {
        let product_search_key = search_key(&name);
        let sku_search_key = search_key(&sku);
        transaction.execute(
            "UPDATE products SET stock = ?1 WHERE id = ?2",
            params![input.stock, product_id],
        )?;
        transaction.execute(
            "INSERT INTO inventory_movements (
                 product_id, product_name, sku, product_search_key, sku_search_key,
                 sale_id, origin, reason, type, quantity
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'initial_stock', 'ajuste', 'entrada', ?6)",
            params![
                product_id,
                name,
                sku,
                product_search_key,
                sku_search_key,
                input.stock
            ],
        )?;
    }

    let product = get(&transaction, product_id)?;
    transaction.commit()?;
    Ok(product)
}

pub fn update(
    connection: &Connection,
    id: i64,
    input: UpdateProductInput,
) -> ServiceResult<Product> {
    validate_id(id)?;
    let (name, sku, price) = validate(&input.name, &input.sku, &input.price)?;
    let name_search_key = search_key(&name);
    let sku_search_key = search_key(&sku);
    ensure_unique_sku(connection, &sku, Some(id))?;
    ensure_catalog_references(connection, input.brand_id, input.category_id)?;

    let changed = connection.execute(
        "UPDATE products
         SET name = ?1, name_search_key = ?2, price = ?3, sku = ?4, sku_search_key = ?5,
             brand_id = ?6, category_id = ?7, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?8",
        params![
            name,
            name_search_key,
            price,
            sku,
            sku_search_key,
            input.brand_id,
            input.category_id,
            id
        ],
    )?;
    if changed == 0 {
        return Err(ServiceError::not_found(
            "PRODUCT_NOT_FOUND",
            "Producto no encontrado",
        ));
    }
    get(connection, id)
}

fn ensure_unique_sku(connection: &Connection, sku: &str, id: Option<i64>) -> ServiceResult<()> {
    let key = search_key(sku);
    let exists: bool = match id {
        Some(id) => connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM products WHERE sku_search_key = ?1 AND id <> ?2)",
            params![key, id],
            |row| row.get(0),
        )?,
        None => connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM products WHERE sku_search_key = ?1)",
            [key],
            |row| row.get(0),
        )?,
    };
    if exists {
        return Err(ServiceError::conflict(
            "SKU_ALREADY_EXISTS",
            "El SKU ya existe",
        ));
    }
    Ok(())
}

fn ensure_catalog_references(
    connection: &Connection,
    brand_id: i64,
    category_id: i64,
) -> ServiceResult<()> {
    validate_id(brand_id)?;
    validate_id(category_id)?;
    let brand_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM brands WHERE id = ?1)",
        [brand_id],
        |row| row.get(0),
    )?;
    if !brand_exists {
        return Err(ServiceError::not_found(
            "BRAND_NOT_FOUND",
            "Marca no encontrada",
        ));
    }

    let category_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE id = ?1)",
        [category_id],
        |row| row.get(0),
    )?;
    if !category_exists {
        return Err(ServiceError::not_found(
            "CATEGORY_NOT_FOUND",
            "Categoria no encontrada",
        ));
    }
    Ok(())
}

fn get(connection: &Connection, id: i64) -> ServiceResult<Product> {
    connection
        .query_row(
            "SELECT id, name, price, stock, sku, brand_id, category_id, created_at, updated_at
             FROM products WHERE id = ?1",
            [id],
            map_product,
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado")
            }
            other => other.into(),
        })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::{
        db::migrations,
        models::product::{CreateProductInput, UpdateProductInput},
    };

    #[test]
    fn initial_stock_creates_an_inventory_movement() {
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

        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Producto".into(),
                price: "10.00".into(),
                stock: 4,
                sku: "SKU-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        let movement_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM inventory_movements
                 WHERE product_id = ?1 AND reason = 'ajuste' AND type = 'entrada' AND quantity = 4",
                [product.id],
                |row| row.get(0),
            )
            .unwrap();
        let snapshots: (String, String, String) = connection
            .query_row(
                "SELECT product_name, sku, origin FROM inventory_movements WHERE product_id = ?1",
                [product.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert_eq!(product.stock, 4);
        assert_eq!(movement_count, 1);
        assert_eq!(
            snapshots,
            ("Producto".into(), "SKU-1".into(), "initial_stock".into())
        );
    }

    #[test]
    fn normalizes_sku_to_uppercase_and_enforces_case_insensitive_uniqueness() {
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

        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Producto".into(),
                price: "10.00".into(),
                stock: 0,
                sku: " sku-1 ".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        let duplicate = super::create(
            &mut connection,
            CreateProductInput {
                name: "Otro".into(),
                price: "10.00".into(),
                stock: 0,
                sku: "SkU-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap_err();

        assert_eq!(product.sku, "SKU-1");
        assert_eq!(duplicate.code, "SKU_ALREADY_EXISTS");
    }

    #[test]
    fn persists_nfc_sku_and_matches_canonically_equivalent_service_inputs() {
        let mut connection = Connection::open_in_memory().unwrap();
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

        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Cafe\u{301}".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "cafe\u{301}-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        let duplicate = super::create(
            &mut connection,
            CreateProductInput {
                name: "Otro".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "CAFÉ-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap_err();
        let search = super::view(
            &connection,
            crate::models::product::ProductViewFilters {
                search: Some("cafe\u{301}".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(product.name, "Café");
        assert_eq!(product.sku, "CAFÉ-1");
        assert_eq!(duplicate.code, "SKU_ALREADY_EXISTS");
        let found = super::find_by_sku(&connection, "cafe\u{301}-1")
            .unwrap()
            .unwrap();
        assert_eq!(found.id, product.id);
        assert_eq!(found.sku, "CAFÉ-1");
        assert_eq!(search.items.len(), 1);
    }

    #[test]
    fn sku_uniqueness_uses_the_persisted_compatibility_casefold_key() {
        let mut connection = Connection::open_in_memory().unwrap();
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

        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Straße ς".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "ＡＢＣ-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        let duplicate = super::create(
            &mut connection,
            CreateProductInput {
                name: "Otro".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "ABC-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap_err();
        let key: String = connection
            .query_row(
                "SELECT sku_search_key FROM products WHERE id = ?1",
                [product.id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(product.sku, "ＡＢＣ-1");
        assert_eq!(key, "abc-1");
        assert_eq!(duplicate.code, "SKU_ALREADY_EXISTS");
        for search in ["STRASSE", "Σ"] {
            let view = super::view(
                &connection,
                crate::models::product::ProductViewFilters {
                    search: Some(search.into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(view.items.len(), 1, "search did not match: {search}");
        }
    }

    #[test]
    fn translates_missing_catalog_references_on_create_and_update() {
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

        let missing_brand = super::create(
            &mut connection,
            CreateProductInput {
                name: "Producto".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "SKU-1".into(),
                brand_id: 99,
                category_id: 1,
            },
        )
        .unwrap_err();
        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Producto".into(),
                price: "1.00".into(),
                stock: 0,
                sku: "SKU-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        let missing_category = super::update(
            &connection,
            product.id,
            UpdateProductInput {
                name: "Producto".into(),
                price: "1.00".into(),
                sku: "SKU-1".into(),
                brand_id: 1,
                category_id: 99,
            },
        )
        .unwrap_err();

        assert_eq!(missing_brand.code, "BRAND_NOT_FOUND");
        assert_eq!(missing_category.code, "CATEGORY_NOT_FOUND");
    }

    #[test]
    fn rejects_product_strings_over_their_limits() {
        let name_error = super::validate(&"a".repeat(201), "SKU", "1.00").unwrap_err();
        let sku_error = super::validate("Producto", &"a".repeat(65), "1.00").unwrap_err();
        let expanded_sku_error =
            super::validate("Producto", &format!("{}ß", "a".repeat(63)), "1.00").unwrap_err();
        let whitespace_name_error = super::validate("\u{2003}", "SKU", "1.00").unwrap_err();

        assert_eq!(name_error.code, "PRODUCT_NAME_TOO_LONG");
        assert_eq!(sku_error.code, "SKU_TOO_LONG");
        assert_eq!(expanded_sku_error.code, "SKU_TOO_LONG");
        assert_eq!(whitespace_name_error.code, "PRODUCT_NAME_REQUIRED");
    }

    #[test]
    fn parses_product_price_as_decimal_text_without_floating_point() {
        assert_eq!(super::parse_price(" 12.3 ").unwrap(), 1230);
        assert_eq!(super::parse_price("12.34").unwrap(), 1234);
        assert_eq!(
            super::parse_price("90071992547409.91").unwrap(),
            crate::services::JS_MAX_SAFE_INTEGER
        );
        for invalid in [
            "",
            "0",
            "0.00",
            ".50",
            "1.",
            "1.234",
            "-1",
            "+1",
            "1,00",
            "NaN",
            "1e2",
            "90071992547409.92",
            "92233720368547758.07",
        ] {
            assert_eq!(
                super::parse_price(invalid).unwrap_err().code,
                "INVALID_PRODUCT_PRICE",
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn creates_and_updates_product_with_decimal_price_text_without_changing_stock() {
        let mut connection = Connection::open_in_memory().unwrap();
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

        let product = super::create(
            &mut connection,
            CreateProductInput {
                name: "Producto".into(),
                price: "12.34".into(),
                stock: 7,
                sku: "sku-1".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        assert_eq!(product.price, 1234);
        assert_eq!(product.stock, 7);

        let create_error = super::create(
            &mut connection,
            CreateProductInput {
                name: "Gratis".into(),
                price: "0.00".into(),
                stock: 0,
                sku: "sku-free".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap_err();
        assert_eq!(create_error.code, "INVALID_PRODUCT_PRICE");

        let updated = super::update(
            &connection,
            product.id,
            UpdateProductInput {
                name: "Producto editado".into(),
                price: "20.5".into(),
                sku: "sku-2".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap();
        assert_eq!(updated.price, 2050);
        assert_eq!(updated.stock, 7);

        let update_error = super::update(
            &connection,
            product.id,
            UpdateProductInput {
                name: "Producto".into(),
                price: "0".into(),
                sku: "sku-2".into(),
                brand_id: 1,
                category_id: 1,
            },
        )
        .unwrap_err();
        assert_eq!(update_error.code, "INVALID_PRODUCT_PRICE");
    }

    #[test]
    fn product_view_searches_filters_resolves_names_and_calculates_stats() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO brands (name, name_search_key) VALUES ('Acme', 'acme'), ('Otra', 'otra');
                 INSERT INTO categories (name, name_search_key) VALUES ('Bebidas', 'bebidas'), ('Comida', 'comida');
                 INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES
                   ('CAFÉ Negro', 'café negro', 250, 4, 'CAF-01', 'caf-01', 1, 1),
                   ('Galleta', 'galleta', 100, 3, 'GAL-02', 'gal-02', 2, 2);",
            )
            .unwrap();

        let empty = super::view(
            &connection,
            crate::models::product::ProductViewFilters {
                search: Some("inexistente".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(empty.items.is_empty());
        assert_eq!(empty.stats.product_count, 0);
        assert_eq!(empty.stats.inventory_value, 0);

        let view = super::view(
            &connection,
            crate::models::product::ProductViewFilters {
                search: Some(" café ".into()),
                brand_id: Some(1),
                category_id: Some(1),
            },
        )
        .unwrap();
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].brand_name, "Acme");
        assert_eq!(view.items[0].category_name, "Bebidas");
        assert_eq!(view.stats.product_count, 1);
        assert_eq!(view.stats.inventory_units, 4);
        assert_eq!(view.stats.brand_count, 1);
        assert_eq!(view.stats.inventory_value, 1000);
    }

    #[test]
    fn selector_subtracts_session_cart_and_validates_inputs_and_safe_totals() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO brands (name, name_search_key) VALUES ('Marca', 'marca');
                 INSERT INTO categories (name, name_search_key) VALUES ('Categoria', 'categoria');
                 INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id)
                 VALUES ('Producto', 'producto', 100, 5, 'SKU-ABC', 'sku-abc', 1, 1);",
            )
            .unwrap();
        let cart = [(1, 3)].into();
        let rows = super::selector(
            &connection,
            &cart,
            crate::models::product::ProductSelectorFilters {
                search: Some("sku-abc".into()),
                category_id: Some(1),
            },
        )
        .unwrap();
        assert_eq!(rows[0].cart_quantity, 3);
        assert_eq!(rows[0].available_stock, 2);
        assert_eq!(
            super::view(
                &connection,
                crate::models::product::ProductViewFilters {
                    search: Some("x".repeat(101)),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
            "SEARCH_TOO_LONG"
        );

        connection
            .execute(
                "UPDATE products SET price = ?1, stock = 2 WHERE id = 1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        assert_eq!(
            super::view(&connection, Default::default())
                .unwrap_err()
                .code,
            "PRODUCT_STATS_OVERFLOW"
        );
    }
}
