use rusqlite::{named_params, params, Connection, OptionalExtension, Row, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::models::{
    inventory_movement::{
        CreateInventoryMovementInput, InventoryMovement, InventoryMovementPresentation,
        InventoryMovementProduct, InventoryMovementQuote, InventoryProductSearchResult,
        InventoryView, InventoryViewFilters, InventoryViewStats, ProductInventoryDetail,
        ProductInventoryDetailFilters, QuoteInventoryMovementInput,
    },
    product::{BaseUnit, InventoryProductRow},
    product_presentation::PresentationMode,
};

use super::{
    checked_multiply, checked_total, divide_half_up_positive, line_total, normalize_persisted,
    normalize_search, parse_money_text_allow_zero, presentation_base_quantity,
    proportional_cost_removal, search_key, validate_id, ServiceError, ServiceResult,
    JS_MAX_SAFE_INTEGER,
};

const OPERATION_TOKEN_MAX_LENGTH: usize = 128;
const PAGE_SIZE: i64 = 20;

fn map_movement(row: &Row<'_>) -> rusqlite::Result<InventoryMovement> {
    let base_unit = row.get::<_, Option<String>>(14)?.map(|value| {
        BaseUnit::try_from(value.as_str()).expect("database base unit is constrained")
    });
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
        presentation_id: row.get(9)?,
        presentation_name: row.get(10)?,
        presentation_barcode: row.get(11)?,
        presentation_mode: row.get(12)?,
        presentation_base_quantity: row.get(13)?,
        base_unit,
        reason: row.get(15)?,
        movement_type: row.get(16)?,
        quantity: row.get(17)?,
        entered_quantity: row.get(18)?,
        base_quantity: row.get(19)?,
        stock_before: row.get(20)?,
        stock_after: row.get(21)?,
        movement_cost_cents: row.get(22)?,
        inventory_cost_before_cents: row.get(23)?,
        inventory_cost_after_cents: row.get(24)?,
        supplier: row.get(25)?,
        document_reference: row.get(26)?,
        note: row.get(27)?,
        operation_token: row.get(28)?,
        unit_price_cents: row.get(29)?,
        created_at: row.get(30)?,
        updated_at: row.get(31)?,
    })
}

const MOVEMENT_COLUMNS: &str = "m.id,m.product_id,p.name,p.sku,m.product_name,m.sku,m.sale_id,
     CASE WHEN m.sale_id IS NULL THEN NULL ELSE printf('POS-%08d',m.sale_id) END,
     m.record_version,m.presentation_id,m.presentation_name,m.presentation_barcode,
     m.presentation_mode,m.presentation_base_quantity,m.base_unit,m.reason,m.type,m.quantity,
     m.entered_quantity,m.base_quantity,m.stock_before,m.stock_after,m.movement_cost_cents,
     m.inventory_cost_before_cents,m.inventory_cost_after_cents,m.supplier,
      m.document_reference,m.note,m.operation_token,m.unit_price_cents,m.created_at,m.updated_at";

pub fn list(connection: &Connection) -> ServiceResult<Vec<InventoryMovement>> {
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m
         JOIN products p ON p.id=m.product_id ORDER BY m.created_at DESC,m.id DESC"
    );
    let mut statement = connection.prepare(&sql)?;
    let movements = statement
        .query_map([], map_movement)?
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(movements)
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
    let where_sql = "?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0";
    let total = transaction.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {where_sql}"),
        [&search],
        |row| row.get(0),
    )?;
    let mut statement = transaction.prepare(&format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit,p.inventory_cost_cents
         FROM products p WHERE {where_sql}
         ORDER BY p.name_search_key,p.id LIMIT ?2 OFFSET ?3"
    ))?;
    let product_rows = statement
        .query_map(params![search, PAGE_SIZE, offset], map_inventory_product)?
        .collect::<Result<Vec<_>, _>>()?;
    let items = product_rows
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
    let view = view_snapshot(&transaction, filters)?;
    transaction.commit()?;
    Ok(view)
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
    let cost_status = normalize_filter(
        filters.cost_status,
        &["known", "unknown"],
        "INVALID_COST_STATUS",
    )?;
    let base_unit = filters.base_unit.map(|value| value.as_str());
    let product_page = page(filters.product_page, "product_page")?;
    let movement_page = page(filters.movement_page, "movement_page")?;
    let product_offset = (product_page - 1) * PAGE_SIZE;
    let movement_offset = (movement_page - 1) * PAGE_SIZE;

    let product_where = "(?1='' OR instr(p.name_search_key,?1)>0 OR instr(p.sku_search_key,?1)>0)
        AND (?2 IS NULL OR p.base_unit=?2)
        AND (?3 IS NULL OR (?3='known' AND p.inventory_cost_cents IS NOT NULL)
             OR (?3='unknown' AND p.inventory_cost_cents IS NULL))";
    let product_total = connection.query_row(
        &format!("SELECT COUNT(*) FROM products p WHERE {product_where}"),
        params![product_search, base_unit, cost_status],
        |row| row.get(0),
    )?;
    let mut product_statement = connection.prepare(&format!(
        "SELECT p.id,p.name,p.sku,p.stock,p.base_unit,p.inventory_cost_cents
         FROM products p WHERE {product_where} ORDER BY p.name_search_key,p.id LIMIT ?4 OFFSET ?5"
    ))?;
    let product_rows = product_statement
        .query_map(
            params![
                product_search,
                base_unit,
                cost_status,
                PAGE_SIZE,
                product_offset
            ],
            map_inventory_product,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let products = product_rows
        .into_iter()
        .map(|product| inventory_movement_product(connection, product))
        .collect::<ServiceResult<Vec<_>>>()?;

    let movement_where = "(?1 IS NULL OR m.reason=?1) AND (?2 IS NULL OR m.type=?2)
        AND (?3='' OR instr(p.name_search_key,?3)>0 OR instr(p.sku_search_key,?3)>0
             OR instr(m.product_search_key,?3)>0 OR instr(m.sku_search_key,?3)>0
              OR instr(coalesce(m.provider_search_key,''),?3)>0
              OR instr(coalesce(m.document_reference_search_key,''),?3)>0
              OR instr(coalesce(m.note_search_key,''),?3)>0
             OR (m.sale_id IS NOT NULL AND instr(search_key_ticket(m.sale_id),?3)>0))";
    // SQLite has no application search-key function; ticket references are ASCII and can be compared lower-case.
    let movement_where = movement_where.replace(
        "search_key_ticket(m.sale_id)",
        "lower(printf('POS-%08d',m.sale_id))",
    );
    let movement_total = connection.query_row(
        &format!(
            "SELECT COUNT(*) FROM inventory_movements m JOIN products p ON p.id=m.product_id WHERE {movement_where}"
        ),
        params![reason, movement_type, movement_search],
        |row| row.get(0),
    )?;
    let movement_sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id
         WHERE {movement_where} ORDER BY m.created_at DESC,m.id DESC LIMIT ?4 OFFSET ?5"
    );
    let mut movement_statement = connection.prepare(&movement_sql)?;
    let movements = movement_statement
        .query_map(
            params![
                reason,
                movement_type,
                movement_search,
                PAGE_SIZE,
                movement_offset
            ],
            map_movement,
        )?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(InventoryView {
        products,
        items: movements.clone(),
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
    let detail = product_detail_snapshot(&transaction, product_id, filters)?;
    transaction.commit()?;
    Ok(detail)
}

fn product_detail_snapshot(
    connection: &Connection,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> ServiceResult<ProductInventoryDetail> {
    validate_id(product_id)?;
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
    let date_from = normalize_date(filters.date_from, "date_from")?;
    let date_to = normalize_date(filters.date_to, "date_to")?;
    if matches!((&date_from, &date_to), (Some(from), Some(to)) if from > to) {
        return Err(ServiceError::validation(
            "INVALID_DATE_RANGE",
            "La fecha inicial no puede ser posterior a la fecha final",
        ));
    }
    let page = page(filters.page, "page")?;
    let offset = (page - 1) * PAGE_SIZE;
    let product = connection
        .query_row(
            "SELECT id,name,sku,stock,base_unit,inventory_cost_cents FROM products WHERE id=?1",
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
    let query_params = named_params! {
        ":product_id": product_id,
        ":reason": reason,
        ":movement_type": movement_type,
        ":date_from": date_from,
        ":date_to": date_to,
        ":search": movement_search,
    };
    let total = connection.query_row(
        &format!("SELECT COUNT(*) FROM inventory_movements m WHERE {where_sql}"),
        query_params,
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS} FROM inventory_movements m JOIN products p ON p.id=m.product_id
         WHERE {where_sql} ORDER BY m.created_at DESC,m.id DESC LIMIT :limit OFFSET :offset"
    );
    let mut statement = connection.prepare(&sql)?;
    let movements = statement
        .query_map(
            named_params! {
                ":product_id": product_id,
                ":reason": reason,
                ":movement_type": movement_type,
                ":date_from": date_from,
                ":date_to": date_to,
                ":search": movement_search,
                ":limit": PAGE_SIZE,
                ":offset": offset,
            },
            map_movement,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let last_movement_at = connection
        .query_row(
            "SELECT created_at FROM inventory_movements WHERE product_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1",
            [product_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(ProductInventoryDetail {
        product,
        last_movement_at,
        movements,
        page,
        total,
        page_size: PAGE_SIZE,
    })
}

pub fn create(
    connection: &mut Connection,
    input: CreateInventoryMovementInput,
) -> ServiceResult<InventoryMovement> {
    validate_id(input.product_id)?;
    let operation_token = normalize_token(&input.operation_token)?;
    let reason = search_key(&input.reason);
    let movement_type = search_key(&input.movement_type);
    validate_movement_kind(&reason, &movement_type)?;
    let entered_quantity = parse_entered_quantity(&input.quantity, reason == "valoracion")?;
    if let Some(id) = input.presentation_id {
        validate_id(id)?;
    }
    let legacy_fingerprint =
        legacy_fingerprint(input.product_id, &reason, &movement_type, entered_quantity);
    let legacy_shape = input.presentation_id.is_none()
        && input.unit_price.is_none()
        && input.supplier.is_none()
        && input.document_reference.is_none()
        && input.note.is_none()
        && input.quote_fingerprint.is_none();

    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some((movement, persisted_fingerprint)) =
        movement_by_token(&transaction, &operation_token)?
    {
        if legacy_shape && persisted_fingerprint == legacy_fingerprint {
            return Ok(movement);
        }
        let normalized = normalize_new_input(input, reason, movement_type, entered_quantity)?;
        if persisted_fingerprint == normalized.fingerprint {
            return Ok(movement);
        }
        return Err(token_reused());
    }

    if legacy_shape {
        return Err(ServiceError::conflict(
            "INVENTORY_LEGACY_REQUOTE_REQUIRED",
            "La intencion pendiente debe cotizarse nuevamente antes de confirmarla",
        ));
    }

    let normalized = normalize_new_input(input, reason, movement_type, entered_quantity)?;
    let submitted_quote = normalized.quote_fingerprint.as_deref().ok_or_else(|| {
        ServiceError::validation(
            "INVENTORY_QUOTE_REQUIRED",
            "Calcula el movimiento antes de confirmarlo",
        )
    })?;
    let current_quote = calculate_quote(
        &transaction,
        normalized.product_id,
        normalized.presentation_id,
        &normalized.reason,
        &normalized.movement_type,
        normalized.entered_quantity,
        normalized.unit_price,
    )?;
    if current_quote.quote_fingerprint != submitted_quote {
        return Err(ServiceError::conflict(
            "QUOTE_CHANGED",
            "La cotizacion cambio; revisa el nuevo importe y confirma nuevamente",
        ));
    }
    let product = transaction
        .query_row(
            "SELECT name,sku,stock,base_unit,inventory_cost_cents FROM products WHERE id=?1",
            [normalized.product_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))?;
    let presentation = normalized
        .presentation_id
        .map(|id| load_presentation(&transaction, normalized.product_id, id))
        .transpose()?;
    let (base_quantity, presentation_name, presentation_barcode, presentation_mode, factor) =
        match presentation {
            Some((name, barcode, mode, factor)) => (
                presentation_base_quantity(mode, normalized.entered_quantity, factor)?,
                Some(name),
                barcode,
                Some(mode.as_str()),
                Some(factor),
            ),
            None => (normalized.entered_quantity, None, None, None, None),
        };
    let stock_before = product.2;
    let cost_before = product.4;
    let (stock_after, unit_price_cents, movement_cost, cost_after) =
        match normalized.movement_type.as_str() {
            "entrada" => {
                let unit_price = normalized.unit_price.ok_or_else(|| {
                    ServiceError::validation_field(
                        "INVENTORY_COST_REQUIRED",
                        "Las entradas requieren un precio unitario",
                        "unit_price",
                    )
                })?;
                let total = match (presentation_mode, factor) {
                    (Some(mode), Some(factor)) => line_total(
                        PresentationMode::try_from(mode).unwrap(),
                        normalized.entered_quantity,
                        factor,
                        unit_price,
                    )?,
                    _ => checked_multiply(base_quantity, unit_price, "COST_OVERFLOW")?,
                };
                if total == 0 {
                    return Err(ServiceError::validation_field(
                        "INVALID_INVENTORY_COST",
                        "El costo total debe ser positivo",
                        "unit_price",
                    ));
                }
                let stock_after = safe_add(stock_before, base_quantity, "STOCK_OVERFLOW")?;
                let cost_after = cost_before
                    .map(|cost| safe_add(cost, total, "COST_OVERFLOW"))
                    .transpose()?;
                (stock_after, Some(unit_price), Some(total), cost_after)
            }
            "salida" => {
                let stock_after = stock_before
                    .checked_sub(base_quantity)
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| {
                        ServiceError::conflict(
                            "INSUFFICIENT_STOCK",
                            "Stock insuficiente para completar el movimiento",
                        )
                    })?;
                let removed = cost_before
                    .map(|cost| proportional_cost_removal(cost, base_quantity, stock_before))
                    .transpose()?;
                let cost_after = match (cost_before, removed) {
                    (Some(cost), Some(removed)) => Some(cost - removed),
                    (None, _) if stock_after == 0 => Some(0),
                    (None, _) => None,
                    _ => unreachable!(),
                };
                let applied_unit_cost = cost_before
                    .map(|cost| {
                        divide_half_up_positive(
                            i128::from(cost),
                            i128::from(stock_before),
                            "COST_OVERFLOW",
                        )
                    })
                    .transpose()?;
                (stock_after, applied_unit_cost, removed, cost_after)
            }
            "valoracion" => (
                stock_before,
                None,
                normalized.unit_price,
                normalized.unit_price,
            ),
            _ => unreachable!(),
        };
    transaction.execute(
        "UPDATE products SET stock=?1,inventory_cost_cents=?2,updated_at=CURRENT_TIMESTAMP WHERE id=?3",
        params![stock_after, cost_after, normalized.product_id],
    )?;
    transaction.execute(
        "INSERT INTO inventory_movements (
           product_id,product_name,sku,product_search_key,sku_search_key,record_version,
           presentation_id,presentation_name,presentation_barcode,base_unit,presentation_mode,
           presentation_base_quantity,unit_price_cents,origin,reason,type,quantity,entered_quantity,base_quantity,
           stock_before,stock_after,movement_cost_cents,inventory_cost_before_cents,
           inventory_cost_after_cents,supplier,provider_search_key,document_reference,
           document_reference_search_key,note,note_search_key,operation_token,intent_fingerprint)
         VALUES (?1,?2,?3,?4,?5,2,?6,?7,?8,?9,?10,?11,?12,'manual',?13,?14,?15,?16,?15,
                  ?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29)",
        params![
            normalized.product_id, product.0, product.1, search_key(&product.0), search_key(&product.1),
            normalized.presentation_id, presentation_name, presentation_barcode, product.3,
            presentation_mode, factor, unit_price_cents, normalized.reason, normalized.movement_type,
            base_quantity, normalized.entered_quantity, stock_before, stock_after, movement_cost,
            cost_before, cost_after, normalized.supplier,
            normalized.supplier.as_deref().map(search_key), normalized.document_reference,
            normalized.document_reference.as_deref().map(search_key), normalized.note,
            normalized.note.as_deref().map(search_key), operation_token, normalized.fingerprint
        ],
    )?;
    let movement_id = transaction.last_insert_rowid();
    let movement = movement_by_id(&transaction, movement_id)?;
    transaction.commit()?;
    Ok(movement)
}

pub fn quote(
    connection: &Connection,
    input: QuoteInventoryMovementInput,
) -> ServiceResult<InventoryMovementQuote> {
    validate_id(input.product_id)?;
    if let Some(id) = input.presentation_id {
        validate_id(id)?;
    }
    let reason = search_key(&input.reason);
    let movement_type = search_key(&input.movement_type);
    validate_movement_kind(&reason, &movement_type)?;
    let entered_quantity = parse_entered_quantity(&input.quantity, reason == "valoracion")?;
    let unit_price = input
        .unit_price
        .as_deref()
        .map(|value| parse_money_text_allow_zero(value, reason == "valoracion"))
        .transpose()
        .map_err(|_| {
            ServiceError::validation_field(
                "INVALID_INVENTORY_COST",
                "El precio unitario no es valido",
                "unit_price",
            )
        })?;
    if movement_type == "salida" && unit_price.is_some() {
        return Err(ServiceError::validation_field(
            "UNEXPECTED_INVENTORY_COST",
            "Las salidas calculan su costo desde el inventario",
            "unit_price",
        ));
    }
    if (movement_type == "entrada" || reason == "valoracion") && unit_price.is_none() {
        return Err(ServiceError::validation_field(
            "INVENTORY_COST_REQUIRED",
            "La operacion requiere un costo",
            "unit_price",
        ));
    }

    let transaction = connection.unchecked_transaction()?;
    let quote = calculate_quote(
        &transaction,
        input.product_id,
        input.presentation_id,
        &reason,
        &movement_type,
        entered_quantity,
        unit_price,
    )?;
    transaction.commit()?;
    Ok(quote)
}

fn calculate_quote(
    connection: &Connection,
    product_id: i64,
    presentation_id: Option<i64>,
    reason: &str,
    movement_type: &str,
    entered_quantity: i64,
    unit_price: Option<i64>,
) -> ServiceResult<InventoryMovementQuote> {
    let (product_name, product_sku, stock, base_unit, inventory_cost): (
        String,
        String,
        i64,
        String,
        Option<i64>,
    ) = connection
        .query_row(
            "SELECT name,sku,stock,base_unit,inventory_cost_cents FROM products WHERE id=?1",
            [product_id],
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
        .ok_or_else(|| ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"))?;
    let presentation = presentation_id
        .map(|id| load_presentation(connection, product_id, id))
        .transpose()?;
    let base_quantity = match &presentation {
        Some((_, _, mode, factor)) => presentation_base_quantity(*mode, entered_quantity, *factor)?,
        None => entered_quantity,
    };
    let total_cost_cents = match movement_type {
        "entrada" => Some(match &presentation {
            Some((_, _, mode, factor)) => {
                line_total(*mode, entered_quantity, *factor, unit_price.unwrap())?
            }
            None => checked_multiply(base_quantity, unit_price.unwrap(), "COST_OVERFLOW")?,
        }),
        "salida" => {
            if base_quantity > stock {
                return Err(ServiceError::conflict(
                    "INSUFFICIENT_STOCK",
                    "Stock insuficiente para completar el movimiento",
                ));
            }
            inventory_cost
                .map(|cost| proportional_cost_removal(cost, base_quantity, stock))
                .transpose()?
        }
        "valoracion" => {
            if stock == 0 && unit_price.is_some_and(|cost| cost > 0) {
                return Err(ServiceError::validation_field(
                    "INVALID_INVENTORY_COST",
                    "Un producto sin stock solo puede tener valor cero",
                    "unit_price",
                ));
            }
            unit_price
        }
        _ => unreachable!(),
    };
    if movement_type == "entrada" && total_cost_cents == Some(0) {
        return Err(ServiceError::validation_field(
            "INVALID_INVENTORY_COST",
            "El costo total debe ser positivo",
            "unit_price",
        ));
    }
    let base_unit =
        BaseUnit::try_from(base_unit.as_str()).expect("database base unit is constrained");
    let (presentation_name, presentation_mode, presentation_base_quantity) = presentation
        .as_ref()
        .map_or((None, None, 1), |(name, _, mode, factor)| {
            (Some(name.clone()), Some(*mode), *factor)
        });
    let capture_name = presentation_name
        .clone()
        .unwrap_or_else(|| direct_capture_name(base_unit).to_owned());
    let entered_quantity_description = entered_quantity_description(
        entered_quantity,
        presentation_name.as_deref(),
        presentation_mode,
        base_unit,
    );
    let base_quantity_description = quantity_description(base_quantity, base_unit);
    let resolved_unit_price = match movement_type {
        "salida" => inventory_cost
            .map(|cost| {
                divide_half_up_positive(i128::from(cost), i128::from(stock), "COST_OVERFLOW")
            })
            .transpose()?,
        _ => unit_price,
    };
    let unit_price_description = resolved_unit_price.map(|_| {
        price_basis_description(
            presentation_name.as_deref(),
            presentation_mode,
            presentation_base_quantity,
            base_unit,
            reason,
        )
    });
    let presentation_state = presentation.as_ref().map_or_else(
        || "-|-|-".to_owned(),
        |(_, _, mode, factor)| format!("{}|{}|{}", presentation_id.unwrap(), mode.as_str(), factor),
    );
    let canonical = format!(
        "{product_id}|{}|{}|{presentation_state}|{}|{}|{}:{reason}|{}:{movement_type}|{entered_quantity}|{}|{base_quantity}|{}|{}|{}|{}",
        length_value(Some(&product_name)),
        length_value(Some(&product_sku)),
        length_value(presentation_name.as_deref()),
        presentation_base_quantity,
        reason.len(),
        movement_type.len(),
        optional_number(unit_price),
        optional_number(total_cost_cents),
        base_unit.as_str(),
        stock,
        optional_number(inventory_cost),
    );
    Ok(InventoryMovementQuote {
        product_id,
        product_name,
        product_sku,
        reason: reason.to_owned(),
        movement_type: movement_type.to_owned(),
        presentation_id,
        presentation_name,
        capture_name,
        presentation_mode,
        presentation_base_quantity,
        entered_quantity,
        entered_quantity_description,
        base_quantity,
        base_quantity_description,
        unit_price_cents: resolved_unit_price,
        unit_price_description,
        total_cost_cents,
        base_unit,
        quote_fingerprint: opaque_quote_fingerprint(&canonical),
    })
}

fn direct_capture_name(base_unit: BaseUnit) -> &'static str {
    match base_unit {
        BaseUnit::Piece => "Piezas individuales",
        BaseUnit::Gram => "Peso exacto en gramos",
        BaseUnit::Milliliter => "Volumen exacto en mililitros",
    }
}

fn quantity_description(quantity: i64, base_unit: BaseUnit) -> String {
    let unit = match (base_unit, quantity) {
        (BaseUnit::Piece, 1) => "pieza",
        (BaseUnit::Piece, _) => "piezas",
        (BaseUnit::Gram, 1) => "gramo",
        (BaseUnit::Gram, _) => "gramos",
        (BaseUnit::Milliliter, 1) => "mililitro",
        (BaseUnit::Milliliter, _) => "mililitros",
    };
    format!("{quantity} {unit}")
}

fn entered_quantity_description(
    quantity: i64,
    presentation_name: Option<&str>,
    mode: Option<PresentationMode>,
    base_unit: BaseUnit,
) -> String {
    match (presentation_name, mode) {
        (Some(name), Some(PresentationMode::Fixed)) => {
            format!("{quantity} x {name}")
        }
        _ => quantity_description(quantity, base_unit),
    }
}

fn price_basis_description(
    presentation_name: Option<&str>,
    mode: Option<PresentationMode>,
    factor: i64,
    base_unit: BaseUnit,
    reason: &str,
) -> String {
    if reason == "valoracion" {
        return "valor total del inventario".to_owned();
    }
    match (presentation_name, mode) {
        (Some(name), Some(PresentationMode::Fixed)) => format!("por {name}"),
        (Some(_), Some(PresentationMode::Measured)) => {
            format!("por {}", quantity_description(factor, base_unit))
        }
        _ => format!("por {}", quantity_description(1, base_unit)),
    }
}

struct NormalizedInput {
    product_id: i64,
    presentation_id: Option<i64>,
    reason: String,
    movement_type: String,
    entered_quantity: i64,
    unit_price: Option<i64>,
    supplier: Option<String>,
    document_reference: Option<String>,
    note: Option<String>,
    quote_fingerprint: Option<String>,
    fingerprint: String,
}

fn normalize_new_input(
    input: CreateInventoryMovementInput,
    reason: String,
    movement_type: String,
    entered_quantity: i64,
) -> ServiceResult<NormalizedInput> {
    let unit_price = input
        .unit_price
        .as_deref()
        .map(|value| parse_money_text_allow_zero(value, reason == "valoracion"))
        .transpose()
        .map_err(|_| {
            ServiceError::validation_field(
                "INVALID_INVENTORY_COST",
                "El precio unitario no es valido",
                "unit_price",
            )
        })?;
    if movement_type == "salida" && unit_price.is_some() {
        return Err(ServiceError::validation_field(
            "UNEXPECTED_INVENTORY_COST",
            "Las salidas calculan su costo desde el inventario",
            "unit_price",
        ));
    }
    if reason == "valoracion" && unit_price.is_none() {
        return Err(ServiceError::validation_field(
            "INVENTORY_COST_REQUIRED",
            "La valoracion requiere el nuevo costo acumulado",
            "unit_price",
        ));
    }
    let supplier = normalize_optional(input.supplier, 200, "supplier")?;
    let document_reference =
        normalize_optional(input.document_reference, 200, "document_reference")?;
    let note = normalize_optional(input.note, 1000, "note")?;
    let quote_fingerprint = normalize_optional(input.quote_fingerprint, 128, "quote_fingerprint")?;
    let canonical = format!(
        "{}|{}|{}:{}|{}:{}|{}|{}|{}|{}|{}|{}",
        input.product_id,
        input
            .presentation_id
            .map_or_else(|| "-".into(), |v| v.to_string()),
        reason.len(),
        reason,
        movement_type.len(),
        movement_type,
        entered_quantity,
        optional_number(unit_price),
        length_value(supplier.as_deref()),
        length_value(document_reference.as_deref()),
        length_value(note.as_deref()),
        length_value(quote_fingerprint.as_deref()),
    );
    let fingerprint = format!(
        "inventory-manual-v2:{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    Ok(NormalizedInput {
        product_id: input.product_id,
        presentation_id: input.presentation_id,
        reason,
        movement_type,
        entered_quantity,
        unit_price,
        supplier,
        document_reference,
        note,
        quote_fingerprint,
        fingerprint,
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
    let sql = format!(
        "SELECT {MOVEMENT_COLUMNS},m.intent_fingerprint FROM inventory_movements m
         JOIN products p ON p.id=m.product_id WHERE m.operation_token=?1"
    );
    connection
        .query_row(&sql, [token], |row| Ok((map_movement(row)?, row.get(32)?)))
        .optional()
        .map_err(Into::into)
}

fn map_inventory_product(row: &Row<'_>) -> rusqlite::Result<InventoryProductRow> {
    let stock: i64 = row.get(3)?;
    let cost: Option<i64> = row.get(5)?;
    Ok(InventoryProductRow {
        id: row.get(0)?,
        name: row.get(1)?,
        sku: row.get(2)?,
        stock,
        base_unit: BaseUnit::try_from(row.get::<_, String>(4)?.as_str())
            .expect("database base unit is constrained"),
        inventory_cost_cents: cost,
        inventory_cost_status: if cost.is_some() { "known" } else { "unknown" },
        average_cost_cents: cost.and_then(|value| {
            (stock > 0)
                .then(|| {
                    divide_half_up_positive(i128::from(value), i128::from(stock), "COST_OVERFLOW")
                        .ok()
                })
                .flatten()
        }),
    })
}

fn inventory_movement_product(
    connection: &Connection,
    product: InventoryProductRow,
) -> ServiceResult<InventoryMovementProduct> {
    let mut statement = connection.prepare(
        "SELECT id,product_id,name,mode,base_quantity,is_active
         FROM product_presentations WHERE product_id=?1 AND is_active=1 ORDER BY is_default DESC,id",
    )?;
    let presentations = statement
        .query_map([product.id], |row| {
            Ok(InventoryMovementPresentation {
                id: row.get(0)?,
                product_id: row.get(1)?,
                name: row.get(2)?,
                mode: PresentationMode::try_from(row.get::<_, String>(3)?.as_str())
                    .expect("database presentation mode is constrained"),
                base_quantity: row.get(4)?,
                is_active: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
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
        inventory_value_cents: 0,
        products_without_cost: 0,
    };
    let mut statement = connection
        .prepare("SELECT stock,base_unit,inventory_cost_cents FROM products ORDER BY id")?;
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })? {
        let (stock, unit, cost) = row?;
        stats.product_count = checked_total(stats.product_count, 1, "INVENTORY_STATS_OVERFLOW")?;
        let target = match unit.as_str() {
            "piece" => &mut stats.piece_stock,
            "gram" => &mut stats.gram_stock,
            "milliliter" => &mut stats.milliliter_stock,
            _ => unreachable!("database base unit is constrained"),
        };
        *target = checked_total(*target, stock, "INVENTORY_STATS_OVERFLOW")?;
        match cost {
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

fn load_presentation(
    connection: &Connection,
    product_id: i64,
    presentation_id: i64,
) -> ServiceResult<(String, Option<String>, PresentationMode, i64)> {
    connection
        .query_row(
            "SELECT name,barcode,mode,base_quantity FROM product_presentations WHERE id=?1 AND product_id=?2 AND is_active=1",
            params![presentation_id, product_id],
            |row| {
                let mode = row.get::<_, String>(2)?;
                Ok((row.get(0)?, row.get(1)?, PresentationMode::try_from(mode.as_str()).unwrap(), row.get(3)?))
            },
        )
        .optional()?
        .ok_or_else(|| ServiceError::conflict("PRESENTATION_UNAVAILABLE", "La presentacion no existe o no esta activa"))
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

fn normalize_optional(
    value: Option<String>,
    max: usize,
    field: &'static str,
) -> ServiceResult<Option<String>> {
    let value = value
        .map(|value| normalize_persisted(&value))
        .filter(|value| !value.is_empty());
    if value
        .as_ref()
        .is_some_and(|value| value.chars().count() > max)
    {
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
    Ok(value.to_owned())
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
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_quantity());
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|quantity| {
            *quantity <= JS_MAX_SAFE_INTEGER && (*quantity > 0 || (allow_zero && *quantity == 0))
        })
        .ok_or_else(invalid_quantity)
}

fn invalid_quantity() -> ServiceError {
    ServiceError::validation_field(
        "INVALID_MOVEMENT_QUANTITY",
        "La cantidad no es valida",
        "quantity",
    )
}

fn page(value: Option<i64>, field: &'static str) -> ServiceResult<i64> {
    match value.unwrap_or(1) {
        value @ 1..=450_359_962_737_049 => Ok(value),
        _ => Err(ServiceError::validation_field(
            "INVALID_PAGE",
            "La pagina no es valida",
            field,
        )),
    }
}

fn normalize_date(value: Option<String>, field: &'static str) -> ServiceResult<Option<String>> {
    let Some(value) = value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let valid = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && rusqlite::Connection::open_in_memory()?
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

fn safe_add(left: i64, right: i64, code: &'static str) -> ServiceResult<i64> {
    left.checked_add(right)
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(|| ServiceError::conflict(code, "El valor excede el limite permitido"))
}

fn legacy_fingerprint(product_id: i64, reason: &str, movement_type: &str, quantity: i64) -> String {
    format!(
        "inventory-manual-v1|{product_id}|{}:{reason}|{}:{movement_type}|{quantity}",
        reason.len(),
        movement_type.len()
    )
}

fn length_value(value: Option<&str>) -> String {
    value.map_or_else(|| "-".into(), |value| format!("{}:{value}", value.len()))
}

fn optional_number(value: Option<i64>) -> String {
    value.map_or_else(|| "-".into(), |value| value.to_string())
}

fn opaque_quote_fingerprint(canonical: &str) -> String {
    format!(
        "inventory-quote-v1:{:x}",
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
    use std::{
        fs,
        sync::{Arc, Barrier},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use rusqlite::{params, Connection};

    use crate::{db::migrations, models::inventory_movement::CreateInventoryMovementInput};

    fn database(stock: i64, cost: Option<i64>) -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        seed_database(&mut connection, stock, cost);
        connection
    }

    fn seed_database(connection: &mut Connection, stock: i64, cost: Option<i64>) {
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        migrations::run(connection).unwrap();
        connection.execute_batch("INSERT INTO brands(name,name_search_key) VALUES('Marca','marca'); INSERT INTO categories(name,name_search_key) VALUES('Categoria','categoria');").unwrap();
        connection.execute("INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES('Producto','producto',100,?1,'SKU','sku',1,1,'piece',?2)", params![stock,cost]).unwrap();
        connection.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default,is_active) VALUES(1,'Caja','fixed',10,500,1,1)", []).unwrap();
    }

    fn input(
        reason: &str,
        movement_type: &str,
        quantity: &str,
        price: Option<&str>,
        token: &str,
    ) -> CreateInventoryMovementInput {
        CreateInventoryMovementInput {
            product_id: 1,
            presentation_id: None,
            reason: reason.into(),
            movement_type: movement_type.into(),
            quantity: quantity.into(),
            unit_price: price.map(Into::into),
            supplier: None,
            document_reference: None,
            note: None,
            operation_token: token.into(),
            quote_fingerprint: None,
        }
    }

    fn create_with_quote(
        connection: &mut Connection,
        mut input: CreateInventoryMovementInput,
    ) -> crate::services::ServiceResult<crate::models::inventory_movement::InventoryMovement> {
        let quote = super::quote(
            connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: input.product_id,
                presentation_id: input.presentation_id,
                reason: input.reason.clone(),
                movement_type: input.movement_type.clone(),
                quantity: input.quantity.clone(),
                unit_price: input.unit_price.clone(),
            },
        )?;
        input.quote_fingerprint = Some(quote.quote_fingerprint);
        super::create(connection, input)
    }

    fn create(
        connection: &mut Connection,
        input: CreateInventoryMovementInput,
    ) -> crate::services::ServiceResult<crate::models::inventory_movement::InventoryMovement> {
        create_with_quote(connection, input)
    }

    #[test]
    fn purchase_adds_stock_and_known_cost_but_does_not_invent_an_unknown_accumulated_cost() {
        let mut known = database(5, Some(500));
        let movement = create(
            &mut known,
            input("compra", "entrada", "2", Some("1.25"), "purchase"),
        )
        .unwrap();
        assert_eq!(
            (
                movement.stock_before,
                movement.stock_after,
                movement.unit_price_cents,
                movement.movement_cost_cents,
                movement.inventory_cost_after_cents
            ),
            (Some(5), Some(7), Some(125), Some(250), Some(750))
        );

        let mut unknown = database(5, None);
        let movement = create(
            &mut unknown,
            input("ajuste", "entrada", "2", Some("1.25"), "unknown"),
        )
        .unwrap();
        assert_eq!(movement.movement_cost_cents, Some(250));
        assert_eq!(movement.inventory_cost_after_cents, None);
        assert_eq!(
            unknown
                .query_row("SELECT inventory_cost_cents FROM products", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })
                .unwrap(),
            None
        );
    }

    #[test]
    fn entry_requires_cost_and_presentation_calculates_base_quantity_and_total() {
        let mut connection = database(5, Some(500));
        assert_eq!(
            create(
                &mut connection,
                input("ajuste", "entrada", "2", None, "missing")
            )
            .unwrap_err()
            .code,
            "INVENTORY_COST_REQUIRED"
        );
        let mut value = input("compra", "entrada", "2", Some("8.00"), "box");
        value.presentation_id = Some(1);
        let movement = create(&mut connection, value).unwrap();
        assert_eq!(
            (
                movement.entered_quantity,
                movement.base_quantity,
                movement.unit_price_cents,
                movement.movement_cost_cents
            ),
            (Some(2), Some(20), Some(800), Some(1600))
        );
    }

    #[test]
    fn quote_calculates_fixed_and_measured_entries_without_writes() {
        let connection = database(5, Some(500));
        let before = connection
            .query_row(
                "SELECT stock,inventory_cost_cents,(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, i64>(2)?)),
            )
            .unwrap();
        let fixed = super::quote(
            &connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: Some(1),
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "2".into(),
                unit_price: Some("8.00".into()),
            },
        )
        .unwrap();
        assert_eq!(
            (fixed.base_quantity, fixed.total_cost_cents),
            (20, Some(1600))
        );

        connection
            .execute("UPDATE products SET base_unit='gram' WHERE id=1", [])
            .unwrap();
        connection
            .execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_active) VALUES(1,'Kilogramo','measured',1000,12000,1)", [])
            .unwrap();
        let measured = super::quote(
            &connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: Some(2),
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "250".into(),
                unit_price: Some("120.00".into()),
            },
        )
        .unwrap();
        assert_eq!(
            (
                measured.base_quantity,
                measured.total_cost_cents,
                measured.base_unit
            ),
            (250, Some(3000), crate::models::product::BaseUnit::Gram)
        );
        connection
            .execute("UPDATE products SET base_unit='piece' WHERE id=1", [])
            .unwrap();
        let after = connection
            .query_row(
                "SELECT stock,inventory_cost_cents,(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, i64>(2)?)),
            )
            .unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn quote_fingerprint_is_opaque_covers_quote_state_and_blocks_stale_confirmation() {
        let mut connection = database(5, Some(500));
        let quote_input = crate::models::inventory_movement::QuoteInventoryMovementInput {
            product_id: 1,
            presentation_id: Some(1),
            reason: "compra".into(),
            movement_type: "entrada".into(),
            quantity: "2".into(),
            unit_price: Some("8.00".into()),
        };
        let original = super::quote(&connection, quote_input).unwrap();
        assert_eq!(
            original.quote_fingerprint,
            "inventory-quote-v1:0ed24e8145c99d050b9379546f0209a0e03071edf478d59dd800ad45dfcb2997"
        );
        assert!(original
            .quote_fingerprint
            .starts_with("inventory-quote-v1:"));
        assert_eq!(original.quote_fingerprint.len(), 83);
        assert!(!original.quote_fingerprint.contains("compra"));

        connection
            .execute(
                "UPDATE products SET stock=6,inventory_cost_cents=650 WHERE id=1",
                [],
            )
            .unwrap();
        let changed_state = super::quote(
            &connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: Some(1),
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "2".into(),
                unit_price: Some("8.00".into()),
            },
        )
        .unwrap();
        assert_ne!(original.quote_fingerprint, changed_state.quote_fingerprint);

        let mut stale = input("compra", "entrada", "2", Some("8.00"), "stale-quote");
        stale.presentation_id = Some(1);
        stale.quote_fingerprint = Some(original.quote_fingerprint);
        assert_eq!(
            super::create(&mut connection, stale).unwrap_err().code,
            "QUOTE_CHANGED"
        );
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products WHERE id=1", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            6
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );

        connection
            .execute(
                "UPDATE product_presentations SET base_quantity=12 WHERE id=1",
                [],
            )
            .unwrap();
        let changed_conversion = super::quote(
            &connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: Some(1),
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "2".into(),
                unit_price: Some("8.00".into()),
            },
        )
        .unwrap();
        assert_ne!(
            changed_state.quote_fingerprint,
            changed_conversion.quote_fingerprint
        );
        assert_ne!(
            changed_state.base_quantity,
            changed_conversion.base_quantity
        );

        let changed_price = super::quote(
            &connection,
            crate::models::inventory_movement::QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: Some(1),
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "2".into(),
                unit_price: Some("9.00".into()),
            },
        )
        .unwrap();
        assert_ne!(
            changed_conversion.quote_fingerprint,
            changed_price.quote_fingerprint
        );
        assert_ne!(
            changed_conversion.total_cost_cents,
            changed_price.total_cost_cents
        );
    }

    #[test]
    fn zero_stock_rejects_positive_valuation_but_accepts_zero() {
        let mut connection = database(0, Some(0));
        let positive = crate::models::inventory_movement::QuoteInventoryMovementInput {
            product_id: 1,
            presentation_id: None,
            reason: "valoracion".into(),
            movement_type: "valoracion".into(),
            quantity: "0".into(),
            unit_price: Some("0.01".into()),
        };
        assert_eq!(
            super::quote(&connection, positive).unwrap_err().code,
            "INVALID_INVENTORY_COST"
        );
        let movement = create(
            &mut connection,
            input("valoracion", "valoracion", "0", Some("0.00"), "zero-value"),
        )
        .unwrap();
        assert_eq!(movement.inventory_cost_after_cents, Some(0));
    }

    #[test]
    fn exits_remove_average_cost_and_exhaustion_normalizes_unknown_cost() {
        let mut known = database(3, Some(100));
        let movement = create(&mut known, input("ajuste", "salida", "1", None, "exit")).unwrap();
        assert_eq!(
            (
                movement.movement_cost_cents,
                movement.unit_price_cents,
                movement.inventory_cost_after_cents
            ),
            (Some(33), Some(33), Some(67))
        );
        let mut unknown = database(2, None);
        let movement = create(&mut unknown, input("ajuste", "salida", "2", None, "empty")).unwrap();
        assert_eq!(
            (
                movement.movement_cost_cents,
                movement.inventory_cost_after_cents
            ),
            (None, Some(0))
        );
    }

    #[test]
    fn valuation_changes_only_accumulated_cost() {
        let mut connection = database(7, None);
        let movement = create(
            &mut connection,
            input("valoracion", "valoracion", "0", Some("12.34"), "valuation"),
        )
        .unwrap();
        assert_eq!(
            (
                movement.stock_before,
                movement.stock_after,
                movement.unit_price_cents,
                movement.inventory_cost_before_cents,
                movement.inventory_cost_after_cents
            ),
            (Some(7), Some(7), None, None, Some(1234))
        );
    }

    #[test]
    fn retry_is_idempotent_and_v1_fingerprints_remain_compatible() {
        let mut connection = database(5, Some(500));
        let mut value = input("compra", "entrada", "2", Some("1.00"), "same");
        value.quote_fingerprint = Some(
            super::quote(
                &connection,
                crate::models::inventory_movement::QuoteInventoryMovementInput {
                    product_id: 1,
                    presentation_id: None,
                    reason: "compra".into(),
                    movement_type: "entrada".into(),
                    quantity: "2".into(),
                    unit_price: Some("1.00".into()),
                },
            )
            .unwrap()
            .quote_fingerprint,
        );
        let first = super::create(&mut connection, value.clone()).unwrap();
        assert_eq!(super::create(&mut connection, value).unwrap().id, first.id);
        connection.execute("INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,origin,reason,type,quantity,operation_token,intent_fingerprint) VALUES(1,'Old','OLD','old','old','manual','ajuste','salida',1,'legacy','inventory-manual-v1|1|6:ajuste|6:salida|1')", []).unwrap();
        assert!(super::create(
            &mut connection,
            input("ajuste", "salida", "01", None, "legacy")
        )
        .is_ok());
    }

    #[test]
    fn unapplied_v1_exit_requires_requote_without_writing() {
        let mut connection = database(5, Some(500));

        let error = super::create(
            &mut connection,
            input("ajuste", "salida", "2", None, "missing-legacy"),
        )
        .unwrap_err();

        assert_eq!(error.code, "INVENTORY_LEGACY_REQUOTE_REQUIRED");
        assert_eq!(
            connection
                .query_row(
                    "SELECT stock,inventory_cost_cents,(SELECT COUNT(*) FROM inventory_movements) FROM products WHERE id=1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, i64>(2)?)),
                )
                .unwrap(),
            (5, Some(500), 0)
        );
    }

    #[test]
    fn applied_v1_exit_is_idempotent_only_for_the_exact_fingerprint() {
        let mut connection = database(5, Some(500));
        connection.execute("INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,origin,reason,type,quantity,operation_token,intent_fingerprint) VALUES(1,'Old','OLD','old','old','manual','ajuste','salida',2,'applied-legacy','inventory-manual-v1|1|6:ajuste|6:salida|2')", []).unwrap();

        let movement = super::create(
            &mut connection,
            input("ajuste", "salida", "2", None, "applied-legacy"),
        )
        .unwrap();
        assert_eq!(movement.operation_token.as_deref(), Some("applied-legacy"));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            super::create(
                &mut connection,
                input("ajuste", "salida", "1", None, "applied-legacy"),
            )
            .unwrap_err()
            .code,
            "INVENTORY_OPERATION_TOKEN_REUSED"
        );
        assert_eq!(
            super::create(
                &mut connection,
                input("ajuste", "salida", "2", None, " applied-legacy"),
            )
            .unwrap_err()
            .code,
            "INVALID_INVENTORY_OPERATION_TOKEN"
        );
    }

    #[test]
    fn rollback_restores_stock_cost_and_token() {
        let mut connection = database(5, Some(500));
        connection.execute_batch("CREATE TRIGGER fail_movement BEFORE INSERT ON inventory_movements BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
        let value = input("compra", "entrada", "2", Some("1.00"), "rollback");
        assert!(create(&mut connection, value.clone()).is_err());
        assert_eq!(
            connection
                .query_row(
                    "SELECT stock,inventory_cost_cents FROM products",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
                )
                .unwrap(),
            (5, Some(500))
        );
        connection
            .execute_batch("DROP TRIGGER fail_movement")
            .unwrap();
        assert!(create(&mut connection, value).is_ok());
    }

    #[test]
    fn fingerprint_v2_covers_presentation_quantity_cost_and_metadata() {
        let base = input("compra", "entrada", "2", Some("1.00"), "fingerprint");
        let mut variants = Vec::new();
        let mut value = base.clone();
        value.presentation_id = Some(1);
        variants.push(value);
        let mut value = base.clone();
        value.quantity = "3".into();
        variants.push(value);
        let mut value = base.clone();
        value.unit_price = Some("2.00".into());
        variants.push(value);
        let mut value = base.clone();
        value.supplier = Some("Proveedor".into());
        variants.push(value);
        let mut value = base.clone();
        value.document_reference = Some("FAC-1".into());
        variants.push(value);
        let mut value = base.clone();
        value.note = Some("Nota".into());
        variants.push(value);

        for variant in variants {
            let mut connection = database(50, Some(5000));
            create(&mut connection, base.clone()).unwrap();
            assert_eq!(
                create(&mut connection, variant).unwrap_err().code,
                "INVENTORY_OPERATION_TOKEN_REUSED"
            );
            let fingerprint: String = connection
                .query_row(
                    "SELECT intent_fingerprint FROM inventory_movements",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(fingerprint.starts_with("inventory-manual-v2:"));
            assert_eq!(fingerprint.len(), 84);
            assert!(!fingerprint.contains("fnv"));
        }
    }

    #[test]
    fn fingerprint_v2_hashes_the_complete_unambiguous_canonical_intent() {
        let mut connection = database(5, Some(500));
        let mut value = input("compra", "entrada", "2", Some("1.25"), "canonical");
        value.supplier = Some("A|B".into());
        value.document_reference = Some("12:factura".into());
        value.note = Some("Cafe\u{301}".into());

        create(&mut connection, value).unwrap();
        let fingerprint: String = connection
            .query_row(
                "SELECT intent_fingerprint FROM inventory_movements",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(fingerprint.starts_with("inventory-manual-v2:"));
        assert_eq!(fingerprint.len(), 84);
        for private_value in ["compra", "entrada", "A|B", "factura", "Café"] {
            assert!(!fingerprint.contains(private_value));
        }
    }

    #[test]
    fn manual_fingerprint_has_an_exact_sha256_vector() {
        let mut value = input("compra", "entrada", "2", Some("1.25"), "digest-vector");
        value.supplier = Some("A|B".into());
        value.document_reference = Some("12:factura".into());
        value.note = Some("Cafe\u{301}".into());
        value.quote_fingerprint = Some("inventory-quote-v1:test".into());

        let normalized =
            super::normalize_new_input(value, "compra".into(), "entrada".into(), 2).unwrap();

        assert_eq!(
            normalized.fingerprint,
            "inventory-manual-v2:b5f31b7f407851d3840a89c329e500b3fcc6646a215ffe25bd6ae60de3edcbaf"
        );
    }

    #[test]
    fn product_and_movement_filters_are_independent_and_metadata_search_is_unicode_safe() {
        let mut connection = database(10, Some(1000));
        let mut value = input("compra", "entrada", "1", Some("1.00"), "unicode-search");
        value.supplier = Some("Straße Café".into());
        value.document_reference = Some("Αρχείο".into());
        value.note = Some("Cafe\u{301}".into());
        create(&mut connection, value).unwrap();

        for movement_search in ["STRASSE", "Cafe\u{301}", "ΑΡΧΕΊΟ"] {
            let view = super::view(
                &connection,
                crate::models::inventory_movement::InventoryViewFilters {
                    product_search: Some("no existe".into()),
                    movement_search: Some(movement_search.into()),
                    reason: Some("COMPRA".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(view.product_total, 0);
            assert_eq!(view.movement_total, 1, "did not match {movement_search}");
        }

        let keys: (String, String, String) = connection
            .query_row(
                "SELECT provider_search_key,document_reference_search_key,note_search_key FROM inventory_movements",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(keys.0, crate::services::search_key("Straße Café"));
        assert_eq!(keys.1, crate::services::search_key("Αρχείο"));
        assert_eq!(keys.2, crate::services::search_key("Café"));

        let product_only = super::view(
            &connection,
            crate::models::inventory_movement::InventoryViewFilters {
                product_search: Some("producto".into()),
                movement_search: Some("no existe".into()),
                movement_page: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(product_only.product_total, 1);
        assert_eq!(product_only.movement_total, 0);
        assert_eq!(product_only.movement_page, 2);
    }

    #[test]
    fn inactive_presentations_and_input_limits_are_rejected_without_writes() {
        let mut connection = database(5, Some(500));
        connection
            .execute(
                "INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_active) VALUES(1,'Inactiva','fixed',1,100,0)",
                [],
            )
            .unwrap();
        let mut inactive = input("compra", "entrada", "1", Some("1.00"), "inactive");
        inactive.presentation_id = Some(2);
        assert_eq!(
            create(&mut connection, inactive).unwrap_err().code,
            "PRESENTATION_UNAVAILABLE"
        );

        for quantity in ["", "0", "-1", "1.0", "+1", " 1", "9007199254740992"] {
            let value = input(
                "compra",
                "entrada",
                quantity,
                Some("1.00"),
                "invalid-quantity",
            );
            assert_eq!(
                create(&mut connection, value).unwrap_err().code,
                "INVALID_MOVEMENT_QUANTITY",
                "accepted {quantity}"
            );
        }
        let mut oversized = input("compra", "entrada", "1", Some("1.00"), "oversized");
        oversized.note = Some("x".repeat(1001));
        assert_eq!(
            create(&mut connection, oversized).unwrap_err().code,
            "INVALID_INVENTORY_METADATA"
        );
        assert_eq!(
            super::view(
                &connection,
                crate::models::inventory_movement::InventoryViewFilters {
                    product_page: Some(0),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .field,
            Some("product_page")
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
    fn two_connections_with_the_same_token_apply_once() {
        let path = temporary_database_path("concurrent");
        let concurrent_input = {
            let mut connection = Connection::open(&path).unwrap();
            seed_database(&mut connection, 5, Some(500));
            let mut input = input("compra", "entrada", "2", Some("1.00"), "concurrent-token");
            let quote = super::quote(
                &connection,
                crate::models::inventory_movement::QuoteInventoryMovementInput {
                    product_id: input.product_id,
                    presentation_id: input.presentation_id,
                    reason: input.reason.clone(),
                    movement_type: input.movement_type.clone(),
                    quantity: input.quantity.clone(),
                    unit_price: input.unit_price.clone(),
                },
            )
            .unwrap();
            input.quote_fingerprint = Some(quote.quote_fingerprint);
            input
        };
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [(), ()]
            .into_iter()
            .map(|()| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                let input = concurrent_input.clone();
                std::thread::spawn(move || {
                    let mut connection = Connection::open(path).unwrap();
                    connection.busy_timeout(Duration::from_secs(5)).unwrap();
                    barrier.wait();
                    super::create(&mut connection, input).map(|movement| movement.id)
                })
            })
            .collect();
        let ids: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect();
        let connection = Connection::open(&path).unwrap();
        assert_eq!(ids[0], ids[1]);
        assert_eq!(
            connection
                .query_row("SELECT stock FROM products", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM inventory_movements", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn detail_is_paginated_filterable_and_uses_current_identity_with_snapshots() {
        let path = temporary_database_path("detail");
        {
            let mut connection = Connection::open(&path).unwrap();
            seed_database(&mut connection, 100, Some(10_000));
            for index in 1..=25 {
                let mut value = input("ajuste", "salida", "1", None, &format!("detail-{index}"));
                if index == 7 {
                    value.supplier = Some("Proveedor Norte".into());
                    value.document_reference = Some("FAC-007".into());
                    value.note = Some("Conteo especial".into());
                }
                create(&mut connection, value).unwrap();
            }
            connection
                .execute(
                    "UPDATE products SET name='Renombrado',name_search_key='renombrado',sku='NEW',sku_search_key='new' WHERE id=1",
                    [],
                )
                .unwrap();
            connection
                .execute_batch(
                    "UPDATE inventory_movements SET created_at='2026-01-01 10:00:00' WHERE id=1;
                     UPDATE inventory_movements SET created_at='2026-02-01 10:00:00' WHERE id=2;",
                )
                .unwrap();
        }
        let connection = Connection::open(&path).unwrap();
        let first = super::product_detail(&connection, 1, Default::default()).unwrap();
        assert_eq!(
            (first.page, first.total, first.movements.len()),
            (1, 25, 20)
        );
        assert_eq!(
            (first.product.name.as_str(), first.product.sku.as_str()),
            ("Renombrado", "NEW")
        );
        assert!(first.movements.iter().all(|movement| {
            movement.product_name == "Renombrado"
                && movement.sku == "NEW"
                && movement.product_name_snapshot == "Producto"
                && movement.sku_snapshot == "SKU"
        }));
        assert!(first
            .movements
            .windows(2)
            .all(|pair| pair[0].id > pair[1].id));
        let second = super::product_detail(
            &connection,
            1,
            crate::models::inventory_movement::ProductInventoryDetailFilters {
                page: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second.movements.len(), 5);
        let filtered = super::product_detail(
            &connection,
            1,
            crate::models::inventory_movement::ProductInventoryDetailFilters {
                movement_search: Some("fac-007".into()),
                reason: Some("AJUSTE".into()),
                movement_type: Some("SALIDA".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.total, 1);
        assert_eq!(
            filtered.movements[0].supplier.as_deref(),
            Some("Proveedor Norte")
        );
        let dated = super::product_detail(
            &connection,
            1,
            crate::models::inventory_movement::ProductInventoryDetailFilters {
                date_from: Some("2026-01-01".into()),
                date_to: Some("2026-01-31".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(dated.total, 1);
        assert_eq!(dated.movements[0].id, 1);
        drop(connection);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn inventory_view_pages_twenty_and_groups_units_and_known_value() {
        let connection = database(3, Some(300));
        for index in 2..=23 {
            let unit = if index == 2 { "gram" } else { "piece" };
            connection.execute(
                "INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES(?1,?2,100,2,?3,?4,1,1,?5,NULL)",
                params![format!("Product {index:02}"), format!("product {index:02}"), format!("SKU-{index:02}"), format!("sku-{index:02}"), unit],
            ).unwrap();
        }
        let view = super::view(&connection, Default::default()).unwrap();
        assert_eq!(
            (view.products.len(), view.product_total, view.page_size),
            (20, 23, 20)
        );
        assert_eq!((view.stats.piece_stock, view.stats.gram_stock), (45, 2));
        assert_eq!(view.stats.inventory_value_cents, 300);
        assert_eq!(view.stats.products_without_cost, 22);
        let searched = super::view(
            &connection,
            crate::models::inventory_movement::InventoryViewFilters {
                product_search: Some("Producto".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(searched.product_total, 1);
        assert_eq!(searched.products[0].presentations.len(), 1);
    }

    #[test]
    fn dedicated_product_search_is_normalized_stable_paginated_and_has_active_presentations() {
        let connection = database(3, Some(300));
        connection
            .execute(
                "UPDATE products SET name='Café',name_search_key=?1,sku='ÁBC',sku_search_key=?2 WHERE id=1",
                params![crate::services::search_key("Café"), crate::services::search_key("ÁBC")],
            )
            .unwrap();
        connection.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_active) VALUES(1,'Inactiva','fixed',1,100,0)", []).unwrap();
        for index in 2..=25 {
            connection.execute(
                "INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES(?1,?2,100,0,?3,?4,1,1,'piece',NULL)",
                params![format!("Producto {index:02}"), format!("producto {index:02}"), format!("SKU-{index:02}"), format!("sku-{index:02}")],
            ).unwrap();
        }

        let by_name = super::search_products(&connection, Some("Cafe\u{301}".into()), 1).unwrap();
        assert_eq!((by_name.total, by_name.items[0].id), (1, 1));
        assert_eq!(by_name.items[0].presentations.len(), 1);
        let by_sku = super::search_products(&connection, Some("ábc".into()), 1).unwrap();
        assert_eq!(by_sku.items[0].id, 1);

        let first = super::search_products(&connection, None, 1).unwrap();
        let second = super::search_products(&connection, None, 2).unwrap();
        assert_eq!(
            (first.items.len(), second.items.len(), first.total),
            (20, 5, 25)
        );
        assert!(first.items.windows(2).all(|pair| {
            crate::services::search_key(&pair[0].name) <= crate::services::search_key(&pair[1].name)
        }));
        let outside = super::search_products(&connection, None, 99).unwrap();
        assert!(outside.items.is_empty());
        assert_eq!(
            (outside.page, outside.total, outside.page_size),
            (99, 25, 20)
        );
        assert_eq!(
            super::search_products(&connection, None, 0)
                .unwrap_err()
                .field,
            Some("page")
        );
    }

    #[test]
    fn dedicated_product_search_does_not_depend_on_movements_or_inventory_stats() {
        let connection = database(3, Some(300));
        connection
            .execute_batch(
                "DROP TABLE inventory_movements;
                 INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
                 VALUES('Costo grande','costo grande',100,1,'BIG','big',1,1,'piece',9007199254740991);
                 WITH RECURSIVE sequence(value) AS (
                   VALUES(1) UNION ALL SELECT value+1 FROM sequence WHERE value<1024
                 )
                 INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents)
                 SELECT printf('Carga %04d',value),printf('carga %04d',value),100,1,
                        printf('BULK-%04d',value),printf('bulk-%04d',value),1,1,'piece',9007199254740991
                 FROM sequence;",
            )
            .unwrap();

        let result = super::search_products(&connection, Some("big".into()), 1).unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].sku, "BIG");
    }

    fn temporary_database_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pos-tauri-inventory-{label}-{}-{unique}.db",
            std::process::id()
        ))
    }
}
