use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::{
        cart::{Cart, CashChangeQuote, CheckoutCartInput},
        sale::SaleReceipt,
    },
    services::{cart, sales, ServiceResult},
};

use super::AuthSession;

fn with_cart<T>(
    database: &State<'_, Database>,
    session: &State<'_, AuthSession>,
    operation: impl FnOnce(&mut rusqlite::Connection, i64, &mut cart::SessionCart) -> ServiceResult<T>,
) -> Result<T, AppError> {
    // Session first is the single lock order for cart operations and serializes checkout.
    let mut state = session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo acceder al carrito"))?;
    let user_id = state.authenticated_user()?.id;
    let mut connection = database.0.lock().map_err(|_| {
        AppError::internal(
            "DATABASE_LOCK_ERROR",
            "No se pudo acceder a la base de datos",
        )
    })?;
    operation(&mut connection, user_id, &mut state.cart)
}

#[tauri::command]
pub fn get_cart(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::get(connection, items)
    })
}

#[tauri::command]
pub fn add_cart_item(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::add(connection, items, product_id)
    })
}

#[tauri::command]
pub fn increment_cart_item(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::increment(connection, items, product_id)
    })
}

#[tauri::command]
pub fn decrement_cart_item(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::decrement(connection, items, product_id)
    })
}

#[tauri::command]
pub fn remove_cart_item(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::remove(connection, items, product_id)
    })
}

#[tauri::command]
pub fn clear_cart(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Cart, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        cart::clear(connection, items)
    })
}

#[tauri::command]
pub fn quote_cash_change(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    cash_received: String,
) -> Result<CashChangeQuote, AppError> {
    with_cart(&database, &session, |connection, _, items| {
        sales::quote_cash_change(connection, items, &cash_received)
    })
}

#[tauri::command]
pub fn checkout_cart(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CheckoutCartInput,
) -> Result<SaleReceipt, AppError> {
    with_cart(&database, &session, |connection, user_id, items| {
        checkout_and_clear(connection, user_id, items, input)
    })
}

fn checkout_and_clear(
    connection: &mut rusqlite::Connection,
    user_id: i64,
    items: &mut cart::SessionCart,
    input: CheckoutCartInput,
) -> ServiceResult<SaleReceipt> {
    let receipt = sales::checkout(connection, user_id, items, input)?;
    items.clear();
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::{db::migrations, models::cart::CheckoutCartInput, services::cart::SessionCart};

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute("INSERT INTO users (first_name, last_name, email, password_hash) VALUES ('Admin', 'POS', 'admin@pos.local', 'hash')", []).unwrap();
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
        connection.execute("INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES ('Producto', 'producto', 1250, 2, 'SKU-1', 'sku-1', 1, 1)", []).unwrap();
        connection
    }

    fn input(token: &str, cash: &str) -> CheckoutCartInput {
        CheckoutCartInput {
            checkout_token: token.into(),
            cart_fingerprint: "cart-v2|1:1:1250".into(),
            payment_method: "efectivo".into(),
            cash_received: Some(cash.into()),
        }
    }

    #[test]
    fn keeps_cart_on_error_and_clears_it_only_after_success() {
        let mut connection = database();
        let mut cart: SessionCart = [(1, 1)].into();

        let error =
            super::checkout_and_clear(&mut connection, 1, &mut cart, input("failed", "1.00"))
                .unwrap_err();
        assert_eq!(error.code, "INSUFFICIENT_CASH");
        assert_eq!(cart.get(&1), Some(&1));

        super::checkout_and_clear(&mut connection, 1, &mut cart, input("successful", "20.00"))
            .unwrap();
        assert!(cart.is_empty());

        let retried =
            super::checkout_and_clear(&mut connection, 1, &mut cart, input("successful", "20.00"))
                .unwrap();
        assert_eq!(retried.sale.checkout_token, "successful");
        assert!(cart.is_empty());
    }
}
