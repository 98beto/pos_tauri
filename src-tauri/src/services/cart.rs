use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};

use crate::models::cart::{Cart, CartLine};

use super::{validate_id, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER};

pub type SessionCart = BTreeMap<i64, i64>;

pub fn get(connection: &Connection, items: &SessionCart) -> ServiceResult<Cart> {
    let mut lines = Vec::with_capacity(items.len());
    let mut item_count = 0_i64;
    let mut total = 0_i64;

    for (&product_id, &quantity) in items {
        validate_id(product_id)?;
        if !(1..=JS_MAX_SAFE_INTEGER).contains(&quantity) {
            return Err(amount_overflow());
        }
        let (name, sku, unit_price, stock) = product(connection, product_id)?;
        let subtotal = unit_price
            .checked_mul(quantity)
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(amount_overflow)?;
        item_count = item_count
            .checked_add(quantity)
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(amount_overflow)?;
        total = total
            .checked_add(subtotal)
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(amount_overflow)?;
        lines.push(CartLine {
            product_id,
            sku,
            name,
            unit_price,
            quantity,
            stock,
            available: stock >= quantity,
            subtotal,
        });
    }

    let cart_fingerprint = fingerprint(
        lines
            .iter()
            .map(|line| (line.product_id, line.quantity, line.unit_price)),
    );
    Ok(Cart {
        lines,
        item_count,
        subtotal: total,
        total,
        cart_fingerprint,
    })
}

pub fn add(
    connection: &Connection,
    items: &mut SessionCart,
    product_id: i64,
) -> ServiceResult<Cart> {
    change_quantity(connection, items, product_id, 1)
}

pub fn increment(
    connection: &Connection,
    items: &mut SessionCart,
    product_id: i64,
) -> ServiceResult<Cart> {
    validate_id(product_id)?;
    if !items.contains_key(&product_id) {
        return Err(ServiceError::not_found(
            "CART_ITEM_NOT_FOUND",
            "El producto no esta en el carrito",
        ));
    }
    change_quantity(connection, items, product_id, 1)
}

pub fn decrement(
    connection: &Connection,
    items: &mut SessionCart,
    product_id: i64,
) -> ServiceResult<Cart> {
    validate_id(product_id)?;
    mutate(connection, items, |candidate| {
        let quantity = candidate.get(&product_id).copied().ok_or_else(|| {
            ServiceError::not_found("CART_ITEM_NOT_FOUND", "El producto no esta en el carrito")
        })?;
        product(connection, product_id)?;
        if quantity == 1 {
            candidate.remove(&product_id);
        } else {
            candidate.insert(product_id, quantity - 1);
        }
        Ok(())
    })
}

pub fn remove(
    connection: &Connection,
    items: &mut SessionCart,
    product_id: i64,
) -> ServiceResult<Cart> {
    validate_id(product_id)?;
    mutate(connection, items, |candidate| {
        if candidate.remove(&product_id).is_none() {
            return Err(ServiceError::not_found(
                "CART_ITEM_NOT_FOUND",
                "El producto no esta en el carrito",
            ));
        }
        Ok(())
    })
}

pub fn clear(connection: &Connection, items: &mut SessionCart) -> ServiceResult<Cart> {
    mutate(connection, items, |candidate| {
        candidate.clear();
        Ok(())
    })
}

fn change_quantity(
    connection: &Connection,
    items: &mut SessionCart,
    product_id: i64,
    amount: i64,
) -> ServiceResult<Cart> {
    validate_id(product_id)?;
    let (_, _, _, stock) = product(connection, product_id)?;
    let current = items.get(&product_id).copied().unwrap_or(0);
    let quantity = current
        .checked_add(amount)
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(amount_overflow)?;
    if quantity > stock {
        return Err(ServiceError::conflict(
            "INSUFFICIENT_STOCK",
            "No hay stock suficiente para agregar otra unidad",
        ));
    }
    mutate(connection, items, |candidate| {
        candidate.insert(product_id, quantity);
        Ok(())
    })
}

fn mutate(
    connection: &Connection,
    items: &mut SessionCart,
    operation: impl FnOnce(&mut SessionCart) -> ServiceResult<()>,
) -> ServiceResult<Cart> {
    let mut candidate = items.clone();
    operation(&mut candidate)?;
    let cart = get(connection, &candidate)?;
    *items = candidate;
    Ok(cart)
}

pub fn fingerprint(items: impl IntoIterator<Item = (i64, i64, i64)>) -> String {
    let mut value = String::from("cart-v2");
    for (product_id, quantity, unit_price) in items {
        value.push('|');
        value.push_str(&product_id.to_string());
        value.push(':');
        value.push_str(&quantity.to_string());
        value.push(':');
        value.push_str(&unit_price.to_string());
    }
    value
}

fn product(connection: &Connection, product_id: i64) -> ServiceResult<(String, String, i64, i64)> {
    connection
        .query_row(
            "SELECT name, sku, price, stock FROM products WHERE id = ?1",
            [product_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "El producto no existe"))
}

fn amount_overflow() -> ServiceError {
    ServiceError::validation(
        "CART_AMOUNT_OVERFLOW",
        "Los totales del carrito exceden el limite permitido",
    )
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::db::migrations;

    fn database() -> Connection {
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
        connection.execute("INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES ('Uno', 'uno', 125, 2, 'SKU-1', 'sku-1', 1, 1)", []).unwrap();
        connection.execute("INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES ('Dos', 'dos', 250, 3, 'SKU-2', 'sku-2', 1, 1)", []).unwrap();
        connection
    }

    #[test]
    fn supports_cart_crud_and_calculates_server_totals() {
        let connection = database();
        let mut items = super::SessionCart::new();

        let cart = super::add(&connection, &mut items, 1).unwrap();
        assert_eq!(cart.item_count, 1);
        assert_eq!(cart.lines[0].sku, "SKU-1");
        assert_eq!(cart.lines[0].name, "Uno");
        assert_eq!(cart.lines[0].unit_price, 125);
        assert_eq!(cart.lines[0].stock, 2);
        assert!(cart.lines[0].available);

        super::increment(&connection, &mut items, 1).unwrap();
        let cart = super::add(&connection, &mut items, 2).unwrap();
        assert_eq!(cart.item_count, 3);
        assert_eq!(cart.subtotal, 500);
        assert_eq!(cart.total, 500);
        assert_eq!(cart.lines[0].subtotal, 250);

        super::decrement(&connection, &mut items, 1).unwrap();
        super::remove(&connection, &mut items, 2).unwrap();
        let cart = super::decrement(&connection, &mut items, 1).unwrap();
        assert!(cart.lines.is_empty());

        super::add(&connection, &mut items, 2).unwrap();
        let cart = super::clear(&connection, &mut items).unwrap();
        assert_eq!(cart.item_count, 0);
        assert_eq!(cart.total, 0);
    }

    #[test]
    fn enforces_stock_and_missing_product_and_item_limits() {
        let connection = database();
        let mut items = super::SessionCart::new();
        super::add(&connection, &mut items, 1).unwrap();
        super::increment(&connection, &mut items, 1).unwrap();

        assert_eq!(
            super::increment(&connection, &mut items, 1)
                .unwrap_err()
                .code,
            "INSUFFICIENT_STOCK"
        );
        assert_eq!(
            super::add(&connection, &mut items, 999).unwrap_err().code,
            "PRODUCT_NOT_FOUND"
        );
        assert_eq!(
            super::decrement(&connection, &mut items, 2)
                .unwrap_err()
                .code,
            "CART_ITEM_NOT_FOUND"
        );
        assert_eq!(items.get(&1), Some(&2));
    }

    #[test]
    fn reports_current_availability_without_changing_the_cart() {
        let connection = database();
        let items: super::SessionCart = [(1, 2)].into();
        connection
            .execute("UPDATE products SET stock = 1 WHERE id = 1", [])
            .unwrap();

        let cart = super::get(&connection, &items).unwrap();

        assert!(!cart.lines[0].available);
        assert_eq!(cart.lines[0].quantity, 2);
        assert_eq!(cart.lines[0].stock, 1);
    }

    #[test]
    fn rejects_total_overflow() {
        let connection = database();
        connection
            .execute(
                "UPDATE products SET price = ?1, stock = ?1 WHERE id = 1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        let items: super::SessionCart = [(1, 2)].into();

        let error = super::get(&connection, &items).unwrap_err();

        assert_eq!(error.code, "CART_AMOUNT_OVERFLOW");
    }

    #[test]
    fn failed_mutation_leaves_cart_unchanged() {
        let connection = database();
        connection
            .execute(
                "UPDATE products SET price = ?1, stock = 2 WHERE id = 1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        let mut items: super::SessionCart = [(1, 1)].into();

        let error = super::increment(&connection, &mut items, 1).unwrap_err();

        assert_eq!(error.code, "CART_AMOUNT_OVERFLOW");
        assert_eq!(items, [(1, 1)].into());
    }

    #[test]
    fn fingerprint_includes_current_unit_prices() {
        let connection = database();
        let items: super::SessionCart = [(1, 1)].into();
        let first = super::get(&connection, &items).unwrap().cart_fingerprint;
        connection
            .execute("UPDATE products SET price = 126 WHERE id = 1", [])
            .unwrap();
        let second = super::get(&connection, &items).unwrap().cart_fingerprint;

        assert_eq!(first, "cart-v2|1:1:125");
        assert_eq!(second, "cart-v2|1:1:126");
        assert_ne!(first, second);
    }
}
