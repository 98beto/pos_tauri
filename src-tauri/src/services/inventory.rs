use rusqlite::{named_params, params, Connection, OptionalExtension, Row, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::models::{
    inventory_movement::{
        ConfirmPurchasePriceCorrectionInput, CreatePurchaseInput, CreateStockCountInput,
        InventoryMovement, InventoryMovementPresentation, InventoryMovementProduct,
        InventoryProductSearchResult, InventoryView, InventoryViewFilters, InventoryViewStats,
        LookupInventoryMovementInput, ProductInventoryDetail, ProductInventoryDetailFilters,
        PurchasePriceCorrectionQuote, PurchaseQuote, QuotePurchaseInput,
        QuotePurchasePriceCorrectionInput, QuoteStockCountInput, StockCountQuote,
    },
    product::{BaseUnit, InventoryProductRow},
    product_presentation::PresentationMode,
};

use super::{
    checked_multiply, checked_total, divide_half_up_positive, normalize_persisted,
    normalize_search, parse_money_text, presentation_base_quantity, search_key, validate_id,
    validate_presentation_mode, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER,
};

const OPERATION_TOKEN_MAX_LENGTH: usize = 128;
const PAGE_SIZE: i64 = 20;
const MOVEMENT_COLUMNS: &str = "m.id,m.product_id,p.name,p.sku,m.product_name,m.sku,m.sale_id,
    CASE WHEN m.sale_id IS NULL THEN NULL ELSE printf('POS-%08d',m.sale_id) END,
    m.record_version,m.origin,m.presentation_id,m.presentation_name,m.presentation_barcode,
    m.presentation_mode,m.presentation_base_quantity,m.base_unit,m.reason,m.type,m.quantity,
    m.entered_quantity,m.base_quantity,m.stock_before,m.stock_after,m.supplier,
    m.document_reference,m.note,m.operation_token,m.purchase_unit_price_cents,
    m.purchase_total_cents,m.created_at,m.updated_at";

#[derive(Clone)]
struct ProductSnapshot {
    id: i64,
    name: String,
    sku: String,
    stock: i64,
    base_unit: BaseUnit,
}

#[derive(Clone)]
struct PresentationSnapshot {
    name: String,
    barcode: Option<String>,
    mode: PresentationMode,
    base_quantity: i64,
}

#[derive(Clone, Debug)]
struct NormalizedPurchase {
    product_id: i64,
    presentation_id: Option<i64>,
    entered_quantity: i64,
    purchase_unit_price: i64,
    supplier: String,
    document_reference: String,
    note: Option<String>,
    intent_fingerprint: String,
}

struct CalculatedPurchase {
    product: ProductSnapshot,
    presentation: Option<PresentationSnapshot>,
    base_quantity: i64,
    stock_after: i64,
    purchase_total: i64,
    quote_fingerprint: String,
}

#[derive(Clone, Debug)]
struct NormalizedCount {
    product_id: i64,
    counted_stock: i64,
    adjustment_reason: String,
    intent_fingerprint: String,
}

struct CalculatedCount {
    product: ProductSnapshot,
    movement_type: &'static str,
    base_quantity: i64,
    difference: i64,
    quote_fingerprint: String,
}

#[derive(Clone)]
struct CorrectionMovement {
    id: i64,
    product_id: i64,
    product_name: String,
    sku: String,
    base_unit: BaseUnit,
    presentation_id: Option<i64>,
    presentation_name: Option<String>,
    presentation_barcode: Option<String>,
    presentation_mode: Option<PresentationMode>,
    presentation_base_quantity: Option<i64>,
    entered_quantity: i64,
    base_quantity: i64,
    purchase_unit_price: i64,
    purchase_total: i64,
    supplier: String,
    document_reference: String,
    note: Option<String>,
    created_at: String,
    updated_at: String,
}

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
        purchase_unit_price_cents: row.get(27)?,
        purchase_total_cents: row.get(28)?,
        created_at: row.get(29)?,
        updated_at: row.get(30)?,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<InventoryMovement>> {
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id \
         ORDER BY m.created_at DESC,m.id DESC"
    );
    Ok(connection
        .prepare(&sql)?
        .query_map([], map_movement)?
        .collect::<Result<Vec<_>, _>>()?)
}

pub fn lookup_movement(
    connection: &Connection,
    input: LookupInventoryMovementInput,
) -> ServiceResult<Option<InventoryMovement>> {
    let operation_token = normalize_token(&input.operation_token)?;
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id \
         WHERE m.operation_token=?1"
    );
    Ok(connection
        .query_row(&sql, [operation_token], map_movement)
        .optional()?)
}

pub fn search_products(
    connection: &Connection,
    search: Option<String>,
    requested_page: i64,
) -> ServiceResult<InventoryProductSearchResult> {
    let search = normalize_search(search)?;
    let page = page(Some(requested_page), "page")?;
    let condition = "?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0";
    let total = connection.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {condition}"),
        [&search],
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit FROM products p WHERE {condition} \
         ORDER BY p.name_search_key,p.id LIMIT ?2 OFFSET ?3"
    );
    let products = connection
        .prepare(&sql)?
        .query_map(
            params![search, PAGE_SIZE, (page - 1) * PAGE_SIZE],
            map_inventory_product,
        )?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|product| inventory_movement_product(connection, product))
        .collect::<ServiceResult<Vec<_>>>()?;
    Ok(InventoryProductSearchResult {
        items: products,
        page,
        total,
        page_size: PAGE_SIZE,
    })
}

pub fn view(
    connection: &Connection,
    filters: InventoryViewFilters,
) -> ServiceResult<InventoryView> {
    let product_search = normalize_search(filters.product_search)?;
    let movement_search = normalize_search(filters.movement_search)?;
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
    let base_unit = filters.base_unit.map(|unit| unit.as_str());
    let product_page = page(filters.product_page, "product_page")?;
    let movement_page = page(filters.movement_page, "movement_page")?;
    let product_where = "(?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0) AND (?2 IS NULL OR p.base_unit=?2)";
    let product_total = connection.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {product_where}"),
        params![product_search, base_unit],
        |row| row.get(0),
    )?;
    let product_sql = format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit FROM products p WHERE {product_where} \
         ORDER BY p.name_search_key,p.id LIMIT ?3 OFFSET ?4"
    );
    let products = connection
        .prepare(&product_sql)?
        .query_map(
            params![
                product_search,
                base_unit,
                PAGE_SIZE,
                (product_page - 1) * PAGE_SIZE
            ],
            map_inventory_product,
        )?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|product| inventory_movement_product(connection, product))
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
    let movement_sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id \
         WHERE {movement_where} ORDER BY m.created_at DESC,m.id DESC LIMIT ?4 OFFSET ?5"
    );
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
    validate_id(product_id)?;
    let search = normalize_search(filters.movement_search)?;
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
    let date_from = normalize_date(filters.date_from, "date_from")?;
    let date_to = normalize_date(filters.date_to, "date_to")?;
    if matches!((&date_from, &date_to), (Some(from), Some(to)) if from > to) {
        return Err(ServiceError::validation(
            "INVALID_DATE_RANGE",
            "La fecha inicial no puede ser posterior a la fecha final",
        ));
    }
    let page = page(filters.page, "page")?;
    let product = connection
        .query_row(
            "SELECT id,name,sku,stock,base_unit FROM products WHERE id=?1",
            [product_id],
            map_inventory_product,
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))?;
    let where_sql = "m.product_id=:product_id AND (:reason IS NULL OR m.reason=:reason)
        AND (:movement_type IS NULL OR m.type=:movement_type)
        AND (:date_from IS NULL OR m.created_at>=:date_from)
        AND (:date_to IS NULL OR m.created_at<datetime(:date_to,'+1 day'))
        AND (:search='' OR instr(coalesce(m.provider_search_key,''),:search)>0
        OR instr(coalesce(m.document_reference_search_key,''),:search)>0
        OR instr(coalesce(m.note_search_key,''),:search)>0
        OR (m.sale_id IS NOT NULL AND instr(lower(printf('POS-%08d',m.sale_id)),:search)>0))";
    let total = connection.query_row(
        &format!("SELECT COUNT(*) FROM inventory_movements m WHERE {where_sql}"),
        named_params! {":product_id":product_id,":reason":reason,":movement_type":movement_type,":date_from":date_from,":date_to":date_to,":search":search},
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id \
         WHERE {where_sql} ORDER BY m.created_at DESC,m.id DESC LIMIT :limit OFFSET :offset"
    );
    let movements = connection.prepare(&sql)?.query_map(
        named_params! {":product_id":product_id,":reason":reason,":movement_type":movement_type,":date_from":date_from,":date_to":date_to,":search":search,":limit":PAGE_SIZE,":offset":(page-1)*PAGE_SIZE},
        map_movement,
    )?.collect::<Result<Vec<_>,_>>()?;
    let last_movement_at = connection.query_row(
        "SELECT created_at FROM inventory_movements WHERE product_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1",
        [product_id], |row| row.get(0),
    ).optional()?;
    Ok(ProductInventoryDetail {
        product,
        last_movement_at,
        movements,
        page,
        total,
        page_size: PAGE_SIZE,
    })
}

pub fn quote_purchase(
    connection: &Connection,
    input: QuotePurchaseInput,
) -> ServiceResult<PurchaseQuote> {
    let normalized = normalize_purchase(
        input.product_id,
        input.presentation_id,
        input.quantity,
        input.purchase_unit_price,
        input.supplier,
        input.document_reference,
        input.note,
    )?;
    let calculated = calculate_purchase(connection, &normalized)?;
    Ok(public_purchase_quote(&normalized, calculated))
}

pub fn create_purchase(
    connection: &mut Connection,
    input: CreatePurchaseInput,
) -> ServiceResult<InventoryMovement> {
    let operation_token = normalize_token(&input.operation_token)?;
    validate_quote_fingerprint(&input.quote_fingerprint)?;
    let normalized = normalize_purchase(
        input.product_id,
        input.presentation_id,
        input.quantity,
        input.purchase_unit_price,
        input.supplier,
        input.document_reference,
        input.note,
    )?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((movement, fingerprint)) = movement_by_token(&transaction, &operation_token)? {
        return if fingerprint == normalized.intent_fingerprint {
            Ok(movement)
        } else {
            Err(token_reused())
        };
    }
    let calculated = calculate_purchase(&transaction, &normalized)?;
    if calculated.quote_fingerprint != input.quote_fingerprint {
        return Err(stale_quote());
    }
    let changed = transaction.execute(
        "UPDATE products SET stock=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2 AND stock=?3",
        params![
            calculated.stock_after,
            normalized.product_id,
            calculated.product.stock
        ],
    )?;
    if changed != 1 {
        return Err(stale_quote());
    }
    let presentation_name = calculated
        .presentation
        .as_ref()
        .map(|value| value.name.as_str());
    let presentation_barcode = calculated
        .presentation
        .as_ref()
        .and_then(|value| value.barcode.as_deref());
    let presentation_mode = calculated
        .presentation
        .as_ref()
        .map(|value| value.mode.as_str());
    let presentation_base = calculated
        .presentation
        .as_ref()
        .map(|value| value.base_quantity);
    transaction.execute(
        "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,
         record_version,presentation_id,presentation_name,presentation_barcode,base_unit,
         presentation_mode,presentation_base_quantity,purchase_unit_price_cents,purchase_total_cents,
         origin,reason,type,quantity,entered_quantity,base_quantity,stock_before,stock_after,supplier,
         provider_search_key,document_reference,document_reference_search_key,note,note_search_key,
         operation_token,intent_fingerprint)
         VALUES(?1,?2,?3,?4,?5,3,?6,?7,?8,?9,?10,?11,?12,?13,'manual','compra','entrada',
         ?14,?15,?14,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
        params![
            normalized.product_id,
            calculated.product.name,
            calculated.product.sku,
            search_key(&calculated.product.name),
            search_key(&calculated.product.sku),
            normalized.presentation_id,
            presentation_name,
            presentation_barcode,
            calculated.product.base_unit.as_str(),
            presentation_mode,
            presentation_base,
            normalized.purchase_unit_price,
            calculated.purchase_total,
            calculated.base_quantity,
            normalized.entered_quantity,
            calculated.product.stock,
            calculated.stock_after,
            normalized.supplier,
            search_key(&normalized.supplier),
            normalized.document_reference,
            search_key(&normalized.document_reference),
            normalized.note,
            normalized.note.as_deref().map(search_key),
            operation_token,
            normalized.intent_fingerprint,
        ],
    )?;
    let movement = movement_by_id(&transaction, transaction.last_insert_rowid())?;
    transaction.commit()?;
    Ok(movement)
}

pub fn quote_stock_count(
    connection: &Connection,
    input: QuoteStockCountInput,
) -> ServiceResult<StockCountQuote> {
    let normalized = normalize_count(
        input.product_id,
        input.counted_stock,
        input.adjustment_reason,
    )?;
    let calculated = calculate_count(connection, &normalized)?;
    Ok(public_count_quote(&normalized, calculated))
}

pub fn create_stock_count(
    connection: &mut Connection,
    input: CreateStockCountInput,
) -> ServiceResult<InventoryMovement> {
    let operation_token = normalize_token(&input.operation_token)?;
    validate_quote_fingerprint(&input.quote_fingerprint)?;
    let normalized = normalize_count(
        input.product_id,
        input.counted_stock,
        input.adjustment_reason,
    )?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((movement, fingerprint)) = movement_by_token(&transaction, &operation_token)? {
        return if fingerprint == normalized.intent_fingerprint {
            Ok(movement)
        } else {
            Err(token_reused())
        };
    }
    let calculated = calculate_count(&transaction, &normalized)?;
    if calculated.quote_fingerprint != input.quote_fingerprint {
        return Err(stale_quote());
    }
    let changed = transaction.execute(
        "UPDATE products SET stock=?1,updated_at=CURRENT_TIMESTAMP WHERE id=?2 AND stock=?3",
        params![
            normalized.counted_stock,
            normalized.product_id,
            calculated.product.stock
        ],
    )?;
    if changed != 1 {
        return Err(stale_quote());
    }
    transaction.execute(
        "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,
         record_version,base_unit,origin,reason,type,quantity,entered_quantity,base_quantity,
         stock_before,stock_after,note,note_search_key,operation_token,intent_fingerprint)
         VALUES(?1,?2,?3,?4,?5,3,?6,'manual','ajuste',?7,?8,?8,?8,?9,?10,?11,?12,?13,?14)",
        params![
            normalized.product_id,
            calculated.product.name,
            calculated.product.sku,
            search_key(&calculated.product.name),
            search_key(&calculated.product.sku),
            calculated.product.base_unit.as_str(),
            calculated.movement_type,
            calculated.base_quantity,
            calculated.product.stock,
            normalized.counted_stock,
            normalized.adjustment_reason,
            search_key(&normalized.adjustment_reason),
            operation_token,
            normalized.intent_fingerprint,
        ],
    )?;
    let movement = movement_by_id(&transaction, transaction.last_insert_rowid())?;
    transaction.commit()?;
    Ok(movement)
}

pub fn quote_purchase_price_correction(
    connection: &Connection,
    input: QuotePurchasePriceCorrectionInput,
) -> ServiceResult<PurchasePriceCorrectionQuote> {
    validate_id(input.movement_id)?;
    let new_price = parse_correction_price(&input.new_purchase_unit_price)?;
    let movement = load_correction_movement(connection, input.movement_id)?;
    if movement.purchase_unit_price == new_price {
        return Err(ServiceError::validation_field(
            "PURCHASE_PRICE_CORRECTION_SAME_PRICE",
            "El nuevo precio debe ser diferente al precio de compra actual",
            "new_purchase_unit_price",
        ));
    }
    correction_quote(&movement, movement.purchase_unit_price, new_price)
}

pub fn confirm_purchase_price_correction(
    connection: &mut Connection,
    input: ConfirmPurchasePriceCorrectionInput,
) -> ServiceResult<PurchasePriceCorrectionQuote> {
    validate_id(input.movement_id)?;
    let new_price = parse_correction_price(&input.new_purchase_unit_price)?;
    let quoted_previous = correction_fingerprint_previous_price(&input.quote_fingerprint)?;
    if quoted_previous == new_price {
        return Err(stale_correction());
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let movement = load_correction_movement(&transaction, input.movement_id)?;
    if movement.purchase_unit_price != quoted_previous && movement.purchase_unit_price != new_price
    {
        return Err(stale_correction());
    }
    let quote = correction_quote(&movement, quoted_previous, new_price)?;
    if quote.quote_fingerprint != input.quote_fingerprint {
        return Err(stale_correction());
    }
    if movement.purchase_unit_price == new_price {
        transaction.commit()?;
        return Ok(quote);
    }
    let changed = transaction.execute(
        "UPDATE inventory_movements SET purchase_unit_price_cents=?1,purchase_total_cents=?2
         WHERE id=?3 AND record_version=3 AND origin='manual' AND reason='compra' AND type='entrada'
         AND purchase_unit_price_cents=?4",
        params![
            new_price,
            quote.new_purchase_total_cents,
            input.movement_id,
            quoted_previous
        ],
    )?;
    if changed != 1 {
        return Err(stale_correction());
    }
    transaction.commit()?;
    Ok(quote)
}

fn normalize_purchase(
    product_id: i64,
    presentation_id: Option<i64>,
    quantity: String,
    purchase_unit_price: String,
    supplier: String,
    document_reference: String,
    note: Option<String>,
) -> ServiceResult<NormalizedPurchase> {
    validate_id(product_id)?;
    if let Some(id) = presentation_id {
        validate_id(id)?;
    }
    let entered_quantity = parse_integer(&quantity, false, "quantity")?;
    let purchase_unit_price = parse_money_text(&purchase_unit_price).map_err(|_| {
        ServiceError::validation_field(
            "INVALID_PURCHASE_PRICE",
            "El precio de compra debe ser positivo y tener hasta dos decimales",
            "purchase_unit_price",
        )
    })?;
    let supplier = required_text(supplier, 200, "SUPPLIER_REQUIRED", "supplier")?;
    let document_reference = required_text(
        document_reference,
        200,
        "DOCUMENT_REFERENCE_REQUIRED",
        "document_reference",
    )?;
    let note = optional_text(note, 1000, "note")?;
    let intent_fingerprint = opaque_fingerprint(&format!(
        "purchase-intent-v1|{product_id}|{}|{entered_quantity}|{purchase_unit_price}|{}|{}|{}",
        optional_number(presentation_id),
        length_value(&supplier),
        length_value(&document_reference),
        note.as_deref().map(length_value).unwrap_or_default(),
    ));
    Ok(NormalizedPurchase {
        product_id,
        presentation_id,
        entered_quantity,
        purchase_unit_price,
        supplier,
        document_reference,
        note,
        intent_fingerprint,
    })
}

fn calculate_purchase(
    connection: &Connection,
    input: &NormalizedPurchase,
) -> ServiceResult<CalculatedPurchase> {
    let product = load_product_snapshot(connection, input.product_id)?;
    let presentation = input
        .presentation_id
        .map(|id| load_presentation_snapshot(connection, input.product_id, id))
        .transpose()?;
    let base_quantity = match &presentation {
        Some(value) => {
            validate_presentation_mode(product.base_unit, value.mode)?;
            presentation_base_quantity(value.mode, input.entered_quantity, value.base_quantity)?
        }
        None => input.entered_quantity,
    };
    let stock_after = checked_total(product.stock, base_quantity, "STOCK_OVERFLOW")?;
    let purchase_total = purchase_total(
        presentation.as_ref().map(|value| value.mode),
        input.entered_quantity,
        presentation.as_ref().map(|value| value.base_quantity),
        input.purchase_unit_price,
    )?;
    let quote_fingerprint = opaque_fingerprint(&format!(
        "purchase-quote-v1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        product.id,
        length_value(&product.name),
        length_value(&product.sku),
        product.stock,
        product.base_unit.as_str(),
        optional_number(input.presentation_id),
        presentation
            .as_ref()
            .map(|value| length_value(&value.name))
            .unwrap_or_default(),
        presentation
            .as_ref()
            .and_then(|value| value.barcode.as_deref())
            .map(length_value)
            .unwrap_or_default(),
        presentation
            .as_ref()
            .map(|value| value.mode.as_str())
            .unwrap_or(""),
        presentation
            .as_ref()
            .map(|value| value.base_quantity)
            .unwrap_or(0),
        input.entered_quantity,
        base_quantity,
        input.purchase_unit_price,
        purchase_total,
        input.intent_fingerprint,
    ));
    Ok(CalculatedPurchase {
        product,
        presentation,
        base_quantity,
        stock_after,
        purchase_total,
        quote_fingerprint,
    })
}

fn public_purchase_quote(input: &NormalizedPurchase, value: CalculatedPurchase) -> PurchaseQuote {
    let presentation_base_quantity = value
        .presentation
        .as_ref()
        .map(|presentation| presentation.base_quantity)
        .unwrap_or(1);
    let presentation_mode = value
        .presentation
        .as_ref()
        .map(|presentation| presentation.mode);
    let presentation_name = value
        .presentation
        .as_ref()
        .map(|presentation| presentation.name.clone());
    let capture_name = presentation_name
        .clone()
        .unwrap_or_else(|| direct_capture_name(value.product.base_unit).into());
    let purchase_price_description = match presentation_mode {
        Some(PresentationMode::Fixed) => format!("Precio de compra por {capture_name}"),
        Some(PresentationMode::Measured) => format!("Precio de compra por {capture_name}"),
        None => format!(
            "Precio de compra por {}",
            base_unit_singular(value.product.base_unit)
        ),
    };
    PurchaseQuote {
        product_id: value.product.id,
        product_name: value.product.name,
        product_sku: value.product.sku,
        presentation_id: input.presentation_id,
        presentation_name,
        capture_name,
        presentation_mode,
        presentation_base_quantity,
        entered_quantity: input.entered_quantity,
        base_quantity: value.base_quantity,
        stock_before: value.product.stock,
        stock_after: value.stock_after,
        purchase_unit_price_cents: input.purchase_unit_price,
        purchase_total_cents: value.purchase_total,
        purchase_price_description,
        base_unit: value.product.base_unit,
        supplier: input.supplier.clone(),
        document_reference: input.document_reference.clone(),
        note: input.note.clone(),
        quote_fingerprint: value.quote_fingerprint,
    }
}

fn normalize_count(
    product_id: i64,
    counted_stock: String,
    adjustment_reason: String,
) -> ServiceResult<NormalizedCount> {
    validate_id(product_id)?;
    let counted_stock = parse_integer(&counted_stock, true, "counted_stock")?;
    let adjustment_reason = required_text(
        adjustment_reason,
        1000,
        "ADJUSTMENT_REASON_REQUIRED",
        "adjustment_reason",
    )?;
    let intent_fingerprint = opaque_fingerprint(&format!(
        "stock-count-intent-v1|{product_id}|{counted_stock}|{}",
        length_value(&adjustment_reason)
    ));
    Ok(NormalizedCount {
        product_id,
        counted_stock,
        adjustment_reason,
        intent_fingerprint,
    })
}

fn calculate_count(
    connection: &Connection,
    input: &NormalizedCount,
) -> ServiceResult<CalculatedCount> {
    let product = load_product_snapshot(connection, input.product_id)?;
    let difference = input.counted_stock - product.stock;
    if difference == 0 {
        return Err(ServiceError::validation(
            "STOCK_COUNT_UNCHANGED",
            "El conteo debe ser diferente al stock registrado",
        ));
    }
    let (movement_type, base_quantity) = if difference > 0 {
        ("entrada", difference)
    } else {
        (
            "salida",
            difference.checked_abs().ok_or_else(invalid_quantity)?,
        )
    };
    let quote_fingerprint = opaque_fingerprint(&format!(
        "stock-count-quote-v1|{}|{}|{}|{}|{}|{}|{}",
        product.id,
        length_value(&product.name),
        length_value(&product.sku),
        product.stock,
        product.base_unit.as_str(),
        input.counted_stock,
        input.intent_fingerprint,
    ));
    Ok(CalculatedCount {
        product,
        movement_type,
        base_quantity,
        difference,
        quote_fingerprint,
    })
}

fn public_count_quote(input: &NormalizedCount, value: CalculatedCount) -> StockCountQuote {
    StockCountQuote {
        product_id: value.product.id,
        product_name: value.product.name,
        product_sku: value.product.sku,
        base_unit: value.product.base_unit,
        stock_before: value.product.stock,
        counted_stock: input.counted_stock,
        difference: value.difference,
        movement_type: value.movement_type.into(),
        base_quantity: value.base_quantity,
        adjustment_reason: input.adjustment_reason.clone(),
        quote_fingerprint: value.quote_fingerprint,
    }
}

fn load_product_snapshot(connection: &Connection, id: i64) -> ServiceResult<ProductSnapshot> {
    connection
        .query_row(
            "SELECT id,name,sku,stock,base_unit FROM products WHERE id=?1",
            [id],
            |row| {
                let unit: String = row.get(4)?;
                Ok(ProductSnapshot {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    sku: row.get(2)?,
                    stock: row.get(3)?,
                    base_unit: BaseUnit::try_from(unit.as_str())
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))
}

fn load_presentation_snapshot(
    connection: &Connection,
    product_id: i64,
    id: i64,
) -> ServiceResult<PresentationSnapshot> {
    connection
        .query_row(
            "SELECT id,name,barcode,mode,base_quantity FROM product_presentations
             WHERE id=?1 AND product_id=?2 AND is_active=1",
            params![id, product_id],
            |row| {
                let mode: String = row.get(3)?;
                Ok(PresentationSnapshot {
                    name: row.get(1)?,
                    barcode: row.get(2)?,
                    mode: PresentationMode::try_from(mode.as_str())
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    base_quantity: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            ServiceError::conflict(
                "PRESENTATION_UNAVAILABLE",
                "La presentacion no pertenece al producto o ya no esta activa",
            )
        })
}

fn purchase_total(
    mode: Option<PresentationMode>,
    entered_quantity: i64,
    presentation_base_quantity: Option<i64>,
    purchase_unit_price: i64,
) -> ServiceResult<i64> {
    let total = match mode {
        Some(PresentationMode::Measured) => divide_half_up_positive(
            i128::from(entered_quantity) * i128::from(purchase_unit_price),
            i128::from(presentation_base_quantity.ok_or_else(invalid_quantity)?),
            "PURCHASE_TOTAL_OVERFLOW",
        )?,
        Some(PresentationMode::Fixed) | None => checked_multiply(
            entered_quantity,
            purchase_unit_price,
            "PURCHASE_TOTAL_OVERFLOW",
        )?,
    };
    if total == 0 {
        return Err(ServiceError::validation_field(
            "PURCHASE_TOTAL_ROUNDS_TO_ZERO",
            "El importe total de la compra debe ser al menos un centavo",
            "purchase_unit_price",
        ));
    }
    Ok(total)
}

fn load_correction_movement(
    connection: &Connection,
    movement_id: i64,
) -> ServiceResult<CorrectionMovement> {
    connection
        .query_row(
            "SELECT id,product_id,product_name,sku,base_unit,presentation_id,presentation_name,
             presentation_barcode,presentation_mode,presentation_base_quantity,entered_quantity,base_quantity,
             purchase_unit_price_cents,purchase_total_cents,supplier,document_reference,note,
             created_at,updated_at
             FROM inventory_movements WHERE id=?1 AND record_version=3 AND origin='manual'
             AND reason='compra' AND type='entrada' AND purchase_unit_price_cents IS NOT NULL
             AND purchase_total_cents IS NOT NULL",
            [movement_id],
            |row| {
                let unit: String = row.get(4)?;
                let mode: Option<String> = row.get(8)?;
                Ok(CorrectionMovement {
                    id: row.get(0)?,
                    product_id: row.get(1)?,
                    product_name: row.get(2)?,
                    sku: row.get(3)?,
                    base_unit: BaseUnit::try_from(unit.as_str())
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    presentation_id: row.get(5)?,
                    presentation_name: row.get(6)?,
                    presentation_barcode: row.get(7)?,
                    presentation_mode: mode
                        .as_deref()
                        .map(PresentationMode::try_from)
                        .transpose()
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    presentation_base_quantity: row.get(9)?,
                    entered_quantity: row.get(10)?,
                    base_quantity: row.get(11)?,
                    purchase_unit_price: row.get(12)?,
                    purchase_total: row.get(13)?,
                    supplier: row.get(14)?,
                    document_reference: row.get(15)?,
                    note: row.get(16)?,
                    created_at: row.get(17)?,
                    updated_at: row.get(18)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            ServiceError::validation(
                "PURCHASE_PRICE_CORRECTION_NOT_ELIGIBLE",
                "Solo se puede corregir una compra v3 completa",
            )
        })
}

fn correction_quote(
    movement: &CorrectionMovement,
    previous_price: i64,
    new_price: i64,
) -> ServiceResult<PurchasePriceCorrectionQuote> {
    let expected_base = match movement.presentation_mode {
        Some(mode) => presentation_base_quantity(
            mode,
            movement.entered_quantity,
            movement
                .presentation_base_quantity
                .ok_or_else(stale_correction)?,
        )?,
        None => movement.entered_quantity,
    };
    if expected_base != movement.base_quantity {
        return Err(stale_correction());
    }
    let previous_total = purchase_total(
        movement.presentation_mode,
        movement.entered_quantity,
        movement.presentation_base_quantity,
        previous_price,
    )?;
    if movement.purchase_unit_price == previous_price && movement.purchase_total != previous_total {
        return Err(stale_correction());
    }
    let new_total = purchase_total(
        movement.presentation_mode,
        movement.entered_quantity,
        movement.presentation_base_quantity,
        new_price,
    )?;
    if (movement.purchase_unit_price == previous_price && movement.purchase_total != previous_total)
        || (movement.purchase_unit_price == new_price && movement.purchase_total != new_total)
    {
        return Err(stale_correction());
    }
    let canonical = format!(
        "purchase-price-correction-v1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        movement.id,
        previous_price,
        new_price,
        movement.product_id,
        length_value(&movement.product_name),
        length_value(&movement.sku),
        movement.base_unit.as_str(),
        optional_number(movement.presentation_id),
        movement
            .presentation_name
            .as_deref()
            .map(length_value)
            .unwrap_or_default(),
        movement
            .presentation_barcode
            .as_deref()
            .map(length_value)
            .unwrap_or_default(),
        movement
            .presentation_mode
            .map(|mode| mode.as_str())
            .unwrap_or(""),
        movement.presentation_base_quantity.unwrap_or(0),
        movement.entered_quantity,
        movement.base_quantity,
        length_value(&movement.supplier),
        length_value(&movement.document_reference),
        movement
            .note
            .as_deref()
            .map(length_value)
            .unwrap_or_default(),
        length_value(&movement.created_at),
        length_value(&movement.updated_at),
    );
    let fingerprint = format!(
        "purchase-price-correction-v1:{previous_price}:{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    Ok(PurchasePriceCorrectionQuote {
        movement_id: movement.id,
        product_id: movement.product_id,
        product_name: movement.product_name.clone(),
        product_sku: movement.sku.clone(),
        base_unit: movement.base_unit,
        capture_name: movement
            .presentation_name
            .clone()
            .unwrap_or_else(|| direct_capture_name(movement.base_unit).into()),
        presentation_id: movement.presentation_id,
        presentation_name: movement.presentation_name.clone(),
        presentation_mode: movement.presentation_mode,
        presentation_base_quantity: movement.presentation_base_quantity,
        entered_quantity: movement.entered_quantity,
        base_quantity: movement.base_quantity,
        previous_purchase_unit_price_cents: previous_price,
        new_purchase_unit_price_cents: new_price,
        previous_purchase_total_cents: previous_total,
        new_purchase_total_cents: new_total,
        quote_fingerprint: fingerprint,
    })
}

fn correction_fingerprint_previous_price(value: &str) -> ServiceResult<i64> {
    let mut parts = value.split(':');
    if parts.next() != Some("purchase-price-correction-v1") {
        return Err(stale_correction());
    }
    let previous = parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (1..=JS_MAX_SAFE_INTEGER).contains(value))
        .ok_or_else(stale_correction)?;
    let digest = parts.next().filter(|value| value.len() == 64);
    if digest.is_none() || parts.next().is_some() {
        return Err(stale_correction());
    }
    Ok(previous)
}

fn parse_correction_price(value: &str) -> ServiceResult<i64> {
    parse_money_text(value).map_err(|_| {
        ServiceError::validation_field(
            "INVALID_PURCHASE_PRICE",
            "El nuevo precio de compra no es valido",
            "new_purchase_unit_price",
        )
    })
}

fn movement_by_id(connection: &Connection, id: i64) -> ServiceResult<InventoryMovement> {
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE m.id=?1"
    );
    connection
        .query_row(&sql, [id], map_movement)
        .map_err(Into::into)
}

fn movement_by_token(
    connection: &Connection,
    token: &str,
) -> ServiceResult<Option<(InventoryMovement, String)>> {
    let id_and_fingerprint = connection
        .query_row(
            "SELECT id,intent_fingerprint FROM inventory_movements WHERE operation_token=?1",
            [token],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    id_and_fingerprint
        .map(|(id, fingerprint)| Ok((movement_by_id(connection, id)?, fingerprint)))
        .transpose()
}

fn map_inventory_product(row: &Row<'_>) -> rusqlite::Result<InventoryProductRow> {
    let base_unit: String = row.get(4)?;
    Ok(InventoryProductRow {
        id: row.get(0)?,
        name: row.get(1)?,
        sku: row.get(2)?,
        stock: row.get(3)?,
        base_unit: BaseUnit::try_from(base_unit.as_str())
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
    })
}

fn inventory_movement_product(
    connection: &Connection,
    product: InventoryProductRow,
) -> ServiceResult<InventoryMovementProduct> {
    let presentations = connection
        .prepare(
            "SELECT id,product_id,name,mode,base_quantity,is_active FROM product_presentations
         WHERE product_id=?1 ORDER BY is_default DESC,name,id",
        )?
        .query_map([product.id], |row| {
            let mode: String = row.get(3)?;
            Ok(InventoryMovementPresentation {
                id: row.get(0)?,
                product_id: row.get(1)?,
                name: row.get(2)?,
                mode: PresentationMode::try_from(mode.as_str())
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                base_quantity: row.get(4)?,
                is_active: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(InventoryMovementProduct {
        id: product.id,
        name: product.name,
        sku: product.sku,
        stock: product.stock,
        base_unit: product.base_unit,
        presentations,
    })
}

fn inventory_stats(connection: &Connection) -> ServiceResult<InventoryViewStats> {
    let mut statement = connection.prepare("SELECT base_unit,stock FROM products")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut stats = InventoryViewStats {
        product_count: 0,
        piece_stock: 0,
        gram_stock: 0,
        milliliter_stock: 0,
        centimeter_stock: 0,
    };
    for row in rows {
        let (unit, stock) = row?;
        stats.product_count = checked_total(stats.product_count, 1, "INVENTORY_STATS_OVERFLOW")?;
        let target = match unit.as_str() {
            "piece" => &mut stats.piece_stock,
            "gram" => &mut stats.gram_stock,
            "milliliter" => &mut stats.milliliter_stock,
            "centimeter" => &mut stats.centimeter_stock,
            _ => {
                return Err(ServiceError::internal(
                    "INVALID_BASE_UNIT",
                    "La unidad guardada no es valida",
                ))
            }
        };
        *target = checked_total(*target, stock, "INVENTORY_STATS_OVERFLOW")?;
    }
    Ok(stats)
}

fn parse_integer(value: &str, allow_zero: bool, field: &'static str) -> ServiceResult<i64> {
    let value = value.trim();
    let parsed = (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<i64>().ok())
        .flatten()
        .filter(|value| (if allow_zero { 0 } else { 1 }..=JS_MAX_SAFE_INTEGER).contains(value));
    parsed.ok_or_else(|| {
        ServiceError::validation_field(
            "INVALID_QUANTITY",
            if allow_zero {
                "El conteo debe ser un entero no negativo"
            } else {
                "La cantidad debe ser un entero positivo"
            },
            field,
        )
    })
}

fn required_text(
    value: String,
    max: usize,
    code: &'static str,
    field: &'static str,
) -> ServiceResult<String> {
    let value = normalize_persisted(&value);
    if value.is_empty() {
        return Err(ServiceError::validation_field(
            code,
            "El campo es obligatorio",
            field,
        ));
    }
    if value.chars().count() > max {
        return Err(ServiceError::validation_field(
            code,
            "El campo excede la longitud permitida",
            field,
        ));
    }
    Ok(value)
}

fn optional_text(
    value: Option<String>,
    max: usize,
    field: &'static str,
) -> ServiceResult<Option<String>> {
    value
        .map(|value| normalize_persisted(&value))
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > max {
                Err(ServiceError::validation_field(
                    "FIELD_TOO_LONG",
                    "El campo excede la longitud permitida",
                    field,
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn normalize_token(value: &str) -> ServiceResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > OPERATION_TOKEN_MAX_LENGTH {
        return Err(ServiceError::validation(
            "INVALID_OPERATION_TOKEN",
            "El token de operacion es obligatorio y debe tener como maximo 128 caracteres",
        ));
    }
    Ok(value.into())
}

fn validate_quote_fingerprint(value: &str) -> ServiceResult<()> {
    if value.is_empty() || value.len() > 128 {
        Err(ServiceError::validation(
            "INVENTORY_QUOTE_REQUIRED",
            "Calcula la operacion antes de confirmarla",
        ))
    } else {
        Ok(())
    }
}

fn normalize_filter(
    value: Option<String>,
    allowed: &[&str],
    code: &'static str,
) -> ServiceResult<Option<String>> {
    let value = value
        .map(|value| search_key(&value))
        .filter(|value| !value.is_empty());
    match value {
        Some(value) if allowed.contains(&value.as_str()) => Ok(Some(value)),
        Some(_) => Err(ServiceError::validation(code, "El filtro no es valido")),
        None => Ok(None),
    }
}

fn page(value: Option<i64>, field: &'static str) -> ServiceResult<i64> {
    let value = value.unwrap_or(1);
    if (1..=JS_MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(ServiceError::validation_field(
            "INVALID_PAGE",
            "La pagina debe ser un entero positivo",
            field,
        ))
    }
}

fn normalize_date(value: Option<String>, field: &'static str) -> ServiceResult<Option<String>> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(|value| {
            let valid = value.len() == 10
                && value.as_bytes()[4] == b'-'
                && value.as_bytes()[7] == b'-'
                && value
                    .bytes()
                    .enumerate()
                    .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
            if valid {
                Ok(value)
            } else {
                Err(ServiceError::validation_field(
                    "INVALID_DATE",
                    "La fecha debe usar el formato AAAA-MM-DD",
                    field,
                ))
            }
        })
        .transpose()
}

fn direct_capture_name(unit: BaseUnit) -> &'static str {
    match unit {
        BaseUnit::Piece => "Captura directa en piezas",
        BaseUnit::Gram => "Captura directa en gramos",
        BaseUnit::Milliliter => "Captura directa en mililitros",
        BaseUnit::Centimeter => "Captura directa en centimetros",
    }
}

fn base_unit_singular(unit: BaseUnit) -> &'static str {
    match unit {
        BaseUnit::Piece => "pieza",
        BaseUnit::Gram => "gramo",
        BaseUnit::Milliliter => "mililitro",
        BaseUnit::Centimeter => "centimetro",
    }
}

fn optional_number(value: Option<i64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn length_value(value: &str) -> String {
    format!("{}:{value}", value.len())
}

fn opaque_fingerprint(canonical: &str) -> String {
    format!("inventory-v3:{:x}", Sha256::digest(canonical.as_bytes()))
}

fn invalid_quantity() -> ServiceError {
    ServiceError::validation("INVALID_QUANTITY", "La cantidad no es valida")
}

fn stale_quote() -> ServiceError {
    ServiceError::conflict(
        "QUOTE_CHANGED",
        "El stock o la presentacion cambio; cotiza nuevamente",
    )
}

fn stale_correction() -> ServiceError {
    ServiceError::conflict(
        "PURCHASE_PRICE_CORRECTION_CHANGED",
        "La compra cambio; cotiza nuevamente",
    )
}

fn token_reused() -> ServiceError {
    ServiceError::conflict(
        "OPERATION_TOKEN_REUSED",
        "El token de operacion ya fue usado con otra intencion",
    )
}

#[cfg(test)]
mod tests {
    use super::purchase_total;
    use crate::{
        db::migrations,
        models::{
            cart::CheckoutCartInput,
            inventory_movement::{
                ConfirmPurchasePriceCorrectionInput, CreatePurchaseInput, CreateStockCountInput,
                InventoryViewFilters, LookupInventoryMovementInput, QuotePurchaseInput,
                QuotePurchasePriceCorrectionInput, QuoteStockCountInput,
            },
            product::BaseUnit,
            product_presentation::PresentationMode,
        },
        services::{
            cart::{self, SessionCart},
            sales,
        },
    };
    use rusqlite::Connection;
    use std::{
        sync::{Arc, Barrier},
        thread,
        time::Duration,
    };

    fn seed_database(connection: &mut Connection) -> i64 {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(connection).unwrap();
        connection.execute_batch(
            "INSERT INTO brands(name,name_search_key) VALUES('Marca','marca');
             INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');
             INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit)
             VALUES('Aceite','aceite',12000,1000,'ACE-1','ace-1',1,1,'milliliter');
             INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)
             VALUES(1,'Litro','measured',1000,12000,1);",
        ).unwrap();
        connection.last_insert_rowid()
    }

    fn database() -> (Connection, i64) {
        let mut connection = Connection::open_in_memory().unwrap();
        let presentation_id = seed_database(&mut connection);
        (connection, presentation_id)
    }

    fn add_product(
        connection: &Connection,
        name: &str,
        sku: &str,
        unit: &str,
        stock: i64,
        presentation: Option<(&str, &str, i64)>,
    ) -> (i64, Option<i64>) {
        connection.execute(
            "INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit)
             VALUES(?1,lower(?1),100,?2,?3,lower(?3),1,1,?4)",
            rusqlite::params![name, stock, sku, unit],
        ).unwrap();
        let product_id = connection.last_insert_rowid();
        let presentation_id = presentation.map(|(presentation_name, mode, base_quantity)| {
            connection.execute(
                "INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default)
                 VALUES(?1,?2,?3,?4,100,1)",
                rusqlite::params![product_id, presentation_name, mode, base_quantity],
            ).unwrap();
            connection.last_insert_rowid()
        });
        (product_id, presentation_id)
    }

    fn purchase_input(
        product_id: i64,
        presentation_id: Option<i64>,
        quantity: &str,
        price: &str,
    ) -> QuotePurchaseInput {
        QuotePurchaseInput {
            product_id,
            presentation_id,
            quantity: quantity.into(),
            purchase_unit_price: price.into(),
            supplier: "Proveedor Uno".into(),
            document_reference: "FAC-1".into(),
            note: Some("Recepcion".into()),
        }
    }

    fn create_purchase_input(
        quote: &super::PurchaseQuote,
        quantity: &str,
        price: &str,
        token: &str,
    ) -> CreatePurchaseInput {
        CreatePurchaseInput {
            product_id: quote.product_id,
            presentation_id: quote.presentation_id,
            quantity: quantity.into(),
            purchase_unit_price: price.into(),
            supplier: quote.supplier.clone(),
            document_reference: quote.document_reference.clone(),
            note: quote.note.clone(),
            operation_token: token.into(),
            quote_fingerprint: quote.quote_fingerprint.clone(),
        }
    }

    #[test]
    fn purchase_totals_cover_fixed_measured_and_direct_half_up() {
        assert_eq!(
            purchase_total(Some(PresentationMode::Fixed), 3, Some(12), 250).unwrap(),
            750
        );
        assert_eq!(
            purchase_total(Some(PresentationMode::Measured), 250, Some(1_000), 12_000).unwrap(),
            3_000
        );
        assert_eq!(
            purchase_total(Some(PresentationMode::Measured), 1, Some(2), 1).unwrap(),
            1
        );
        assert_eq!(purchase_total(None, 75, None, 10).unwrap(), 750);
        assert_eq!(
            purchase_total(Some(PresentationMode::Measured), 1, Some(3), 1)
                .unwrap_err()
                .code,
            "PURCHASE_TOTAL_ROUNDS_TO_ZERO"
        );
    }

    #[test]
    fn count_and_purchase_normalization_enforce_required_fields() {
        for (field, code, max) in [
            ("adjustment_reason", "ADJUSTMENT_REASON_REQUIRED", 1000),
            ("supplier", "SUPPLIER_REQUIRED", 200),
            ("document_reference", "DOCUMENT_REFERENCE_REQUIRED", 200),
        ] {
            let normalize = |value: String| match field {
                "adjustment_reason" => super::normalize_count(1, "4".into(), value)
                    .map(|input| input.adjustment_reason),
                _ => super::normalize_purchase(
                    1,
                    None,
                    "1".into(),
                    "1.00".into(),
                    if field == "supplier" {
                        value.clone()
                    } else {
                        "Proveedor Uno".into()
                    },
                    if field == "document_reference" {
                        value
                    } else {
                        "FAC-1".into()
                    },
                    None,
                )
                .map(|input| {
                    if field == "supplier" {
                        input.supplier
                    } else {
                        input.document_reference
                    }
                }),
            };
            for (value, message) in [
                (String::new(), "El campo es obligatorio"),
                (" \t\n\u{00a0} ".into(), "El campo es obligatorio"),
                ("á".repeat(max + 1), "El campo excede la longitud permitida"),
            ] {
                let error = normalize(value).unwrap_err();
                assert_eq!(error.message, message, "{field}");
                assert_eq!(error.code, code, "{field}");
                assert_eq!(error.field, Some(field));
            }

            let boundary = "á".repeat(max);
            assert_eq!(
                normalize(format!(" \t{boundary}\n ")).unwrap(),
                boundary,
                "{field}"
            );
        }
    }

    #[test]
    fn measured_purchase_is_transactional_idempotent_and_correctable_without_replay() {
        let (mut connection, presentation_id) = database();
        let quote = super::quote_purchase(
            &connection,
            QuotePurchaseInput {
                product_id: 1,
                presentation_id: Some(presentation_id),
                quantity: "250".into(),
                purchase_unit_price: "120.00".into(),
                supplier: " Proveedor Uno ".into(),
                document_reference: " FAC-1 ".into(),
                note: Some("Recepcion".into()),
            },
        )
        .unwrap();
        assert_eq!(
            (quote.base_quantity, quote.purchase_total_cents),
            (250, 3000)
        );

        let create = || CreatePurchaseInput {
            product_id: 1,
            presentation_id: Some(presentation_id),
            quantity: "250".into(),
            purchase_unit_price: "120.00".into(),
            supplier: "Proveedor Uno".into(),
            document_reference: "FAC-1".into(),
            note: Some("Recepcion".into()),
            operation_token: "purchase-1".into(),
            quote_fingerprint: quote.quote_fingerprint.clone(),
        };
        let first = super::create_purchase(&mut connection, create()).unwrap();
        let retry = super::create_purchase(&mut connection, create()).unwrap();
        assert_eq!(first.id, retry.id);
        assert_eq!(first.record_version, 3);
        assert_eq!(first.purchase_unit_price_cents, Some(12_000));
        assert_eq!(first.purchase_total_cents, Some(3_000));
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1250
        );

        let count_quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "1100".into(),
                adjustment_reason: "Conteo fisico".into(),
            },
        )
        .unwrap();
        let count = super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "1100".into(),
                adjustment_reason: "Conteo fisico".into(),
                operation_token: "count-1".into(),
                quote_fingerprint: count_quote.quote_fingerprint,
            },
        )
        .unwrap();
        let count_retry = super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "1100".into(),
                adjustment_reason: "Conteo fisico".into(),
                operation_token: "count-1".into(),
                quote_fingerprint: "inventory-v3:lost-response-retry".into(),
            },
        )
        .unwrap();
        assert_eq!(count.id, count_retry.id);
        assert_eq!(
            (count.movement_type.as_str(), count.base_quantity),
            ("salida", Some(150))
        );
        assert_eq!(count.note.as_deref(), Some("Conteo fisico"));

        let correction = super::quote_purchase_price_correction(
            &connection,
            QuotePurchasePriceCorrectionInput {
                movement_id: first.id,
                new_purchase_unit_price: "100.00".into(),
            },
        )
        .unwrap();
        assert_eq!(correction.new_purchase_total_cents, 2500);
        let confirm = || ConfirmPurchasePriceCorrectionInput {
            movement_id: first.id,
            new_purchase_unit_price: "100.00".into(),
            quote_fingerprint: correction.quote_fingerprint.clone(),
        };
        super::confirm_purchase_price_correction(&mut connection, confirm()).unwrap();
        super::confirm_purchase_price_correction(&mut connection, confirm()).unwrap();
        assert_eq!(
            connection.query_row(
                "SELECT purchase_unit_price_cents,purchase_total_cents FROM inventory_movements WHERE id=?1",
                [first.id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            ).unwrap(),
            (10_000, 2_500)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT stock_after FROM inventory_movements WHERE id=?1",
                    [count.id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1100
        );
    }

    #[test]
    fn lookup_recovers_a_lost_response_and_reports_absence_without_writing() {
        let (mut connection, presentation_id) = database();
        let quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "250", "120.00"),
        )
        .unwrap();
        let created = super::create_purchase(
            &mut connection,
            create_purchase_input(&quote, "250", "120.00", "lost-response"),
        )
        .unwrap();
        let stock_before_lookup = connection
            .query_row("SELECT stock FROM products WHERE id=1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();

        let recovered = super::lookup_movement(
            &connection,
            LookupInventoryMovementInput {
                operation_token: "lost-response".into(),
            },
        )
        .unwrap()
        .unwrap();
        let absent = super::lookup_movement(
            &connection,
            LookupInventoryMovementInput {
                operation_token: "not-applied".into(),
            },
        )
        .unwrap();
        let invalid = super::lookup_movement(
            &connection,
            LookupInventoryMovementInput {
                operation_token: "   ".into(),
            },
        )
        .unwrap_err();

        assert_eq!(recovered.id, created.id);
        assert_eq!(recovered.operation_token.as_deref(), Some("lost-response"));
        assert!(absent.is_none());
        assert_eq!(invalid.code, "INVALID_OPERATION_TOKEN");
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            stock_before_lookup
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn purchase_and_count_reject_stale_quotes_without_writes() {
        let (mut connection, presentation_id) = database();
        assert_eq!(
            super::quote_stock_count(
                &connection,
                QuoteStockCountInput {
                    product_id: 1,
                    counted_stock: "1000".into(),
                    adjustment_reason: "Conteo".into(),
                },
            )
            .unwrap_err()
            .code,
            "STOCK_COUNT_UNCHANGED"
        );
        let purchase = QuotePurchaseInput {
            product_id: 1,
            presentation_id: Some(presentation_id),
            quantity: "100".into(),
            purchase_unit_price: "10.00".into(),
            supplier: "Proveedor".into(),
            document_reference: "FAC-2".into(),
            note: None,
        };
        let quote = super::quote_purchase(&connection, purchase).unwrap();
        connection
            .execute(
                "UPDATE product_presentations SET base_quantity=500 WHERE id=?1",
                [presentation_id],
            )
            .unwrap();
        assert_eq!(
            super::create_purchase(
                &mut connection,
                CreatePurchaseInput {
                    product_id: 1,
                    presentation_id: Some(presentation_id),
                    quantity: "100".into(),
                    purchase_unit_price: "10.00".into(),
                    supplier: "Proveedor".into(),
                    document_reference: "FAC-2".into(),
                    note: None,
                    operation_token: "stale-purchase".into(),
                    quote_fingerprint: quote.quote_fingerprint,
                }
            )
            .unwrap_err()
            .code,
            "QUOTE_CHANGED"
        );

        let quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "900".into(),
                adjustment_reason: "Conteo".into(),
            },
        )
        .unwrap();
        connection
            .execute("UPDATE products SET stock=950 WHERE id=1", [])
            .unwrap();
        assert_eq!(
            super::create_stock_count(
                &mut connection,
                CreateStockCountInput {
                    product_id: 1,
                    counted_stock: "900".into(),
                    adjustment_reason: "Conteo".into(),
                    operation_token: "stale-count".into(),
                    quote_fingerprint: quote.quote_fingerprint,
                }
            )
            .unwrap_err()
            .code,
            "QUOTE_CHANGED"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn purchases_cover_fixed_direct_and_measured_kg_liter_meter() {
        let (connection, liter) = database();
        let (piece, box_id) = add_product(
            &connection,
            "Tornillo",
            "TOR-1",
            "piece",
            10,
            Some(("Caja", "fixed", 6)),
        );
        let (gram, kilogram_id) = add_product(
            &connection,
            "Harina",
            "HAR-1",
            "gram",
            2_000,
            Some(("Kilogramo", "measured", 1_000)),
        );
        let (centimeter, meter_id) = add_product(
            &connection,
            "Cable",
            "CAB-1",
            "centimeter",
            500,
            Some(("Metro", "measured", 100)),
        );

        let fixed =
            super::quote_purchase(&connection, purchase_input(piece, box_id, "3", "5.00")).unwrap();
        assert_eq!(
            (fixed.base_quantity, fixed.purchase_total_cents),
            (18, 1_500)
        );
        let direct =
            super::quote_purchase(&connection, purchase_input(piece, None, "4", "0.25")).unwrap();
        assert_eq!(
            (direct.base_quantity, direct.purchase_total_cents),
            (4, 100)
        );
        assert_eq!(direct.capture_name, "Captura directa en piezas");

        for (product_id, presentation_id, quantity, price, expected_total) in [
            (gram, kilogram_id.unwrap(), "250", "40.00", 1_000),
            (1, liter, "250", "120.00", 3_000),
            (centimeter, meter_id.unwrap(), "75", "25.00", 1_875),
        ] {
            let quote = super::quote_purchase(
                &connection,
                purchase_input(product_id, Some(presentation_id), quantity, price),
            )
            .unwrap();
            assert_eq!(quote.base_quantity, quantity.parse::<i64>().unwrap());
            assert_eq!(quote.purchase_total_cents, expected_total);
        }
    }

    #[test]
    fn purchase_persists_metadata_rejects_zero_and_conflicting_token() {
        let (mut connection, presentation_id) = database();
        let quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "250", "120.00"),
        )
        .unwrap();
        let movement = super::create_purchase(
            &mut connection,
            create_purchase_input(&quote, "250", "120.00", "purchase-metadata"),
        )
        .unwrap();
        assert_eq!(movement.supplier.as_deref(), Some("Proveedor Uno"));
        assert_eq!(movement.document_reference.as_deref(), Some("FAC-1"));
        assert_eq!(movement.purchase_unit_price_cents, Some(12_000));
        assert_eq!(movement.purchase_total_cents, Some(3_000));

        let conflict_quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "100", "120.00"),
        )
        .unwrap();
        assert_eq!(
            super::create_purchase(
                &mut connection,
                create_purchase_input(&conflict_quote, "100", "120.00", "purchase-metadata"),
            )
            .unwrap_err()
            .code,
            "OPERATION_TOKEN_REUSED"
        );
        assert_eq!(
            super::quote_purchase(
                &connection,
                purchase_input(1, Some(presentation_id), "1", "0.01"),
            )
            .unwrap_err()
            .code,
            "PURCHASE_TOTAL_ROUNDS_TO_ZERO"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn purchase_overflow_and_insert_failure_leave_stock_unchanged() {
        let (mut connection, presentation_id) = database();
        connection
            .execute(
                "UPDATE products SET stock=?1 WHERE id=1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        assert_eq!(
            super::quote_purchase(
                &connection,
                purchase_input(1, Some(presentation_id), "1", "1.00"),
            )
            .unwrap_err()
            .code,
            "STOCK_OVERFLOW"
        );
        connection
            .execute("UPDATE products SET stock=1000 WHERE id=1", [])
            .unwrap();
        let quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "100", "10.00"),
        )
        .unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_purchase_movement BEFORE INSERT ON inventory_movements
             BEGIN SELECT RAISE(ABORT,'forced rollback'); END;",
            )
            .unwrap();
        assert!(super::create_purchase(
            &mut connection,
            create_purchase_input(&quote, "100", "10.00", "purchase-rollback"),
        )
        .is_err());
        assert_eq!(
            connection.query_row(
                "SELECT stock,(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            ).unwrap(),
            (1000, 0)
        );
    }

    #[test]
    fn stock_counts_cover_higher_lower_zero_equal_and_all_units() {
        let (mut connection, _) = database();
        let (piece, _) = add_product(&connection, "Pieza", "PIE-1", "piece", 10, None);
        let (gram, _) = add_product(&connection, "Granel", "GRA-1", "gram", 10, None);
        let (centimeter, _) = add_product(&connection, "Longitud", "LON-1", "centimeter", 10, None);

        for (product_id, counted, expected_difference, expected_type) in [
            (piece, "15", 5, "entrada"),
            (gram, "4", -6, "salida"),
            (centimeter, "0", -10, "salida"),
            (1, "900", -100, "salida"),
        ] {
            let quote = super::quote_stock_count(
                &connection,
                QuoteStockCountInput {
                    product_id,
                    counted_stock: counted.into(),
                    adjustment_reason: "Conteo fisico".into(),
                },
            )
            .unwrap();
            assert_eq!(quote.difference, expected_difference);
            assert_eq!(quote.movement_type, expected_type);
            assert_eq!(quote.base_quantity, expected_difference.abs());
            let movement = super::create_stock_count(
                &mut connection,
                CreateStockCountInput {
                    product_id,
                    counted_stock: counted.into(),
                    adjustment_reason: "Conteo fisico".into(),
                    operation_token: format!("count-{product_id}"),
                    quote_fingerprint: quote.quote_fingerprint,
                },
            )
            .unwrap();
            assert_eq!(movement.stock_after, Some(counted.parse().unwrap()));
        }
        assert_eq!(
            super::quote_stock_count(
                &connection,
                QuoteStockCountInput {
                    product_id: piece,
                    counted_stock: "15".into(),
                    adjustment_reason: "Reconteo".into(),
                },
            )
            .unwrap_err()
            .code,
            "STOCK_COUNT_UNCHANGED"
        );
    }

    #[test]
    fn stock_count_rejects_conflicting_token_without_changing_stock() {
        let (mut connection, _) = database();
        let first_quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "900".into(),
                adjustment_reason: "Conteo".into(),
            },
        )
        .unwrap();
        super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "900".into(),
                adjustment_reason: "Conteo".into(),
                operation_token: "count-token".into(),
                quote_fingerprint: first_quote.quote_fingerprint,
            },
        )
        .unwrap();
        assert_eq!(
            super::create_stock_count(
                &mut connection,
                CreateStockCountInput {
                    product_id: 1,
                    counted_stock: "800".into(),
                    adjustment_reason: "Otro conteo".into(),
                    operation_token: "count-token".into(),
                    quote_fingerprint: "inventory-v3:retry".into(),
                },
            )
            .unwrap_err()
            .code,
            "OPERATION_TOKEN_REUSED"
        );
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            900
        );
    }

    #[test]
    fn price_correction_enforces_eligibility_staleness_idempotency_and_immutability() {
        let (mut connection, presentation_id) = database();
        let purchase_quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "250", "120.00"),
        )
        .unwrap();
        let purchase = super::create_purchase(
            &mut connection,
            create_purchase_input(&purchase_quote, "250", "120.00", "correctable"),
        )
        .unwrap();
        let immutable_before: (i64, String, String, i64, i64, String, String, Option<String>, String) = connection.query_row(
            "SELECT product_id,product_name,sku,entered_quantity,base_quantity,supplier,document_reference,note,operation_token
             FROM inventory_movements WHERE id=?1",
            [purchase.id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
        ).unwrap();
        let correction = super::quote_purchase_price_correction(
            &connection,
            QuotePurchasePriceCorrectionInput {
                movement_id: purchase.id,
                new_purchase_unit_price: "100.00".into(),
            },
        )
        .unwrap();
        let confirmation = || ConfirmPurchasePriceCorrectionInput {
            movement_id: purchase.id,
            new_purchase_unit_price: "100.00".into(),
            quote_fingerprint: correction.quote_fingerprint.clone(),
        };
        let applied =
            super::confirm_purchase_price_correction(&mut connection, confirmation()).unwrap();
        let retried =
            super::confirm_purchase_price_correction(&mut connection, confirmation()).unwrap();
        assert_eq!(applied.quote_fingerprint, retried.quote_fingerprint);
        assert_eq!(applied.new_purchase_total_cents, 2_500);
        let immutable_after: (i64, String, String, i64, i64, String, String, Option<String>, String) = connection.query_row(
            "SELECT product_id,product_name,sku,entered_quantity,base_quantity,supplier,document_reference,note,operation_token
             FROM inventory_movements WHERE id=?1",
            [purchase.id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
        ).unwrap();
        assert_eq!(immutable_after, immutable_before);

        let count_quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "1200".into(),
                adjustment_reason: "Conteo".into(),
            },
        )
        .unwrap();
        let count = super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "1200".into(),
                adjustment_reason: "Conteo".into(),
                operation_token: "not-correctable".into(),
                quote_fingerprint: count_quote.quote_fingerprint,
            },
        )
        .unwrap();
        assert_eq!(
            super::quote_purchase_price_correction(
                &connection,
                QuotePurchasePriceCorrectionInput {
                    movement_id: count.id,
                    new_purchase_unit_price: "2.00".into(),
                },
            )
            .unwrap_err()
            .code,
            "PURCHASE_PRICE_CORRECTION_NOT_ELIGIBLE"
        );

        let stale_quote = super::quote_purchase_price_correction(
            &connection,
            QuotePurchasePriceCorrectionInput {
                movement_id: purchase.id,
                new_purchase_unit_price: "90.00".into(),
            },
        )
        .unwrap();
        connection.execute(
            "UPDATE inventory_movements SET supplier='Proveedor editado',provider_search_key='proveedor editado' WHERE id=?1",
            [purchase.id],
        ).unwrap();
        assert_eq!(
            super::confirm_purchase_price_correction(
                &mut connection,
                ConfirmPurchasePriceCorrectionInput {
                    movement_id: purchase.id,
                    new_purchase_unit_price: "90.00".into(),
                    quote_fingerprint: stale_quote.quote_fingerprint,
                },
            )
            .unwrap_err()
            .code,
            "PURCHASE_PRICE_CORRECTION_CHANGED"
        );
        assert_eq!(
            connection.query_row(
                "SELECT purchase_unit_price_cents,purchase_total_cents FROM inventory_movements WHERE id=?1",
                [purchase.id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            ).unwrap(),
            (10_000, 2_500)
        );
    }

    #[test]
    fn purchase_concurrency_after_reopen_applies_only_one_quote() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory-race.db");
        let create = {
            let mut connection = Connection::open(&path).unwrap();
            let presentation_id = seed_database(&mut connection);
            let quote = super::quote_purchase(
                &connection,
                purchase_input(1, Some(presentation_id), "250", "120.00"),
            )
            .unwrap();
            create_purchase_input(&quote, "250", "120.00", "race-placeholder")
        };
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = ["race-purchase-a", "race-purchase-b"]
            .into_iter()
            .map(|token| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                let mut input = create.clone();
                input.operation_token = token.into();
                thread::spawn(move || {
                    let mut connection = Connection::open(path).unwrap();
                    connection.busy_timeout(Duration::from_secs(10)).unwrap();
                    connection
                        .pragma_update(None, "foreign_keys", true)
                        .unwrap();
                    barrier.wait();
                    super::create_purchase(&mut connection, input)
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .find_map(|result| result.as_ref().err())
                .unwrap()
                .code,
            "QUOTE_CHANGED"
        );
        let reopened = Connection::open(&path).unwrap();
        assert_eq!(
            reopened
                .query_row(
                    "SELECT stock,(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .unwrap(),
            (1_250, 1)
        );
    }

    #[test]
    fn inventory_view_filters_current_movements_and_reports_unit_statistics() {
        let (mut connection, presentation_id) = database();
        let (piece, _) = add_product(&connection, "Tornillo", "TOR-1", "piece", 10, None);
        let (gram, _) = add_product(&connection, "Harina", "HAR-1", "gram", 20, None);
        let (centimeter, _) = add_product(&connection, "Cable", "CAB-1", "centimeter", 30, None);
        let purchase_quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "250", "120.00"),
        )
        .unwrap();
        super::create_purchase(
            &mut connection,
            create_purchase_input(&purchase_quote, "250", "120.00", "filtered-purchase"),
        )
        .unwrap();
        let count_quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "1200".into(),
                adjustment_reason: "Merma de bodega".into(),
            },
        )
        .unwrap();
        super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "1200".into(),
                adjustment_reason: "Merma de bodega".into(),
                operation_token: "filtered-count".into(),
                quote_fingerprint: count_quote.quote_fingerprint,
            },
        )
        .unwrap();

        let view = super::view(
            &connection,
            InventoryViewFilters {
                product_search: Some(" cab-1 ".into()),
                base_unit: Some(BaseUnit::Centimeter),
                movement_search: Some("proveedor uno".into()),
                reason: Some(" COMPRA ".into()),
                movement_type: Some("entrada".into()),
                ..InventoryViewFilters::default()
            },
        )
        .unwrap();
        assert_eq!(view.products.len(), 1);
        assert_eq!(view.products[0].id, centimeter);
        assert_eq!(view.movements.len(), 1);
        assert_eq!(view.movements[0].reason, "compra");
        assert_eq!(
            (
                view.stats.product_count,
                view.stats.piece_stock,
                view.stats.gram_stock,
                view.stats.milliliter_stock,
                view.stats.centimeter_stock,
            ),
            (4, 10, 20, 1_200, 30)
        );
        assert_eq!(piece, 2);
        assert_eq!(gram, 3);
        assert_eq!(
            super::view(
                &connection,
                InventoryViewFilters {
                    reason: Some("devolucion".into()),
                    ..InventoryViewFilters::default()
                },
            )
            .unwrap_err()
            .code,
            "INVALID_REASON"
        );
    }

    #[test]
    fn inventory_statistics_reject_aggregate_overflow() {
        let (connection, _) = database();
        connection
            .execute(
                "UPDATE products SET stock=?1 WHERE id=1",
                [crate::services::JS_MAX_SAFE_INTEGER],
            )
            .unwrap();
        add_product(&connection, "Otro aceite", "ACE-2", "milliliter", 1, None);

        assert_eq!(
            super::view(&connection, InventoryViewFilters::default())
                .unwrap_err()
                .code,
            "INVENTORY_STATS_OVERFLOW"
        );
    }

    #[test]
    fn correction_changes_only_the_target_purchase_and_rejects_legacy_or_zero_total() {
        let (mut connection, presentation_id) = database();
        connection.execute_batch(
            "INSERT INTO users(first_name,last_name,email,password_hash) VALUES('Ada','POS','ada@pos.local','hash');",
        ).unwrap();
        let purchase_quote = super::quote_purchase(
            &connection,
            purchase_input(1, Some(presentation_id), "250", "120.00"),
        )
        .unwrap();
        let purchase = super::create_purchase(
            &mut connection,
            create_purchase_input(&purchase_quote, "250", "120.00", "correction-target"),
        )
        .unwrap();

        let cart = SessionCart::from([(presentation_id, 100)]);
        let fingerprint = cart::get(&connection, &cart).unwrap().cart_fingerprint;
        let sale = sales::checkout(
            &mut connection,
            1,
            &cart,
            CheckoutCartInput {
                checkout_token: "sale-after-purchase".into(),
                cart_fingerprint: fingerprint,
                payment_method: "tarjeta".into(),
                cash_received: None,
            },
        )
        .unwrap();
        let count_quote = super::quote_stock_count(
            &connection,
            QuoteStockCountInput {
                product_id: 1,
                counted_stock: "1100".into(),
                adjustment_reason: "Conteo posterior".into(),
            },
        )
        .unwrap();
        let count = super::create_stock_count(
            &mut connection,
            CreateStockCountInput {
                product_id: 1,
                counted_stock: "1100".into(),
                adjustment_reason: "Conteo posterior".into(),
                operation_token: "count-after-purchase".into(),
                quote_fingerprint: count_quote.quote_fingerprint,
            },
        )
        .unwrap();
        connection.execute_batch(
            "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,
                 record_version,origin,reason,type,quantity,operation_token,intent_fingerprint)
             VALUES(1,'Aceite','ACE-1','aceite','ace-1',1,'manual','compra','entrada',10,'legacy-v1','legacy-v1');
             INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,
                 record_version,base_unit,origin,reason,type,quantity,entered_quantity,base_quantity,
                 stock_before,stock_after,operation_token,intent_fingerprint)
             VALUES(1,'Aceite','ACE-1','aceite','ace-1',2,'milliliter','manual','compra','entrada',10,10,10,
                 1100,1110,'legacy-v2','legacy-v2');",
        ).unwrap();
        let legacy_ids: Vec<i64> = connection
            .prepare("SELECT id FROM inventory_movements WHERE operation_token LIKE 'legacy-v%' ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for movement_id in legacy_ids {
            assert_eq!(
                super::quote_purchase_price_correction(
                    &connection,
                    QuotePurchasePriceCorrectionInput {
                        movement_id,
                        new_purchase_unit_price: "2.00".into(),
                    },
                )
                .unwrap_err()
                .code,
                "PURCHASE_PRICE_CORRECTION_NOT_ELIGIBLE"
            );
        }
        assert_eq!(
            super::quote_purchase_price_correction(
                &connection,
                QuotePurchasePriceCorrectionInput {
                    movement_id: purchase.id,
                    new_purchase_unit_price: "0.01".into(),
                },
            )
            .unwrap_err()
            .code,
            "PURCHASE_TOTAL_ROUNDS_TO_ZERO"
        );

        let sale_before =
            serde_json::to_value(sales::history_detail(&connection, sale.sale.id).unwrap())
                .unwrap();
        let later_movements_before = serde_json::to_value(
            super::list(&connection)
                .unwrap()
                .into_iter()
                .filter(|movement| movement.id != purchase.id)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let correction = super::quote_purchase_price_correction(
            &connection,
            QuotePurchasePriceCorrectionInput {
                movement_id: purchase.id,
                new_purchase_unit_price: "100.00".into(),
            },
        )
        .unwrap();
        super::confirm_purchase_price_correction(
            &mut connection,
            ConfirmPurchasePriceCorrectionInput {
                movement_id: purchase.id,
                new_purchase_unit_price: "100.00".into(),
                quote_fingerprint: correction.quote_fingerprint,
            },
        )
        .unwrap();

        let sale_after =
            serde_json::to_value(sales::history_detail(&connection, sale.sale.id).unwrap())
                .unwrap();
        let later_movements_after = serde_json::to_value(
            super::list(&connection)
                .unwrap()
                .into_iter()
                .filter(|movement| movement.id != purchase.id)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(sale_after, sale_before);
        assert_eq!(later_movements_after, later_movements_before);
        assert_eq!(count.stock_after, Some(1_100));
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1_100
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT purchase_unit_price_cents,purchase_total_cents FROM inventory_movements WHERE id=?1",
                    [purchase.id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )
                .unwrap(),
            (10_000, 2_500)
        );
    }
}
