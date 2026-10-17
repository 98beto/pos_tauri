use std::collections::BTreeSet;

use rusqlite::{named_params, params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::models::{
    inventory_movement::{
        ConfirmPurchaseCostCorrectionInput, CreateInventoryMovementInput, InventoryMovement,
        InventoryMovementPresentation, InventoryMovementProduct, InventoryMovementQuote,
        InventoryProductSearchResult, InventoryView, InventoryViewFilters, InventoryViewStats,
        ProductInventoryDetail, ProductInventoryDetailFilters, PurchaseCostCorrectionQuote,
        QuoteInventoryMovementInput, QuotePurchaseCostCorrectionInput,
    },
    product::{BaseUnit, InventoryProductRow},
    product_presentation::PresentationMode,
};

use super::{
    allocate_grouped_cost, checked_total, direct_entry_movement_cost, divide_half_up_positive,
    fixed_entry_movement_cost, measured_entry_movement_cost, normalize_persisted, normalize_search,
    parse_money_text, parse_money_text_allow_zero, presentation_base_quantity,
    proportional_cost_withdrawal, search_key, validate_id, validate_presentation_mode,
    CostAllocationLine, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER,
};

const OPERATION_TOKEN_MAX_LENGTH: usize = 128;
const PAGE_SIZE: i64 = 20;

const MOVEMENT_COLUMNS: &str = "m.id,m.product_id,p.name,p.sku,m.product_name,m.sku,m.sale_id,
    CASE WHEN m.sale_id IS NULL THEN NULL ELSE printf('POS-%08d',m.sale_id) END,
    m.record_version,m.origin,m.presentation_id,m.presentation_name,m.presentation_barcode,
    m.presentation_mode,m.presentation_base_quantity,m.base_unit,m.reason,m.type,m.quantity,
    m.entered_quantity,m.base_quantity,m.stock_before,m.stock_after,m.supplier,
    m.document_reference,m.note,m.operation_token,m.unit_price_cents,
    m.movement_cost_cents,m.inventory_cost_before_cents,m.inventory_cost_after_cents,
    m.created_at,m.updated_at";

fn map_movement(row: &Row<'_>) -> rusqlite::Result<InventoryMovement> {
    Ok(InventoryMovement {
        id: row.get(0)?,
        product_id: row.get(1)?,
        product_name: row.get(2)?,
        sku: row.get(3)?,
        product_name_snapshot: row.get(4)?,
        sku_snapshot: row.get(5)?,
        sale_id: row.get(6)?,
        ticket_reference: row.get(7)?,
        record_version: row.get(8)?,
        origin: row.get(9)?,
        presentation_id: row.get(10)?,
        presentation_name: row.get(11)?,
        presentation_barcode: row.get(12)?,
        presentation_mode: row.get(13)?,
        presentation_base_quantity: row.get(14)?,
        base_unit: row.get::<_, Option<String>>(15)?.map(|value| {
            BaseUnit::try_from(value.as_str()).expect("database base unit is constrained")
        }),
        reason: row.get(16)?,
        movement_type: row.get(17)?,
        quantity: row.get(18)?,
        entered_quantity: row.get(19)?,
        base_quantity: row.get(20)?,
        stock_before: row.get(21)?,
        stock_after: row.get(22)?,
        supplier: row.get(23)?,
        document_reference: row.get(24)?,
        note: row.get(25)?,
        operation_token: row.get(26)?,
        unit_price_cents: row.get(27)?,
        movement_cost_cents: row.get(28)?,
        inventory_cost_before_cents: row.get(29)?,
        inventory_cost_after_cents: row.get(30)?,
        created_at: row.get(31)?,
        updated_at: row.get(32)?,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<InventoryMovement>> {
    let sql = format!("SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id ORDER BY m.created_at DESC,m.id DESC");
    Ok(connection
        .prepare(&sql)?
        .query_map([], map_movement)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn search_products(
    connection: &Connection,
    search: Option<String>,
    requested_page: i64,
) -> ServiceResult<InventoryProductSearchResult> {
    let search = normalize_search(search)?;
    let page = page(Some(requested_page), "page")?;
    let offset = (page - 1) * PAGE_SIZE;
    let transaction = connection.unchecked_transaction()?;
    let condition = "?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0";
    let total = transaction.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {condition}"),
        [&search],
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit,p.inventory_cost_cents
         FROM products p
         WHERE {condition} ORDER BY p.name_search_key,p.id LIMIT ?2 OFFSET ?3"
    );
    let mut statement = transaction.prepare(&sql)?;
    let rows = statement
        .query_map(params![search, PAGE_SIZE, offset], map_inventory_product)?
        .collect::<Result<Vec<_>, _>>()?;
    let items = rows
        .into_iter()
        .map(|product| inventory_movement_product(&transaction, product))
        .collect::<ServiceResult<Vec<_>>>()?;
    drop(statement);
    transaction.commit()?;
    Ok(InventoryProductSearchResult {
        items,
        page,
        total,
        page_size: PAGE_SIZE,
    })
}

pub fn view(
    connection: &Connection,
    filters: InventoryViewFilters,
) -> ServiceResult<InventoryView> {
    let transaction = connection.unchecked_transaction()?;
    let result = view_snapshot(&transaction, filters)?;
    transaction.commit()?;
    Ok(result)
}

fn view_snapshot(
    connection: &Connection,
    filters: InventoryViewFilters,
) -> ServiceResult<InventoryView> {
    let product_search = normalize_search(filters.product_search)?;
    let movement_search = normalize_search(filters.movement_search)?;
    let reason = normalize_filter(
        filters.reason,
        &["venta", "compra", "ajuste", "valoracion"],
        "INVALID_REASON",
    )?;
    let movement_type = normalize_filter(
        filters.movement_type,
        &["entrada", "salida", "valoracion"],
        "INVALID_MOVEMENT_TYPE",
    )?;
    let base_unit = filters.base_unit.map(|value| value.as_str());
    let product_page = page(filters.product_page, "product_page")?;
    let movement_page = page(filters.movement_page, "movement_page")?;
    let cost_status = normalize_filter(
        filters.cost_status,
        &["known", "unknown"],
        "INVALID_COST_STATUS",
    )?;
    let product_where = "(?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0) AND (?2 IS NULL OR p.base_unit=?2) AND (?3 IS NULL OR (?3='known' AND p.inventory_cost_cents IS NOT NULL) OR (?3='unknown' AND p.inventory_cost_cents IS NULL))";
    let product_total = connection.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {product_where}"),
        params![product_search, base_unit, cost_status],
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit,p.inventory_cost_cents
         FROM products p
         WHERE {product_where} ORDER BY p.name_search_key,p.id LIMIT ?4 OFFSET ?5"
    );
    let mut product_statement = connection.prepare(&sql)?;
    let product_rows = product_statement
        .query_map(
            params![
                product_search,
                base_unit,
                cost_status,
                PAGE_SIZE,
                (product_page - 1) * PAGE_SIZE
            ],
            map_inventory_product,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let products = product_rows
        .into_iter()
        .map(|row| inventory_movement_product(connection, row))
        .collect::<ServiceResult<Vec<_>>>()?;

    let movement_where = "(?1 IS NULL OR m.reason=?1) AND (?2 IS NULL OR m.type=?2)
        AND (?3='' OR instr(p.name_search_key,?3)>0 OR instr(p.sku_search_key,?3)>0
        OR instr(m.product_search_key,?3)>0 OR instr(m.sku_search_key,?3)>0
        OR instr(coalesce(m.provider_search_key,''),?3)>0
        OR instr(coalesce(m.document_reference_search_key,''),?3)>0
        OR instr(coalesce(m.note_search_key,''),?3)>0
        OR (m.sale_id IS NOT NULL AND instr(lower(printf('POS-%08d',m.sale_id)),?3)>0))";
    let movement_total = connection.query_row(
        &format!("SELECT COUNT(*) FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE {movement_where}"),
        params![reason, movement_type, movement_search],
        |row| row.get(0),
    )?;
    let movement_sql = format!("SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE {movement_where} ORDER BY m.created_at DESC,m.id DESC LIMIT ?4 OFFSET ?5");
    let movements = connection
        .prepare(&movement_sql)?
        .query_map(
            params![
                reason,
                movement_type,
                movement_search,
                PAGE_SIZE,
                (movement_page - 1) * PAGE_SIZE
            ],
            map_movement,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(InventoryView {
        items: movements.clone(),
        products,
        movements,
        product_page,
        product_total,
        movement_page,
        movement_total,
        page_size: PAGE_SIZE,
        stats: inventory_stats(connection)?,
    })
}

pub fn product_detail(
    connection: &Connection,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> ServiceResult<ProductInventoryDetail> {
    let transaction = connection.unchecked_transaction()?;
    let result = product_detail_snapshot(&transaction, product_id, filters)?;
    transaction.commit()?;
    Ok(result)
}

fn product_detail_snapshot(
    connection: &Connection,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> ServiceResult<ProductInventoryDetail> {
    validate_id(product_id)?;
    let search = normalize_search(filters.movement_search)?;
    let reason = normalize_filter(
        filters.reason,
        &["venta", "compra", "ajuste", "valoracion"],
        "INVALID_REASON",
    )?;
    let movement_type = normalize_filter(
        filters.movement_type,
        &["entrada", "salida", "valoracion"],
        "INVALID_MOVEMENT_TYPE",
    )?;
    let date_from = normalize_date(filters.date_from, "date_from")?;
    let date_to = normalize_date(filters.date_to, "date_to")?;
    if matches!((&date_from,&date_to),(Some(from),Some(to)) if from>to) {
        return Err(ServiceError::validation(
            "INVALID_DATE_RANGE",
            "La fecha inicial no puede ser posterior a la fecha final",
        ));
    }
    let page = page(filters.page, "page")?;
    let product = connection.query_row(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit,p.inventory_cost_cents FROM products p WHERE p.id=?1",
        [product_id], map_inventory_product,
    ).optional()?.ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))?;
    let where_sql = "m.product_id=:product_id AND (:reason IS NULL OR m.reason=:reason) AND (:movement_type IS NULL OR m.type=:movement_type) AND (:date_from IS NULL OR m.created_at>=:date_from) AND (:date_to IS NULL OR m.created_at<datetime(:date_to,'+1 day')) AND (:search='' OR instr(coalesce(m.provider_search_key,''),:search)>0 OR instr(coalesce(m.document_reference_search_key,''),:search)>0 OR instr(coalesce(m.note_search_key,''),:search)>0 OR (m.sale_id IS NOT NULL AND instr(lower(printf('POS-%08d',m.sale_id)),:search)>0))";
    let total = connection.query_row(
        &format!("SELECT COUNT(*) FROM inventory_movements m WHERE {where_sql}"),
        named_params! {":product_id":product_id,":reason":reason,":movement_type":movement_type,":date_from":date_from,":date_to":date_to,":search":search},
        |row| row.get(0),
    )?;
    let sql = format!("SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE {where_sql} ORDER BY m.created_at DESC,m.id DESC LIMIT :limit OFFSET :offset");
    let movements = connection.prepare(&sql)?.query_map(
        named_params! {":product_id":product_id,":reason":reason,":movement_type":movement_type,":date_from":date_from,":date_to":date_to,":search":search,":limit":PAGE_SIZE,":offset":(page-1)*PAGE_SIZE},
        map_movement,
    )?.collect::<Result<Vec<_>,_>>()?;
    let last_movement_at = connection.query_row("SELECT created_at FROM inventory_movements WHERE product_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1", [product_id], |row| row.get(0)).optional()?;
    Ok(ProductInventoryDetail {
        product,
        last_movement_at,
        movements,
        page,
        total,
        page_size: PAGE_SIZE,
    })
}

#[derive(Clone)]
struct PriceSnapshot {
    name: String,
    barcode: Option<String>,
    mode: PresentationMode,
    base_quantity: i64,
}

#[derive(Clone)]
struct CalculatedQuote {
    product_name: String,
    sku: String,
    stock: i64,
    inventory_cost: Option<i64>,
    base_unit: BaseUnit,
    presentation: PriceSnapshot,
    base_quantity: i64,
    unit_price: Option<i64>,
    amount: Option<i64>,
    fingerprint: String,
}

pub fn quote(
    connection: &Connection,
    input: QuoteInventoryMovementInput,
) -> ServiceResult<InventoryMovementQuote> {
    let normalized = normalize_quote_input(input)?;
    let calculated = calculate_quote(connection, &normalized)?;
    Ok(public_quote(normalized, calculated))
}

pub fn create(
    connection: &mut Connection,
    input: CreateInventoryMovementInput,
) -> ServiceResult<InventoryMovement> {
    let operation_token = normalize_token(&input.operation_token)?;
    let normalized = normalize_create_input(input)?;
    let submitted_quote = normalized.quote_fingerprint.as_deref().ok_or_else(|| {
        ServiceError::validation(
            "INVENTORY_QUOTE_REQUIRED",
            "Calcula el movimiento antes de confirmarlo",
        )
    })?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((movement, persisted)) = movement_by_token(&transaction, &operation_token)? {
        if persisted == normalized.intent_fingerprint {
            return Ok(movement);
        }
        return Err(token_reused());
    }
    let calculated = calculate_quote(&transaction, &normalized.quote)?;
    if calculated.fingerprint != submitted_quote {
        return Err(ServiceError::conflict(
            "QUOTE_CHANGED",
            "La cotizacion cambio; revisa el nuevo importe y confirma nuevamente",
        ));
    }
    let stock_after = match normalized.quote.movement_type.as_str() {
        "entrada" => safe_add(calculated.stock, calculated.base_quantity, "STOCK_OVERFLOW")?,
        "salida" => calculated.stock - calculated.base_quantity,
        "valoracion" => calculated.stock,
        _ => unreachable!(),
    };
    let inventory_cost_after = match normalized.quote.movement_type.as_str() {
        "entrada" => calculated
            .inventory_cost
            .map(|cost| safe_add(cost, calculated.amount.unwrap(), "COST_OVERFLOW"))
            .transpose()?,
        "salida" => {
            proportional_cost_withdrawal(
                calculated.inventory_cost,
                calculated.stock,
                calculated.base_quantity,
            )?
            .inventory_cost_after_cents
        }
        "valoracion" => calculated.amount,
        _ => unreachable!(),
    };
    let changed = transaction.execute(
        "UPDATE products SET stock=?1,inventory_cost_cents=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3 AND stock=?4 AND inventory_cost_cents IS ?5",
        params![stock_after, inventory_cost_after, normalized.quote.product_id, calculated.stock, calculated.inventory_cost],
    )?;
    if changed != 1 {
        return Err(ServiceError::conflict(
            "QUOTE_CHANGED",
            "El stock cambio; cotiza nuevamente",
        ));
    }
    let persisted_unit_price = (normalized.quote.movement_type != "valoracion")
        .then_some(calculated.unit_price)
        .flatten();
    transaction.execute(
        "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,presentation_name,presentation_barcode,base_unit,presentation_mode,presentation_base_quantity,unit_price_cents,movement_cost_cents,origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,inventory_cost_before_cents,inventory_cost_after_cents,supplier,provider_search_key,document_reference,document_reference_search_key,note,note_search_key,operation_token,intent_fingerprint)
         VALUES(?1,?2,?3,?4,?5,2,?6,?7,?8,?9,?10,?11,?12,?13,'manual',?14,?15,?16,?17,?16,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29)",
        params![normalized.quote.product_id,calculated.product_name,calculated.sku,search_key(&calculated.product_name),search_key(&calculated.sku),normalized.quote.presentation_id,normalized.quote.presentation_id.map(|_|calculated.presentation.name.clone()),normalized.quote.presentation_id.and_then(|_|calculated.presentation.barcode.clone()),calculated.base_unit.as_str(),normalized.quote.presentation_id.map(|_|calculated.presentation.mode.as_str()),normalized.quote.presentation_id.map(|_|calculated.presentation.base_quantity),persisted_unit_price,calculated.amount,normalized.quote.reason,normalized.quote.movement_type,calculated.base_quantity,normalized.quote.entered_quantity,calculated.stock,stock_after,calculated.inventory_cost,inventory_cost_after,normalized.supplier,normalized.supplier.as_deref().map(search_key),normalized.document_reference,normalized.document_reference.as_deref().map(search_key),normalized.note,normalized.note.as_deref().map(search_key),operation_token,normalized.intent_fingerprint],
    )?;
    let movement = movement_by_id(&transaction, transaction.last_insert_rowid())?;
    transaction.commit()?;
    Ok(movement)
}

#[derive(Clone, Debug, Serialize)]
struct ReplayMovement {
    id: i64,
    product_id: i64,
    product_name: String,
    sku: String,
    record_version: i64,
    origin: String,
    reason: String,
    movement_type: String,
    presentation_id: Option<i64>,
    presentation_name: Option<String>,
    presentation_barcode: Option<String>,
    presentation_mode: Option<String>,
    presentation_base_quantity: Option<i64>,
    base_unit: Option<String>,
    quantity: i64,
    entered_quantity: Option<i64>,
    base_quantity: Option<i64>,
    unit_price_cents: Option<i64>,
    movement_cost_cents: Option<i64>,
    stock_before: Option<i64>,
    stock_after: Option<i64>,
    inventory_cost_before_cents: Option<i64>,
    inventory_cost_after_cents: Option<i64>,
    sale_id: Option<i64>,
    sale_detail_id: Option<i64>,
    supplier: Option<String>,
    document_reference: Option<String>,
    note: Option<String>,
    operation_token: Option<String>,
    intent_fingerprint: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
struct ReplaySaleDetail {
    id: i64,
    sale_id: i64,
    product_id: i64,
    record_version: i64,
    presentation_id: Option<i64>,
    product_name: String,
    sku: String,
    presentation_name: Option<String>,
    presentation_barcode: Option<String>,
    base_unit: Option<String>,
    presentation_mode: Option<String>,
    presentation_base_quantity: Option<i64>,
    quantity: i64,
    catalog_unit_price: Option<i64>,
    unit_price: i64,
    entered_quantity: Option<i64>,
    base_quantity: Option<i64>,
    line_total: Option<i64>,
    movement_cost_cents: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Serialize)]
struct ReplaySale {
    id: i64,
    user_id: i64,
    cashier_name: String,
    cashier_email: String,
    checkout_token: String,
    cart_fingerprint: String,
    sale_date: String,
    subtotal: i64,
    discount_type: String,
    discount_value: i64,
    discount_amount: i64,
    total: i64,
    payment_method: String,
    cash_received: Option<i64>,
    change_amount: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct ReplayProduct {
    id: i64,
    name: String,
    name_search_key: String,
    price: i64,
    stock: i64,
    sku: String,
    sku_search_key: String,
    brand_id: i64,
    category_id: i64,
    created_at: String,
    updated_at: String,
    base_unit: String,
    inventory_cost_cents: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
struct ReplayFingerprint<'a> {
    movement_id: i64,
    previous_unit_price_cents: i64,
    new_unit_price_cents: i64,
    product: &'a ReplayProduct,
    target: &'a ReplayMovement,
    later_movements: &'a [ReplayMovement],
    sales: &'a [ReplaySale],
    sale_details: &'a [ReplaySaleDetail],
}

#[derive(Clone)]
struct ReplayUpdate {
    movement_id: i64,
    movement_cost: Option<i64>,
    inventory_cost_before: Option<i64>,
    inventory_cost_after: Option<i64>,
    sale_detail: Option<(i64, Option<i64>)>,
}

struct CorrectionPlan {
    quote: PurchaseCostCorrectionQuote,
    target_movement_cost: i64,
    target_inventory_cost_after: Option<i64>,
    updates: Vec<ReplayUpdate>,
}

pub fn quote_purchase_cost_correction(
    connection: &Connection,
    input: QuotePurchaseCostCorrectionInput,
) -> ServiceResult<PurchaseCostCorrectionQuote> {
    validate_id(input.movement_id)?;
    let new_price = parse_correction_price(&input.new_unit_price)?;
    Ok(build_correction_plan(connection, input.movement_id, new_price, None)?.quote)
}

pub fn confirm_purchase_cost_correction(
    connection: &mut Connection,
    input: ConfirmPurchaseCostCorrectionInput,
) -> ServiceResult<PurchaseCostCorrectionQuote> {
    validate_id(input.movement_id)?;
    let new_price = parse_correction_price(&input.new_unit_price)?;
    if input.quote_fingerprint.is_empty() || input.quote_fingerprint.len() > 128 {
        return Err(stale_correction());
    }
    let quoted_previous_price = correction_fingerprint_previous_price(&input.quote_fingerprint)?;
    if quoted_previous_price == new_price {
        return Err(stale_correction());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let plan = build_correction_plan(
        &transaction,
        input.movement_id,
        new_price,
        Some(quoted_previous_price),
    )?;

    let current_price = plan.quote.previous_unit_price_cents;
    if current_price != new_price && current_price != quoted_previous_price {
        return Err(stale_correction());
    }
    if plan.quote.quote_fingerprint != input.quote_fingerprint {
        return Err(stale_correction());
    }
    // The stable fingerprint proves this exact quote produced the current replay state.
    if current_price == new_price {
        transaction.commit()?;
        return Ok(plan.quote);
    }

    transaction.execute(
        "UPDATE inventory_movements SET unit_price_cents=?1,movement_cost_cents=?2,inventory_cost_after_cents=?3 WHERE id=?4",
        params![new_price, plan.target_movement_cost, plan.target_inventory_cost_after, input.movement_id],
    )?;
    for update in &plan.updates {
        transaction.execute(
            "UPDATE inventory_movements SET movement_cost_cents=?1,inventory_cost_before_cents=?2,inventory_cost_after_cents=?3 WHERE id=?4",
            params![update.movement_cost,update.inventory_cost_before,update.inventory_cost_after,update.movement_id],
        )?;
        if let Some((detail_id, cost)) = update.sale_detail {
            transaction.execute(
                "UPDATE sale_details SET movement_cost_cents=?1 WHERE id=?2",
                params![cost, detail_id],
            )?;
        }
    }
    transaction.execute(
        "UPDATE products SET inventory_cost_cents=?1 WHERE id=?2",
        params![
            plan.quote.corrected_inventory_cost_cents,
            plan.quote.product_id
        ],
    )?;
    transaction.commit()?;
    Ok(plan.quote)
}

fn parse_correction_price(value: &str) -> ServiceResult<i64> {
    parse_money_text(value).map_err(|_| {
        ServiceError::validation_field(
            "INVALID_PURCHASE_PRICE",
            "El nuevo precio de compra no es valido",
            "new_unit_price",
        )
    })
}

fn build_correction_plan(
    connection: &Connection,
    movement_id: i64,
    new_price: i64,
    quoted_previous_price: Option<i64>,
) -> ServiceResult<CorrectionPlan> {
    let target = load_replay_movement(connection, movement_id)?.ok_or_else(|| {
        ServiceError::not_found("INVENTORY_MOVEMENT_NOT_FOUND", "Movimiento no encontrado")
    })?;
    validate_correction_target(&target)?;
    let product_id: i64 = connection.query_row(
        "SELECT product_id FROM inventory_movements WHERE id=?1",
        [movement_id],
        |row| row.get(0),
    )?;
    let (mut product, product_cost): (ReplayProduct, Option<i64>) = connection
        .query_row(
            "SELECT id,name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,created_at,updated_at,base_unit,inventory_cost_cents FROM products WHERE id=?1",
            [product_id],
            |row| {
                Ok((
                    ReplayProduct {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        name_search_key: row.get(2)?,
                        price: row.get(3)?,
                        stock: row.get(4)?,
                        sku: row.get(5)?,
                        sku_search_key: row.get(6)?,
                        brand_id: row.get(7)?,
                        category_id: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                        base_unit: row.get(11)?,
                        inventory_cost_cents: row.get(12)?,
                    },
                    row.get(12)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))?;
    if target.base_unit.as_deref() != Some(product.base_unit.as_str()) {
        if quoted_previous_price.is_some() {
            return Err(stale_correction());
        }
        return Err(unsafe_replay(
            "La unidad historica de la compra esta incompleta",
        ));
    }
    let old_price = target.unit_price_cents.expect("eligible target price");
    if quoted_previous_price.is_none() && old_price == new_price {
        return Err(ServiceError::validation_field(
            "PURCHASE_COST_CORRECTION_SAME_PRICE",
            "El nuevo precio debe ser diferente al precio de compra actual",
            "new_unit_price",
        ));
    }
    let old_movement_cost = target.movement_cost_cents.expect("eligible target cost");
    let calculated_old = entry_cost(&target, old_price)?;
    let calculated_new = entry_cost(&target, new_price)?;
    if calculated_old != old_movement_cost {
        return Err(unsafe_replay(
            "El costo original de la compra es inconsistente",
        ));
    }
    let stock_before = target.stock_before.expect("eligible target stock");
    let stock_after = target.stock_after.expect("eligible target stock");
    let base_quantity = target.base_quantity.expect("eligible target quantity");
    if stock_before.checked_add(base_quantity) != Some(stock_after) {
        return Err(unsafe_replay(
            "La compra contiene una cadena de stock inconsistente",
        ));
    }
    let old_target_after = add_entry_cost(target.inventory_cost_before_cents, calculated_old)?;
    if old_target_after != target.inventory_cost_after_cents {
        return Err(unsafe_replay(
            "La compra contiene una cadena de costo inconsistente",
        ));
    }
    let new_target_after = add_entry_cost(target.inventory_cost_before_cents, calculated_new)?;

    let later = load_later_movements(connection, product_id, &target)?;
    validate_post_target_sale_details(connection, product_id, &target)?;
    let sale_ids = later
        .iter()
        .filter_map(|movement| movement.sale_id)
        .collect::<BTreeSet<_>>();
    let sales = load_replay_sales(connection, &sale_ids)?;
    let details = load_replay_sale_details(connection, product_id, &sale_ids)?;
    if sales.len() != sale_ids.len() {
        return Err(unsafe_replay("Una venta relacionada esta incompleta"));
    }

    let mut old_cost = old_target_after;
    let mut new_cost = new_target_after;
    let mut expected_stock = stock_after;
    let mut updates = Vec::with_capacity(later.len());
    let mut affected_movements = 0_i64;
    let mut affected_sales = BTreeSet::new();
    let mut last_affected_at = target.created_at.clone();
    let mut seen_sales = BTreeSet::new();
    let mut index = 0;
    while index < later.len() {
        let movement = &later[index];
        validate_replay_row(movement, expected_stock, old_cost)?;
        if movement.origin == "sale" {
            let sale_id = movement
                .sale_id
                .ok_or_else(|| unsafe_replay("Una venta no tiene identificador"))?;
            if !seen_sales.insert(sale_id) {
                return Err(unsafe_replay(
                    "Los movimientos de una venta no son contiguos",
                ));
            }
            let end = later[index..]
                .iter()
                .position(|row| row.sale_id != Some(sale_id))
                .map_or(later.len(), |offset| index + offset);
            let group = &later[index..end];
            let group_details: Vec<&ReplaySaleDetail> = details
                .iter()
                .filter(|detail| detail.sale_id == sale_id)
                .collect();
            if group_details.len() != group.len() {
                return Err(unsafe_replay(
                    "Los detalles y movimientos de una venta no coinciden",
                ));
            }
            let total_quantity = group.iter().try_fold(0_i64, |total, row| {
                checked_total(
                    total,
                    row.base_quantity.unwrap_or(0),
                    "CORRECTION_COST_OVERFLOW",
                )
            })?;
            let old_withdrawal =
                proportional_cost_withdrawal(old_cost, expected_stock, total_quantity)?;
            let new_withdrawal =
                proportional_cost_withdrawal(new_cost, expected_stock, total_quantity)?;
            let allocation_lines = group
                .iter()
                .map(|row| CostAllocationLine {
                    stable_id: row.presentation_id.unwrap_or(0),
                    base_quantity: row.base_quantity.unwrap_or(0),
                })
                .collect::<Vec<_>>();
            let old_allocations = old_withdrawal
                .movement_cost_cents
                .map(|cost| allocate_grouped_cost(cost, &allocation_lines))
                .transpose()?;
            let new_allocations = new_withdrawal
                .movement_cost_cents
                .map(|cost| allocate_grouped_cost(cost, &allocation_lines))
                .transpose()?;
            let mut old_line_cost = old_cost;
            let mut new_line_cost = new_cost;
            for (position, row) in group.iter().enumerate() {
                validate_replay_row(row, expected_stock, old_line_cost)?;
                let detail = group_details
                    .iter()
                    .find(|detail| Some(detail.id) == row.sale_detail_id)
                    .ok_or_else(|| unsafe_replay("Una venta no tiene su detalle relacionado"))?;
                validate_sale_link(row, detail)?;
                let quantity = row.base_quantity.expect("validated replay quantity");
                let next_stock = expected_stock - quantity;
                let old_line_movement = old_allocations.as_ref().map(|values| values[position]);
                let new_line_movement = new_allocations.as_ref().map(|values| values[position]);
                let old_after = subtract_line_cost(old_line_cost, old_line_movement, next_stock)?;
                let new_after = subtract_line_cost(new_line_cost, new_line_movement, next_stock)?;
                if row.movement_cost_cents != old_line_movement
                    || row.inventory_cost_after_cents != old_after
                    || detail.movement_cost_cents != old_line_movement
                {
                    return Err(unsafe_replay(
                        "Los costos relacionados de una venta son inconsistentes",
                    ));
                }
                let changed = row.inventory_cost_before_cents != new_line_cost
                    || row.movement_cost_cents != new_line_movement
                    || row.inventory_cost_after_cents != new_after
                    || detail.movement_cost_cents != new_line_movement;
                if changed {
                    affected_movements += 1;
                    affected_sales.insert(sale_id);
                    last_affected_at = row.created_at.clone();
                }
                updates.push(ReplayUpdate {
                    movement_id: row.id,
                    movement_cost: new_line_movement,
                    inventory_cost_before: new_line_cost,
                    inventory_cost_after: new_after,
                    sale_detail: Some((detail.id, new_line_movement)),
                });
                expected_stock = next_stock;
                old_line_cost = old_after;
                new_line_cost = new_after;
            }
            old_cost = old_withdrawal.inventory_cost_after_cents;
            new_cost = new_withdrawal.inventory_cost_after_cents;
            if old_line_cost != old_cost || new_line_cost != new_cost {
                return Err(unsafe_replay(
                    "La distribucion de costo de la venta es inconsistente",
                ));
            }
            index = end;
            continue;
        }

        let quantity = movement.base_quantity.expect("validated replay quantity");
        let (next_stock, old_movement_cost, old_after, new_movement_cost, new_after) =
            match movement.movement_type.as_str() {
                "entrada" => {
                    let persisted = movement
                        .movement_cost_cents
                        .ok_or_else(|| unsafe_replay("Una entrada no tiene costo individual"))?;
                    let after_old = add_entry_cost(old_cost, persisted)?;
                    let after_new = add_entry_cost(new_cost, persisted)?;
                    (
                        expected_stock
                            .checked_add(quantity)
                            .ok_or_else(|| unsafe_replay("El stock excede el limite permitido"))?,
                        Some(persisted),
                        after_old,
                        Some(persisted),
                        after_new,
                    )
                }
                "salida" => {
                    let old = proportional_cost_withdrawal(old_cost, expected_stock, quantity)?;
                    let new = proportional_cost_withdrawal(new_cost, expected_stock, quantity)?;
                    (
                        expected_stock - quantity,
                        old.movement_cost_cents,
                        old.inventory_cost_after_cents,
                        new.movement_cost_cents,
                        new.inventory_cost_after_cents,
                    )
                }
                "valoracion" => {
                    let absolute = movement
                        .movement_cost_cents
                        .ok_or_else(|| unsafe_replay("Una valoracion no tiene costo absoluto"))?;
                    (
                        expected_stock,
                        Some(absolute),
                        Some(absolute),
                        Some(absolute),
                        Some(absolute),
                    )
                }
                _ => return Err(unsafe_replay("El historial contiene un tipo desconocido")),
            };
        if movement.stock_after != Some(next_stock)
            || movement.movement_cost_cents != old_movement_cost
            || movement.inventory_cost_after_cents != old_after
        {
            return Err(unsafe_replay(
                "La cadena historica de stock o costo es inconsistente",
            ));
        }
        let changed = movement.inventory_cost_before_cents != new_cost
            || movement.movement_cost_cents != new_movement_cost
            || movement.inventory_cost_after_cents != new_after;
        if changed {
            affected_movements += 1;
            last_affected_at = movement.created_at.clone();
        }
        updates.push(ReplayUpdate {
            movement_id: movement.id,
            movement_cost: new_movement_cost,
            inventory_cost_before: new_cost,
            inventory_cost_after: new_after,
            sale_detail: None,
        });
        expected_stock = next_stock;
        old_cost = old_after;
        new_cost = new_after;
        index += 1;
    }
    if expected_stock != product.stock || old_cost != product_cost {
        return Err(unsafe_replay(
            "El saldo actual del producto no coincide con su historial",
        ));
    }

    product.inventory_cost_cents = new_cost;
    let mut fingerprint_target = target.clone();
    fingerprint_target.unit_price_cents = Some(new_price);
    fingerprint_target.movement_cost_cents = Some(calculated_new);
    fingerprint_target.inventory_cost_after_cents = new_target_after;
    let mut fingerprint_later = later.clone();
    for (movement, update) in fingerprint_later.iter_mut().zip(&updates) {
        movement.movement_cost_cents = update.movement_cost;
        movement.inventory_cost_before_cents = update.inventory_cost_before;
        movement.inventory_cost_after_cents = update.inventory_cost_after;
    }
    let mut fingerprint_details = details.clone();
    for update in &updates {
        if let Some((detail_id, cost)) = update.sale_detail {
            fingerprint_details
                .iter_mut()
                .find(|detail| detail.id == detail_id)
                .expect("validated replay sale detail")
                .movement_cost_cents = cost;
        }
    }
    let fingerprint_state = ReplayFingerprint {
        movement_id,
        previous_unit_price_cents: quoted_previous_price.unwrap_or(old_price),
        new_unit_price_cents: new_price,
        product: &product,
        target: &fingerprint_target,
        later_movements: &fingerprint_later,
        sales: &sales,
        sale_details: &fingerprint_details,
    };
    let canonical = serde_json::to_vec(&fingerprint_state).map_err(|_| {
        ServiceError::internal(
            "CORRECTION_FINGERPRINT_ERROR",
            "No se pudo cotizar la correccion",
        )
    })?;
    let fingerprint = format!(
        "purchase-cost-correction-v3:{}:{:x}",
        quoted_previous_price.unwrap_or(old_price),
        Sha256::digest(&canonical)
    );

    let base_unit = BaseUnit::try_from(product.base_unit.as_str())
        .map_err(|_| unsafe_replay("La unidad del producto no es valida"))?;
    let presentation_mode = target
        .presentation_mode
        .as_deref()
        .map(PresentationMode::try_from)
        .transpose()
        .map_err(|_| unsafe_replay("La forma de captura historica no es valida"))?;
    let capture_name = target
        .presentation_name
        .clone()
        .unwrap_or_else(|| direct_capture_name(base_unit).into());
    let quote = PurchaseCostCorrectionQuote {
        movement_id,
        product_id,
        product_name: product.name,
        product_sku: product.sku,
        base_unit,
        capture_name,
        presentation_id: target.presentation_id,
        presentation_name: target.presentation_name.clone(),
        presentation_mode,
        presentation_base_quantity: target.presentation_base_quantity,
        entered_quantity: target.entered_quantity.expect("eligible target quantity"),
        base_quantity,
        previous_unit_price_cents: old_price,
        new_unit_price_cents: new_price,
        previous_movement_cost_cents: old_movement_cost,
        new_movement_cost_cents: calculated_new,
        current_inventory_cost_cents: product_cost,
        corrected_inventory_cost_cents: new_cost,
        affected_movement_count: affected_movements,
        affected_sale_count: i64::try_from(affected_sales.len())
            .map_err(|_| unsafe_replay("Demasiadas ventas afectadas"))?,
        first_affected_at: target.created_at.clone(),
        last_affected_at,
        quote_fingerprint: fingerprint,
    };
    Ok(CorrectionPlan {
        quote,
        target_movement_cost: calculated_new,
        target_inventory_cost_after: new_target_after,
        updates,
    })
}

fn validate_correction_target(target: &ReplayMovement) -> ServiceResult<()> {
    if target.record_version != 2
        || target.origin != "manual"
        || target.reason != "compra"
        || target.movement_type != "entrada"
        || target.unit_price_cents.is_none()
        || target.movement_cost_cents.is_none()
        || target.entered_quantity.is_none()
        || target.base_quantity.is_none()
        || target.stock_before.is_none()
        || target.stock_after.is_none()
    {
        return Err(ServiceError::validation(
            "PURCHASE_COST_CORRECTION_NOT_ELIGIBLE",
            "Solo se puede corregir una compra manual con historial completo",
        ));
    }
    Ok(())
}

fn entry_cost(movement: &ReplayMovement, price: i64) -> ServiceResult<i64> {
    let entered = movement
        .entered_quantity
        .ok_or_else(|| unsafe_replay("La cantidad capturada esta incompleta"))?;
    let base = movement
        .base_quantity
        .ok_or_else(|| unsafe_replay("La cantidad base esta incompleta"))?;
    match movement.presentation_id {
        None => {
            if entered != base {
                return Err(unsafe_replay(
                    "La captura directa y la cantidad base no coinciden",
                ));
            }
            direct_entry_movement_cost(base, price)
        }
        Some(_) => {
            let mode = PresentationMode::try_from(
                movement
                    .presentation_mode
                    .as_deref()
                    .ok_or_else(|| unsafe_replay("La conversion historica esta incompleta"))?,
            )
            .map_err(|_| unsafe_replay("La conversion historica no es valida"))?;
            let presentation_base = movement
                .presentation_base_quantity
                .ok_or_else(|| unsafe_replay("La base historica esta incompleta"))?;
            if presentation_base_quantity(mode, entered, presentation_base)? != base {
                return Err(unsafe_replay(
                    "La conversion y las cantidades historicas no coinciden",
                ));
            }
            match mode {
                PresentationMode::Fixed => fixed_entry_movement_cost(entered, price),
                PresentationMode::Measured => {
                    measured_entry_movement_cost(entered, price, presentation_base)
                }
            }
        }
    }
}

fn add_entry_cost(cost: Option<i64>, movement_cost: i64) -> ServiceResult<Option<i64>> {
    cost.map(|value| safe_add(value, movement_cost, "CORRECTION_COST_OVERFLOW"))
        .transpose()
}

fn subtract_line_cost(
    cost: Option<i64>,
    movement_cost: Option<i64>,
    stock_after: i64,
) -> ServiceResult<Option<i64>> {
    match (cost, movement_cost) {
        (Some(before), Some(removed)) => before
            .checked_sub(removed)
            .filter(|value| *value >= 0)
            .map(Some)
            .ok_or_else(|| unsafe_replay("El replay produciria un costo negativo")),
        (None, None) if stock_after == 0 => Ok(Some(0)),
        (None, None) => Ok(None),
        _ => Err(unsafe_replay(
            "La cadena mezcla costos conocidos y desconocidos",
        )),
    }
}

fn validate_replay_row(
    movement: &ReplayMovement,
    stock: i64,
    cost: Option<i64>,
) -> ServiceResult<()> {
    if movement.record_version != 2
        || movement.base_quantity.is_none()
        || movement.stock_before != Some(stock)
        || movement.inventory_cost_before_cents != cost
    {
        return Err(unsafe_replay(
            "El historial posterior es v1, incompleto o inconsistente",
        ));
    }
    let quantity = movement.base_quantity.unwrap();
    if movement.movement_type != "valoracion" && quantity <= 0 {
        return Err(unsafe_replay("El historial contiene una cantidad invalida"));
    }
    Ok(())
}

fn validate_sale_link(movement: &ReplayMovement, detail: &ReplaySaleDetail) -> ServiceResult<()> {
    if detail.record_version == 1
        || detail.product_id <= 0
        || movement.sale_id != Some(detail.sale_id)
        || movement.presentation_id != detail.presentation_id
        || movement.base_quantity != detail.base_quantity
        || movement.entered_quantity != detail.entered_quantity
        || movement.presentation_mode != detail.presentation_mode
        || movement.presentation_base_quantity != detail.presentation_base_quantity
        || movement.base_unit != detail.base_unit
    {
        return Err(unsafe_replay(
            "Una venta relacionada es v1, incompleta o inconsistente",
        ));
    }
    Ok(())
}

fn validate_post_target_sale_details(
    connection: &Connection,
    product_id: i64,
    target: &ReplayMovement,
) -> ServiceResult<()> {
    let incomplete: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM sale_details d
            WHERE d.product_id=?1
              AND (
                d.created_at>?2
                OR EXISTS(
                    SELECT 1 FROM inventory_movements related
                    WHERE related.product_id=?1 AND related.sale_id=d.sale_id
                      AND related.origin='sale'
                      AND (related.created_at>?2 OR (related.created_at=?2 AND related.id>?3))
                )
                OR (d.created_at=?2 AND NOT EXISTS(
                    SELECT 1 FROM inventory_movements linked
                    WHERE linked.origin='sale'
                      AND linked.sale_detail_id=d.id
                      AND linked.sale_id=d.sale_id
                      AND linked.product_id=d.product_id
                      AND linked.presentation_id=d.presentation_id
                ))
              )
              AND 1<>(
                SELECT COUNT(*) FROM inventory_movements matching
                WHERE matching.origin='sale'
                  AND matching.sale_detail_id=d.id
                  AND matching.sale_id=d.sale_id
                  AND matching.product_id=d.product_id
                  AND matching.presentation_id=d.presentation_id
                  AND (matching.created_at>?2 OR (matching.created_at=?2 AND matching.id>?3))
              )
        )",
        params![product_id, target.created_at, target.id],
        |row| row.get(0),
    )?;
    if incomplete {
        return Err(unsafe_replay(
            "Los detalles y movimientos posteriores de venta no coinciden",
        ));
    }
    Ok(())
}

fn load_replay_movement(connection: &Connection, id: i64) -> ServiceResult<Option<ReplayMovement>> {
    connection
        .query_row(
            "SELECT id,record_version,origin,reason,type,presentation_id,presentation_name,presentation_mode,presentation_base_quantity,base_unit,entered_quantity,base_quantity,unit_price_cents,movement_cost_cents,stock_before,stock_after,inventory_cost_before_cents,inventory_cost_after_cents,sale_id,sale_detail_id,created_at,product_id,product_name,sku,presentation_barcode,quantity,supplier,document_reference,note,operation_token,intent_fingerprint,updated_at FROM inventory_movements WHERE id=?1",
            [id],
            map_replay_movement,
        )
        .optional()
        .map_err(Into::into)
}

fn load_later_movements(
    connection: &Connection,
    product_id: i64,
    target: &ReplayMovement,
) -> ServiceResult<Vec<ReplayMovement>> {
    Ok(connection
        .prepare("SELECT id,record_version,origin,reason,type,presentation_id,presentation_name,presentation_mode,presentation_base_quantity,base_unit,entered_quantity,base_quantity,unit_price_cents,movement_cost_cents,stock_before,stock_after,inventory_cost_before_cents,inventory_cost_after_cents,sale_id,sale_detail_id,created_at,product_id,product_name,sku,presentation_barcode,quantity,supplier,document_reference,note,operation_token,intent_fingerprint,updated_at FROM inventory_movements WHERE product_id=?1 AND (created_at>?2 OR (created_at=?2 AND id>?3)) ORDER BY created_at,id")?
        .query_map(params![product_id,target.created_at,target.id], map_replay_movement)?
        .collect::<Result<Vec<_>,_>>()?)
}

fn map_replay_movement(row: &Row<'_>) -> rusqlite::Result<ReplayMovement> {
    Ok(ReplayMovement {
        id: row.get(0)?,
        product_id: row.get(21)?,
        product_name: row.get(22)?,
        sku: row.get(23)?,
        record_version: row.get(1)?,
        origin: row.get(2)?,
        reason: row.get(3)?,
        movement_type: row.get(4)?,
        presentation_id: row.get(5)?,
        presentation_name: row.get(6)?,
        presentation_barcode: row.get(24)?,
        presentation_mode: row.get(7)?,
        presentation_base_quantity: row.get(8)?,
        base_unit: row.get(9)?,
        quantity: row.get(25)?,
        entered_quantity: row.get(10)?,
        base_quantity: row.get(11)?,
        unit_price_cents: row.get(12)?,
        movement_cost_cents: row.get(13)?,
        stock_before: row.get(14)?,
        stock_after: row.get(15)?,
        inventory_cost_before_cents: row.get(16)?,
        inventory_cost_after_cents: row.get(17)?,
        sale_id: row.get(18)?,
        sale_detail_id: row.get(19)?,
        supplier: row.get(26)?,
        document_reference: row.get(27)?,
        note: row.get(28)?,
        operation_token: row.get(29)?,
        intent_fingerprint: row.get(30)?,
        created_at: row.get(20)?,
        updated_at: row.get(31)?,
    })
}

fn load_replay_sales(
    connection: &Connection,
    ids: &BTreeSet<i64>,
) -> ServiceResult<Vec<ReplaySale>> {
    let mut sales = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(sale) = connection.query_row("SELECT id,user_id,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method,cash_received,change_amount,created_at,updated_at,cashier_name,cashier_email FROM sales WHERE id=?1", [id], |row| Ok(ReplaySale { id:row.get(0)?,user_id:row.get(1)?,cashier_name:row.get(15)?,cashier_email:row.get(16)?,checkout_token:row.get(2)?,cart_fingerprint:row.get(3)?,sale_date:row.get(4)?,subtotal:row.get(5)?,discount_type:row.get(6)?,discount_value:row.get(7)?,discount_amount:row.get(8)?,total:row.get(9)?,payment_method:row.get(10)?,cash_received:row.get(11)?,change_amount:row.get(12)?,created_at:row.get(13)?,updated_at:row.get(14)? })).optional()? { sales.push(sale); }
    }
    Ok(sales)
}

fn load_replay_sale_details(
    connection: &Connection,
    product_id: i64,
    ids: &BTreeSet<i64>,
) -> ServiceResult<Vec<ReplaySaleDetail>> {
    let mut details = Vec::new();
    for sale_id in ids {
        let mut rows = connection.prepare("SELECT id,sale_id,product_id,record_version,presentation_id,product_name,sku,presentation_name,presentation_barcode,base_unit,presentation_mode,presentation_base_quantity,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total,movement_cost_cents,created_at,updated_at FROM sale_details WHERE sale_id=?1 AND product_id=?2 ORDER BY id")?.query_map(params![sale_id,product_id], |row| Ok(ReplaySaleDetail { id:row.get(0)?,sale_id:row.get(1)?,product_id:row.get(2)?,record_version:row.get(3)?,presentation_id:row.get(4)?,product_name:row.get(5)?,sku:row.get(6)?,presentation_name:row.get(7)?,presentation_barcode:row.get(8)?,base_unit:row.get(9)?,presentation_mode:row.get(10)?,presentation_base_quantity:row.get(11)?,quantity:row.get(12)?,catalog_unit_price:row.get(13)?,unit_price:row.get(14)?,entered_quantity:row.get(15)?,base_quantity:row.get(16)?,line_total:row.get(17)?,movement_cost_cents:row.get(18)?,created_at:row.get(19)?,updated_at:row.get(20)? }))?.collect::<Result<Vec<_>,_>>()?;
        details.append(&mut rows);
    }
    Ok(details)
}

fn unsafe_replay(message: &'static str) -> ServiceError {
    ServiceError::conflict("UNSAFE_PURCHASE_COST_REPLAY", message)
}

fn stale_correction() -> ServiceError {
    ServiceError::conflict(
        "PURCHASE_COST_CORRECTION_STALE",
        "El inventario cambio; cotiza nuevamente la correccion",
    )
}

fn correction_fingerprint_previous_price(fingerprint: &str) -> ServiceResult<i64> {
    let value = fingerprint
        .strip_prefix("purchase-cost-correction-v3:")
        .and_then(|rest| rest.split_once(':'))
        .filter(|(_, digest)| digest.len() == 64)
        .and_then(|(price, digest)| {
            digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
                .then(|| price.parse::<i64>().ok())
                .flatten()
        })
        .filter(|price| (1..=JS_MAX_SAFE_INTEGER).contains(price));
    value.ok_or_else(stale_correction)
}

#[derive(Clone)]
struct NormalizedQuote {
    product_id: i64,
    presentation_id: Option<i64>,
    reason: String,
    movement_type: String,
    entered_quantity: i64,
    unit_price: Option<i64>,
    supplier: Option<String>,
    document_reference: Option<String>,
    note: Option<String>,
}

struct NormalizedCreate {
    quote: NormalizedQuote,
    supplier: Option<String>,
    document_reference: Option<String>,
    note: Option<String>,
    quote_fingerprint: Option<String>,
    intent_fingerprint: String,
}

fn normalize_quote_input(input: QuoteInventoryMovementInput) -> ServiceResult<NormalizedQuote> {
    validate_id(input.product_id)?;
    if let Some(id) = input.presentation_id {
        validate_id(id)?;
    }
    let reason = search_key(&input.reason);
    let movement_type = search_key(&input.movement_type);
    validate_movement_kind(&reason, &movement_type)?;
    let entered_quantity = parse_entered_quantity(&input.quantity, reason == "valoracion")?;
    let unit_price = normalize_unit_price(input.unit_price.as_deref(), &reason, &movement_type)?;
    Ok(NormalizedQuote {
        product_id: input.product_id,
        presentation_id: input.presentation_id,
        reason,
        movement_type,
        entered_quantity,
        unit_price,
        supplier: normalize_optional(input.supplier, 200, "supplier")?,
        document_reference: normalize_optional(
            input.document_reference,
            200,
            "document_reference",
        )?,
        note: normalize_optional(input.note, 1000, "note")?,
    })
}

fn normalize_create_input(input: CreateInventoryMovementInput) -> ServiceResult<NormalizedCreate> {
    let quote = normalize_quote_input(QuoteInventoryMovementInput {
        product_id: input.product_id,
        presentation_id: input.presentation_id,
        reason: input.reason,
        movement_type: input.movement_type,
        quantity: input.quantity,
        unit_price: input.unit_price,
        supplier: input.supplier.clone(),
        document_reference: input.document_reference.clone(),
        note: input.note.clone(),
    })?;
    let quote_fingerprint = normalize_optional(input.quote_fingerprint, 128, "quote_fingerprint")?;
    let canonical = format!(
        "{}|{}|{}:{}|{}:{}|{}|{}|{}|{}|{}|{}",
        quote.product_id,
        quote
            .presentation_id
            .map_or_else(|| "-".into(), |v| v.to_string()),
        quote.reason.len(),
        quote.reason,
        quote.movement_type.len(),
        quote.movement_type,
        quote.entered_quantity,
        optional_number(quote.unit_price),
        length_value(quote.supplier.as_deref()),
        length_value(quote.document_reference.as_deref()),
        length_value(quote.note.as_deref()),
        length_value(quote_fingerprint.as_deref())
    );
    let intent_fingerprint = format!(
        "inventory-manual-v4:{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    Ok(NormalizedCreate {
        supplier: quote.supplier.clone(),
        document_reference: quote.document_reference.clone(),
        note: quote.note.clone(),
        quote,
        quote_fingerprint,
        intent_fingerprint,
    })
}

fn calculate_quote(
    connection: &Connection,
    input: &NormalizedQuote,
) -> ServiceResult<CalculatedQuote> {
    let (name, sku, stock, unit, inventory_cost): (String, String, i64, String, Option<i64>) =
        connection
            .query_row(
                "SELECT name,sku,stock,base_unit,inventory_cost_cents FROM products WHERE id=?1",
                [input.product_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado")
            })?;
    let base_unit = BaseUnit::try_from(unit.as_str()).expect("database base unit is constrained");
    if input.movement_type == "valoracion" && input.presentation_id.is_some() {
        return Err(ServiceError::validation(
            "UNEXPECTED_VALUATION_PRESENTATION",
            "La valoracion se aplica al saldo total y no usa presentacion",
        ));
    }
    let presentation =
        load_price_presentation(connection, input.product_id, input.presentation_id)?;
    if input.presentation_id.is_some() {
        validate_presentation_mode(base_unit, presentation.mode)?;
    }
    let base_quantity = match (input.movement_type.as_str(), input.presentation_id) {
        ("valoracion", _) => 0,
        (_, Some(_)) => presentation_base_quantity(
            presentation.mode,
            input.entered_quantity,
            presentation.base_quantity,
        )?,
        (_, None) => input.entered_quantity,
    };
    if input.movement_type == "salida" && base_quantity > stock {
        return Err(ServiceError::conflict(
            "INSUFFICIENT_STOCK",
            "Stock insuficiente para completar el movimiento",
        ));
    }
    let amount = match input.movement_type.as_str() {
        "entrada" => Some(match input.presentation_id {
            Some(_) if presentation.mode == PresentationMode::Fixed => {
                fixed_entry_movement_cost(input.entered_quantity, input.unit_price.unwrap())?
            }
            Some(_) => measured_entry_movement_cost(
                input.entered_quantity,
                input.unit_price.unwrap(),
                presentation.base_quantity,
            )?,
            None => direct_entry_movement_cost(base_quantity, input.unit_price.unwrap())?,
        }),
        "salida" => {
            proportional_cost_withdrawal(inventory_cost, stock, base_quantity)?.movement_cost_cents
        }
        "valoracion" => {
            if stock == 0 && input.unit_price != Some(0) {
                return Err(ServiceError::validation_field(
                    "INVALID_INVENTORY_COST",
                    "Un producto sin stock solo puede tener valor cero",
                    "unit_price",
                ));
            }
            input.unit_price
        }
        _ => unreachable!(),
    };
    if input.movement_type == "entrada" && amount == Some(0) {
        return Err(ServiceError::validation_field(
            "INVALID_INVENTORY_COST",
            "El costo del movimiento debe ser positivo",
            "unit_price",
        ));
    }
    let resolved_unit_price = if input.movement_type == "salida" {
        inventory_cost
            .map(|cost| {
                divide_half_up_positive(i128::from(cost), i128::from(stock), "COST_OVERFLOW")
            })
            .transpose()?
    } else {
        input.unit_price
    };
    let canonical = format!(
        "{}|{}|{}|{}|{}|{}|{}:{}|{}:{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        input.product_id,
        input
            .presentation_id
            .map_or_else(|| "-".into(), |v| v.to_string()),
        length_value(Some(&name)),
        length_value(Some(&sku)),
        length_value(input.presentation_id.map(|_| presentation.name.as_str())),
        length_value(input.presentation_id.and(presentation.barcode.as_deref())),
        input.reason.len(),
        input.reason,
        input.movement_type.len(),
        input.movement_type,
        input.entered_quantity,
        base_quantity,
        base_unit.as_str(),
        presentation.mode.as_str(),
        presentation.base_quantity,
        optional_number(input.unit_price),
        optional_number(amount),
        stock,
        optional_number(inventory_cost),
        length_value(input.supplier.as_deref()),
        length_value(input.document_reference.as_deref()),
        length_value(input.note.as_deref())
    );
    Ok(CalculatedQuote {
        product_name: name,
        sku,
        stock,
        inventory_cost,
        base_unit,
        presentation,
        base_quantity,
        unit_price: resolved_unit_price,
        amount,
        fingerprint: opaque_quote_fingerprint(&canonical),
    })
}

fn public_quote(input: NormalizedQuote, value: CalculatedQuote) -> InventoryMovementQuote {
    let selected = input.presentation_id.is_some();
    InventoryMovementQuote {
        product_id: input.product_id,
        product_name: value.product_name,
        product_sku: value.sku,
        reason: input.reason.clone(),
        movement_type: input.movement_type,
        presentation_id: input.presentation_id,
        presentation_name: selected.then(|| value.presentation.name.clone()),
        capture_name: if selected {
            value.presentation.name.clone()
        } else {
            direct_capture_name(value.base_unit).into()
        },
        presentation_mode: selected.then_some(value.presentation.mode),
        presentation_base_quantity: if selected {
            value.presentation.base_quantity
        } else {
            1
        },
        entered_quantity: input.entered_quantity,
        entered_quantity_description: entered_quantity_description(
            input.entered_quantity,
            selected.then_some(value.presentation.name.as_str()),
            Some(value.presentation.mode),
            value.base_unit,
        ),
        base_quantity: value.base_quantity,
        base_quantity_description: quantity_description(value.base_quantity, value.base_unit),
        unit_price_cents: value.unit_price,
        unit_price_description: value.unit_price.map(|_| {
            price_basis_description(
                selected.then_some(value.presentation.name.as_str()),
                selected.then_some(value.presentation.mode),
                if selected {
                    value.presentation.base_quantity
                } else {
                    1
                },
                value.base_unit,
                &input.reason,
            )
        }),
        total_cost_cents: value.amount,
        base_unit: value.base_unit,
        quote_fingerprint: value.fingerprint,
    }
}

fn load_price_presentation(
    connection: &Connection,
    product_id: i64,
    presentation_id: Option<i64>,
) -> ServiceResult<PriceSnapshot> {
    let Some(presentation_id) = presentation_id else {
        return Ok(PriceSnapshot {
            name: String::new(),
            barcode: None,
            mode: PresentationMode::Fixed,
            base_quantity: 1,
        });
    };
    connection
        .query_row("SELECT name,barcode,mode,base_quantity FROM product_presentations WHERE id=?1 AND product_id=?2 AND is_active=1", params![presentation_id, product_id], |row| {
            Ok(PriceSnapshot {
                name: row.get(0)?,
                barcode: row.get(1)?,
                mode: PresentationMode::try_from(row.get::<_, String>(2)?.as_str()).unwrap(),
                base_quantity: row.get(3)?,
            })
        })
        .optional()?
        .ok_or_else(|| {
            ServiceError::conflict(
                "PRESENTATION_UNAVAILABLE",
                "La presentacion no existe o no esta activa",
            )
        })
}

fn movement_by_id(connection: &Connection, id: i64) -> ServiceResult<InventoryMovement> {
    let sql=format!("SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE m.id=?1");
    connection
        .query_row(&sql, [id], map_movement)
        .map_err(Into::into)
}
fn movement_by_token(
    connection: &Connection,
    token: &str,
) -> ServiceResult<Option<(InventoryMovement, String)>> {
    let sql=format!("SELECT {MOVEMENT_COLUMNS},m.intent_fingerprint FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE m.operation_token=?1");
    connection
        .query_row(&sql, [token], |row| Ok((map_movement(row)?, row.get(33)?)))
        .optional()
        .map_err(Into::into)
}

fn map_inventory_product(row: &Row<'_>) -> rusqlite::Result<InventoryProductRow> {
    let stock = row.get::<_, i64>(3)?;
    let inventory_cost_cents = row.get::<_, Option<i64>>(5)?;
    let average_cost_cents = inventory_cost_cents.map(|cost| {
        if stock == 0 {
            0
        } else {
            (cost + stock / 2) / stock
        }
    });
    Ok(InventoryProductRow {
        id: row.get(0)?,
        name: row.get(1)?,
        sku: row.get(2)?,
        stock,
        base_unit: BaseUnit::try_from(row.get::<_, String>(4)?.as_str())
            .expect("database base unit is constrained"),
        inventory_cost_cents,
        inventory_cost_status: if inventory_cost_cents.is_some() {
            "known"
        } else {
            "unknown"
        },
        average_cost_cents,
    })
}

fn inventory_movement_product(
    connection: &Connection,
    product: InventoryProductRow,
) -> ServiceResult<InventoryMovementProduct> {
    let presentations=connection.prepare("SELECT id,product_id,name,mode,base_quantity,is_active FROM product_presentations WHERE product_id=?1 AND is_active=1 ORDER BY is_default DESC,id")?.query_map([product.id],|row|Ok(InventoryMovementPresentation{id:row.get(0)?,product_id:row.get(1)?,name:row.get(2)?,mode:PresentationMode::try_from(row.get::<_,String>(3)?.as_str()).unwrap(),base_quantity:row.get(4)?,is_active:row.get::<_,i64>(5)?!=0}))?.collect::<Result<Vec<_>,_>>()?;
    Ok(InventoryMovementProduct {
        id: product.id,
        name: product.name,
        sku: product.sku,
        stock: product.stock,
        base_unit: product.base_unit,
        inventory_cost_cents: product.inventory_cost_cents,
        inventory_cost_status: product.inventory_cost_status,
        average_cost_cents: product.average_cost_cents,
        presentations,
    })
}

fn inventory_stats(connection: &Connection) -> ServiceResult<InventoryViewStats> {
    let mut stats = InventoryViewStats {
        product_count: 0,
        piece_stock: 0,
        gram_stock: 0,
        milliliter_stock: 0,
        centimeter_stock: 0,
        inventory_value_cents: 0,
        products_without_cost: 0,
    };
    let mut statement =
        connection.prepare("SELECT stock,base_unit,inventory_cost_cents FROM products")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })? {
        let (stock, unit, inventory_cost) = row?;
        stats.product_count = checked_total(stats.product_count, 1, "INVENTORY_STATS_OVERFLOW")?;
        let target = match unit.as_str() {
            "piece" => &mut stats.piece_stock,
            "gram" => &mut stats.gram_stock,
            "milliliter" => &mut stats.milliliter_stock,
            "centimeter" => &mut stats.centimeter_stock,
            _ => unreachable!(),
        };
        *target = checked_total(*target, stock, "INVENTORY_STATS_OVERFLOW")?;
        match inventory_cost {
            Some(cost) => {
                stats.inventory_value_cents = checked_total(
                    stats.inventory_value_cents,
                    cost,
                    "INVENTORY_STATS_OVERFLOW",
                )?
            }
            None => {
                stats.products_without_cost =
                    checked_total(stats.products_without_cost, 1, "INVENTORY_STATS_OVERFLOW")?
            }
        }
    }
    Ok(stats)
}

fn validate_movement_kind(reason: &str, movement_type: &str) -> ServiceResult<()> {
    match (reason, movement_type) {
        ("compra", "entrada")
        | ("ajuste", "entrada")
        | ("ajuste", "salida")
        | ("valoracion", "valoracion") => Ok(()),
        ("venta", _) => Err(ServiceError::validation(
            "INVALID_SALE_MOVEMENT",
            "Los movimientos de venta solo se crean desde el servicio de ventas",
        )),
        _ => Err(ServiceError::validation(
            "INVALID_MOVEMENT_TYPE",
            "Combinacion de motivo y tipo de movimiento invalida",
        )),
    }
}
fn parse_entered_quantity(value: &str, allow_zero: bool) -> ServiceResult<i64> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid_quantity());
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|q| (*q > 0 || (allow_zero && *q == 0)) && *q <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(invalid_quantity)
}
fn normalize_unit_price(
    value: Option<&str>,
    reason: &str,
    movement_type: &str,
) -> ServiceResult<Option<i64>> {
    let price = value
        .map(|value| parse_money_text_allow_zero(value, reason == "valoracion"))
        .transpose()
        .map_err(|_| {
            ServiceError::validation_field(
                "INVALID_INVENTORY_COST",
                "El precio de compra no es valido",
                "unit_price",
            )
        })?;
    if movement_type == "salida" && price.is_some() {
        return Err(ServiceError::validation_field(
            "UNEXPECTED_INVENTORY_COST",
            "Las salidas calculan su costo desde el inventario",
            "unit_price",
        ));
    }
    if (movement_type == "entrada" || reason == "valoracion") && price.is_none() {
        return Err(ServiceError::validation_field(
            "INVENTORY_COST_REQUIRED",
            "La operacion requiere un costo",
            "unit_price",
        ));
    }
    Ok(price)
}
fn invalid_quantity() -> ServiceError {
    ServiceError::validation_field(
        "INVALID_MOVEMENT_QUANTITY",
        "La cantidad no es valida",
        "quantity",
    )
}
fn safe_add(left: i64, right: i64, code: &'static str) -> ServiceResult<i64> {
    left.checked_add(right)
        .filter(|v| *v <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(|| ServiceError::conflict(code, "El valor excede el limite permitido"))
}
fn normalize_filter(
    value: Option<String>,
    allowed: &[&str],
    code: &'static str,
) -> ServiceResult<Option<String>> {
    match value.map(|v| search_key(&v)) {
        Some(v) if v.is_empty() => Ok(None),
        Some(v) if allowed.contains(&v.as_str()) => Ok(Some(v)),
        Some(_) => Err(ServiceError::validation(
            code,
            "El filtro seleccionado no es valido",
        )),
        None => Ok(None),
    }
}
fn normalize_optional(
    value: Option<String>,
    max: usize,
    field: &'static str,
) -> ServiceResult<Option<String>> {
    let value = value
        .map(|v| normalize_persisted(&v))
        .filter(|v| !v.is_empty());
    if value.as_ref().is_some_and(|v| v.chars().count() > max) {
        return Err(ServiceError::validation_field(
            "INVALID_INVENTORY_METADATA",
            "El campo excede la longitud permitida",
            field,
        ));
    }
    Ok(value)
}
fn normalize_token(value: &str) -> ServiceResult<String> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > OPERATION_TOKEN_MAX_LENGTH
    {
        return Err(ServiceError::validation(
            "INVALID_INVENTORY_OPERATION_TOKEN",
            "El token de operacion no es valido",
        ));
    }
    Ok(value.into())
}
fn page(value: Option<i64>, field: &'static str) -> ServiceResult<i64> {
    match value.unwrap_or(1) {
        v @ 1..=450_359_962_737_049 => Ok(v),
        _ => Err(ServiceError::validation_field(
            "INVALID_PAGE",
            "La pagina no es valida",
            field,
        )),
    }
}
fn normalize_date(value: Option<String>, field: &'static str) -> ServiceResult<Option<String>> {
    let Some(value) = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let valid = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        && Connection::open_in_memory()?
            .query_row("SELECT date(?1)=?1", [&value], |row| row.get::<_, i64>(0))
            .unwrap_or(0)
            == 1;
    if !valid {
        return Err(ServiceError::validation_field(
            "INVALID_DATE",
            "La fecha no es valida",
            field,
        ));
    }
    Ok(Some(value))
}
fn direct_capture_name(unit: BaseUnit) -> &'static str {
    match unit {
        BaseUnit::Piece => "Piezas individuales",
        BaseUnit::Gram => "Peso exacto en gramos",
        BaseUnit::Milliliter => "Volumen exacto en mililitros",
        BaseUnit::Centimeter => "Longitud exacta en centimetros",
    }
}
fn quantity_description(quantity: i64, unit: BaseUnit) -> String {
    let name = match (unit, quantity) {
        (BaseUnit::Piece, 1) => "pieza",
        (BaseUnit::Piece, _) => "piezas",
        (BaseUnit::Gram, 1) => "gramo",
        (BaseUnit::Gram, _) => "gramos",
        (BaseUnit::Milliliter, 1) => "mililitro",
        (BaseUnit::Milliliter, _) => "mililitros",
        (BaseUnit::Centimeter, 1) => "centimetro",
        (BaseUnit::Centimeter, _) => "centimetros",
    };
    format!("{quantity} {name}")
}
fn entered_quantity_description(
    quantity: i64,
    name: Option<&str>,
    mode: Option<PresentationMode>,
    unit: BaseUnit,
) -> String {
    match (name, mode) {
        (Some(name), Some(PresentationMode::Fixed)) => format!("{quantity} x {name}"),
        _ => quantity_description(quantity, unit),
    }
}
fn price_basis_description(
    presentation_name: Option<&str>,
    mode: Option<PresentationMode>,
    factor: i64,
    unit: BaseUnit,
    reason: &str,
) -> String {
    if reason == "valoracion" {
        return "valor total del inventario".into();
    }
    match (presentation_name, mode) {
        (Some(name), Some(PresentationMode::Fixed)) => format!("por {name}"),
        (Some(_), Some(PresentationMode::Measured)) => {
            format!("por {}", quantity_description(factor, unit))
        }
        _ => format!("por {}", quantity_description(1, unit)),
    }
}
fn optional_number(value: Option<i64>) -> String {
    value.map_or_else(|| "-".into(), |value| value.to_string())
}
fn length_value(value: Option<&str>) -> String {
    value.map_or_else(|| "-".into(), |v| format!("{}:{v}", v.len()))
}
fn opaque_quote_fingerprint(canonical: &str) -> String {
    format!(
        "inventory-quote-v4:{:x}",
        Sha256::digest(canonical.as_bytes())
    )
}
fn token_reused() -> ServiceError {
    ServiceError::conflict(
        "INVENTORY_OPERATION_TOKEN_REUSED",
        "El token de operacion ya fue usado con otra intencion",
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        db::migrations,
        models::{
            cart::CheckoutCartInput,
            inventory_movement::{
                ConfirmPurchaseCostCorrectionInput, CreateInventoryMovementInput,
                InventoryMovement, InventoryMovementQuote, InventoryViewFilters,
                QuoteInventoryMovementInput, QuotePurchaseCostCorrectionInput,
            },
        },
        services::{cart::SessionCart, sales},
    };
    use rusqlite::Connection;

    type CostRow = (i64, Option<i64>, Option<i64>, Option<i64>);
    type CommercialLine = (i64, i64, i64, Option<i64>);
    type CommercialSnapshot = (i64, String, i64, i64, i64, Vec<CommercialLine>);

    fn database(unit: &str, stock: i64, cost: Option<i64>, base: i64, mode: &str) -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut c).unwrap();
        c.execute_batch("INSERT INTO brands(name,name_search_key)VALUES('B','b');INSERT INTO categories(name,name_search_key)VALUES('C','c');").unwrap();
        c.execute("INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)VALUES('P','p',100,?1,'SKU','sku',1,1,?2,?3)",rusqlite::params![stock,unit,cost]).unwrap();
        c.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)VALUES(1,'Default',?1,?2,100,1)",rusqlite::params![mode,base]).unwrap();
        c
    }
    fn quote_input(
        presentation_id: Option<i64>,
        reason: &str,
        kind: &str,
        quantity: &str,
        price: Option<&str>,
    ) -> QuoteInventoryMovementInput {
        QuoteInventoryMovementInput {
            product_id: 1,
            presentation_id,
            reason: reason.into(),
            movement_type: kind.into(),
            quantity: quantity.into(),
            unit_price: price.map(str::to_owned),
            supplier: None,
            document_reference: None,
            note: None,
        }
    }
    fn create_input(
        quote: &InventoryMovementQuote,
        token: &str,
        price: Option<&str>,
    ) -> CreateInventoryMovementInput {
        CreateInventoryMovementInput {
            product_id: quote.product_id,
            presentation_id: quote.presentation_id,
            reason: quote.reason.clone(),
            movement_type: quote.movement_type.clone(),
            quantity: quote.entered_quantity.to_string(),
            unit_price: price.map(str::to_owned),
            supplier: None,
            document_reference: None,
            note: None,
            operation_token: token.into(),
            quote_fingerprint: Some(quote.quote_fingerprint.clone()),
        }
    }

    fn create_manual(
        connection: &mut Connection,
        presentation_id: Option<i64>,
        reason: &str,
        kind: &str,
        quantity: &str,
        price: Option<&str>,
        token: &str,
    ) -> InventoryMovement {
        let quote = super::quote(
            connection,
            quote_input(presentation_id, reason, kind, quantity, price),
        )
        .unwrap();
        super::create(connection, create_input(&quote, token, price)).unwrap()
    }

    fn correction_quote(
        connection: &Connection,
        movement_id: i64,
        price: &str,
    ) -> crate::models::inventory_movement::PurchaseCostCorrectionQuote {
        super::quote_purchase_cost_correction(
            connection,
            QuotePurchaseCostCorrectionInput {
                movement_id,
                new_unit_price: price.into(),
            },
        )
        .unwrap()
    }

    fn confirm_correction(
        connection: &mut Connection,
        quote: &crate::models::inventory_movement::PurchaseCostCorrectionQuote,
        price: &str,
    ) -> crate::models::inventory_movement::PurchaseCostCorrectionQuote {
        super::confirm_purchase_cost_correction(
            connection,
            ConfirmPurchaseCostCorrectionInput {
                movement_id: quote.movement_id,
                new_unit_price: price.into(),
                quote_fingerprint: quote.quote_fingerprint.clone(),
            },
        )
        .unwrap()
    }

    #[test]
    fn fixed_measured_and_direct_entries_use_shared_cost_rules() {
        let mut fixed = database("piece", 5, Some(1_000), 10, "fixed");
        let quote = super::quote(
            &fixed,
            quote_input(Some(1), "compra", "entrada", "3", Some("2.50")),
        )
        .unwrap();
        assert_eq!(
            (quote.base_quantity, quote.total_cost_cents),
            (30, Some(750))
        );
        let movement =
            super::create(&mut fixed, create_input(&quote, "fixed", Some("2.50"))).unwrap();
        assert_eq!(
            (movement.stock_after, movement.inventory_cost_after_cents),
            (Some(35), Some(1_750))
        );

        let mut measured = database("gram", 1_000, Some(4_000), 1_000, "measured");
        let quote = super::quote(
            &measured,
            quote_input(Some(1), "compra", "entrada", "250", Some("12.00")),
        )
        .unwrap();
        assert_eq!(
            (quote.base_quantity, quote.total_cost_cents),
            (250, Some(300))
        );
        super::create(
            &mut measured,
            create_input(&quote, "measured", Some("12.00")),
        )
        .unwrap();

        let mut direct = database("centimeter", 100, Some(1_000), 100, "measured");
        let quote = super::quote(
            &direct,
            quote_input(None, "ajuste", "entrada", "75", Some("0.10")),
        )
        .unwrap();
        assert_eq!(
            (quote.base_quantity, quote.total_cost_cents),
            (75, Some(750))
        );
        let movement =
            super::create(&mut direct, create_input(&quote, "direct", Some("0.10"))).unwrap();
        assert_eq!(
            (
                movement.presentation_id,
                movement.presentation_mode.as_deref()
            ),
            (None, None)
        );
    }

    #[test]
    fn quote_and_create_reject_fixed_presentations_for_measured_products() {
        for unit in ["gram", "milliliter", "centimeter"] {
            let invalid = database(unit, 1_000, Some(4_000), 100, "fixed");
            assert_eq!(
                super::quote(
                    &invalid,
                    quote_input(Some(1), "compra", "entrada", "2", Some("1.00")),
                )
                .unwrap_err()
                .code,
                "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT"
            );

            let mut changed = database(unit, 1_000, Some(4_000), 100, "measured");
            let quote = super::quote(
                &changed,
                quote_input(Some(1), "compra", "entrada", "2", Some("1.00")),
            )
            .unwrap();
            changed
                .execute(
                    "UPDATE product_presentations SET mode='fixed' WHERE id=1",
                    [],
                )
                .unwrap();
            assert_eq!(
                super::create(
                    &mut changed,
                    create_input(&quote, &format!("invalid-{unit}"), Some("1.00")),
                )
                .unwrap_err()
                .code,
                "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT"
            );
        }
    }

    #[test]
    fn known_incoming_cost_does_not_make_an_unknown_accumulated_cost_known() {
        for reason in ["compra", "ajuste"] {
            let mut c = database("gram", 1_000, None, 1_000, "measured");
            let quote = super::quote(
                &c,
                quote_input(Some(1), reason, "entrada", "250", Some("12.00")),
            )
            .unwrap();
            assert_eq!(quote.total_cost_cents, Some(300));

            let movement = super::create(
                &mut c,
                create_input(&quote, &format!("unknown-{reason}"), Some("12.00")),
            )
            .unwrap();
            assert_eq!(movement.movement_cost_cents, Some(300));
            assert_eq!(movement.inventory_cost_before_cents, None);
            assert_eq!(movement.inventory_cost_after_cents, None);
            assert_eq!(
                c.query_row(
                    "SELECT inventory_cost_cents FROM products WHERE id=1",
                    [],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .unwrap(),
                None
            );
        }
    }

    #[test]
    fn output_removes_proportional_cost_and_depletion_normalizes_unknown_cost() {
        let mut known = database("piece", 4, Some(1_001), 1, "fixed");
        let quote = super::quote(&known, quote_input(None, "ajuste", "salida", "1", None)).unwrap();
        assert_eq!(quote.total_cost_cents, Some(250));
        let movement = super::create(&mut known, create_input(&quote, "known-out", None)).unwrap();
        assert_eq!(
            (
                movement.unit_price_cents,
                movement.inventory_cost_after_cents
            ),
            (Some(250), Some(751))
        );

        let mut unknown = database("piece", 3, None, 1, "fixed");
        let quote =
            super::quote(&unknown, quote_input(None, "ajuste", "salida", "1", None)).unwrap();
        let movement =
            super::create(&mut unknown, create_input(&quote, "unknown-partial", None)).unwrap();
        assert_eq!(
            (
                movement.movement_cost_cents,
                movement.inventory_cost_after_cents
            ),
            (None, None)
        );
        let quote =
            super::quote(&unknown, quote_input(None, "ajuste", "salida", "2", None)).unwrap();
        let movement =
            super::create(&mut unknown, create_input(&quote, "unknown-empty", None)).unwrap();
        assert_eq!(
            (movement.stock_after, movement.inventory_cost_after_cents),
            (Some(0), Some(0))
        );
    }

    #[test]
    fn valuation_changes_only_accumulated_cost_and_allows_zero_for_empty_stock() {
        let mut c = database("centimeter", 175, None, 100, "measured");
        let quote = super::quote(
            &c,
            quote_input(None, "valoracion", "valoracion", "0", Some("43.75")),
        )
        .unwrap();
        let movement =
            super::create(&mut c, create_input(&quote, "valuation", Some("43.75"))).unwrap();
        assert_eq!(
            (movement.stock_before, movement.stock_after),
            (Some(175), Some(175))
        );
        assert_eq!(
            (
                movement.movement_cost_cents,
                movement.inventory_cost_after_cents
            ),
            (Some(4_375), Some(4_375))
        );

        let empty = database("piece", 0, Some(0), 1, "fixed");
        assert_eq!(
            super::quote(
                &empty,
                quote_input(None, "valoracion", "valoracion", "0", Some("1.00"))
            )
            .unwrap_err()
            .code,
            "INVALID_INVENTORY_COST"
        );
    }

    #[test]
    fn quote_fingerprint_covers_cost_state_conversion_price_and_metadata() {
        let mut c = database("gram", 1_000, Some(4_000), 1_000, "measured");
        let mut with_metadata = quote_input(Some(1), "compra", "entrada", "250", Some("12.00"));
        with_metadata.supplier = Some("  Proveedor  ".into());
        with_metadata.document_reference = Some(" FAC-1 ".into());
        with_metadata.note = Some(" Recepcion ".into());
        let quote = super::quote(&c, with_metadata).unwrap();
        let plain = super::quote(
            &c,
            quote_input(Some(1), "compra", "entrada", "250", Some("12.00")),
        )
        .unwrap();
        assert_ne!(quote.quote_fingerprint, plain.quote_fingerprint);

        let mut input = create_input(&quote, "metadata", Some("12.00"));
        input.supplier = Some("Proveedor".into());
        input.document_reference = Some("FAC-1".into());
        input.note = Some("Recepcion".into());
        let movement = super::create(&mut c, input).unwrap();
        assert_eq!(movement.supplier.as_deref(), Some("Proveedor"));
        assert_eq!(movement.document_reference.as_deref(), Some("FAC-1"));
        assert_eq!(movement.note.as_deref(), Some("Recepcion"));

        for (token, sql) in [
            (
                "stale-stock",
                "UPDATE products SET stock=stock+1 WHERE id=1",
            ),
            (
                "stale-cost",
                "UPDATE products SET inventory_cost_cents=inventory_cost_cents+1 WHERE id=1",
            ),
            (
                "stale-conversion",
                "UPDATE product_presentations SET base_quantity=500 WHERE id=1",
            ),
            (
                "stale-sku",
                "UPDATE products SET sku='SKU-EDITADO' WHERE id=1",
            ),
            (
                "stale-presentation-name",
                "UPDATE product_presentations SET name='Kilogramo editado' WHERE id=1",
            ),
            (
                "stale-presentation-barcode",
                "UPDATE product_presentations SET barcode='750000000001',barcode_search_key='750000000001' WHERE id=1",
            ),
        ] {
            let mut stale = database("gram", 1_000, Some(4_000), 1_000, "measured");
            let quote = super::quote(
                &stale,
                quote_input(Some(1), "compra", "entrada", "250", Some("12.00")),
            )
            .unwrap();
            stale.execute(sql, []).unwrap();
            assert_eq!(
                super::create(&mut stale, create_input(&quote, token, Some("12.00")))
                    .unwrap_err()
                    .code,
                "QUOTE_CHANGED"
            );
        }

        let quote = super::quote(
            &c,
            quote_input(Some(1), "compra", "entrada", "250", Some("12.00")),
        )
        .unwrap();
        assert_eq!(
            super::create(&mut c, create_input(&quote, "stale-price", Some("13.00")))
                .unwrap_err()
                .code,
            "QUOTE_CHANGED"
        );
    }

    #[test]
    fn inventory_view_filters_known_and_unknown_costs() {
        let c = database("piece", 5, Some(500), 1, "fixed");
        let known = super::view(
            &c,
            InventoryViewFilters {
                cost_status: Some("known".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(known.product_total, 1);
        let unknown = super::view(
            &c,
            InventoryViewFilters {
                cost_status: Some("unknown".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(unknown.product_total, 0);
    }

    #[test]
    fn purchase_correction_without_later_rows_is_read_only_then_idempotent() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "10",
            Some("1.00"),
            "purchase",
        );
        let immutable_before: (i64, i64, i64, String, Option<String>) = connection
            .query_row(
                "SELECT entered_quantity,base_quantity,stock_after,operation_token,note FROM inventory_movements WHERE id=?1",
                [purchase.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();

        let quote = correction_quote(&connection, purchase.id, "1.50");
        assert_eq!(
            (
                quote.previous_movement_cost_cents,
                quote.new_movement_cost_cents,
                quote.current_inventory_cost_cents,
                quote.corrected_inventory_cost_cents,
                quote.affected_movement_count,
                quote.affected_sale_count,
            ),
            (1_000, 1_500, Some(1_000), Some(1_500), 0, 0)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            100
        );

        let applied = confirm_correction(&mut connection, &quote, "1.50");
        assert_eq!(applied.corrected_inventory_cost_cents, Some(1_500));
        let retry = confirm_correction(&mut connection, &quote, "1.50");
        assert_eq!(retry.current_inventory_cost_cents, Some(1_500));
        assert_eq!(
            connection
                .query_row(
                    "SELECT entered_quantity,base_quantity,stock_after,operation_token,note FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                )
                .unwrap(),
            immutable_before
        );

        create_manual(
            &mut connection,
            None,
            "ajuste",
            "entrada",
            "1",
            Some("1.00"),
            "after-correction",
        );
        assert_eq!(
            super::confirm_purchase_cost_correction(
                &mut connection,
                ConfirmPurchaseCostCorrectionInput {
                    movement_id: quote.movement_id,
                    new_unit_price: "1.50".into(),
                    quote_fingerprint: quote.quote_fingerprint,
                },
            )
            .unwrap_err()
            .code,
            "PURCHASE_COST_CORRECTION_STALE"
        );
    }

    #[test]
    fn purchase_correction_rejects_same_price_quotes() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "same-price",
        );

        let error = super::quote_purchase_cost_correction(
            &connection,
            QuotePurchaseCostCorrectionInput {
                movement_id: purchase.id,
                new_unit_price: "1.0".into(),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "PURCHASE_COST_CORRECTION_SAME_PRICE");
        assert_eq!(error.field, Some("new_unit_price"));
    }

    #[test]
    fn product_metadata_changes_invalidate_purchase_correction_quote() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "metadata-stale",
        );
        let quote = correction_quote(&connection, purchase.id, "2.00");
        connection
            .execute(
                "UPDATE products SET name='Renamed',name_search_key='renamed',sku='RENAMED',sku_search_key='renamed',updated_at='2099-01-01 00:00:00' WHERE id=1",
                [],
            )
            .unwrap();

        assert_eq!(
            super::confirm_purchase_cost_correction(
                &mut connection,
                ConfirmPurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "2.00".into(),
                    quote_fingerprint: quote.quote_fingerprint,
                },
            )
            .unwrap_err()
            .code,
            "PURCHASE_COST_CORRECTION_STALE"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            100
        );
    }

    #[test]
    fn correction_quote_rejects_post_target_sale_detail_without_a_movement() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "orphan-detail",
        );
        connection.execute("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','orphan@b.co','hash')", []).unwrap();
        connection.execute("INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method,created_at,updated_at) VALUES(1,'A B','orphan@b.co','orphan-sale','cart',CURRENT_TIMESTAMP,100,'none',0,0,100,'tarjeta',datetime(?1,'+1 second'),datetime(?1,'+1 second'))", [&purchase.created_at]).unwrap();
        connection.execute("INSERT INTO sale_details(product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total,movement_cost_cents,created_at,updated_at) VALUES(1,1,3,1,'P','SKU','Default','piece','fixed',1,1,100,100,1,1,100,100,datetime(?1,'+1 second'),datetime(?1,'+1 second'))", [&purchase.created_at]).unwrap();

        assert_eq!(
            super::quote_purchase_cost_correction(
                &connection,
                QuotePurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "2.00".into(),
                },
            )
            .unwrap_err()
            .code,
            "UNSAFE_PURCHASE_COST_REPLAY"
        );
    }

    #[test]
    fn correction_confirm_rejects_incomplete_sale_association_without_writes() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        connection.execute("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','incomplete@b.co','hash')", []).unwrap();
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "incomplete-detail",
        );
        let quote = correction_quote(&connection, purchase.id, "2.00");
        let mut cart = SessionCart::from([(1, 1)]);
        let resolved =
            crate::services::cart::set_discount(&connection, &mut cart, "percentage", "0").unwrap();
        let receipt = sales::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "incomplete-sale".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        connection.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price) VALUES(1,'Extra','fixed',1,100)", []).unwrap();
        connection.execute("INSERT INTO sale_details(product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,catalog_unit_price,unit_price,entered_quantity,base_quantity,line_total,movement_cost_cents) VALUES(1,?1,3,2,'P','SKU','Extra','piece','fixed',1,1,100,100,1,1,100,100)", [receipt.sale.id]).unwrap();
        let before: (i64, Option<i64>) = connection
            .query_row(
                "SELECT m.unit_price_cents,p.inventory_cost_cents FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE m.id=?1",
                [purchase.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(
            super::confirm_purchase_cost_correction(
                &mut connection,
                ConfirmPurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "2.00".into(),
                    quote_fingerprint: quote.quote_fingerprint,
                },
            )
            .unwrap_err()
            .code,
            "UNSAFE_PURCHASE_COST_REPLAY"
        );
        let after: (i64, Option<i64>) = connection
            .query_row(
                "SELECT m.unit_price_cents,p.inventory_cost_cents FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE m.id=?1",
                [purchase.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn correction_quote_rejects_duplicate_sale_movement_association() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        connection.execute("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','duplicate@b.co','hash')", []).unwrap();
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "duplicate-detail",
        );
        let mut cart = SessionCart::from([(1, 1)]);
        let resolved =
            crate::services::cart::set_discount(&connection, &mut cart, "percentage", "0").unwrap();
        sales::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "duplicate-sale".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        connection
            .execute_batch(
                "DROP INDEX idx_inventory_movements_sale_detail_id;
                 INSERT INTO inventory_movements(
                    product_id,product_name,sku,product_search_key,sku_search_key,sale_id,
                    record_version,presentation_id,sale_detail_id,presentation_name,
                    presentation_barcode,base_unit,presentation_mode,presentation_base_quantity,
                    unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                    stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                    inventory_cost_after_cents,supplier,provider_search_key,document_reference,
                    document_reference_search_key,note,note_search_key,operation_token,
                    intent_fingerprint,created_at,updated_at)
                 SELECT product_id,product_name,sku,product_search_key,sku_search_key,sale_id,
                    record_version,presentation_id,sale_detail_id,presentation_name,
                    presentation_barcode,base_unit,presentation_mode,presentation_base_quantity,
                    unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
                    stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
                    inventory_cost_after_cents,supplier,provider_search_key,document_reference,
                    document_reference_search_key,note,note_search_key,operation_token,
                    intent_fingerprint,created_at,updated_at
                 FROM inventory_movements WHERE origin='sale' LIMIT 1;",
            )
            .unwrap();

        assert_eq!(
            super::quote_purchase_cost_correction(
                &connection,
                QuotePurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "2.00".into(),
                },
            )
            .unwrap_err()
            .code,
            "UNSAFE_PURCHASE_COST_REPLAY"
        );
    }

    #[test]
    fn correction_replays_entries_outputs_and_stops_at_valuation() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "10",
            Some("1.00"),
            "p",
        );
        let entry = create_manual(
            &mut connection,
            None,
            "ajuste",
            "entrada",
            "10",
            Some("2.00"),
            "in",
        );
        let output = create_manual(&mut connection, None, "ajuste", "salida", "5", None, "out");
        let valuation = create_manual(
            &mut connection,
            None,
            "valoracion",
            "valoracion",
            "0",
            Some("30.00"),
            "value",
        );
        let after = create_manual(
            &mut connection,
            None,
            "ajuste",
            "salida",
            "5",
            None,
            "after",
        );
        let quote = correction_quote(&connection, purchase.id, "2.00");
        assert_eq!(quote.corrected_inventory_cost_cents, Some(2_000));
        assert_eq!(quote.affected_movement_count, 3);
        confirm_correction(&mut connection, &quote, "2.00");
        let rows: Vec<CostRow> = connection
            .prepare("SELECT id,movement_cost_cents,inventory_cost_before_cents,inventory_cost_after_cents FROM inventory_movements ORDER BY id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))
            .unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(rows[1], (entry.id, Some(2_000), Some(2_000), Some(4_000)));
        assert_eq!(rows[2], (output.id, Some(1_000), Some(4_000), Some(3_000)));
        assert_eq!(
            rows[3],
            (valuation.id, Some(3_000), Some(3_000), Some(3_000))
        );
        assert_eq!(rows[4], (after.id, Some(1_000), Some(3_000), Some(2_000)));
    }

    #[test]
    fn later_purchase_quote_is_stale_when_earlier_cost_replay_ends_at_same_valuation() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        connection.execute("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','replay@b.co','hash')", []).unwrap();
        let earlier = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "10",
            Some("1.00"),
            "purchase-a",
        );
        let target = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "10",
            Some("2.00"),
            "purchase-b",
        );
        let cart = SessionCart::from([(1, 5)]);
        let resolved = crate::services::cart::get(&connection, &cart).unwrap();
        sales::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "replay-sale".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        create_manual(
            &mut connection,
            None,
            "valoracion",
            "valoracion",
            "0",
            Some("30.00"),
            "replay-valuation",
        );

        let target_quote = correction_quote(&connection, target.id, "3.00");
        let earlier_quote = correction_quote(&connection, earlier.id, "1.50");
        assert_eq!(target_quote.corrected_inventory_cost_cents, Some(3_000));
        assert_eq!(earlier_quote.corrected_inventory_cost_cents, Some(3_000));
        let applied_earlier = confirm_correction(&mut connection, &earlier_quote, "1.50");
        let retried_earlier = confirm_correction(&mut connection, &earlier_quote, "1.50");
        assert_eq!(
            retried_earlier.quote_fingerprint,
            applied_earlier.quote_fingerprint
        );
        assert_eq!(
            connection
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            3_000
        );

        let error = super::confirm_purchase_cost_correction(
            &mut connection,
            ConfirmPurchaseCostCorrectionInput {
                movement_id: target.id,
                new_unit_price: "3.00".into(),
                quote_fingerprint: target_quote.quote_fingerprint,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "PURCHASE_COST_CORRECTION_STALE");
        assert_eq!(
            connection
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [target.id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            200
        );
    }

    #[test]
    fn correction_replays_grouped_sale_costs_without_touching_commercial_fields() {
        let mut connection = database("piece", 0, Some(0), 1, "fixed");
        connection.execute("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','a@b.co','hash')", []).unwrap();
        connection.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price) VALUES(1,'Bolsa','fixed',100,8000)", []).unwrap();
        let purchase = create_manual(
            &mut connection,
            None,
            "compra",
            "entrada",
            "250",
            Some("1.00"),
            "sale-purchase",
        );
        let mut cart = SessionCart::from([(1, 3), (2, 1)]);
        crate::services::cart::set_unit_price(&connection, &mut cart, 1, "0.75").unwrap();
        let resolved =
            crate::services::cart::set_discount(&connection, &mut cart, "percentage", "10")
                .unwrap();
        let receipt = sales::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "sale".into(),
                cart_fingerprint: resolved.cart_fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        let commercial_before: CommercialSnapshot = (
            receipt.sale.subtotal,
            format!("{:?}", receipt.sale.discount_type),
            receipt.sale.discount_value,
            receipt.sale.discount_amount,
            receipt.sale.total,
            receipt
                .details
                .iter()
                .map(|line| {
                    (
                        line.catalog_unit_price.unwrap(),
                        line.unit_price,
                        line.line_total.unwrap(),
                        line.base_quantity,
                    )
                })
                .collect(),
        );
        let quote = correction_quote(&connection, purchase.id, "2.00");
        assert_eq!(
            (quote.affected_movement_count, quote.affected_sale_count),
            (2, 1)
        );
        connection.execute_batch("CREATE TRIGGER fail_correction_detail BEFORE UPDATE OF movement_cost_cents ON sale_details BEGIN SELECT RAISE(ABORT,'injected correction failure'); END;").unwrap();
        assert!(super::confirm_purchase_cost_correction(
            &mut connection,
            ConfirmPurchaseCostCorrectionInput {
                movement_id: quote.movement_id,
                new_unit_price: "2.00".into(),
                quote_fingerprint: quote.quote_fingerprint.clone(),
            },
        )
        .is_err());
        assert_eq!(
            connection
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            100
        );
        assert_eq!(
            connection
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            14_700
        );
        connection
            .execute_batch("DROP TRIGGER fail_correction_detail")
            .unwrap();
        confirm_correction(&mut connection, &quote, "2.00");
        let updated = sales::history_detail(&connection, receipt.sale.id).unwrap();
        assert_eq!(
            (
                updated.subtotal,
                updated.discount_value,
                updated.discount_amount,
                updated.total
            ),
            (
                commercial_before.0,
                commercial_before.2,
                commercial_before.3,
                commercial_before.4
            )
        );
        assert_eq!(
            updated
                .lines
                .iter()
                .map(|line| (
                    line.catalog_unit_price.unwrap(),
                    line.unit_price,
                    line.subtotal.unwrap(),
                    Some(line.base_consumed)
                ))
                .collect::<Vec<_>>(),
            commercial_before.5
        );
        let movement_costs: Vec<i64> = connection
            .prepare(
                "SELECT movement_cost_cents FROM inventory_movements WHERE sale_id=?1 ORDER BY id",
            )
            .unwrap()
            .query_map([receipt.sale.id], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let detail_costs: Vec<i64> = connection
            .prepare("SELECT movement_cost_cents FROM sale_details WHERE sale_id=?1 ORDER BY id")
            .unwrap()
            .query_map([receipt.sale.id], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(movement_costs, detail_costs);
        assert_eq!(movement_costs.iter().sum::<i64>(), 20_600);
    }

    #[test]
    fn correction_preserves_unknown_cost_and_rejects_v1_stale_and_overflow_atomically() {
        let mut unknown = database("piece", 5, None, 1, "fixed");
        let purchase = create_manual(
            &mut unknown,
            None,
            "compra",
            "entrada",
            "5",
            Some("1.00"),
            "unknown",
        );
        let quote = correction_quote(&unknown, purchase.id, "2.00");
        assert_eq!(quote.corrected_inventory_cost_cents, None);
        confirm_correction(&mut unknown, &quote, "2.00");
        assert_eq!(
            unknown
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })
                .unwrap(),
            None
        );

        let mut stale = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut stale,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "stale",
        );
        let quote = correction_quote(&stale, purchase.id, "2.00");
        create_manual(
            &mut stale,
            None,
            "ajuste",
            "entrada",
            "1",
            Some("1.00"),
            "later",
        );
        let error = super::confirm_purchase_cost_correction(
            &mut stale,
            ConfirmPurchaseCostCorrectionInput {
                movement_id: purchase.id,
                new_unit_price: "2.00".into(),
                quote_fingerprint: quote.quote_fingerprint,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "PURCHASE_COST_CORRECTION_STALE");
        assert_eq!(
            stale
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            100
        );

        let mut legacy = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut legacy,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "legacy",
        );
        legacy.execute("UPDATE inventory_movements SET record_version=1,stock_before=NULL,stock_after=NULL,entered_quantity=NULL,base_quantity=NULL,inventory_cost_before_cents=NULL,inventory_cost_after_cents=NULL WHERE id=?1",[purchase.id]).unwrap();
        assert_eq!(
            super::quote_purchase_cost_correction(
                &legacy,
                QuotePurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "2.00".into()
                }
            )
            .unwrap_err()
            .code,
            "PURCHASE_COST_CORRECTION_NOT_ELIGIBLE"
        );

        let mut overflow = database("piece", 0, Some(0), 1, "fixed");
        let purchase = create_manual(
            &mut overflow,
            None,
            "compra",
            "entrada",
            "2",
            Some("1.00"),
            "overflow",
        );
        assert_eq!(
            super::quote_purchase_cost_correction(
                &overflow,
                QuotePurchaseCostCorrectionInput {
                    movement_id: purchase.id,
                    new_unit_price: "90071992547409.91".into()
                }
            )
            .unwrap_err()
            .code,
            "PURCHASE_COST_OVERFLOW"
        );
        assert_eq!(
            overflow
                .query_row(
                    "SELECT unit_price_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            100
        );
    }
}
