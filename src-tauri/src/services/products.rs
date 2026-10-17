use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use crate::models::{
    product::{
        BaseUnit, CreateProductWithPresentationsInput, Product, ProductCodeMatch,
        ProductCodeMatchKind, ProductSelectorFilters, ProductSelectorRow, ProductView,
        ProductViewFilters, ProductViewRow, ProductViewStats, UpdateProductWithPresentationsInput,
    },
    product_presentation::{
        CreateProductPresentationInput, PresentationMode, ProductPresentation,
        UpdateProductPresentationInput,
    },
};

use super::{
    checked_total, normalize_persisted, normalize_search, normalize_uppercase, parse_money_text,
    presentation_base_quantity, search_key, validate_id, validate_presentation_mode, ServiceError,
    ServiceResult, JS_MAX_SAFE_INTEGER,
};

const NAME_MAX_LENGTH: usize = 200;
const SKU_MAX_LENGTH: usize = 64;
const PRESENTATION_NAME_MAX_LENGTH: usize = 100;
const BARCODE_MAX_LENGTH: usize = 128;

#[derive(Clone, Debug)]
struct ValidPresentation {
    id: Option<i64>,
    name: String,
    mode: PresentationMode,
    base_quantity: i64,
    price: i64,
    barcode: Option<String>,
    barcode_key: Option<String>,
    is_default: bool,
    is_active: bool,
}

fn validate_product(name: &str, sku: &str) -> ServiceResult<(String, String)> {
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
    if sku.chars().count() > SKU_MAX_LENGTH || search_key(&sku).chars().count() > SKU_MAX_LENGTH {
        return Err(ServiceError::validation(
            "SKU_TOO_LONG",
            "El SKU debe tener como maximo 64 caracteres",
        ));
    }
    Ok((name, sku))
}

fn validate_presentations(
    base_unit: BaseUnit,
    inputs: impl IntoIterator<
        Item = (
            Option<i64>,
            String,
            PresentationMode,
            i64,
            String,
            Option<String>,
            bool,
            bool,
        ),
    >,
) -> ServiceResult<Vec<ValidPresentation>> {
    validate_presentations_for_update(base_unit, inputs, &BTreeMap::new())
}

fn validate_presentations_for_update(
    base_unit: BaseUnit,
    inputs: impl IntoIterator<
        Item = (
            Option<i64>,
            String,
            PresentationMode,
            i64,
            String,
            Option<String>,
            bool,
            bool,
        ),
    >,
    persisted_conversions: &BTreeMap<i64, (PresentationMode, i64)>,
) -> ServiceResult<Vec<ValidPresentation>> {
    let mut values = Vec::new();
    let mut default_count = 0;
    let mut active_count = 0;
    let mut barcode_keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for (id, name, mode, base_quantity, price, barcode, is_default, is_active) in inputs {
        if let Some(id) = id {
            validate_id(id)?;
            if !ids.insert(id) {
                return Err(ServiceError::validation(
                    "DUPLICATE_PRESENTATION",
                    "La presentacion esta repetida",
                ));
            }
        }
        let name = normalize_persisted(&name);
        if name.is_empty() || name.chars().count() > PRESENTATION_NAME_MAX_LENGTH {
            return Err(ServiceError::validation("INVALID_PRESENTATION_NAME", "El nombre de la presentacion es obligatorio y debe tener como maximo 100 caracteres"));
        }
        if !(1..=JS_MAX_SAFE_INTEGER).contains(&base_quantity) {
            return Err(ServiceError::validation(
                "INVALID_BASE_QUANTITY",
                "La cantidad base debe ser positiva",
            ));
        }
        let preserves_persisted_conversion =
            id.is_some_and(|id| persisted_conversions.get(&id) == Some(&(mode, base_quantity)));
        if !preserves_persisted_conversion {
            validate_presentation_mode(base_unit, mode)?;
        }
        let price = parse_money_text(&price)?;
        let barcode = barcode
            .map(|value| normalize_persisted(&value))
            .filter(|value| !value.is_empty());
        if barcode
            .as_ref()
            .is_some_and(|value| value.chars().count() > BARCODE_MAX_LENGTH)
        {
            return Err(ServiceError::validation(
                "BARCODE_TOO_LONG",
                "El codigo de barras debe tener como maximo 128 caracteres",
            ));
        }
        let barcode_key = barcode.as_deref().map(search_key);
        if barcode_key
            .as_ref()
            .is_some_and(|value| value.chars().count() > BARCODE_MAX_LENGTH)
        {
            return Err(ServiceError::validation(
                "BARCODE_SEARCH_KEY_TOO_LONG",
                "La clave normalizada del codigo de barras debe tener como maximo 128 caracteres",
            ));
        }
        if barcode_key
            .as_ref()
            .is_some_and(|key| !barcode_keys.insert(key.clone()))
        {
            return Err(ServiceError::conflict(
                "BARCODE_ALREADY_EXISTS",
                "El codigo de barras esta repetido",
            ));
        }
        if is_default {
            default_count += 1;
        }
        if is_active {
            active_count += 1;
        }
        if is_default && !is_active {
            return Err(ServiceError::validation(
                "DEFAULT_PRESENTATION_INACTIVE",
                "La presentacion predeterminada debe estar activa",
            ));
        }
        values.push(ValidPresentation {
            id,
            name,
            mode,
            base_quantity,
            price,
            barcode,
            barcode_key,
            is_default,
            is_active,
        });
    }
    if values.is_empty() || active_count == 0 {
        return Err(ServiceError::validation(
            "ACTIVE_PRESENTATION_REQUIRED",
            "Debe existir al menos una presentacion activa",
        ));
    }
    if default_count != 1 {
        return Err(ServiceError::validation(
            "DEFAULT_PRESENTATION_REQUIRED",
            "Debe existir exactamente una presentacion predeterminada",
        ));
    }
    Ok(values)
}

fn create_inputs(
    base_unit: BaseUnit,
    inputs: Vec<CreateProductPresentationInput>,
) -> ServiceResult<Vec<ValidPresentation>> {
    validate_presentations(
        base_unit,
        inputs.into_iter().map(|input| {
            (
                None,
                input.name,
                input.mode,
                input.base_quantity,
                input.price,
                input.barcode,
                input.is_default,
                input.is_active,
            )
        }),
    )
}

fn update_inputs(
    base_unit: BaseUnit,
    inputs: Vec<UpdateProductPresentationInput>,
    existing: &[ProductPresentation],
) -> ServiceResult<Vec<ValidPresentation>> {
    let persisted_conversions = existing
        .iter()
        .map(|presentation| {
            (
                presentation.id,
                (presentation.mode, presentation.base_quantity),
            )
        })
        .collect();
    validate_presentations_for_update(
        base_unit,
        inputs.into_iter().map(|input| {
            (
                input.id,
                input.name,
                input.mode,
                input.base_quantity,
                input.price,
                input.barcode,
                input.is_default,
                input.is_active,
            )
        }),
        &persisted_conversions,
    )
}

fn parse_base_unit(value: String) -> rusqlite::Result<BaseUnit> {
    BaseUnit::try_from(value.as_str()).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn parse_mode(value: String) -> rusqlite::Result<PresentationMode> {
    PresentationMode::try_from(value.as_str()).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn presentations(
    connection: &Connection,
    product_id: i64,
    active_only: bool,
) -> ServiceResult<Vec<ProductPresentation>> {
    let mut statement = connection.prepare(
        "SELECT id, product_id, name, mode, base_quantity, price, barcode, is_default, is_active,
                EXISTS(SELECT 1 FROM sale_details WHERE presentation_id=product_presentations.id
                       UNION ALL
                       SELECT 1 FROM inventory_movements WHERE presentation_id=product_presentations.id),
                created_at, updated_at
         FROM product_presentations WHERE product_id = ?1 AND (?2 = 0 OR is_active = 1)
         ORDER BY is_default DESC, name COLLATE NOCASE, id",
    )?;
    let rows = statement
        .query_map(params![product_id, active_only], |row| {
            Ok(ProductPresentation {
                id: row.get(0)?,
                product_id: row.get(1)?,
                name: row.get(2)?,
                mode: parse_mode(row.get(3)?)?,
                base_quantity: row.get(4)?,
                price: row.get(5)?,
                barcode: row.get(6)?,
                is_default: row.get(7)?,
                is_active: row.get(8)?,
                is_conversion_locked: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn load_product(
    connection: &Connection,
    id: i64,
    active_presentations_only: bool,
) -> ServiceResult<Product> {
    let data = connection.query_row(
        "SELECT id, name, price, stock, sku, brand_id, category_id, created_at, updated_at, base_unit, inventory_cost_cents
         FROM products WHERE id = ?1", [id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?, row.get::<_, i64>(5)?, row.get::<_, i64>(6)?, row.get::<_, String>(7)?,
                row.get::<_, String>(8)?, parse_base_unit(row.get(9)?)?, row.get::<_, Option<i64>>(10)?))
        },
    ).map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado"),
        other => other.into(),
    })?;
    let presentations = presentations(connection, id, active_presentations_only)?;
    let default_presentation = presentations
        .iter()
        .find(|item| item.is_default)
        .cloned()
        .ok_or_else(|| {
            ServiceError::internal(
                "INVALID_PRODUCT_PRESENTATIONS",
                "El producto no tiene presentacion predeterminada",
            )
        })?;
    let average_cost_cents = match (data.10, data.3) {
        (Some(cost), stock) if stock > 0 => Some(super::divide_half_up_positive(
            i128::from(cost),
            i128::from(stock),
            "AVERAGE_COST_OVERFLOW",
        )?),
        (Some(_), 0) => Some(0),
        (None, _) => None,
        _ => {
            return Err(ServiceError::internal(
                "INVALID_STOCK",
                "El stock guardado no es valido",
            ))
        }
    };
    Ok(Product {
        id: data.0,
        name: data.1,
        price: data.2,
        stock: data.3,
        stock_base: data.3,
        sku: data.4,
        brand_id: data.5,
        category_id: data.6,
        created_at: data.7,
        updated_at: data.8,
        base_unit: data.9,
        inventory_cost_cents: data.10,
        inventory_cost_status: if data.10.is_some() {
            "known"
        } else {
            "unknown"
        },
        average_cost_cents,
        presentations,
        default_presentation,
    })
}

pub fn list(connection: &Connection) -> ServiceResult<Vec<Product>> {
    let ids = connection
        .prepare("SELECT id FROM products ORDER BY name COLLATE NOCASE, id")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    ids.into_iter()
        .map(|id| load_product(connection, id, false))
        .collect()
}

fn list_operational(connection: &Connection) -> ServiceResult<Vec<Product>> {
    let ids = connection
        .prepare("SELECT id FROM products ORDER BY name COLLATE NOCASE, id")?
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    ids.into_iter()
        .map(|id| load_product(connection, id, true))
        .collect()
}

pub fn find_by_sku(connection: &Connection, sku: &str) -> ServiceResult<Option<Product>> {
    let key = search_key(sku);
    let id = connection
        .query_row(
            "SELECT id FROM products WHERE sku_search_key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    id.map(|id| load_product(connection, id, true)).transpose()
}

pub fn find_by_code(
    connection: &Connection,
    code: &str,
) -> ServiceResult<Option<ProductCodeMatch>> {
    let key = search_key(code);
    if key.chars().count() > BARCODE_MAX_LENGTH {
        return Err(ServiceError::validation(
            "CODE_SEARCH_KEY_TOO_LONG",
            "La clave normalizada del codigo debe tener como maximo 128 caracteres",
        ));
    }
    let matched = connection
        .query_row(
            "SELECT product_id, match_kind, presentation_id FROM (
                 SELECT id AS product_id, 'sku' AS match_kind, NULL AS presentation_id, 0 AS priority
                 FROM products WHERE sku_search_key = ?1
                 UNION ALL
                 SELECT product_id, 'barcode', id, 1
                 FROM product_presentations WHERE barcode_search_key = ?1 AND is_active = 1
             ) ORDER BY priority LIMIT 1",
            [key],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()?;
    matched
        .map(|(product_id, match_kind, presentation_id)| {
            Ok(ProductCodeMatch {
                product: load_product(connection, product_id, true)?,
                match_kind: if match_kind == "sku" {
                    ProductCodeMatchKind::Sku
                } else {
                    ProductCodeMatchKind::Barcode
                },
                presentation_id,
            })
        })
        .transpose()
}

fn ensure_catalog_references(
    connection: &Connection,
    brand_id: i64,
    category_id: i64,
) -> ServiceResult<()> {
    validate_id(brand_id)?;
    validate_id(category_id)?;
    let brand: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM brands WHERE id=?1)",
        [brand_id],
        |row| row.get(0),
    )?;
    let category: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM categories WHERE id=?1)",
        [category_id],
        |row| row.get(0),
    )?;
    if !brand {
        return Err(ServiceError::not_found(
            "BRAND_NOT_FOUND",
            "Marca no encontrada",
        ));
    }
    if !category {
        return Err(ServiceError::not_found(
            "CATEGORY_NOT_FOUND",
            "Categoria no encontrada",
        ));
    }
    Ok(())
}

fn ensure_codes(
    connection: &Connection,
    sku: &str,
    product_id: Option<i64>,
    values: &[ValidPresentation],
) -> ServiceResult<()> {
    let sku_key = search_key(sku);
    if values
        .iter()
        .any(|value| value.barcode_key.as_deref() == Some(sku_key.as_str()))
    {
        return Err(ServiceError::conflict(
            "SKU_ALREADY_EXISTS",
            "El SKU coincide con un codigo de barras",
        ));
    }
    let sku_conflict: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM products WHERE sku_search_key=?1 AND (?2 IS NULL OR id<>?2))",
        params![sku_key, product_id],
        |row| row.get(0),
    )?;
    let sku_barcode_conflict: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM product_presentations WHERE barcode_search_key=?1)",
        [sku_key],
        |row| row.get(0),
    )?;
    if sku_conflict || sku_barcode_conflict {
        return Err(ServiceError::conflict(
            "SKU_ALREADY_EXISTS",
            "El SKU ya existe o coincide con un codigo de barras",
        ));
    }
    for value in values {
        if let Some(key) = &value.barcode_key {
            let product_sku: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM products WHERE sku_search_key=?1)",
                [key],
                |row| row.get(0),
            )?;
            let barcode_owner: Option<i64> = connection
                .query_row(
                    "SELECT id FROM product_presentations WHERE barcode_search_key=?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()?;
            let barcode_conflict = barcode_owner.is_some_and(|owner_id| Some(owner_id) != value.id);
            if product_sku || barcode_conflict {
                return Err(ServiceError::conflict(
                    "BARCODE_ALREADY_EXISTS",
                    "El codigo de barras ya existe o coincide con un SKU",
                ));
            }
        }
    }
    Ok(())
}

fn insert_presentation(
    transaction: &Transaction<'_>,
    product_id: i64,
    value: &ValidPresentation,
) -> ServiceResult<()> {
    let changed = transaction.execute(
        "INSERT INTO product_presentations (product_id,name,mode,base_quantity,price,barcode,barcode_search_key,is_default,is_active) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![product_id, value.name, value.mode.as_str(), value.base_quantity, value.price, value.barcode, value.barcode_key, value.is_default, value.is_active],
    )?;
    require_one_row(changed)
}

pub fn create_with_presentations(
    connection: &mut Connection,
    input: CreateProductWithPresentationsInput,
) -> ServiceResult<Product> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (name, sku) = validate_product(&input.name, &input.sku)?;
    let values = create_inputs(input.base_unit, input.presentations)?;
    ensure_catalog_references(&transaction, input.brand_id, input.category_id)?;
    ensure_codes(&transaction, &sku, None, &values)?;
    let default_price = values.iter().find(|value| value.is_default).unwrap().price;
    let changed = transaction.execute(
        "INSERT INTO products (name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit,inventory_cost_cents) VALUES (?1,?2,?3,0,?4,?5,?6,?7,?8,0)",
        params![name, search_key(&name), default_price, sku, search_key(&sku), input.brand_id, input.category_id, input.base_unit.as_str()],
    )?;
    require_one_row(changed)?;
    let product_id = transaction.last_insert_rowid();
    for value in &values {
        insert_presentation(&transaction, product_id, value)?;
    }
    let product = load_product(&transaction, product_id, false)?;
    transaction.commit()?;
    Ok(product)
}

fn presentation_used(connection: &Connection, id: i64) -> ServiceResult<bool> {
    connection.query_row("SELECT EXISTS(SELECT 1 FROM sale_details WHERE presentation_id=?1 UNION ALL SELECT 1 FROM inventory_movements WHERE presentation_id=?1)", [id], |row| row.get(0)).map_err(Into::into)
}

pub fn update_with_presentations(
    connection: &mut Connection,
    id: i64,
    input: UpdateProductWithPresentationsInput,
) -> ServiceResult<Product> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    validate_id(id)?;
    let (name, sku) = validate_product(&input.name, &input.sku)?;
    let stored_unit: String = transaction
        .query_row("SELECT base_unit FROM products WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => {
                ServiceError::not_found("PRODUCT_NOT_FOUND", "Producto no encontrado")
            }
            other => other.into(),
        })?;
    if stored_unit != input.base_unit.as_str() {
        return Err(ServiceError::validation(
            "BASE_UNIT_IMMUTABLE",
            "La unidad base no se puede cambiar",
        ));
    }
    let stored_unit = BaseUnit::try_from(stored_unit.as_str()).map_err(|_| {
        ServiceError::internal("INVALID_BASE_UNIT", "La unidad base guardada no es valida")
    })?;
    let existing = presentations(&transaction, id, false)?;
    for input_presentation in &input.presentations {
        if let Some(old) = input_presentation
            .id
            .and_then(|presentation_id| existing.iter().find(|item| item.id == presentation_id))
        {
            if old.is_conversion_locked
                && (old.mode != input_presentation.mode
                    || old.base_quantity != input_presentation.base_quantity)
            {
                return Err(ServiceError::validation(
                    "USED_PRESENTATION_CONVERSION_IMMUTABLE",
                    "No se puede cambiar el modo ni la cantidad base de una presentacion usada",
                ));
            }
        }
    }
    let values = update_inputs(stored_unit, input.presentations, &existing)?;
    ensure_catalog_references(&transaction, input.brand_id, input.category_id)?;
    ensure_codes(&transaction, &sku, Some(id), &values)?;
    for value in &values {
        if let Some(presentation_id) = value.id {
            existing
                .iter()
                .find(|item| item.id == presentation_id)
                .ok_or_else(|| {
                    ServiceError::validation(
                        "INVALID_PRESENTATION",
                        "La presentacion no pertenece al producto",
                    )
                })?;
        }
    }
    let supplied: BTreeSet<i64> = values.iter().filter_map(|item| item.id).collect();
    let default_price = values.iter().find(|value| value.is_default).unwrap().price;
    transaction.execute(
        "UPDATE product_presentations SET is_default=0 WHERE product_id=?1",
        [id],
    )?;
    for old in &existing {
        if !supplied.contains(&old.id) {
            if presentation_used(&transaction, old.id)? {
                let changed = transaction.execute("UPDATE product_presentations SET is_active=0,is_default=0,updated_at=CURRENT_TIMESTAMP WHERE id=?1", [old.id])?;
                require_one_row(changed)?;
            } else {
                let changed = transaction
                    .execute("DELETE FROM product_presentations WHERE id=?1", [old.id])?;
                require_one_row(changed)?;
            }
        }
    }
    for value in &values {
        if let Some(presentation_id) = value.id {
            let changed = transaction.execute("UPDATE product_presentations SET name=?1,mode=?2,base_quantity=?3,price=?4,barcode=?5,barcode_search_key=?6,is_default=?7,is_active=?8,updated_at=CURRENT_TIMESTAMP WHERE id=?9 AND product_id=?10", params![value.name,value.mode.as_str(),value.base_quantity,value.price,value.barcode,value.barcode_key,value.is_default,value.is_active,presentation_id,id])?;
            require_one_row(changed)?;
        } else {
            insert_presentation(&transaction, id, value)?;
        }
    }
    let changed = transaction.execute("UPDATE products SET name=?1,name_search_key=?2,price=?3,sku=?4,sku_search_key=?5,brand_id=?6,category_id=?7,updated_at=CURRENT_TIMESTAMP WHERE id=?8", params![name,search_key(&name),default_price,sku,search_key(&sku),input.brand_id,input.category_id,id])?;
    require_one_row(changed)?;
    let product = load_product(&transaction, id, false)?;
    transaction.commit()?;
    Ok(product)
}

fn require_one_row(changed: usize) -> ServiceResult<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(ServiceError::internal(
            "UNEXPECTED_AFFECTED_ROWS",
            "La operacion no afecto exactamente una fila",
        ))
    }
}

pub fn view(connection: &Connection, filters: ProductViewFilters) -> ServiceResult<ProductView> {
    let search = normalize_search(filters.search)?;
    if let Some(id) = filters.brand_id {
        validate_id(id)?;
    }
    if let Some(id) = filters.category_id {
        validate_id(id)?;
    }
    let mut items = Vec::new();
    for product in list(connection)? {
        if filters.brand_id.is_some_and(|id| product.brand_id != id)
            || filters
                .category_id
                .is_some_and(|id| product.category_id != id)
        {
            continue;
        }
        let barcode_match = product
            .presentations
            .iter()
            .filter_map(|item| item.barcode.as_deref())
            .any(|barcode| search_key(barcode).contains(&search));
        if !search.is_empty()
            && !search_key(&product.name).contains(&search)
            && !search_key(&product.sku).contains(&search)
            && !barcode_match
        {
            continue;
        }
        let (brand_name, category_name) = connection.query_row(
            "SELECT b.name,c.name FROM brands b,categories c WHERE b.id=?1 AND c.id=?2",
            params![product.brand_id, product.category_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        items.push(ProductViewRow {
            id: product.id,
            name: product.name,
            price: product.price,
            stock: product.stock,
            stock_base: product.stock_base,
            base_unit: product.base_unit,
            inventory_cost_cents: product.inventory_cost_cents,
            inventory_cost_status: product.inventory_cost_status,
            average_cost_cents: product.average_cost_cents,
            sku: product.sku,
            brand_id: product.brand_id,
            brand_name,
            category_id: product.category_id,
            category_name,
            created_at: product.created_at,
            updated_at: product.updated_at,
            presentations: product.presentations,
            default_presentation: product.default_presentation,
        });
    }
    let mut piece_stock = 0;
    let mut gram_stock = 0;
    let mut milliliter_stock = 0;
    let mut centimeter_stock = 0;
    let mut inventory_value_cents = 0;
    let mut products_without_cost = 0;
    let mut brands = BTreeSet::new();
    for item in &items {
        let target = match item.base_unit {
            BaseUnit::Piece => &mut piece_stock,
            BaseUnit::Gram => &mut gram_stock,
            BaseUnit::Milliliter => &mut milliliter_stock,
            BaseUnit::Centimeter => &mut centimeter_stock,
        };
        *target = checked_total(*target, item.stock, "PRODUCT_STATS_OVERFLOW")?;
        match item.inventory_cost_cents {
            Some(cost) => {
                inventory_value_cents =
                    checked_total(inventory_value_cents, cost, "PRODUCT_STATS_OVERFLOW")?
            }
            None => {
                products_without_cost =
                    checked_total(products_without_cost, 1, "PRODUCT_STATS_OVERFLOW")?
            }
        }
        brands.insert(item.brand_id);
    }
    Ok(ProductView {
        stats: ProductViewStats {
            product_count: i64::try_from(items.len()).unwrap_or(JS_MAX_SAFE_INTEGER),
            piece_stock,
            gram_stock,
            milliliter_stock,
            centimeter_stock,
            brand_count: i64::try_from(brands.len()).unwrap_or(JS_MAX_SAFE_INTEGER),
            inventory_value_cents,
            products_without_cost,
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
    let mut demand_by_product = BTreeMap::new();
    for (&presentation_id, &entered_quantity) in cart {
        validate_id(presentation_id)?;
        let presentation = connection
            .query_row(
                "SELECT product_id, mode, base_quantity
                 FROM product_presentations WHERE id = ?1 AND is_active = 1",
                [presentation_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        parse_mode(row.get(1)?)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                ServiceError::conflict(
                    "PRESENTATION_UNAVAILABLE",
                    "Una presentacion ya no existe o no esta activa",
                )
            })?;
        let base_demand =
            presentation_base_quantity(presentation.1, entered_quantity, presentation.2)?;
        let current = demand_by_product.get(&presentation.0).copied().unwrap_or(0);
        demand_by_product.insert(
            presentation.0,
            checked_total(current, base_demand, "CART_AMOUNT_OVERFLOW")?,
        );
    }
    let mut rows = Vec::new();
    for product in list_operational(connection)? {
        if filters
            .category_id
            .is_some_and(|id| product.category_id != id)
        {
            continue;
        }
        let barcode_match = product
            .presentations
            .iter()
            .filter_map(|item| item.barcode.as_deref())
            .any(|barcode| search_key(barcode).contains(&search));
        if !search.is_empty()
            && !search_key(&product.name).contains(&search)
            && !search_key(&product.sku).contains(&search)
            && !barcode_match
        {
            continue;
        }
        let (brand_name, category_name) = connection.query_row(
            "SELECT b.name,c.name FROM brands b,categories c WHERE b.id=?1 AND c.id=?2",
            params![product.brand_id, product.category_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let cart_quantity = demand_by_product.get(&product.id).copied().unwrap_or(0);
        rows.push(ProductSelectorRow {
            id: product.id,
            name: product.name,
            sku: product.sku,
            price: product.price,
            stock: product.stock,
            stock_base: product.stock_base,
            base_unit: product.base_unit,
            cart_quantity,
            available_stock: product.stock.saturating_sub(cart_quantity).max(0),
            brand_id: product.brand_id,
            brand_name,
            category_id: product.category_id,
            category_name,
            presentations: product.presentations,
            default_presentation: product.default_presentation,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        sync::{Arc, Barrier},
        thread,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crate::{
        db::migrations,
        models::{
            product::{
                BaseUnit, CreateProductWithPresentationsInput, ProductCodeMatchKind,
                UpdateProductWithPresentationsInput,
            },
            product_presentation::{
                CreateProductPresentationInput, PresentationMode, UpdateProductPresentationInput,
            },
        },
    };
    use rusqlite::{params, Connection};

    fn db() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        c.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut c).unwrap();
        c.execute_batch("INSERT INTO brands(name,name_search_key)VALUES('Marca','marca');INSERT INTO categories(name,name_search_key)VALUES('Categoria','categoria');").unwrap();
        c
    }
    fn presentations() -> Vec<CreateProductPresentationInput> {
        vec![
            CreateProductPresentationInput {
                name: "Pieza".into(),
                mode: PresentationMode::Fixed,
                base_quantity: 1,
                price: "1.00".into(),
                barcode: None,
                is_default: true,
                is_active: true,
            },
            CreateProductPresentationInput {
                name: "Bolsa 100".into(),
                mode: PresentationMode::Fixed,
                base_quantity: 100,
                price: "80.00".into(),
                barcode: Some("BOLSA-100".into()),
                is_default: false,
                is_active: true,
            },
        ]
    }

    fn product_input(sku: &str) -> CreateProductWithPresentationsInput {
        CreateProductWithPresentationsInput {
            name: "Producto".into(),
            sku: sku.into(),
            brand_id: 1,
            category_id: 1,
            base_unit: BaseUnit::Piece,
            presentations: presentations(),
        }
    }

    fn assert_concurrent_conflict(
        inputs: [CreateProductWithPresentationsInput; 2],
        expected_codes: &[&str],
    ) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "pos-tauri-product-race-{}-{unique}.db",
            std::process::id()
        ));
        {
            let mut c = Connection::open(&path).unwrap();
            c.pragma_update(None, "foreign_keys", true).unwrap();
            migrations::run(&mut c).unwrap();
            c.execute_batch("INSERT INTO brands(name,name_search_key)VALUES('Marca','marca');INSERT INTO categories(name,name_search_key)VALUES('Categoria','categoria');").unwrap();
        }
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = inputs
            .into_iter()
            .map(|input| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    let mut c = Connection::open(path).unwrap();
                    c.busy_timeout(Duration::from_secs(10)).unwrap();
                    barrier.wait();
                    super::create_with_presentations(&mut c, input)
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let error = results
            .iter()
            .find_map(|result| result.as_ref().err())
            .unwrap();
        assert!(expected_codes.contains(&error.code), "{}", error.code);
        let verification = Connection::open(&path).unwrap();
        assert_eq!(
            verification
                .query_row("SELECT COUNT(*) FROM products", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(verification);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn creates_product_with_zero_stock_without_an_initial_movement() {
        let mut c = db();
        let p = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Tornillo".into(),
                sku: "tor-1".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap();
        assert_eq!((p.stock_base, p.price, p.presentations.len()), (0, 100, 2));
        assert_eq!(p.inventory_cost_cents, Some(0));
        let counts: (i64, i64) = c
            .query_row(
                "SELECT stock, (SELECT COUNT(*) FROM inventory_movements WHERE product_id=?1)
                 FROM products WHERE id=?1",
                [p.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0));
    }

    #[test]
    fn zero_cost_is_preserved_when_stock_is_changed_outside_product_creation() {
        let mut c = db();
        let product =
            super::create_with_presentations(&mut c, product_input("RETAIL-VALUE")).unwrap();
        c.execute("UPDATE products SET stock=250 WHERE id=?1", [product.id])
            .unwrap();
        let loaded = super::load_product(&c, product.id, false).unwrap();
        assert_eq!(loaded.inventory_cost_cents, Some(0));

        let view = super::view(&c, Default::default()).unwrap();
        assert_eq!(view.items[0].inventory_cost_cents, Some(0));
        assert_eq!(view.stats.inventory_value_cents, 0);
        assert_eq!(view.stats.centimeter_stock, 0);
    }

    #[test]
    fn historical_unknown_cost_stays_unknown_in_reads_and_stats() {
        let mut c = db();
        let product =
            super::create_with_presentations(&mut c, product_input("UNKNOWN-COST")).unwrap();
        c.execute(
            "UPDATE products SET stock=3,inventory_cost_cents=NULL WHERE id=?1",
            [product.id],
        )
        .unwrap();

        let loaded = super::load_product(&c, product.id, false).unwrap();
        assert_eq!(loaded.inventory_cost_cents, None);
        assert_eq!(loaded.inventory_cost_status, "unknown");
        assert_eq!(loaded.average_cost_cents, None);

        let view = super::view(&c, Default::default()).unwrap();
        assert_eq!(view.stats.inventory_value_cents, 0);
        assert_eq!(view.stats.products_without_cost, 1);
    }

    #[test]
    fn create_payload_rejects_retired_initial_inventory_fields() {
        let payload = serde_json::json!({
            "name": "Producto",
            "sku": "SKU",
            "brand_id": 1,
            "category_id": 1,
            "base_unit": "piece",
            "initial_stock": 1,
            "presentations": []
        });
        assert!(serde_json::from_value::<CreateProductWithPresentationsInput>(payload).is_err());
    }

    #[test]
    fn create_rolls_back_product_when_a_later_insert_fails() {
        let mut c = db();
        c.execute_batch(
            "CREATE TRIGGER fail_presentation_insert BEFORE INSERT ON product_presentations
             BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
        )
        .unwrap();

        assert!(super::create_with_presentations(&mut c, product_input("ROLLBACK")).is_err());
        let counts: (i64, i64) = c
            .query_row(
                "SELECT (SELECT COUNT(*) FROM products),
                        (SELECT COUNT(*) FROM product_presentations)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0));
    }
    #[test]
    fn barcodes_are_optional_unique_and_cannot_collide_with_skus() {
        let mut c = db();
        super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Uno".into(),
                sku: "SKU-1".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap();
        let mut p = presentations();
        p[1].barcode = Some("sku-1".into());
        let e = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Dos".into(),
                sku: "SKU-2".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: p,
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "BARCODE_ALREADY_EXISTS");
        let e = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Dos".into(),
                sku: "bolsa-100".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "SKU_ALREADY_EXISTS");
    }

    #[test]
    fn exact_code_match_reports_sku_or_barcode_presentation_with_unicode_keys() {
        let mut c = db();
        let mut input = product_input("SKU-UNICODE");
        input.presentations[1].barcode = Some("Cafe\u{301}".into());
        let product = super::create_with_presentations(&mut c, input).unwrap();
        let barcode_id = product
            .presentations
            .iter()
            .find(|presentation| presentation.barcode.is_some())
            .unwrap()
            .id;

        let barcode_match = super::find_by_code(&c, "CAFÉ").unwrap().unwrap();
        assert_eq!(barcode_match.match_kind, ProductCodeMatchKind::Barcode);
        assert_eq!(barcode_match.presentation_id, Some(barcode_id));
        assert_eq!(barcode_match.product.id, product.id);

        let sku_match = super::find_by_code(&c, "sku-unicode").unwrap().unwrap();
        assert_eq!(sku_match.match_kind, ProductCodeMatchKind::Sku);
        assert_eq!(sku_match.presentation_id, None);
    }

    #[test]
    fn rejects_barcode_search_keys_expanded_past_the_database_limit() {
        let mut c = db();
        let mut input = product_input("SKU-LONG-KEY");
        input.presentations[1].barcode = Some("ß".repeat(128));

        assert_eq!(
            super::create_with_presentations(&mut c, input)
                .unwrap_err()
                .code,
            "BARCODE_SEARCH_KEY_TOO_LONG"
        );
        assert_eq!(
            super::find_by_code(&c, &"ß".repeat(128)).unwrap_err().code,
            "CODE_SEARCH_KEY_TOO_LONG"
        );
    }
    #[test]
    fn base_unit_is_immutable_and_default_price_stays_synced() {
        let mut c = db();
        let p = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Uno".into(),
                sku: "SKU".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap();
        let updates = p
            .presentations
            .iter()
            .map(|x| UpdateProductPresentationInput {
                id: Some(x.id),
                name: x.name.clone(),
                mode: x.mode,
                base_quantity: x.base_quantity,
                price: if x.name == "Bolsa 100" {
                    "90.00".into()
                } else {
                    "1.00".into()
                },
                barcode: x.barcode.clone(),
                is_default: x.name == "Bolsa 100",
                is_active: true,
            })
            .collect();
        let e = super::update_with_presentations(
            &mut c,
            p.id,
            UpdateProductWithPresentationsInput {
                name: "Otro".into(),
                sku: "SKU".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Gram,
                presentations: updates,
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "BASE_UNIT_IMMUTABLE");
    }

    #[test]
    fn concurrent_connections_cannot_create_the_same_sku() {
        assert_concurrent_conflict(
            [product_input("RACE-SKU"), product_input("RACE-SKU")],
            &["SKU_ALREADY_EXISTS"],
        );
    }

    #[test]
    fn concurrent_sku_and_barcode_creation_cannot_claim_the_same_code() {
        let mut sku_input = product_input("RACE-CODE");
        sku_input.presentations[1].barcode = Some("BARCODE-A".into());
        let mut barcode_input = product_input("OTHER-SKU");
        barcode_input.presentations[1].barcode = Some("race-code".into());

        assert_concurrent_conflict(
            [sku_input, barcode_input],
            &["SKU_ALREADY_EXISTS", "BARCODE_ALREADY_EXISTS"],
        );
    }

    #[test]
    fn concurrent_barcode_creation_cannot_claim_the_same_code() {
        let mut first = product_input("RACE-BARCODE-A");
        first.presentations[1].barcode = Some("SHARED-BARCODE".into());
        let mut second = product_input("RACE-BARCODE-B");
        second.presentations[1].barcode = Some("shared-barcode".into());

        assert_concurrent_conflict([first, second], &["BARCODE_ALREADY_EXISTS"]);
    }

    #[test]
    fn update_with_presentations_rolls_back_after_partial_changes() {
        let mut c = db();
        let product =
            super::create_with_presentations(&mut c, product_input("ROLLBACK-UPDATE")).unwrap();
        let updates = product
            .presentations
            .iter()
            .map(|presentation| UpdateProductPresentationInput {
                id: Some(presentation.id),
                name: format!("{} modificada", presentation.name),
                mode: presentation.mode,
                base_quantity: presentation.base_quantity,
                price: "2.00".into(),
                barcode: presentation.barcode.clone(),
                is_default: presentation.is_default,
                is_active: presentation.is_active,
            })
            .collect();
        c.execute_batch(
            "CREATE TRIGGER fail_product_update BEFORE UPDATE ON products
             BEGIN SELECT RAISE(ABORT, 'forced rollback'); END;",
        )
        .unwrap();

        assert!(super::update_with_presentations(
            &mut c,
            product.id,
            UpdateProductWithPresentationsInput {
                name: "Nombre modificado".into(),
                sku: "ROLLBACK-UPDATE-NEW".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: updates,
            },
        )
        .is_err());

        let persisted = super::load_product(&c, product.id, false).unwrap();
        assert_eq!(persisted.name, product.name);
        assert_eq!(persisted.sku, product.sku);
        assert_eq!(persisted.price, product.price);
        assert_eq!(persisted.presentations.len(), product.presentations.len());
        for original in product.presentations {
            let current = persisted
                .presentations
                .iter()
                .find(|presentation| presentation.id == original.id)
                .unwrap();
            assert_eq!(current.name, original.name);
            assert_eq!(current.price, original.price);
            assert_eq!(current.is_default, original.is_default);
        }
    }
    #[test]
    fn editing_default_syncs_legacy_price_and_used_presentations_are_deactivated_not_deleted() {
        let mut c = db();
        let p = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Nombre anterior".into(),
                sku: "SKU".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap();
        let piece = p.presentations.iter().find(|x| x.name == "Pieza").unwrap();
        c.execute_batch("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','a@b.c','hash'); INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method) VALUES(1,'A B','a@b.c','token','fp','2026-01-01',100,'none',0,0,100,'tarjeta');").unwrap();
        c.execute("INSERT INTO sale_details(product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,unit_price,entered_quantity,base_quantity,line_total) VALUES(?1,1,2,?2,'Nombre anterior','SKU','Pieza','piece','fixed',1,1,100,1,1,100)",params![p.id,piece.id]).unwrap();
        c.execute(
            "UPDATE product_presentations SET barcode='PIECE-OLD',barcode_search_key='piece-old' WHERE id=?1",
            [piece.id],
        )
        .unwrap();
        let bag = p
            .presentations
            .iter()
            .find(|x| x.name == "Bolsa 100")
            .unwrap();
        let updated = super::update_with_presentations(
            &mut c,
            p.id,
            UpdateProductWithPresentationsInput {
                name: "Nombre actual".into(),
                sku: "SKU-NUEVO".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: vec![UpdateProductPresentationInput {
                    id: Some(bag.id),
                    name: bag.name.clone(),
                    mode: bag.mode,
                    base_quantity: bag.base_quantity,
                    price: "90.00".into(),
                    barcode: bag.barcode.clone(),
                    is_default: true,
                    is_active: true,
                }],
            },
        )
        .unwrap();
        assert_eq!(updated.price, 9000);
        assert_eq!(updated.default_presentation.id, bag.id);
        let old:(i64,String,String)=c.query_row("SELECT is_active,product_name,sku FROM product_presentations JOIN sale_details ON sale_details.presentation_id=product_presentations.id WHERE product_presentations.id=?1",[piece.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(old, (0, "Nombre anterior".into(), "SKU".into()));
        let listed = super::list(&c).unwrap();
        assert_eq!(listed[0].name, "Nombre actual");
        assert_eq!(listed[0].presentations.len(), 2);
        assert!(
            !listed[0]
                .presentations
                .iter()
                .find(|presentation| presentation.id == piece.id)
                .unwrap()
                .is_active
        );
        assert_eq!(
            super::view(&c, Default::default()).unwrap().items[0]
                .presentations
                .len(),
            2
        );
        assert_eq!(
            super::find_by_sku(&c, "SKU-NUEVO")
                .unwrap()
                .unwrap()
                .presentations
                .len(),
            1
        );
        assert!(super::find_by_code(&c, "PIECE-OLD").unwrap().is_none());
        let selector = super::selector(&c, &BTreeMap::new(), Default::default()).unwrap();
        assert_eq!(selector[0].presentations.len(), 1);

        let reactivation = listed[0]
            .presentations
            .iter()
            .map(|presentation| UpdateProductPresentationInput {
                id: Some(presentation.id),
                name: presentation.name.clone(),
                mode: presentation.mode,
                base_quantity: presentation.base_quantity,
                price: if presentation.id == bag.id {
                    "90.00".into()
                } else {
                    "1.00".into()
                },
                barcode: presentation.barcode.clone(),
                is_default: presentation.is_default,
                is_active: true,
            })
            .collect();
        let reactivated = super::update_with_presentations(
            &mut c,
            p.id,
            UpdateProductWithPresentationsInput {
                name: "Nombre actual".into(),
                sku: "SKU-NUEVO".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: reactivation,
            },
        )
        .unwrap();
        assert!(
            reactivated
                .presentations
                .iter()
                .find(|presentation| presentation.id == piece.id)
                .unwrap()
                .is_active
        );
        assert_eq!(
            super::find_by_code(&c, "PIECE-OLD")
                .unwrap()
                .unwrap()
                .presentation_id,
            Some(piece.id)
        );
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM sale_details WHERE presentation_id=?1 AND product_name='Nombre anterior' AND sku='SKU'",
                [piece.id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn used_presentation_conversion_cannot_change() {
        let mut c = db();
        let p = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Uno".into(),
                sku: "SKU".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: presentations(),
            },
        )
        .unwrap();
        let piece = p.presentations.iter().find(|x| x.name == "Pieza").unwrap();
        c.execute_batch("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('A','B','a@b.c','hash'); INSERT INTO sales(user_id,cashier_name,cashier_email,checkout_token,cart_fingerprint,sale_date,subtotal,discount_type,discount_value,discount_amount,total,payment_method) VALUES(1,'A B','a@b.c','token','fp','2026-01-01',100,'none',0,0,100,'tarjeta');").unwrap();
        c.execute("INSERT INTO sale_details(product_id,sale_id,record_version,presentation_id,product_name,sku,presentation_name,base_unit,presentation_mode,presentation_base_quantity,quantity,unit_price,entered_quantity,base_quantity,line_total) VALUES(?1,1,2,?2,'Uno','SKU','Pieza','piece','fixed',1,1,100,1,1,100)",params![p.id,piece.id]).unwrap();
        let updates = p
            .presentations
            .iter()
            .map(|x| UpdateProductPresentationInput {
                id: Some(x.id),
                name: x.name.clone(),
                mode: x.mode,
                base_quantity: if x.id == piece.id { 2 } else { x.base_quantity },
                price: if x.id == piece.id {
                    "1.00".into()
                } else {
                    "80.00".into()
                },
                barcode: x.barcode.clone(),
                is_default: x.is_default,
                is_active: x.is_active,
            })
            .collect();
        let error = super::update_with_presentations(
            &mut c,
            p.id,
            UpdateProductWithPresentationsInput {
                name: p.name,
                sku: p.sku,
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: updates,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "USED_PRESENTATION_CONVERSION_IMMUTABLE");
        let listed = super::list(&c).unwrap();
        assert!(
            listed[0]
                .presentations
                .iter()
                .find(|presentation| presentation.id == piece.id)
                .unwrap()
                .is_conversion_locked
        );
    }

    #[test]
    fn legacy_fixed_measured_unit_preserves_conversion_on_metadata_update() {
        let mut c = db();
        let product = super::create_with_presentations(
            &mut c,
            CreateProductWithPresentationsInput {
                name: "Cafe".into(),
                sku: "LEGACY-GRAM".into(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Gram,
                presentations: vec![CreateProductPresentationInput {
                    name: "Bolsa 250 g".into(),
                    mode: PresentationMode::Measured,
                    base_quantity: 250,
                    price: "12.50".into(),
                    barcode: None,
                    is_default: true,
                    is_active: true,
                }],
            },
        )
        .unwrap();
        let presentation_id = product.presentations[0].id;
        c.execute(
            "UPDATE product_presentations SET mode='fixed' WHERE id=?1",
            [presentation_id],
        )
        .unwrap();
        c.execute(
            "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,origin,reason,type,quantity) VALUES(?1,'Cafe','LEGACY-GRAM','cafe','legacy-gram',1,?2,'initial_stock','ajuste','entrada',250)",
            params![product.id, presentation_id],
        )
        .unwrap();

        let updated = super::update_with_presentations(
            &mut c,
            product.id,
            UpdateProductWithPresentationsInput {
                name: "Cafe editado".into(),
                sku: product.sku.clone(),
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Gram,
                presentations: vec![UpdateProductPresentationInput {
                    id: Some(presentation_id),
                    name: "Bolsa historica".into(),
                    mode: PresentationMode::Fixed,
                    base_quantity: 250,
                    price: "13.00".into(),
                    barcode: None,
                    is_default: true,
                    is_active: true,
                }],
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Cafe editado");
        assert_eq!(updated.presentations[0].mode, PresentationMode::Fixed);
        assert_eq!(updated.presentations[0].base_quantity, 250);

        for (mode, base_quantity) in [
            (PresentationMode::Measured, 250),
            (PresentationMode::Fixed, 500),
        ] {
            let error = super::update_with_presentations(
                &mut c,
                product.id,
                UpdateProductWithPresentationsInput {
                    name: updated.name.clone(),
                    sku: updated.sku.clone(),
                    brand_id: 1,
                    category_id: 1,
                    base_unit: BaseUnit::Gram,
                    presentations: vec![UpdateProductPresentationInput {
                        id: Some(presentation_id),
                        name: "Bolsa historica".into(),
                        mode,
                        base_quantity,
                        price: "13.00".into(),
                        barcode: None,
                        is_default: true,
                        is_active: true,
                    }],
                },
            )
            .unwrap_err();
            assert_eq!(error.code, "USED_PRESENTATION_CONVERSION_IMMUTABLE");
        }

        let error = super::update_with_presentations(
            &mut c,
            product.id,
            UpdateProductWithPresentationsInput {
                name: updated.name,
                sku: updated.sku,
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Gram,
                presentations: vec![
                    UpdateProductPresentationInput {
                        id: Some(presentation_id),
                        name: "Bolsa historica".into(),
                        mode: PresentationMode::Fixed,
                        base_quantity: 250,
                        price: "13.00".into(),
                        barcode: None,
                        is_default: true,
                        is_active: true,
                    },
                    UpdateProductPresentationInput {
                        id: None,
                        name: "Nueva bolsa fija".into(),
                        mode: PresentationMode::Fixed,
                        base_quantity: 100,
                        price: "6.00".into(),
                        barcode: None,
                        is_default: false,
                        is_active: true,
                    },
                ],
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT");
    }

    #[test]
    fn movement_reference_is_exposed_as_a_conversion_lock() {
        let mut c = db();
        let product =
            super::create_with_presentations(&mut c, product_input("MOVEMENT-LOCK")).unwrap();
        let presentation_id = product.presentations[0].id;
        assert!(!product.presentations[0].is_conversion_locked);
        c.execute(
            "INSERT INTO inventory_movements(product_id,product_name,sku,product_search_key,sku_search_key,record_version,presentation_id,origin,reason,type,quantity) VALUES(?1,?2,?3,'producto','movement-lock',1,?4,'initial_stock','ajuste','entrada',1)",
            params![product.id, product.name, product.sku, presentation_id],
        )
        .unwrap();

        let reloaded = super::list(&c).unwrap();
        assert!(
            reloaded[0]
                .presentations
                .iter()
                .find(|item| item.id == presentation_id)
                .unwrap()
                .is_conversion_locked
        );
    }
    #[test]
    fn requires_one_active_default() {
        let e = super::validate_presentations(
            BaseUnit::Piece,
            vec![(
                None,
                "Pieza".into(),
                PresentationMode::Fixed,
                1,
                "1.00".into(),
                None,
                false,
                true,
            )],
        )
        .unwrap_err();
        assert_eq!(e.code, "DEFAULT_PRESENTATION_REQUIRED");
        let e = super::validate_presentations(
            BaseUnit::Piece,
            vec![(
                None,
                "Pieza".into(),
                PresentationMode::Fixed,
                1,
                "1.00".into(),
                None,
                true,
                false,
            )],
        )
        .unwrap_err();
        assert_eq!(e.code, "DEFAULT_PRESENTATION_INACTIVE");
    }

    #[test]
    fn custom_bases_must_be_positive_and_javascript_safe() {
        for base_quantity in [0, super::JS_MAX_SAFE_INTEGER + 1] {
            let error = super::validate_presentations(
                BaseUnit::Gram,
                vec![(
                    None,
                    "Otra medida".into(),
                    PresentationMode::Measured,
                    base_quantity,
                    "1.00".into(),
                    None,
                    true,
                    true,
                )],
            )
            .unwrap_err();
            assert_eq!(error.code, "INVALID_BASE_QUANTITY");
        }
        assert!(super::validate_presentations(
            BaseUnit::Centimeter,
            vec![(
                None,
                "Otra medida".into(),
                PresentationMode::Measured,
                super::JS_MAX_SAFE_INTEGER,
                "1.00".into(),
                None,
                true,
                true,
            )],
        )
        .is_ok());
    }

    #[test]
    fn selector_aggregates_base_demand_from_presentation_indexed_cart() {
        let mut c = db();
        super::create_with_presentations(&mut c, product_input("OTHER")).unwrap();
        let mut input = product_input("TARGET");
        input.base_unit = BaseUnit::Gram;
        input.presentations = vec![
            CreateProductPresentationInput {
                name: "Kilogramo".into(),
                mode: PresentationMode::Measured,
                base_quantity: 1_000,
                price: "40.00".into(),
                barcode: Some("KILO-TARGET".into()),
                is_default: true,
                is_active: true,
            },
            CreateProductPresentationInput {
                name: "Por cantidad".into(),
                mode: PresentationMode::Measured,
                base_quantity: 100,
                price: "10.00".into(),
                barcode: Some("MEASURED-TARGET".into()),
                is_default: false,
                is_active: true,
            },
        ];
        let product = super::create_with_presentations(&mut c, input).unwrap();
        c.execute("UPDATE products SET stock=500 WHERE id=?1", [product.id])
            .unwrap();
        let kilogram = product
            .presentations
            .iter()
            .find(|presentation| presentation.name == "Kilogramo")
            .unwrap();
        let measured = product
            .presentations
            .iter()
            .find(|presentation| presentation.name == "Por cantidad")
            .unwrap();
        assert_ne!(product.id, kilogram.id);
        assert_ne!(product.id, measured.id);

        let cart = BTreeMap::from([(kilogram.id, 3), (measured.id, 25)]);
        let rows = super::selector(&c, &cart, Default::default()).unwrap();
        let row = rows.iter().find(|row| row.id == product.id).unwrap();
        assert_eq!(row.cart_quantity, 28);
        assert_eq!(row.available_stock, 472);
    }

    #[test]
    fn piece_products_reject_measured_presentations_on_create_and_update() {
        let mut c = db();
        let mut create_input = product_input("MEASURED-PIECE");
        create_input.presentations[0].mode = PresentationMode::Measured;
        assert_eq!(
            super::create_with_presentations(&mut c, create_input)
                .unwrap_err()
                .code,
            "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT"
        );

        let product =
            super::create_with_presentations(&mut c, product_input("FIXED-PIECE")).unwrap();
        let updates = product
            .presentations
            .iter()
            .map(|presentation| UpdateProductPresentationInput {
                id: Some(presentation.id),
                name: presentation.name.clone(),
                mode: if presentation.is_default {
                    PresentationMode::Measured
                } else {
                    presentation.mode
                },
                base_quantity: presentation.base_quantity,
                price: if presentation.is_default {
                    "1.00"
                } else {
                    "80.00"
                }
                .into(),
                barcode: presentation.barcode.clone(),
                is_default: presentation.is_default,
                is_active: presentation.is_active,
            })
            .collect();
        let error = super::update_with_presentations(
            &mut c,
            product.id,
            UpdateProductWithPresentationsInput {
                name: product.name,
                sku: product.sku,
                brand_id: 1,
                category_id: 1,
                base_unit: BaseUnit::Piece,
                presentations: updates,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT");
    }
}
