use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::models::{
    cart::{CashChangeQuote, CheckoutCartInput},
    sale::{
        Sale, SaleHistoryDetail, SaleHistoryLine, SaleReceipt, SalesView, SalesViewFilters,
        SalesViewRow, SalesViewStats,
    },
    sale_detail::SaleDetail,
};

use super::{
    cart::{self, SessionCart},
    checked_total, normalize_search, search_key, validate_id, ServiceError, ServiceResult,
    JS_MAX_SAFE_INTEGER,
};

const CHECKOUT_TOKEN_MAX_LENGTH: usize = 128;

fn map_sale(row: &Row<'_>) -> rusqlite::Result<Sale> {
    Ok(Sale {
        id: row.get(0)?,
        user_id: row.get(1)?,
        cashier_name: row.get(2)?,
        cashier_email: row.get(3)?,
        checkout_token: row.get(4)?,
        cart_fingerprint: row.get(5)?,
        sale_date: row.get(6)?,
        total: row.get(7)?,
        payment_method: row.get(8)?,
        cash_received: row.get(9)?,
        change_amount: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn map_detail(row: &Row<'_>) -> rusqlite::Result<SaleDetail> {
    Ok(SaleDetail {
        id: row.get(0)?,
        product_id: row.get(1)?,
        sale_id: row.get(2)?,
        product_name: row.get(3)?,
        sku: row.get(4)?,
        quantity: row.get(5)?,
        unit_price: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Sale>> {
    let mut statement = connection.prepare(
        "SELECT id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                sale_date, total, payment_method, cash_received, change_amount, created_at, updated_at
         FROM sales ORDER BY id DESC",
    )?;
    let rows = statement.query_map([], map_sale)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn view(connection: &Connection, filters: SalesViewFilters) -> ServiceResult<SalesView> {
    let search = normalize_search(filters.search)?;
    let payment_method = normalize_payment_filter(filters.payment_method)?;
    let mut statement = connection.prepare(
        "SELECT s.id, printf('POS-%08d', s.id), s.sale_date, s.payment_method, s.total,
                s.user_id, s.cashier_name, COALESCE(SUM(d.quantity), 0)
         FROM sales s
         LEFT JOIN sale_details d ON d.sale_id = s.id
         WHERE (?1 = '' OR instr(lower(printf('POS-%08d', s.id)), ?1) > 0)
           AND (?2 IS NULL OR s.payment_method = ?2)
         GROUP BY s.id
         ORDER BY s.sale_date DESC, s.id DESC",
    )?;
    let headers = statement
        .query_map(params![search, payment_method], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::with_capacity(headers.len());
    let mut total_sold = 0;
    let mut items_sold = 0;
    for (id, ticket_number, sale_date, payment_method, total, user_id, user_name, item_count) in
        headers
    {
        total_sold = checked_total(total_sold, total, "SALES_STATS_OVERFLOW")?;
        items_sold = checked_total(items_sold, item_count, "SALES_STATS_OVERFLOW")?;
        items.push(SalesViewRow {
            id,
            ticket_number,
            sale_date,
            payment_method,
            total,
            item_count,
            user_id,
            user_name,
        });
    }
    let sale_count = i64::try_from(items.len()).unwrap_or(JS_MAX_SAFE_INTEGER);
    Ok(SalesView {
        stats: SalesViewStats {
            sale_count,
            total_sold,
            average_sale: rounded_average(total_sold, sale_count)?,
            items_sold,
        },
        items,
    })
}

pub fn history_detail(connection: &Connection, sale_id: i64) -> ServiceResult<SaleHistoryDetail> {
    validate_id(sale_id)?;
    let header = connection
        .query_row(
            "SELECT s.id, printf('POS-%08d', s.id), s.payment_method, s.sale_date, s.user_id,
                    s.cashier_name, s.cashier_email, s.total,
                    s.cash_received, s.change_amount
             FROM sales s WHERE s.id = ?1",
            [sale_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("SALE_NOT_FOUND", "Venta no encontrada"))?;
    let mut statement = connection.prepare(
        "SELECT product_id, product_name, sku, quantity, unit_price
         FROM sale_details WHERE sale_id = ?1 ORDER BY id",
    )?;
    let raw_lines = statement
        .query_map([sale_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut lines = Vec::with_capacity(raw_lines.len());
    let mut item_count = 0;
    for (product_id, product_name, sku, quantity, unit_price) in raw_lines {
        let subtotal = unit_price
            .checked_mul(quantity)
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(|| stats_overflow("SALE_DETAIL_OVERFLOW"))?;
        item_count = checked_total(item_count, quantity, "SALE_DETAIL_OVERFLOW")?;
        lines.push(SaleHistoryLine {
            product_id,
            product_name,
            sku,
            quantity,
            unit_price,
            subtotal,
        });
    }
    Ok(SaleHistoryDetail {
        id: header.0,
        ticket_number: header.1,
        payment_method: header.2,
        sale_date: header.3,
        user_id: header.4,
        user_name: header.5,
        user_email: header.6,
        total: header.7,
        cash_received: header.8,
        change_amount: header.9,
        lines,
        item_count,
    })
}

fn normalize_payment_filter(value: Option<String>) -> ServiceResult<Option<String>> {
    let value = value.map(|value| search_key(&value));
    match value {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if matches!(value.as_str(), "efectivo" | "tarjeta" | "transferencia") => {
            Ok(Some(value))
        }
        Some(_) => Err(ServiceError::validation(
            "INVALID_PAYMENT_METHOD",
            "Metodo de pago invalido",
        )),
        None => Ok(None),
    }
}

fn stats_overflow(code: &'static str) -> ServiceError {
    ServiceError::validation(
        code,
        "El total excede el limite permitido por la aplicacion",
    )
}

// Monetary averages round half up to the nearest cent without adding numerator values.
fn rounded_average(total: i64, count: i64) -> ServiceResult<i64> {
    if count == 0 {
        return Ok(0);
    }
    let quotient = total / count;
    let remainder = total % count;
    let round_up = remainder >= count / 2 + count % 2;
    quotient
        .checked_add(i64::from(round_up))
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(|| stats_overflow("SALES_STATS_OVERFLOW"))
}

pub fn checkout(
    connection: &mut Connection,
    user_id: i64,
    cart: &SessionCart,
    input: CheckoutCartInput,
) -> ServiceResult<SaleReceipt> {
    validate_id(user_id)?;
    let checkout_token = input.checkout_token.trim();
    if checkout_token.is_empty() {
        return Err(ServiceError::validation(
            "CHECKOUT_TOKEN_REQUIRED",
            "El token de cobro es obligatorio",
        ));
    }
    if checkout_token.chars().count() > CHECKOUT_TOKEN_MAX_LENGTH {
        return Err(ServiceError::validation(
            "CHECKOUT_TOKEN_TOO_LONG",
            "El token de cobro debe tener como maximo 128 caracteres",
        ));
    }
    if !matches!(
        input.payment_method.as_str(),
        "efectivo" | "tarjeta" | "transferencia"
    ) {
        return Err(ServiceError::validation(
            "INVALID_PAYMENT_METHOD",
            "Metodo de pago invalido",
        ));
    }

    let cash_received = match input.payment_method.as_str() {
        "efectivo" => Some(parse_money(input.cash_received.as_deref().ok_or_else(
            || {
                ServiceError::validation(
                    "CASH_RECEIVED_REQUIRED",
                    "Debes indicar el efectivo recibido",
                )
            },
        )?)?),
        _ if input.cash_received.is_some() => {
            return Err(ServiceError::validation(
                "CASH_NOT_ALLOWED",
                "El efectivo recibido solo aplica a pagos en efectivo",
            ));
        }
        _ => None,
    };
    let submitted_fingerprint = input.cart_fingerprint.as_str();

    let transaction = connection.transaction()?;
    if let Some(receipt) = receipt_by_token(&transaction, checkout_token)? {
        validate_retry(
            &receipt,
            user_id,
            cart,
            submitted_fingerprint,
            &input.payment_method,
            cash_received,
        )?;
        return Ok(receipt);
    }
    if cart.is_empty() {
        return Err(ServiceError::validation(
            "EMPTY_CART",
            "El carrito no tiene productos",
        ));
    }
    let (cashier_name, cashier_email): (String, String) = transaction
        .query_row(
            "SELECT trim(first_name || ' ' || last_name), email FROM users WHERE id = ?1",
            [user_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("USER_NOT_FOUND", "Usuario no encontrado"))?;

    let mut priced_items = Vec::with_capacity(cart.len());
    let mut total = 0_i64;
    for (&product_id, &quantity) in cart {
        validate_id(product_id)?;
        if !(1..=JS_MAX_SAFE_INTEGER).contains(&quantity) {
            return Err(ServiceError::validation(
                "INVALID_CART_QUANTITY",
                "Todas las cantidades deben ser mayores que cero",
            ));
        }
        let (name, sku, price, stock): (String, String, i64, i64) = transaction
            .query_row(
                "SELECT name, sku, price, stock FROM products WHERE id = ?1",
                [product_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| {
                ServiceError::not_found("PRODUCT_NOT_FOUND", "Uno de los productos no existe")
            })?;
        if price <= 0 {
            return Err(ServiceError::validation(
                "INVALID_PRODUCT_PRICE",
                "No se puede vender un producto sin precio",
            ));
        }
        if price > JS_MAX_SAFE_INTEGER || stock > JS_MAX_SAFE_INTEGER {
            return Err(amount_overflow());
        }
        if stock < quantity {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_STOCK",
                "Stock insuficiente para completar la venta",
            ));
        }
        total = total
            .checked_add(
                price
                    .checked_mul(quantity)
                    .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
                    .ok_or_else(amount_overflow)?,
            )
            .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
            .ok_or_else(amount_overflow)?;
        priced_items.push((product_id, name, sku, quantity, price));
    }

    let current_fingerprint = cart::fingerprint(
        priced_items
            .iter()
            .map(|(product_id, _, _, quantity, price)| (*product_id, *quantity, *price)),
    );
    if current_fingerprint != submitted_fingerprint {
        return Err(ServiceError::conflict(
            "CART_FINGERPRINT_MISMATCH",
            "El carrito o sus precios cambiaron desde que se inicio el cobro",
        ));
    }

    let change_amount = match cash_received {
        Some(received) if received < total => {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_CASH",
                "El efectivo recibido es insuficiente",
            ));
        }
        Some(received) => Some(received - total),
        None => None,
    };

    transaction.execute(
        "INSERT INTO sales (
             user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date,
             total, payment_method, cash_received, change_amount
         ) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, ?6, ?7, ?8, ?9)",
        params![
            user_id,
            cashier_name,
            cashier_email,
            checkout_token,
            submitted_fingerprint,
            total,
            input.payment_method,
            cash_received,
            change_amount
        ],
    )?;
    let sale_id = transaction.last_insert_rowid();

    for (product_id, product_name, sku, quantity, unit_price) in priced_items {
        let product_search_key = search_key(&product_name);
        let sku_search_key = search_key(&sku);
        let changed = transaction.execute(
            "UPDATE products
             SET stock = stock - ?1, updated_at = CURRENT_TIMESTAMP
             WHERE id = ?2 AND stock >= ?1",
            params![quantity, product_id],
        )?;
        if changed == 0 {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_STOCK",
                "Stock insuficiente para completar la venta",
            ));
        }
        transaction.execute(
            "INSERT INTO sale_details (
                 product_id, sale_id, product_name, sku, quantity, unit_price
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![product_id, sale_id, product_name, sku, quantity, unit_price],
        )?;
        transaction.execute(
            "INSERT INTO inventory_movements (
                 product_id, product_name, sku, product_search_key, sku_search_key,
                 sale_id, origin, reason, type, quantity
               ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'sale', 'venta', 'salida', ?7)",
            params![
                product_id,
                product_name,
                sku,
                product_search_key,
                sku_search_key,
                sale_id,
                quantity
            ],
        )?;
    }

    let receipt = receipt_by_id(&transaction, sale_id)?;
    transaction.commit()?;
    Ok(receipt)
}

pub fn quote_cash_change(
    connection: &Connection,
    cart: &SessionCart,
    cash_received: &str,
) -> ServiceResult<CashChangeQuote> {
    if cart.is_empty() {
        return Err(ServiceError::validation(
            "EMPTY_CART",
            "El carrito no tiene productos",
        ));
    }
    let total = cart::get(connection, cart)?.total;
    let cash_received = parse_money(cash_received)?;
    if cash_received < total {
        return Err(ServiceError::conflict(
            "INSUFFICIENT_CASH",
            "El efectivo recibido es insuficiente",
        ));
    }
    Ok(CashChangeQuote {
        cash_received,
        change: cash_received - total,
        total,
    })
}

fn receipt_by_token(
    connection: &Connection,
    checkout_token: &str,
) -> ServiceResult<Option<SaleReceipt>> {
    let sale_id = connection
        .query_row(
            "SELECT id FROM sales WHERE checkout_token = ?1",
            [checkout_token],
            |row| row.get(0),
        )
        .optional()?;
    sale_id.map(|id| receipt_by_id(connection, id)).transpose()
}

fn receipt_by_id(connection: &Connection, sale_id: i64) -> ServiceResult<SaleReceipt> {
    let sale = connection.query_row(
        "SELECT id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                sale_date, total, payment_method, cash_received, change_amount, created_at, updated_at
         FROM sales WHERE id = ?1",
        [sale_id],
        map_sale,
    )?;
    let mut statement = connection.prepare(
        "SELECT id, product_id, sale_id, product_name, sku, quantity, unit_price,
                created_at, updated_at
         FROM sale_details WHERE sale_id = ?1 ORDER BY id",
    )?;
    let details = statement
        .query_map([sale_id], map_detail)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SaleReceipt { sale, details })
}

fn validate_retry(
    receipt: &SaleReceipt,
    user_id: i64,
    cart: &SessionCart,
    submitted_fingerprint: &str,
    payment_method: &str,
    cash_received: Option<i64>,
) -> ServiceResult<()> {
    let historical_items: BTreeMap<_, _> = receipt
        .details
        .iter()
        .map(|detail| (detail.product_id, detail.quantity))
        .collect();
    let same_cart = cart.is_empty() || cart == &historical_items;
    if receipt.sale.user_id != user_id
        || receipt.sale.cart_fingerprint != submitted_fingerprint
        || receipt.sale.payment_method != payment_method
        || receipt.sale.cash_received != cash_received
        || !same_cart
    {
        return Err(ServiceError::conflict(
            "CHECKOUT_TOKEN_REUSED",
            "El token de cobro ya fue usado con otra intencion",
        ));
    }
    Ok(())
}

pub(crate) fn parse_money(value: &str) -> ServiceResult<i64> {
    let value = value.trim();
    let (whole, fraction) = match value.split_once('.') {
        Some((whole, fraction)) if !whole.is_empty() && matches!(fraction.len(), 1 | 2) => {
            (whole, fraction)
        }
        Some(_) => return Err(invalid_money()),
        None if !value.is_empty() => (value, ""),
        None => return Err(invalid_money()),
    };
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_money());
    }
    let whole = whole.parse::<i64>().map_err(|_| invalid_money())?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>().map_err(|_| invalid_money())? * 10,
        _ => fraction.parse::<i64>().map_err(|_| invalid_money())?,
    };
    whole
        .checked_mul(100)
        .and_then(|cents| cents.checked_add(fraction))
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(invalid_money)
}

fn invalid_money() -> ServiceError {
    ServiceError::validation(
        "INVALID_CASH_AMOUNT",
        "El efectivo debe ser un importe decimal valido con hasta dos decimales",
    )
}

fn amount_overflow() -> ServiceError {
    ServiceError::validation(
        "SALE_AMOUNT_OVERFLOW",
        "El total de la venta excede el limite permitido",
    )
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use rusqlite::Connection;

    use crate::{db::migrations, models::cart::CheckoutCartInput, services::cart::SessionCart};

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        prepare(&mut connection);
        connection
    }

    fn prepare(connection: &mut Connection) {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(connection).unwrap();
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
        connection.execute("INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES ('Producto', 'producto', 1250, 5, 'SKU-1', 'sku-1', 1, 1)", []).unwrap();
        connection.execute("INSERT INTO products (name, name_search_key, price, stock, sku, sku_search_key, brand_id, category_id) VALUES ('Otro', 'otro', 500, 5, 'SKU-2', 'sku-2', 1, 1)", []).unwrap();
    }

    fn input(token: &str, method: &str, cash: Option<&str>) -> CheckoutCartInput {
        CheckoutCartInput {
            checkout_token: token.into(),
            cart_fingerprint: "cart-v2|1:1:1250".into(),
            payment_method: method.into(),
            cash_received: cash.map(Into::into),
        }
    }

    fn fingerprint(connection: &Connection, cart: &SessionCart) -> String {
        crate::services::cart::get(connection, cart)
            .unwrap()
            .cart_fingerprint
    }

    #[test]
    fn parses_decimal_money_strictly_without_floating_point() {
        assert_eq!(super::parse_money("0").unwrap(), 0);
        assert_eq!(super::parse_money(" 12.3 ").unwrap(), 1230);
        assert_eq!(super::parse_money("12.34").unwrap(), 1234);
        assert_eq!(
            super::parse_money("90071992547409.91").unwrap(),
            crate::services::JS_MAX_SAFE_INTEGER
        );
        for invalid in [
            "",
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
                super::parse_money(invalid).unwrap_err().code,
                "INVALID_CASH_AMOUNT",
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn rounds_average_sale_half_up_without_overflow() {
        assert_eq!(super::rounded_average(1, 2).unwrap(), 1);
        assert_eq!(super::rounded_average(5, 2).unwrap(), 3);
        assert_eq!(
            super::rounded_average(crate::services::JS_MAX_SAFE_INTEGER, 2).unwrap(),
            4_503_599_627_370_496
        );
    }

    #[test]
    fn checks_out_all_payment_methods_and_cash_change() {
        for (method, cash, expected_cash, expected_change) in [
            ("efectivo", Some("20.00"), Some(2000), Some(750)),
            ("tarjeta", None, None, None),
            ("transferencia", None, None, None),
        ] {
            let mut connection = database();
            let cart: SessionCart = [(1, 1)].into();
            let receipt =
                super::checkout(&mut connection, 1, &cart, input(method, method, cash)).unwrap();

            assert_eq!(receipt.sale.total, 1250);
            assert_eq!(receipt.sale.payment_method, method);
            assert_eq!(receipt.sale.cash_received, expected_cash);
            assert_eq!(receipt.sale.change_amount, expected_change);
            assert_eq!(receipt.details[0].product_name, "Producto");
            assert_eq!(receipt.details[0].sku, "SKU-1");
            assert_eq!(receipt.details[0].unit_price, 1250);
            assert_eq!(receipt.details[0].quantity, 1);
            let stock: i64 = connection
                .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(stock, 4);
        }
    }

    #[test]
    fn rejects_insufficient_cash_and_cash_for_non_cash_methods_without_writes() {
        let mut connection = database();
        let cart: SessionCart = [(1, 1)].into();

        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &cart,
                input("low", "efectivo", Some("12.49")),
            )
            .unwrap_err()
            .code,
            "INSUFFICIENT_CASH"
        );
        for method in ["tarjeta", "transferencia"] {
            assert_eq!(
                super::checkout(
                    &mut connection,
                    1,
                    &cart,
                    input(method, method, Some("20.00")),
                )
                .unwrap_err()
                .code,
                "CASH_NOT_ALLOWED"
            );
        }
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn quotes_cash_change_without_writes() {
        let connection = database();
        let cart: SessionCart = [(1, 2)].into();

        let quote = super::quote_cash_change(&connection, &cart, "30.00").unwrap();
        assert_eq!(quote.cash_received, 3000);
        assert_eq!(quote.total, 2500);
        assert_eq!(quote.change, 500);
        assert_eq!(
            super::quote_cash_change(&connection, &cart, "24.99")
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH"
        );
        let sale_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sale_count, 0);
    }

    #[test]
    fn rejects_checkout_when_a_quoted_price_changed() {
        let mut connection = database();
        let cart: SessionCart = [(1, 1)].into();
        let mut checkout_input = input("stale-price", "tarjeta", None);
        checkout_input.cart_fingerprint = fingerprint(&connection, &cart);
        connection
            .execute("UPDATE products SET price = 1300 WHERE id = 1", [])
            .unwrap();

        let error = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap_err();
        assert_eq!(error.code, "CART_FINGERPRINT_MISMATCH");
        let sale_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sale_count, 0);
    }

    #[test]
    fn idempotent_retry_returns_original_receipt_after_reopen() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path: PathBuf = std::env::temp_dir().join(format!(
            "pos-tauri-checkout-{}-{unique}.db",
            std::process::id()
        ));
        let cart: SessionCart = [(1, 2)].into();
        let original = {
            let mut connection = Connection::open(&path).unwrap();
            prepare(&mut connection);
            let mut checkout_input = input("persistent-token", "efectivo", Some("30.00"));
            checkout_input.cart_fingerprint = fingerprint(&connection, &cart);
            let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
            connection
                .execute("UPDATE products SET price = 1300 WHERE id = 1", [])
                .unwrap();
            receipt
        };

        let mut reopened = Connection::open(&path).unwrap();
        let empty = SessionCart::new();
        let mut retry_input = input("persistent-token", "efectivo", Some("30.00"));
        retry_input.cart_fingerprint = "cart-v2|1:2:1250".into();
        let retried = super::checkout(&mut reopened, 1, &empty, retry_input).unwrap();
        let sale_count: i64 = reopened
            .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
            .unwrap();
        let stock: i64 = reopened
            .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert_eq!(retried, original);
        assert_eq!(sale_count, 1);
        assert_eq!(stock, 3);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_reused_token_with_different_intention_or_user() {
        let mut connection = database();
        let original_cart: SessionCart = [(1, 1)].into();
        super::checkout(
            &mut connection,
            1,
            &original_cart,
            input("same-token", "tarjeta", None),
        )
        .unwrap();

        let other_cart: SessionCart = [(2, 1)].into();
        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &other_cart,
                input("same-token", "tarjeta", None),
            )
            .unwrap_err()
            .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        let mut wrong_fingerprint = input("same-token", "tarjeta", None);
        wrong_fingerprint.cart_fingerprint = "cart-v2|2:1:500".into();
        assert_eq!(
            super::checkout(&mut connection, 1, &SessionCart::new(), wrong_fingerprint,)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &SessionCart::new(),
                input("same-token", "transferencia", None),
            )
            .unwrap_err()
            .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        assert_eq!(
            super::checkout(
                &mut connection,
                2,
                &SessionCart::new(),
                input("same-token", "tarjeta", None),
            )
            .unwrap_err()
            .code,
            "CHECKOUT_TOKEN_REUSED"
        );
    }

    #[test]
    fn rolls_back_sale_details_stock_and_movements_on_real_database_failure() {
        let mut connection = database();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_movement BEFORE INSERT ON inventory_movements
                 BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
            )
            .unwrap();
        let cart: SessionCart = [(1, 1)].into();

        assert!(super::checkout(
            &mut connection,
            1,
            &cart,
            input("rollback", "tarjeta", None),
        )
        .is_err());

        for table in ["sales", "sale_details", "inventory_movements"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} was not rolled back");
        }
        let stock: i64 = connection
            .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stock, 5);
        assert_eq!(cart.get(&1), Some(&1));
    }

    #[test]
    fn rejects_stale_stock_empty_cart_and_amount_overflow() {
        let mut connection = database();
        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &SessionCart::new(),
                input("empty", "tarjeta", None),
            )
            .unwrap_err()
            .code,
            "EMPTY_CART"
        );

        connection
            .execute("UPDATE products SET stock = 0 WHERE id = 1", [])
            .unwrap();
        let cart: SessionCart = [(1, 1)].into();
        assert_eq!(
            super::checkout(&mut connection, 1, &cart, input("stale", "tarjeta", None),)
                .unwrap_err()
                .code,
            "INSUFFICIENT_STOCK"
        );

        connection
            .execute(
                "UPDATE products SET price = ?1, stock = 2 WHERE id = 1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        let overflow_cart: SessionCart = [(1, 2)].into();
        let mut overflow_input = input("overflow", "tarjeta", None);
        overflow_input.cart_fingerprint = "cart-v2|1:2:9007199254740991".into();
        assert_eq!(
            super::checkout(&mut connection, 1, &overflow_cart, overflow_input,)
                .unwrap_err()
                .code,
            "SALE_AMOUNT_OVERFLOW"
        );
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn sales_view_searches_ticket_filters_payment_and_calculates_stats() {
        let mut connection = database();
        let empty = super::view(&connection, Default::default()).unwrap();
        assert!(empty.items.is_empty());
        assert_eq!(empty.stats.total_sold, 0);
        assert_eq!(empty.stats.average_sale, 0);

        let first_cart: SessionCart = [(1, 2)].into();
        let mut first = input("first-view", "tarjeta", None);
        first.cart_fingerprint = fingerprint(&connection, &first_cart);
        super::checkout(&mut connection, 1, &first_cart, first).unwrap();
        let second_cart: SessionCart = [(2, 1)].into();
        let mut second = input("second-view", "efectivo", Some("10.00"));
        second.cart_fingerprint = fingerprint(&connection, &second_cart);
        super::checkout(&mut connection, 1, &second_cart, second).unwrap();

        let all = super::view(&connection, Default::default()).unwrap();
        assert_eq!(all.stats.sale_count, 2);
        assert_eq!(all.stats.total_sold, 3000);
        assert_eq!(all.stats.average_sale, 1500);
        assert_eq!(all.stats.items_sold, 3);
        let filtered = super::view(
            &connection,
            crate::models::sale::SalesViewFilters {
                search: Some(" pos-00000001 ".into()),
                payment_method: Some("TARJETA".into()),
            },
        )
        .unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].ticket_number, "POS-00000001");
        assert_eq!(filtered.items[0].item_count, 2);
        assert_eq!(filtered.items[0].user_name, "Admin POS");
        assert_eq!(filtered.stats.sale_count, 1);
        assert_eq!(filtered.stats.total_sold, 2500);
        assert_eq!(filtered.stats.average_sale, 2500);
        assert_eq!(filtered.stats.items_sold, 2);
    }

    #[test]
    fn historical_detail_keeps_snapshots_after_product_edit_and_database_reopen() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path: PathBuf = std::env::temp_dir().join(format!(
            "pos-tauri-history-{}-{unique}.db",
            std::process::id()
        ));
        {
            let mut connection = Connection::open(&path).unwrap();
            prepare(&mut connection);
            let cart: SessionCart = [(1, 2)].into();
            let mut checkout_input = input("history", "efectivo", Some("30.00"));
            checkout_input.cart_fingerprint = fingerprint(&connection, &cart);
            super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
            connection
                .execute(
                    "UPDATE products
                     SET name = 'Renombrado', name_search_key = 'renombrado',
                         sku = 'NEW-SKU', sku_search_key = 'new-sku', price = 999
                     WHERE id = 1",
                    [],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE users SET first_name = 'Otro', last_name = 'Nombre', email = 'otro@example.com' WHERE id = 1",
                    [],
                )
                .unwrap();
        }

        let reopened = Connection::open(&path).unwrap();
        let detail = super::history_detail(&reopened, 1).unwrap();
        assert_eq!(detail.ticket_number, "POS-00000001");
        assert_eq!(detail.user_name, "Admin POS");
        assert_eq!(detail.user_email, "admin@pos.local");
        assert_eq!(detail.item_count, 2);
        assert_eq!(detail.total, 2500);
        assert_eq!(detail.cash_received, Some(3000));
        assert_eq!(detail.change_amount, Some(500));
        assert_eq!(detail.lines[0].product_name, "Producto");
        assert_eq!(detail.lines[0].sku, "SKU-1");
        assert_eq!(detail.lines[0].unit_price, 1250);
        assert_eq!(detail.lines[0].subtotal, 2500);
        let inventory = crate::services::inventory::view(
            &reopened,
            crate::models::inventory_movement::InventoryViewFilters {
                search: Some("sku-1".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].product_name, "Producto");
        assert_eq!(inventory.items[0].sku, "SKU-1");
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sales_view_rejects_invalid_filters_and_max_safe_aggregate_overflow() {
        let connection = database();
        assert_eq!(
            super::view(
                &connection,
                crate::models::sale::SalesViewFilters {
                    payment_method: Some("cheque".into()),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
            "INVALID_PAYMENT_METHOD"
        );
        for (token, fingerprint) in [("huge-1", "one"), ("huge-2", "two")] {
            connection.execute(
                "INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint, sale_date, total, payment_method)
                 VALUES (1, 'Admin POS', 'admin@pos.local', ?1, ?2, CURRENT_TIMESTAMP, ?3, 'tarjeta')",
                rusqlite::params![token, fingerprint, crate::services::JS_MAX_SAFE_INTEGER],
            ).unwrap();
        }
        assert_eq!(
            super::view(&connection, Default::default())
                .unwrap_err()
                .code,
            "SALES_STATS_OVERFLOW"
        );
        assert_eq!(
            super::history_detail(&connection, 999).unwrap_err().code,
            "SALE_NOT_FOUND"
        );
    }
}
