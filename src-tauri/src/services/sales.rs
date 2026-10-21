use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};

use crate::models::{
    cart::{CartDiscountType, CashChangeQuote, CheckoutCartInput},
    product_presentation::PresentationMode,
    sale::{
        Sale, SaleHistoryDetail, SaleHistoryLine, SaleReceipt, SalesView, SalesViewFilters,
        SalesViewRow, SalesViewStats,
    },
    sale_detail::SaleDetail,
};

use super::{
    cart::{self, SessionCart},
    checked_total, normalize_search, search_key, validate_id, ServiceError, ServiceResult,
};

const CHECKOUT_TOKEN_MAX_LENGTH: usize = 128;

#[derive(Clone)]
struct PricedLine {
    presentation_id: i64,
    presentation_name: String,
    presentation_barcode: Option<String>,
    product_id: i64,
    product_name: String,
    sku: String,
    base_unit: String,
    mode: PresentationMode,
    presentation_base_quantity: i64,
    catalog_unit_price: i64,
    applied_unit_price: i64,
    entered_quantity: i64,
    base_consumed: i64,
    line_total: i64,
}

struct ProductDemand {
    stock_before: i64,
    base_consumed: i64,
    line_indices: Vec<usize>,
}

fn parse_discount_type_row(value: String) -> rusqlite::Result<CartDiscountType> {
    match value.as_str() {
        "none" => Ok(CartDiscountType::None),
        "percentage" => Ok(CartDiscountType::Percentage),
        "fixed" => Ok(CartDiscountType::Fixed),
        _ => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid discount type").into(),
        )),
    }
}

fn map_sale(row: &Row<'_>) -> rusqlite::Result<Sale> {
    Ok(Sale {
        id: row.get(0)?,
        user_id: row.get(1)?,
        cashier_name: row.get(2)?,
        cashier_email: row.get(3)?,
        checkout_token: row.get(4)?,
        cart_fingerprint: row.get(5)?,
        sale_date: row.get(6)?,
        subtotal: row.get(7)?,
        discount_type: parse_discount_type_row(row.get::<_, String>(8)?)?,
        discount_value: row.get(9)?,
        discount_amount: row.get(10)?,
        total: row.get(11)?,
        payment_method: row.get(12)?,
        cash_received: row.get(13)?,
        change_amount: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn map_detail(row: &Row<'_>) -> rusqlite::Result<SaleDetail> {
    Ok(SaleDetail {
        id: row.get(0)?,
        product_id: row.get(1)?,
        sale_id: row.get(2)?,
        record_version: row.get(3)?,
        presentation_id: row.get(4)?,
        product_name: row.get(5)?,
        sku: row.get(6)?,
        presentation_name: row.get(7)?,
        presentation_barcode: row.get(8)?,
        base_unit: row.get(9)?,
        presentation_mode: row.get(10)?,
        presentation_base_quantity: row.get(11)?,
        quantity: row.get(12)?,
        catalog_unit_price: row.get(13)?,
        unit_price: row.get(14)?,
        entered_quantity: row.get(15)?,
        base_quantity: row.get(16)?,
        line_total: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Sale>> {
    let mut statement = connection.prepare(
        "SELECT id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                sale_date, subtotal, discount_type, discount_value, discount_amount, total,
                payment_method, cash_received, change_amount, created_at, updated_at
         FROM sales ORDER BY id DESC",
    )?;
    let sales = statement
        .query_map([], map_sale)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sales)
}

pub fn view(connection: &Connection, filters: SalesViewFilters) -> ServiceResult<SalesView> {
    let search = normalize_search(filters.search)?;
    let payment_method = normalize_payment_filter(filters.payment_method)?;
    let mut statement = connection.prepare(
        "SELECT s.id, printf('POS-%08d', s.id), s.sale_date, s.payment_method, s.total,
                s.user_id, s.cashier_name, COUNT(d.id)
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
    let mut lines_sold = 0;
    for (id, ticket_number, sale_date, method, total, user_id, user_name, line_count) in headers {
        total_sold = checked_total(total_sold, total, "SALES_STATS_OVERFLOW")?;
        lines_sold = checked_total(lines_sold, line_count, "SALES_STATS_OVERFLOW")?;
        items.push(SalesViewRow {
            id,
            ticket_number,
            sale_date,
            payment_method: method,
            total,
            line_count,
            user_id,
            user_name,
        });
    }
    let sale_count =
        i64::try_from(items.len()).map_err(|_| stats_overflow("SALES_STATS_OVERFLOW"))?;
    Ok(SalesView {
        items,
        stats: SalesViewStats {
            sale_count,
            total_sold,
            average_sale: rounded_average(total_sold, sale_count)?,
            lines_sold,
        },
    })
}

pub fn history_detail(connection: &Connection, sale_id: i64) -> ServiceResult<SaleHistoryDetail> {
    validate_id(sale_id)?;
    let header = connection
        .query_row(
            "SELECT s.id, printf('POS-%08d', s.id), s.payment_method, s.sale_date, s.user_id,
                    s.cashier_name, s.cashier_email, s.subtotal, s.discount_type,
                    s.discount_value, s.discount_amount, s.total, s.cash_received, s.change_amount
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
                    row.get::<_, String>(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("SALE_NOT_FOUND", "Venta no encontrada"))?;
    let mut statement = connection.prepare(
        "SELECT product_id, presentation_id, product_name, sku, presentation_name,
                presentation_barcode, base_unit, presentation_mode, presentation_base_quantity,
                 quantity, catalog_unit_price, unit_price, entered_quantity, base_quantity,
                 line_total
         FROM sale_details WHERE sale_id = ?1 ORDER BY id",
    )?;
    let raw_lines = statement
        .query_map([sale_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, Option<i64>>(12)?,
                row.get::<_, Option<i64>>(13)?,
                row.get::<_, Option<i64>>(14)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut lines = Vec::with_capacity(raw_lines.len());
    for line in raw_lines {
        lines.push(SaleHistoryLine {
            product_id: line.0,
            presentation_id: line.1,
            product_name: line.2,
            sku: line.3,
            presentation_name: line.4,
            presentation_barcode: line.5,
            base_unit: line.6,
            presentation_mode: line.7,
            presentation_base_quantity: line.8,
            quantity: line.9,
            catalog_unit_price: line.10,
            unit_price: line.11,
            entered_quantity: line.12.unwrap_or(line.9),
            base_consumed: line.13.unwrap_or(line.9),
            subtotal: line.14,
        });
    }
    let line_totals_verifiable = lines.iter().all(|line| line.subtotal.is_some());
    if line_totals_verifiable {
        let line_subtotal = lines.iter().try_fold(0_i64, |sum, line| {
            checked_total(
                sum,
                line.subtotal.expect("verified line total"),
                "SALE_DETAIL_OVERFLOW",
            )
        })?;
        if line_subtotal != header.7 {
            return Err(ServiceError::internal(
                "SALE_TOTAL_MISMATCH",
                "Los detalles historicos no coinciden con el subtotal de la venta",
            ));
        }
    }
    let line_count =
        i64::try_from(lines.len()).map_err(|_| stats_overflow("SALE_DETAIL_OVERFLOW"))?;
    Ok(SaleHistoryDetail {
        id: header.0,
        ticket_number: header.1,
        payment_method: header.2,
        sale_date: header.3,
        user_id: header.4,
        user_name: header.5,
        user_email: header.6,
        subtotal: header.7,
        discount_type: parse_discount_type_row(header.8)?,
        discount_value: header.9,
        discount_amount: header.10,
        line_totals_verifiable,
        total: header.11,
        cash_received: header.12,
        change_amount: header.13,
        lines,
        line_count,
    })
}

pub fn checkout(
    connection: &mut Connection,
    user_id: i64,
    cart: &SessionCart,
    input: CheckoutCartInput,
) -> ServiceResult<SaleReceipt> {
    validate_id(user_id)?;
    let checkout_token = validate_checkout_token(&input.checkout_token)?;
    validate_payment_method(&input.payment_method)?;
    let cash_received = parse_checkout_cash(&input.payment_method, input.cash_received.as_deref())?;
    let submitted_fingerprint = input.cart_fingerprint.as_str();

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(receipt) = receipt_by_token(&transaction, checkout_token)? {
        validate_retry(
            &transaction,
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

    let resolved_cart = cart::get(&transaction, cart)?;
    if resolved_cart.lines.iter().any(|line| !line.available) {
        return Err(ServiceError::conflict(
            "INSUFFICIENT_STOCK",
            "Stock insuficiente para completar la venta",
        ));
    }
    let mut lines = Vec::with_capacity(cart.len());
    let mut demands: BTreeMap<i64, ProductDemand> = BTreeMap::new();
    for resolved in &resolved_cart.lines {
        let line_index = lines.len();
        let demand = match demands.entry(resolved.product_id) {
            std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::btree_map::Entry::Vacant(entry) => {
                let stock_before = transaction.query_row(
                    "SELECT stock FROM products WHERE id=?1",
                    [resolved.product_id],
                    |row| row.get::<_, i64>(0),
                )?;
                entry.insert(ProductDemand {
                    stock_before,
                    base_consumed: 0,
                    line_indices: Vec::new(),
                })
            }
        };
        demand.base_consumed = checked_total(
            demand.base_consumed,
            resolved.base_consumed,
            "SALE_AMOUNT_OVERFLOW",
        )?;
        demand.line_indices.push(line_index);
        lines.push(PricedLine {
            presentation_id: resolved.presentation_id,
            presentation_name: resolved.presentation_name.clone(),
            presentation_barcode: resolved.presentation_barcode.clone(),
            product_id: resolved.product_id,
            product_name: resolved.product_name.clone(),
            sku: resolved.sku.clone(),
            base_unit: resolved.base_unit.as_str().into(),
            mode: resolved.mode,
            presentation_base_quantity: resolved.base_quantity,
            catalog_unit_price: resolved.catalog_unit_price,
            applied_unit_price: resolved.applied_unit_price,
            entered_quantity: resolved.entered_quantity,
            base_consumed: resolved.base_consumed,
            line_total: resolved.line_total,
        });
    }
    for demand in demands.values() {
        if demand.base_consumed > demand.stock_before {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_STOCK",
                "Stock insuficiente para completar la venta",
            ));
        }
    }
    let current_fingerprint = &resolved_cart.cart_fingerprint;
    if current_fingerprint != submitted_fingerprint {
        return Err(ServiceError::conflict(
            "CART_FINGERPRINT_MISMATCH",
            "El carrito o sus precios cambiaron desde que se inicio el cobro",
        ));
    }
    let subtotal = resolved_cart.subtotal;
    let total = resolved_cart.total;
    let change_amount = match cash_received {
        Some(received) if received < total => {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_CASH",
                "El efectivo recibido es insuficiente",
            ))
        }
        Some(received) => Some(received - total),
        None => None,
    };

    transaction.execute(
        "INSERT INTO sales (user_id, cashier_name, cashier_email, checkout_token,
             cart_fingerprint, sale_date, subtotal, discount_type, discount_value,
             discount_amount, total, payment_method, cash_received, change_amount)
         VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            user_id,
            cashier_name,
            cashier_email,
            checkout_token,
            submitted_fingerprint,
            subtotal,
            match resolved_cart.discount_type {
                CartDiscountType::None => "none",
                CartDiscountType::Percentage => "percentage",
                CartDiscountType::Fixed => "fixed",
            },
            resolved_cart.discount_value,
            resolved_cart.discount_amount,
            total,
            input.payment_method,
            cash_received,
            change_amount,
        ],
    )?;
    let sale_id = transaction.last_insert_rowid();

    for (product_id, demand) in demands {
        let stock_after = demand.stock_before - demand.base_consumed;
        let changed = transaction.execute(
            "UPDATE products SET stock=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2 AND stock=?3",
            params![stock_after, product_id, demand.stock_before],
        )?;
        if changed != 1 {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_STOCK",
                "Stock insuficiente para completar la venta",
            ));
        }

        let mut movement_stock_before = demand.stock_before;
        for &line_index in &demand.line_indices {
            let line = &lines[line_index];
            let movement_stock_after = movement_stock_before - line.base_consumed;
            transaction.execute(
                "INSERT INTO sale_details (product_id, sale_id, record_version, presentation_id,
                     product_name, sku, presentation_name, presentation_barcode, base_unit,
                     presentation_mode, presentation_base_quantity, quantity, catalog_unit_price,
                     unit_price, entered_quantity, base_quantity, line_total)
                  VALUES (?1, ?2, 3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?11, ?14, ?15)",
                params![
                    line.product_id,
                    sale_id,
                    line.presentation_id,
                    line.product_name,
                    line.sku,
                    line.presentation_name,
                    line.presentation_barcode,
                    line.base_unit,
                    line.mode.as_str(),
                    line.presentation_base_quantity,
                    line.entered_quantity,
                    line.catalog_unit_price,
                    line.applied_unit_price,
                    line.base_consumed,
                    line.line_total,
                ],
            )?;
            let sale_detail_id = transaction.last_insert_rowid();
            transaction.execute(
                "INSERT INTO inventory_movements (product_id, product_name, sku,
                     product_search_key, sku_search_key, sale_id, record_version, presentation_id,
                     sale_detail_id, presentation_name, presentation_barcode, base_unit,
                     presentation_mode, presentation_base_quantity, origin, reason, type, quantity,
                     entered_quantity, base_quantity, stock_before, stock_after)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, 3, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                           'sale', 'venta', 'salida', ?14, ?15, ?14, ?16, ?17)",
                params![
                    line.product_id,
                    line.product_name,
                    line.sku,
                    search_key(&line.product_name),
                    search_key(&line.sku),
                    sale_id,
                    line.presentation_id,
                    sale_detail_id,
                    line.presentation_name,
                    line.presentation_barcode,
                    line.base_unit,
                    line.mode.as_str(),
                    line.presentation_base_quantity,
                    line.base_consumed,
                    line.entered_quantity,
                    movement_stock_before,
                    movement_stock_after,
                ],
            )?;
            movement_stock_before = movement_stock_after;
        }
        debug_assert_eq!(movement_stock_before, stock_after);
    }

    let persisted_total: i64 = transaction.query_row(
        "SELECT COALESCE(SUM(line_total), 0) FROM sale_details WHERE sale_id = ?1",
        [sale_id],
        |row| row.get(0),
    )?;
    if persisted_total != subtotal {
        return Err(ServiceError::internal(
            "SALE_TOTAL_MISMATCH",
            "Los detalles de la venta no coinciden con su total",
        ));
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

fn receipt_by_token(connection: &Connection, token: &str) -> ServiceResult<Option<SaleReceipt>> {
    connection
        .query_row(
            "SELECT id FROM sales WHERE checkout_token = ?1",
            [token],
            |row| row.get(0),
        )
        .optional()?
        .map(|id| receipt_by_id(connection, id))
        .transpose()
}

fn receipt_by_id(connection: &Connection, sale_id: i64) -> ServiceResult<SaleReceipt> {
    let sale = connection.query_row(
        "SELECT id, user_id, cashier_name, cashier_email, checkout_token, cart_fingerprint,
                sale_date, subtotal, discount_type, discount_value, discount_amount, total,
                payment_method, cash_received, change_amount, created_at, updated_at
         FROM sales WHERE id = ?1",
        [sale_id],
        map_sale,
    )?;
    let mut statement = connection.prepare(
        "SELECT id, product_id, sale_id, record_version, presentation_id, product_name, sku,
                presentation_name, presentation_barcode, base_unit, presentation_mode,
                 presentation_base_quantity, quantity, catalog_unit_price, unit_price, entered_quantity, base_quantity,
                 line_total, created_at, updated_at
         FROM sale_details WHERE sale_id = ?1 ORDER BY id",
    )?;
    let details = statement
        .query_map([sale_id], map_detail)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SaleReceipt { sale, details })
}

fn validate_retry(
    connection: &Connection,
    receipt: &SaleReceipt,
    user_id: i64,
    cart: &SessionCart,
    fingerprint: &str,
    payment_method: &str,
    cash_received: Option<i64>,
) -> ServiceResult<()> {
    let same_cart = match receipt.sale.cart_fingerprint.split('|').next() {
        Some("cart-v4" | "cart-v3") => {
            let historical_items: BTreeMap<i64, i64> = receipt
                .details
                .iter()
                .filter_map(|detail| {
                    Some((
                        detail.presentation_id?,
                        detail.entered_quantity.unwrap_or(detail.quantity),
                    ))
                })
                .collect();
            cart.is_empty()
                || (cart.quantities() == historical_items
                    && cart
                        .lines
                        .values()
                        .all(|entry| entry.unit_price_override.is_none())
                    && matches!(cart.discount, cart::CartDiscount::None))
        }
        Some("cart-v1" | "cart-v2") => cart.is_empty(),
        _ if receipt.sale.cart_fingerprint.starts_with("cart-v5:") => {
            let reconstructed = reconstruct_v5_fingerprint(receipt)?;
            if reconstructed != receipt.sale.cart_fingerprint {
                return Err(checkout_token_reused());
            }
            cart.is_empty()
                || cart::get(connection, cart)
                    .map(|current| current.cart_fingerprint == reconstructed)
                    .unwrap_or(false)
        }
        _ => {
            return Err(checkout_token_reused());
        }
    };
    if receipt.sale.user_id != user_id
        || receipt.sale.cart_fingerprint != fingerprint
        || receipt.sale.payment_method != payment_method
        || receipt.sale.cash_received != cash_received
        || !same_cart
    {
        return Err(checkout_token_reused());
    }
    Ok(())
}

fn reconstruct_v5_fingerprint(receipt: &SaleReceipt) -> ServiceResult<String> {
    let mut lines = receipt
        .details
        .iter()
        .map(|detail| {
            if !matches!(detail.record_version, 2 | 3) {
                return Err(checkout_token_reused());
            }
            let mode = PresentationMode::try_from(
                detail
                    .presentation_mode
                    .as_deref()
                    .ok_or_else(checkout_token_reused)?,
            )
            .map_err(|_| checkout_token_reused())?;
            Ok(cart::FingerprintLine {
                presentation_id: detail.presentation_id.ok_or_else(checkout_token_reused)?,
                entered_quantity: detail.entered_quantity.ok_or_else(checkout_token_reused)?,
                mode,
                presentation_base_quantity: detail
                    .presentation_base_quantity
                    .ok_or_else(checkout_token_reused)?,
                base_consumed: detail.base_quantity.ok_or_else(checkout_token_reused)?,
                catalog_unit_price: detail
                    .catalog_unit_price
                    .ok_or_else(checkout_token_reused)?,
                applied_unit_price: detail.unit_price,
                line_total: detail.line_total.ok_or_else(checkout_token_reused)?,
            })
        })
        .collect::<ServiceResult<Vec<_>>>()?;
    lines.sort_by_key(|line| line.presentation_id);
    Ok(cart::fingerprint(
        lines,
        receipt.sale.discount_type,
        receipt.sale.discount_value,
        receipt.sale.discount_amount,
        receipt.sale.subtotal,
        receipt.sale.total,
    ))
}

fn checkout_token_reused() -> ServiceError {
    ServiceError::conflict(
        "CHECKOUT_TOKEN_REUSED",
        "El token de cobro ya fue usado con otra intencion",
    )
}

fn validate_checkout_token(value: &str) -> ServiceResult<&str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ServiceError::validation(
            "CHECKOUT_TOKEN_REQUIRED",
            "El token de cobro es obligatorio",
        ));
    }
    if value.chars().count() > CHECKOUT_TOKEN_MAX_LENGTH {
        return Err(ServiceError::validation(
            "CHECKOUT_TOKEN_TOO_LONG",
            "El token de cobro debe tener como maximo 128 caracteres",
        ));
    }
    Ok(value)
}

fn validate_payment_method(value: &str) -> ServiceResult<()> {
    if matches!(value, "efectivo" | "tarjeta" | "transferencia") {
        Ok(())
    } else {
        Err(ServiceError::validation(
            "INVALID_PAYMENT_METHOD",
            "Metodo de pago invalido",
        ))
    }
}

fn parse_checkout_cash(method: &str, cash: Option<&str>) -> ServiceResult<Option<i64>> {
    match (method, cash) {
        ("efectivo", Some(value)) => Ok(Some(parse_money(value)?)),
        ("efectivo", None) => Err(ServiceError::validation(
            "CASH_RECEIVED_REQUIRED",
            "Debes indicar el efectivo recibido",
        )),
        (_, Some(_)) => Err(ServiceError::validation(
            "CASH_NOT_ALLOWED",
            "El efectivo recibido solo aplica a pagos en efectivo",
        )),
        (_, None) => Ok(None),
    }
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

fn rounded_average(total: i64, count: i64) -> ServiceResult<i64> {
    if count == 0 {
        return Ok(0);
    }
    super::divide_half_up_positive(i128::from(total), i128::from(count), "SALES_STATS_OVERFLOW")
}

pub(crate) fn parse_money(value: &str) -> ServiceResult<i64> {
    super::parse_money_text_allow_zero(value, true).map_err(|_| {
        ServiceError::validation(
            "INVALID_CASH_AMOUNT",
            "El efectivo debe ser un importe decimal valido con hasta dos decimales",
        )
    })
}

#[cfg(test)]
mod runtime_tests {
    use crate::{
        db::migrations,
        models::{cart::CheckoutCartInput, sale::SalesViewFilters},
        services::cart::{self, SessionCart},
    };
    use rusqlite::Connection;

    fn seed_database(connection: &mut Connection) -> (i64, i64) {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(connection).unwrap();
        connection.execute_batch(
            "INSERT INTO users(first_name,last_name,email,password_hash) VALUES('Ada','POS','ada@pos.local','hash');
             INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
             INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
             INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit)
             VALUES('Tornillo','tornillo',100,100,'TOR-1','tor-1',1,1,'piece');
             INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)
             VALUES(1,'Pieza','fixed',1,100,1);
             INSERT INTO product_presentations(product_id,name,mode,base_quantity,price)
             VALUES(1,'Paquete','fixed',10,800);",
        ).unwrap();
        (1, 2)
    }

    fn database() -> (Connection, i64, i64) {
        let mut connection = Connection::open_in_memory().unwrap();
        let (piece, pack) = seed_database(&mut connection);
        (connection, piece, pack)
    }

    #[test]
    fn checkout_aggregates_demand_and_persists_v3_physical_rows_without_costs() {
        let (mut connection, piece, pack) = database();
        let cart = SessionCart::from([(piece, 2), (pack, 3)]);
        let resolved = cart::get(&connection, &cart).unwrap();
        let receipt = super::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "checkout-v3".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();

        assert_eq!(receipt.details.len(), 2);
        assert!(receipt
            .details
            .iter()
            .all(|detail| detail.record_version == 3));
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            68
        );
        let movement_shape: Vec<(i64, Option<i64>, Option<i64>)> = connection
            .prepare("SELECT record_version,purchase_unit_price_cents,purchase_total_cents FROM inventory_movements ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(movement_shape, vec![(3, None, None), (3, None, None)]);
    }

    #[test]
    fn checkout_failure_rolls_back_sale_details_movements_and_stock() {
        let (mut connection, piece, _) = database();
        let cart = SessionCart::from([(piece, 1)]);
        let fingerprint = cart::get(&connection, &cart).unwrap().cart_fingerprint;
        connection
            .execute_batch(
                "CREATE TRIGGER fail_v3_movement BEFORE INSERT ON inventory_movements
             BEGIN SELECT RAISE(ABORT,'forced rollback'); END;",
            )
            .unwrap();

        assert!(super::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "rollback-v3".into(),
                cart_fingerprint: fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .is_err());
        assert_eq!(
            connection.query_row(
                "SELECT stock,(SELECT COUNT(*) FROM sales),(SELECT COUNT(*) FROM sale_details) FROM products WHERE id=1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            ).unwrap(),
            (100, 0, 0)
        );
    }

    #[test]
    fn checkout_retry_is_idempotent_and_conflicting_token_is_rejected() {
        let (mut connection, piece, pack) = database();
        let cart = SessionCart::from([(piece, 2)]);
        let input = CheckoutCartInput {
            checkout_token: "retry-v3".into(),
            cart_fingerprint: cart::get(&connection, &cart).unwrap().cart_fingerprint,
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        let first = super::checkout(&mut connection, 1, &cart, input).unwrap();
        let retry = CheckoutCartInput {
            checkout_token: "retry-v3".into(),
            cart_fingerprint: first.sale.cart_fingerprint.clone(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };

        assert_eq!(
            super::checkout(&mut connection, 1, &SessionCart::default(), retry).unwrap(),
            first
        );
        let conflicting_cart = SessionCart::from([(pack, 1)]);
        let conflict = CheckoutCartInput {
            checkout_token: "retry-v3".into(),
            cart_fingerprint: cart::get(&connection, &conflicting_cart)
                .unwrap()
                .cart_fingerprint,
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &conflicting_cart, conflict)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM sales),(SELECT COUNT(*) FROM sale_details),stock FROM products WHERE id=1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
                .unwrap(),
            (1, 1, 98)
        );
    }

    #[test]
    fn checkout_persists_commercial_and_identity_snapshots() {
        let (mut connection, piece, _) = database();
        let mut cart = SessionCart::from([(piece, 2)]);
        cart::set_unit_price(&connection, &mut cart, piece, "0.75").unwrap();
        let resolved = cart::set_discount(&connection, &mut cart, "percentage", "10").unwrap();
        let receipt = super::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "snapshots-v3".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();

        assert_eq!(
            (
                receipt.sale.subtotal,
                receipt.sale.discount_value,
                receipt.sale.discount_amount,
                receipt.sale.total,
            ),
            (150, 1_000, 15, 135)
        );
        assert_eq!(
            (
                receipt.details[0].catalog_unit_price,
                receipt.details[0].unit_price,
                receipt.details[0].line_total,
            ),
            (Some(100), 75, Some(150))
        );
        connection.execute_batch(
            "UPDATE products SET name='Nuevo',sku='NUEVO',name_search_key='nuevo',sku_search_key='nuevo' WHERE id=1;
             UPDATE product_presentations SET name='Nueva presentacion' WHERE id=1;
             UPDATE users SET first_name='Otro',last_name='Usuario',email='otro@pos.local' WHERE id=1;",
        ).unwrap();
        let history = super::history_detail(&connection, receipt.sale.id).unwrap();
        assert_eq!(
            (history.user_name.as_str(), history.user_email.as_str()),
            ("Ada POS", "ada@pos.local")
        );
        assert_eq!(
            (
                history.lines[0].product_name.as_str(),
                history.lines[0].sku.as_str()
            ),
            ("Tornillo", "TOR-1")
        );
        assert_eq!(history.lines[0].presentation_name.as_deref(), Some("Pieza"));
        assert_eq!(
            (history.subtotal, history.discount_amount, history.total),
            (150, 15, 135)
        );
    }

    #[test]
    fn checkout_prices_centimeters_and_rejects_aggregate_demand() {
        let (mut connection, piece, pack) = database();
        connection.execute_batch(
            "INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit)
             VALUES('Cable','cable',2500,200,'CAB-1','cab-1',1,1,'centimeter');
             INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)
             VALUES(2,'Metro','measured',100,2500,1);",
        ).unwrap();
        let meter = connection.last_insert_rowid();
        let cable = SessionCart::from([(meter, 75)]);
        let cable_fingerprint = cart::get(&connection, &cable).unwrap().cart_fingerprint;
        let receipt = super::checkout(
            &mut connection,
            1,
            &cable,
            CheckoutCartInput {
                checkout_token: "centimeters-v3".into(),
                cart_fingerprint: cable_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        assert_eq!(receipt.sale.total, 1_875);
        assert_eq!(receipt.details[0].base_quantity, Some(75));
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=2", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            125
        );

        let excessive = SessionCart::from([(piece, 1), (pack, 10)]);
        let resolved = cart::get(&connection, &excessive).unwrap();
        assert!(resolved.lines.iter().all(|line| !line.available));
        let error = super::checkout(
            &mut connection,
            1,
            &excessive,
            CheckoutCartInput {
                checkout_token: "aggregate-v3".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "INSUFFICIENT_STOCK");
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            100
        );
    }

    #[test]
    fn checkout_covers_payment_cash_filters_and_current_statistics() {
        let (mut connection, piece, _) = database();

        for (index, method, cash, expected_change) in [
            (1, "efectivo", Some("2.00"), Some(100)),
            (2, "tarjeta", None, None),
            (3, "transferencia", None, None),
        ] {
            let cart = SessionCart::from([(piece, 1)]);
            let fingerprint = cart::get(&connection, &cart).unwrap().cart_fingerprint;
            let receipt = super::checkout(
                &mut connection,
                1,
                &cart,
                CheckoutCartInput {
                    checkout_token: format!("payment-{index}"),
                    cart_fingerprint: fingerprint,
                    payment_method: method.into(),
                    cash_received: cash.map(str::to_owned),
                },
            )
            .unwrap();
            assert_eq!(receipt.sale.payment_method, method);
            assert_eq!(receipt.sale.change_amount, expected_change);
            assert!(receipt
                .details
                .iter()
                .all(|detail| detail.record_version == 3));
        }

        let cash_cart = SessionCart::from([(piece, 1)]);
        let quote = super::quote_cash_change(&connection, &cash_cart, "1.50").unwrap();
        assert_eq!(
            (quote.total, quote.cash_received, quote.change),
            (100, 150, 50)
        );
        assert_eq!(
            super::quote_cash_change(&connection, &cash_cart, "0.99")
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH"
        );
        let cash_fingerprint = cart::get(&connection, &cash_cart).unwrap().cart_fingerprint;
        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &cash_cart,
                CheckoutCartInput {
                    checkout_token: "cash-not-allowed".into(),
                    cart_fingerprint: cash_fingerprint,
                    payment_method: "tarjeta".into(),
                    cash_received: Some("1.00".into()),
                },
            )
            .unwrap_err()
            .code,
            "CASH_NOT_ALLOWED"
        );

        let all = super::view(&connection, SalesViewFilters::default()).unwrap();
        assert_eq!(
            (
                all.stats.sale_count,
                all.stats.total_sold,
                all.stats.average_sale,
                all.stats.lines_sold,
            ),
            (3, 300, 100, 3)
        );
        let filtered = super::view(
            &connection,
            SalesViewFilters {
                search: Some("pos-00000003".into()),
                payment_method: Some(" TRANSFERENCIA ".into()),
            },
        )
        .unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].payment_method, "transferencia");
        assert_eq!(filtered.stats.total_sold, 100);
        assert_eq!(
            super::view(
                &connection,
                SalesViewFilters {
                    search: None,
                    payment_method: Some("cheque".into()),
                },
            )
            .unwrap_err()
            .code,
            "INVALID_PAYMENT_METHOD"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    fn checkout_rejects_stale_fingerprint_without_partial_writes() {
        let (mut connection, piece, _) = database();
        let cart = SessionCart::from([(piece, 2)]);
        let stale = cart::get(&connection, &cart).unwrap().cart_fingerprint;
        connection
            .execute(
                "UPDATE product_presentations SET price=125 WHERE id=?1",
                [piece],
            )
            .unwrap();

        let error = super::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "stale-cart".into(),
                cart_fingerprint: stale,
                payment_method: "efectivo".into(),
                cash_received: Some("10.00".into()),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "CART_FINGERPRINT_MISMATCH");
        assert_eq!(
            connection
                .query_row(
                    "SELECT stock,(SELECT COUNT(*) FROM sales),(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
                .unwrap(),
            (100, 0, 0)
        );
    }

    #[test]
    fn checkout_retry_survives_database_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sales-reopen.db");
        let (first, fingerprint) = {
            let mut connection = Connection::open(&path).unwrap();
            let (piece, _) = seed_database(&mut connection);
            let cart = SessionCart::from([(piece, 2)]);
            let fingerprint = cart::get(&connection, &cart).unwrap().cart_fingerprint;
            let receipt = super::checkout(
                &mut connection,
                1,
                &cart,
                CheckoutCartInput {
                    checkout_token: "reopen-sale".into(),
                    cart_fingerprint: fingerprint.clone(),
                    payment_method: "transferencia".into(),
                    cash_received: None,
                },
            )
            .unwrap();
            (receipt, fingerprint)
        };

        let mut reopened = Connection::open(&path).unwrap();
        reopened.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut reopened).unwrap();
        let retry = super::checkout(
            &mut reopened,
            1,
            &SessionCart::default(),
            CheckoutCartInput {
                checkout_token: "reopen-sale".into(),
                cart_fingerprint: fingerprint,
                payment_method: "transferencia".into(),
                cash_received: None,
            },
        )
        .unwrap();
        assert_eq!(retry, first);
        assert_eq!(
            reopened
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM sales),(SELECT COUNT(*) FROM inventory_movements),stock FROM products WHERE id=1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
                )
                .unwrap(),
            (1, 1, 98)
        );
    }

    #[test]
    fn sales_view_rejects_aggregate_overflow() {
        let (connection, _, _) = database();
        connection.execute_batch(
            "INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,
                 subtotal,discount_type,discount_value,discount_amount,total,payment_method)
             VALUES(1,'Ada POS','ada@pos.local','overflow-1','legacy-1',CURRENT_TIMESTAMP,9007199254740991,'none',0,0,9007199254740991,'tarjeta');
             INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,
                 subtotal,discount_type,discount_value,discount_amount,total,payment_method)
             VALUES(1,'Ada POS','ada@pos.local','overflow-2','legacy-2',CURRENT_TIMESTAMP,1,'none',0,0,1,'tarjeta');",
        ).unwrap();

        assert_eq!(
            super::view(&connection, SalesViewFilters::default())
                .unwrap_err()
                .code,
            "SALES_STATS_OVERFLOW"
        );
    }
}
