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
    checked_total, normalize_search, proportional_cost_removal, search_key, validate_id,
    ServiceError, ServiceResult,
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
    inventory_cost_before: Option<i64>,
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
        movement_cost_cents: row.get(18)?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
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
                 quantity, catalog_unit_price, unit_price, entered_quantity, base_quantity, line_total,
                movement_cost_cents
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
                row.get::<_, Option<i64>>(15)?,
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
            movement_cost_cents: line.15,
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
        let inventory_cost_before = transaction.query_row(
            "SELECT inventory_cost_cents FROM products WHERE id = ?1",
            [resolved.product_id],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        let line_index = lines.len();
        let demand = demands.entry(resolved.product_id).or_insert(ProductDemand {
            stock_before: resolved.stock_base,
            inventory_cost_before,
            base_consumed: 0,
            line_indices: Vec::new(),
        });
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
        let removed_cost = demand
            .inventory_cost_before
            .map(|cost| proportional_cost_removal(cost, demand.base_consumed, demand.stock_before))
            .transpose()?;
        let inventory_cost_after = match (demand.inventory_cost_before, removed_cost) {
            (Some(before), Some(removed)) => Some(before - removed),
            (None, _) if stock_after == 0 => Some(0),
            (None, _) => None,
            _ => unreachable!(),
        };
        let changed = transaction.execute(
            "UPDATE products SET stock = ?1, inventory_cost_cents = ?2,
                    updated_at = CURRENT_TIMESTAMP
             WHERE id = ?3 AND stock = ?4",
            params![
                stock_after,
                inventory_cost_after,
                product_id,
                demand.stock_before
            ],
        )?;
        if changed != 1 {
            return Err(ServiceError::conflict(
                "INSUFFICIENT_STOCK",
                "Stock insuficiente para completar la venta",
            ));
        }

        let allocations =
            removed_cost.map(|cost| allocate_cost(cost, &demand.line_indices, &lines));
        let mut movement_stock_before = demand.stock_before;
        let mut movement_cost_before = demand.inventory_cost_before;
        for (position, &line_index) in demand.line_indices.iter().enumerate() {
            let line = &lines[line_index];
            let line_cost = allocations.as_ref().map(|values| values[position]);
            let movement_stock_after = movement_stock_before - line.base_consumed;
            let movement_cost_after = match (movement_cost_before, line_cost) {
                (Some(before), Some(cost)) => Some(before - cost),
                (None, _) if movement_stock_after == 0 => Some(0),
                (None, _) => None,
                _ => unreachable!(),
            };
            transaction.execute(
                "INSERT INTO sale_details (product_id, sale_id, record_version, presentation_id,
                     product_name, sku, presentation_name, presentation_barcode, base_unit,
                     presentation_mode, presentation_base_quantity, quantity, catalog_unit_price,
                     unit_price, entered_quantity, base_quantity, line_total, movement_cost_cents)
                  VALUES (?1, ?2, 3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?11, ?14, ?15, ?16)",
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
                    line_cost
                ],
            )?;
            let sale_detail_id = transaction.last_insert_rowid();
            transaction.execute(
                "INSERT INTO inventory_movements (product_id, product_name, sku,
                     product_search_key, sku_search_key, sale_id, record_version, presentation_id,
                     sale_detail_id, presentation_name, presentation_barcode, base_unit,
                     presentation_mode, presentation_base_quantity, origin, reason, type, quantity,
                     entered_quantity, base_quantity, unit_price_cents, stock_before, stock_after,
                     movement_cost_cents, inventory_cost_before_cents, inventory_cost_after_cents)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 2, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                          'sale', 'venta', 'salida', ?14, ?15, ?14, ?16, ?17, ?18, ?19, ?20, ?21)",
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
                    line.applied_unit_price,
                    movement_stock_before,
                    movement_stock_after,
                    line_cost,
                    movement_cost_before,
                    movement_cost_after
                ],
            )?;
            movement_stock_before = movement_stock_after;
            movement_cost_before = movement_cost_after;
        }
        debug_assert_eq!(movement_stock_before, stock_after);
        debug_assert_eq!(movement_cost_before, inventory_cost_after);
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

fn allocate_cost(total_cost: i64, indices: &[usize], lines: &[PricedLine]) -> Vec<i64> {
    let total_base: i128 = indices
        .iter()
        .map(|index| i128::from(lines[*index].base_consumed))
        .sum();
    let mut allocations = Vec::with_capacity(indices.len());
    let mut remainders = Vec::with_capacity(indices.len());
    let mut allocated = 0_i64;
    for (position, index) in indices.iter().copied().enumerate() {
        let numerator = i128::from(total_cost) * i128::from(lines[index].base_consumed);
        let floor = i64::try_from(numerator / total_base).expect("proportional cost is bounded");
        allocations.push(floor);
        allocated += floor;
        remainders.push((
            numerator % total_base,
            lines[index].presentation_id,
            position,
        ));
    }
    remainders.sort_by_key(|(remainder, presentation_id, _)| {
        (std::cmp::Reverse(*remainder), *presentation_id)
    });
    for &(_, _, position) in remainders.iter().take((total_cost - allocated) as usize) {
        allocations[position] += 1;
    }
    allocations
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
                line_total, movement_cost_cents,
                created_at, updated_at
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
            if detail.record_version != 3 {
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
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{Arc, Barrier},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use rusqlite::{params, Connection};

    use crate::{
        db::migrations,
        models::cart::CheckoutCartInput,
        services::cart::{self, SessionCart},
    };

    fn database(stock: i64, cost: Option<i64>) -> (Connection, i64, i64) {
        let mut connection = Connection::open_in_memory().unwrap();
        let (piece, bag) = prepare(&mut connection, stock, cost);
        (connection, piece, bag)
    }

    fn prepare(connection: &mut Connection, stock: i64, cost: Option<i64>) -> (i64, i64) {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(connection).unwrap();
        connection.execute_batch(
            "INSERT INTO users (first_name,last_name,email,password_hash) VALUES ('Admin','POS','admin@pos.local','hash');
             INSERT INTO brands (name,name_search_key) VALUES ('Marca','marca');
             INSERT INTO categories (name,name_search_key) VALUES ('Categoria','categoria');",
        ).unwrap();
        connection.execute("INSERT INTO products (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES ('Tornillo','tornillo',100,?1,'TOR-1','tor-1',1,1,'piece',?2)", params![stock,cost]).unwrap();
        connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price,is_default) VALUES (1,'Pieza','fixed',1,100,1)", []).unwrap();
        let piece = connection.last_insert_rowid();
        connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price) VALUES (1,'Bolsa 100','fixed',100,8000)", []).unwrap();
        let bag = connection.last_insert_rowid();
        (piece, bag)
    }

    fn input(connection: &Connection, cart: &SessionCart, token: &str) -> CheckoutCartInput {
        payment_input(connection, cart, token, "tarjeta", None)
    }

    fn payment_input(
        connection: &Connection,
        cart: &SessionCart,
        token: &str,
        method: &str,
        cash: Option<&str>,
    ) -> CheckoutCartInput {
        CheckoutCartInput {
            checkout_token: token.into(),
            cart_fingerprint: cart::get(connection, cart).unwrap().cart_fingerprint,
            payment_method: method.into(),
            cash_received: cash.map(Into::into),
        }
    }

    fn temporary_database(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pos-tauri-{name}-{}-{unique}.db",
            std::process::id()
        ))
    }

    fn open_database(path: &Path) -> Connection {
        let connection = Connection::open(path).unwrap();
        connection.busy_timeout(Duration::from_secs(5)).unwrap();
        connection
    }

    #[test]
    fn checkout_persists_two_lines_aggregated_stock_and_exact_cost_sequence() {
        let (mut connection, piece, bag) = database(250, Some(1001));
        let cart: SessionCart = [(piece, 3), (bag, 1)].into();
        let checkout_input = input(&connection, &cart, "two-lines");
        let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
        assert_eq!(receipt.sale.total, 8300);
        assert_eq!(receipt.details.len(), 2);
        assert_eq!(
            receipt
                .details
                .iter()
                .map(|line| line.line_total.unwrap())
                .sum::<i64>(),
            8300
        );
        assert_eq!(
            receipt
                .details
                .iter()
                .map(|line| line.base_quantity.unwrap())
                .sum::<i64>(),
            103
        );
        let product: (i64, i64) = connection
            .query_row(
                "SELECT stock,inventory_cost_cents FROM products WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(product, (147, 589));
        let movements: Vec<[i64; 8]> = connection
            .prepare(
                "SELECT stock_before, stock_after, movement_cost_cents,
                        inventory_cost_before_cents, inventory_cost_after_cents,
                        sale_id, sale_detail_id, unit_price_cents
                 FROM inventory_movements ORDER BY id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ])
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(movements[0], [250, 247, 12, 1001, 989, 1, 1, 100]);
        assert_eq!(movements[1], [247, 147, 400, 989, 589, 1, 2, 8000]);
        assert_eq!(
            movements.iter().map(|movement| movement[2]).sum::<i64>(),
            412
        );
        let foreign_key_errors: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_errors, 0);
    }

    #[test]
    fn exhaustion_removes_all_cost_and_unknown_cost_stays_null_until_zero() {
        let (mut known, piece, _) = database(3, Some(10));
        let cart = [(piece, 3)].into();
        let checkout_input = input(&known, &cart, "known");
        super::checkout(&mut known, 1, &cart, checkout_input).unwrap();
        assert_eq!(
            known
                .query_row(
                    "SELECT stock,inventory_cost_cents FROM products",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                )
                .unwrap(),
            (0, 0)
        );

        let (mut unknown, piece, _) = database(4, None);
        let partial = [(piece, 1)].into();
        let checkout_input = input(&unknown, &partial, "unknown-partial");
        let receipt = super::checkout(&mut unknown, 1, &partial, checkout_input).unwrap();
        assert_eq!(receipt.details[0].movement_cost_cents, None);
        assert_eq!(
            unknown
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })
                .unwrap(),
            None
        );
        let rest = [(piece, 3)].into();
        let checkout_input = input(&unknown, &rest, "unknown-rest");
        super::checkout(&mut unknown, 1, &rest, checkout_input).unwrap();
        assert_eq!(
            unknown
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })
                .unwrap(),
            Some(0)
        );
    }

    #[test]
    fn retry_does_not_duplicate_and_intermediate_failure_rolls_back_everything() {
        let (mut connection, piece, _) = database(5, Some(11));
        let cart = [(piece, 1)].into();
        let checkout_input = input(&connection, &cart, "retry");
        let first = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
        let empty = SessionCart::new();
        let retry = CheckoutCartInput {
            checkout_token: "retry".into(),
            cart_fingerprint: first.sale.cart_fingerprint.clone(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &empty, retry).unwrap(),
            first
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );

        connection.execute_batch("CREATE TRIGGER fail_movement BEFORE INSERT ON inventory_movements BEGIN SELECT RAISE(ABORT, 'fail'); END;").unwrap();
        let cart = [(piece, 1)].into();
        let checkout_input = input(&connection, &cart, "rollback");
        assert!(super::checkout(&mut connection, 1, &cart, checkout_input).is_err());
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            connection
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            9
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sale_details", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn reads_legacy_receipt_from_persisted_snapshots_and_line_values() {
        let (mut connection, _, _) = database(5, None);
        connection.execute("INSERT INTO sales (user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method) VALUES (1,'Historico','old@local','legacy','cart-v2|1:2:125',CURRENT_TIMESTAMP,250,'none',0,0,250,'tarjeta')", []).unwrap();
        connection.execute("INSERT INTO sale_details (product_id,sale_id,record_version,product_name,sku,quantity,unit_price) VALUES (1,1,1,'Nombre viejo','SKU-VIEJO',2,125)", []).unwrap();
        connection
            .execute("UPDATE products SET name='Nuevo',price=999 WHERE id=1", [])
            .unwrap();
        let detail = super::history_detail(&connection, 1).unwrap();
        assert_eq!(detail.line_count, 1);
        assert_eq!(detail.total, 250);
        assert_eq!(detail.lines[0].product_name, "Nombre viejo");
        assert_eq!(detail.lines[0].subtotal, None);

        let retry = CheckoutCartInput {
            checkout_token: "legacy".into(),
            cart_fingerprint: "cart-v2|1:2:125".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        let receipt = super::checkout(&mut connection, 1, &SessionCart::new(), retry).unwrap();
        assert_eq!(receipt.sale.id, 1);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn cart_v2_retry_requires_an_empty_cart_and_never_reinterprets_presentations() {
        let (mut connection, _, _) = database(5, None);
        connection.execute("INSERT INTO products (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit) VALUES ('Tuerca','tuerca',50,20,'TUE-1','tue-1',1,1,'piece')", []).unwrap();
        let product_id = connection.last_insert_rowid();
        connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price,is_default) VALUES (?1,'Unidad','fixed',1,50,1)", [product_id]).unwrap();
        let unit = connection.last_insert_rowid();
        connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price) VALUES (?1,'Paquete','fixed',5,200)", [product_id]).unwrap();
        let pack = connection.last_insert_rowid();
        assert_ne!(product_id, unit);
        assert_ne!(product_id, pack);

        connection.execute("INSERT INTO sales (user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method) VALUES (1,'Historico','old@local','legacy-presentations','cart-v2|2:3:50',CURRENT_TIMESTAMP,150,'none',0,0,150,'tarjeta')", []).unwrap();
        connection.execute("INSERT INTO sale_details (product_id,sale_id,record_version,product_name,sku,quantity,unit_price) VALUES (?1,1,1,'Tuerca','TUE-1',3,50)", [product_id]).unwrap();

        let cart = [(unit, 1), (pack, 2)].into();
        let retry = CheckoutCartInput {
            checkout_token: "legacy-presentations".into(),
            cart_fingerprint: "cart-v2|2:3:50".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &cart, retry)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );

        let empty_retry = CheckoutCartInput {
            checkout_token: "legacy-presentations".into(),
            cart_fingerprint: "cart-v2|2:3:50".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        let receipt =
            super::checkout(&mut connection, 1, &SessionCart::new(), empty_retry).unwrap();
        assert_eq!(receipt.sale.id, 1);
    }

    #[test]
    fn cart_v4_retry_accepts_the_exact_populated_cart_without_duplicate_sale() {
        let (mut connection, piece, _) = database(5, Some(500));
        let fingerprint = format!("cart-v4|{piece}:2:fixed:1:2:100:200");
        connection.execute(
            "INSERT INTO sales (user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method)
             VALUES (1,'Historico','old@local','legacy-v4',?1,CURRENT_TIMESTAMP,200,'none',0,0,200,'tarjeta')",
            [&fingerprint],
        ).unwrap();
        connection.execute(
            "INSERT INTO sale_details (product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total)
             VALUES (1,1,2,?1,'Tornillo','TOR-1','Pieza','piece','fixed',1,2,100,100,2,2,200)",
            [piece],
        ).unwrap();
        let cart = [(piece, 2)].into();
        let retry = CheckoutCartInput {
            checkout_token: "legacy-v4".into(),
            cart_fingerprint: fingerprint,
            payment_method: "tarjeta".into(),
            cash_received: None,
        };

        let receipt = super::checkout(&mut connection, 1, &cart, retry).unwrap();

        assert_eq!(receipt.sale.id, 1);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn checkout_supports_payments_and_validates_cash_without_writes() {
        for (method, cash, expected_cash, expected_change) in [
            ("efectivo", Some("2.00"), Some(200), Some(100)),
            ("tarjeta", None, None, None),
            ("transferencia", None, None, None),
        ] {
            let (mut connection, piece, _) = database(5, Some(500));
            let cart = [(piece, 1)].into();
            let checkout_input = payment_input(&connection, &cart, method, method, cash);
            let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
            assert_eq!(receipt.sale.total, 100);
            assert_eq!(receipt.sale.payment_method, method);
            assert_eq!(receipt.sale.cash_received, expected_cash);
            assert_eq!(receipt.sale.change_amount, expected_change);
        }

        let (mut connection, piece, _) = database(5, Some(500));
        let cart = [(piece, 1)].into();
        let insufficient =
            payment_input(&connection, &cart, "insufficient", "efectivo", Some("0.99"));
        assert_eq!(
            super::checkout(&mut connection, 1, &cart, insufficient)
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH"
        );
        for method in ["tarjeta", "transferencia"] {
            let invalid = payment_input(&connection, &cart, method, method, Some("2.00"));
            assert_eq!(
                super::checkout(&mut connection, 1, &cart, invalid)
                    .unwrap_err()
                    .code,
                "CASH_NOT_ALLOWED"
            );
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn cash_quote_does_not_write() {
        let (connection, piece, _) = database(5, Some(500));
        let cart = [(piece, 2)].into();
        let quote = super::quote_cash_change(&connection, &cart, "3.00").unwrap();
        assert_eq!(
            (quote.cash_received, quote.total, quote.change),
            (300, 200, 100)
        );
        assert_eq!(
            super::quote_cash_change(&connection, &cart, "1.99")
                .unwrap_err()
                .code,
            "INSUFFICIENT_CASH"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn checkout_rejects_changed_presentation_price_and_fingerprint() {
        let (mut connection, piece, _) = database(5, Some(500));
        let cart = [(piece, 1)].into();
        let checkout_input = input(&connection, &cart, "stale-price");
        connection
            .execute(
                "UPDATE product_presentations SET price = 101 WHERE id = ?1",
                [piece],
            )
            .unwrap();
        assert_eq!(
            super::checkout(&mut connection, 1, &cart, checkout_input)
                .unwrap_err()
                .code,
            "CART_FINGERPRINT_MISMATCH"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn checkout_persists_v3_override_discount_and_undiscounted_inventory_cost() {
        let (mut connection, piece, _) = database(10, Some(1000));
        let mut cart = SessionCart::from([(piece, 2)]);
        cart::set_unit_price(&connection, &mut cart, piece, "0.75").unwrap();
        let resolved = cart::set_discount(&connection, &mut cart, "percentage", "10.00").unwrap();
        let checkout_input = CheckoutCartInput {
            checkout_token: "priced-discounted".into(),
            cart_fingerprint: resolved.cart_fingerprint.clone(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };

        let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
        assert_eq!(
            (
                receipt.sale.subtotal,
                receipt.sale.discount_value,
                receipt.sale.discount_amount,
                receipt.sale.total,
            ),
            (150, 1000, 15, 135)
        );
        assert_eq!(
            (
                receipt.details[0].record_version,
                receipt.details[0].catalog_unit_price,
                receipt.details[0].unit_price,
                receipt.details[0].line_total,
                receipt.details[0].movement_cost_cents,
            ),
            (3, Some(100), 75, Some(150), Some(200))
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT movement_cost_cents FROM inventory_movements WHERE sale_id=?1",
                    [receipt.sale.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            200
        );

        let retry = CheckoutCartInput {
            checkout_token: "priced-discounted".into(),
            cart_fingerprint: resolved.cart_fingerprint,
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &SessionCart::new(), retry).unwrap(),
            receipt
        );
    }

    #[test]
    fn catalog_change_invalidates_checkout_even_with_a_price_override() {
        let (mut connection, piece, _) = database(5, Some(500));
        let mut cart = SessionCart::from([(piece, 1)]);
        cart::set_unit_price(&connection, &mut cart, piece, "0.75").unwrap();
        let checkout_input = input(&connection, &cart, "stale-overridden-catalog");
        connection
            .execute(
                "UPDATE product_presentations SET price=101 WHERE id=?1",
                [piece],
            )
            .unwrap();

        assert_eq!(
            super::checkout(&mut connection, 1, &cart, checkout_input)
                .unwrap_err()
                .code,
            "CART_FINGERPRINT_MISMATCH"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn idempotent_retry_returns_original_receipt_after_reopen() {
        let path = temporary_database("checkout-reopen");
        let cart: SessionCart = [(1, 2)].into();
        let original = {
            let mut connection = open_database(&path);
            prepare(&mut connection, 5, Some(500));
            let checkout_input = payment_input(
                &connection,
                &cart,
                "persistent-token",
                "efectivo",
                Some("3.00"),
            );
            super::checkout(&mut connection, 1, &cart, checkout_input).unwrap()
        };

        let mut reopened = open_database(&path);
        reopened.pragma_update(None, "foreign_keys", true).unwrap();
        let retry = CheckoutCartInput {
            checkout_token: "persistent-token".into(),
            cart_fingerprint: original.sale.cart_fingerprint.clone(),
            payment_method: "efectivo".into(),
            cash_received: Some("3.00".into()),
        };
        let retried = super::checkout(&mut reopened, 1, &SessionCart::new(), retry).unwrap();
        assert_eq!(retried, original);
        assert_eq!(
            reopened
                .query_row("SELECT stock FROM products WHERE id = 1", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            3
        );
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn v5_retry_after_reopen_canonicalizes_interleaved_product_presentations() {
        let path = temporary_database("checkout-reopen-interleaved");
        let original = {
            let mut connection = open_database(&path);
            let (_, first_product_bag) = prepare(&mut connection, 200, Some(2000));
            connection.execute("INSERT INTO products (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES ('Tuerca','tuerca',200,20,'TUE-1','tue-1',1,1,'piece',4000)", []).unwrap();
            let second_product = connection.last_insert_rowid();
            connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price,is_default) VALUES (?1,'Pieza','fixed',1,200,1)", [second_product]).unwrap();
            let second_product_piece = connection.last_insert_rowid();
            connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price) VALUES (1,'Caja 5','fixed',5,450)", []).unwrap();
            let first_product_box = connection.last_insert_rowid();
            assert!(first_product_bag < second_product_piece);
            assert!(second_product_piece < first_product_box);
            let cart: SessionCart = [
                (first_product_bag, 1),
                (second_product_piece, 1),
                (first_product_box, 1),
            ]
            .into();
            let checkout_input = input(&connection, &cart, "interleaved-token");
            let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
            assert_eq!(
                receipt
                    .details
                    .iter()
                    .map(|detail| detail.presentation_id.unwrap())
                    .collect::<Vec<_>>(),
                vec![first_product_bag, first_product_box, second_product_piece]
            );
            receipt
        };

        let mut reopened = open_database(&path);
        reopened.pragma_update(None, "foreign_keys", true).unwrap();
        let retry = CheckoutCartInput {
            checkout_token: "interleaved-token".into(),
            cart_fingerprint: original.sale.cart_fingerprint.clone(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        let retried = super::checkout(&mut reopened, 1, &SessionCart::new(), retry).unwrap();
        assert_eq!(retried, original);
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn retry_rejects_different_user_cart_payment_fingerprint_and_unknown_version() {
        let (mut connection, piece, bag) = database(200, Some(1000));
        connection.execute("INSERT INTO users (first_name,last_name,email,password_hash) VALUES ('Otra','Persona','other@pos.local','hash')", []).unwrap();
        let cart = [(piece, 1)].into();
        let original_input = input(&connection, &cart, "same-token");
        let original = super::checkout(&mut connection, 1, &cart, original_input).unwrap();

        let different_cart = [(bag, 1)].into();
        let different_cart_input = input(&connection, &different_cart, "same-token");
        assert_eq!(
            super::checkout(&mut connection, 1, &different_cart, different_cart_input)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        let different_payment = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: original.sale.cart_fingerprint.clone(),
            payment_method: "transferencia".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &SessionCart::new(), different_payment,)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        let different_user = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: original.sale.cart_fingerprint.clone(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 2, &SessionCart::new(), different_user)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
        let different_fingerprint = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: "cart-v4|999:1:fixed:1:1:100:100".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(
                &mut connection,
                1,
                &SessionCart::new(),
                different_fingerprint,
            )
            .unwrap_err()
            .code,
            "CHECKOUT_TOKEN_REUSED"
        );

        let historical_v3 = format!("cart-v3|{piece}:1:1:100:100");
        connection
            .execute(
                "UPDATE sales SET cart_fingerprint = ?1 WHERE id = ?2",
                params![historical_v3, original.sale.id],
            )
            .unwrap();
        let v3_retry = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: historical_v3,
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert!(super::checkout(&mut connection, 1, &cart, v3_retry).is_ok());

        connection
            .execute(
                "UPDATE sales SET cart_fingerprint = 'cart-v1|1:1' WHERE id = ?1",
                [original.sale.id],
            )
            .unwrap();
        let v1_retry = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: "cart-v1|1:1".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert!(super::checkout(&mut connection, 1, &SessionCart::new(), v1_retry).is_ok());

        connection
            .execute(
                "UPDATE sales SET cart_fingerprint = 'cart-v99|opaque' WHERE id = ?1",
                [original.sale.id],
            )
            .unwrap();
        let unknown_retry = CheckoutCartInput {
            checkout_token: "same-token".into(),
            cart_fingerprint: "cart-v99|opaque".into(),
            payment_method: "tarjeta".into(),
            cash_received: None,
        };
        assert_eq!(
            super::checkout(&mut connection, 1, &SessionCart::new(), unknown_retry)
                .unwrap_err()
                .code,
            "CHECKOUT_TOKEN_REUSED"
        );
    }

    #[test]
    fn checkout_rejects_empty_cart_and_stale_stock_without_writes() {
        let (mut connection, piece, _) = database(1, Some(100));
        let empty = SessionCart::new();
        let empty_input = input(&connection, &empty, "empty");
        assert_eq!(
            super::checkout(&mut connection, 1, &empty, empty_input)
                .unwrap_err()
                .code,
            "EMPTY_CART"
        );
        let cart = [(piece, 1)].into();
        let stale_input = input(&connection, &cart, "stale");
        connection
            .execute("UPDATE products SET stock = 0 WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            super::checkout(&mut connection, 1, &cart, stale_input)
                .unwrap_err()
                .code,
            "INSUFFICIENT_STOCK"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn sales_view_filters_stats_and_overflow_are_preserved() {
        let (mut connection, piece, bag) = database(200, Some(1000));
        let empty = super::view(&connection, Default::default()).unwrap();
        assert!(empty.items.is_empty());
        assert_eq!(empty.stats.average_sale, 0);

        let first_cart = [(piece, 2)].into();
        let first = input(&connection, &first_cart, "first-view");
        super::checkout(&mut connection, 1, &first_cart, first).unwrap();
        let second_cart = [(bag, 1)].into();
        let second = payment_input(
            &connection,
            &second_cart,
            "second-view",
            "efectivo",
            Some("100.00"),
        );
        super::checkout(&mut connection, 1, &second_cart, second).unwrap();

        let all = super::view(&connection, Default::default()).unwrap();
        assert_eq!(all.stats.sale_count, 2);
        assert_eq!(all.stats.total_sold, 8200);
        assert_eq!(all.stats.average_sale, 4100);
        assert_eq!(all.stats.lines_sold, 2);
        let filtered = super::view(
            &connection,
            crate::models::sale::SalesViewFilters {
                search: Some(" pos-00000001 ".into()),
                payment_method: Some("TARJETA".into()),
            },
        )
        .unwrap();
        assert_eq!(filtered.items.len(), 1);
        assert_eq!(filtered.items[0].line_count, 1);
        assert_eq!(filtered.stats.total_sold, 200);
        assert_eq!(
            super::view(
                &connection,
                crate::models::sale::SalesViewFilters {
                    payment_method: Some("cheque".into()),
                    ..Default::default()
                },
            )
            .unwrap_err()
            .code,
            "INVALID_PAYMENT_METHOD"
        );
        connection.execute(
            "INSERT INTO sales (user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method)
             VALUES (1,'Admin POS','admin@pos.local','huge','cart-v2',CURRENT_TIMESTAMP,?1,'none',0,0,?1,'tarjeta')",
            [crate::services::JS_MAX_SAFE_INTEGER],
        ).unwrap();
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

    #[test]
    fn historical_snapshots_survive_product_presentation_user_rename_and_reopen() {
        let path = temporary_database("history-reopen");
        {
            let mut connection = open_database(&path);
            let (piece, _) = prepare(&mut connection, 5, Some(500));
            let cart = [(piece, 2)].into();
            let checkout_input =
                payment_input(&connection, &cart, "history", "efectivo", Some("3.00"));
            super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
            connection.execute_batch(
                "UPDATE products SET name='Renombrado',name_search_key='renombrado',sku='NEW-SKU',sku_search_key='new-sku' WHERE id=1;
                 UPDATE product_presentations SET name='Unidad nueva' WHERE id=1;
                 UPDATE users SET first_name='Otro',last_name='Nombre',email='otro@example.com' WHERE id=1;",
            ).unwrap();
        }

        let reopened = open_database(&path);
        let detail = super::history_detail(&reopened, 1).unwrap();
        assert_eq!(detail.user_name, "Admin POS");
        assert_eq!(detail.user_email, "admin@pos.local");
        assert_eq!(detail.lines[0].product_name, "Tornillo");
        assert_eq!(detail.lines[0].sku, "TOR-1");
        assert_eq!(detail.lines[0].presentation_name.as_deref(), Some("Pieza"));
        assert_eq!(detail.lines[0].entered_quantity, 2);
        assert_eq!(detail.lines[0].base_consumed, 2);
        let inventory = crate::services::inventory::view(
            &reopened,
            crate::models::inventory_movement::InventoryViewFilters {
                movement_search: Some("tor-1".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(inventory.items[0].product_name, "Renombrado");
        assert_eq!(inventory.items[0].sku, "NEW-SKU");
        assert_eq!(inventory.items[0].product_name_snapshot, "Tornillo");
        assert_eq!(inventory.items[0].sku_snapshot, "TOR-1");
        drop(reopened);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn concurrent_checkout_from_two_connections_is_idempotent() {
        let path = temporary_database("checkout-concurrent");
        let fingerprint = {
            let mut connection = open_database(&path);
            prepare(&mut connection, 5, Some(500));
            cart::get(&connection, &[(1, 1)].into())
                .unwrap()
                .cart_fingerprint
        };
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            let fingerprint = fingerprint.clone();
            handles.push(thread::spawn(move || {
                let mut connection = open_database(&path);
                connection
                    .pragma_update(None, "foreign_keys", true)
                    .unwrap();
                let cart = [(1, 1)].into();
                let checkout_input = CheckoutCartInput {
                    checkout_token: "concurrent-token".into(),
                    cart_fingerprint: fingerprint,
                    payment_method: "tarjeta".into(),
                    cash_received: None,
                };
                barrier.wait();
                super::checkout(&mut connection, 1, &cart, checkout_input)
                    .unwrap()
                    .sale
                    .id
            }));
        }
        let ids: Vec<i64> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(ids, vec![1, 1]);
        let connection = open_database(&path);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sales", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            4
        );
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn measured_volume_persists_exact_stock_cost_transitions_and_movement_foreign_keys() {
        let (mut connection, _, _) = database(5, Some(500));
        connection.execute("INSERT INTO products (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES ('Aceite','aceite',12000,2000,'ACE-1','ace-1',1,1,'milliliter',1000)", []).unwrap();
        connection.execute("INSERT INTO product_presentations (product_id,name,mode,base_quantity,price,is_default) VALUES (2,'Litro','measured',1000,12000,1)", []).unwrap();
        let liter = connection.last_insert_rowid();
        let cart = [(liter, 250)].into();
        let checkout_input = input(&connection, &cart, "measured-volume");
        let receipt = super::checkout(&mut connection, 1, &cart, checkout_input).unwrap();
        assert_eq!(receipt.sale.total, 3000);
        assert_eq!(receipt.details[0].entered_quantity, Some(250));
        assert_eq!(receipt.details[0].base_quantity, Some(250));
        assert_eq!(receipt.details[0].movement_cost_cents, Some(125));
        assert_eq!(
            connection
                .query_row(
                    "SELECT stock,inventory_cost_cents FROM products WHERE id=2",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .unwrap(),
            (1750, 875)
        );
        let transition: (i64, i64, i64, i64, i64, i64, i64) = connection
            .query_row(
                "SELECT stock_before,stock_after,movement_cost_cents,
                        inventory_cost_before_cents,inventory_cost_after_cents,sale_id,sale_detail_id
                 FROM inventory_movements WHERE product_id=2",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(transition, (2000, 1750, 125, 1000, 875, 1, 1));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn cost_allocation_assigns_residual_without_losing_cents() {
        let lines = vec![
            super::PricedLine {
                presentation_id: 2,
                presentation_name: String::new(),
                presentation_barcode: None,
                product_id: 1,
                product_name: String::new(),
                sku: String::new(),
                base_unit: "piece".into(),
                mode: crate::models::product_presentation::PresentationMode::Fixed,
                presentation_base_quantity: 1,
                catalog_unit_price: 1,
                applied_unit_price: 1,
                entered_quantity: 1,
                base_consumed: 1,
                line_total: 1,
            },
            super::PricedLine {
                presentation_id: 1,
                presentation_name: String::new(),
                presentation_barcode: None,
                product_id: 1,
                product_name: String::new(),
                sku: String::new(),
                base_unit: "piece".into(),
                mode: crate::models::product_presentation::PresentationMode::Fixed,
                presentation_base_quantity: 1,
                catalog_unit_price: 1,
                applied_unit_price: 1,
                entered_quantity: 1,
                base_consumed: 1,
                line_total: 1,
            },
        ];
        let allocated = super::allocate_cost(1, &[0, 1], &lines);
        assert_eq!(allocated, vec![0, 1]);
        assert_eq!(allocated.iter().sum::<i64>(), 1);
    }
}
