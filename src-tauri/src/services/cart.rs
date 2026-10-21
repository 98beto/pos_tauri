use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::models::{
    cart::{Cart, CartDiscountQuote, CartDiscountType, CartLine},
    product::BaseUnit,
    product_presentation::PresentationMode,
};

use super::{
    checked_total, divide_half_up_positive, line_total, parse_money_text,
    presentation_base_quantity, validate_id, ServiceError, ServiceResult, JS_MAX_SAFE_INTEGER,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartEntry {
    pub presentation_id: i64,
    pub entered_quantity: i64,
    pub unit_price_override: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CartDiscount {
    #[default]
    None,
    Percentage(i64),
    Fixed(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionCart {
    pub lines: BTreeMap<i64, CartEntry>,
    pub discount: CartDiscount,
}

pub type CartState = SessionCart;

impl SessionCart {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.discount = CartDiscount::None;
    }

    pub fn quantities(&self) -> BTreeMap<i64, i64> {
        self.lines
            .iter()
            .map(|(&id, entry)| (id, entry.entered_quantity))
            .collect()
    }
}

impl<const N: usize> From<[(i64, i64); N]> for SessionCart {
    fn from(values: [(i64, i64); N]) -> Self {
        Self {
            lines: values
                .into_iter()
                .map(|(presentation_id, entered_quantity)| {
                    (
                        presentation_id,
                        CartEntry {
                            presentation_id,
                            entered_quantity,
                            unit_price_override: None,
                        },
                    )
                })
                .collect(),
            discount: CartDiscount::None,
        }
    }
}

struct Presentation {
    id: i64,
    name: String,
    barcode: Option<String>,
    product_id: i64,
    product_name: String,
    sku: String,
    base_unit: BaseUnit,
    mode: PresentationMode,
    base_quantity: i64,
    price: i64,
    stock: i64,
}

pub fn get(connection: &Connection, state: &SessionCart) -> ServiceResult<Cart> {
    let mut lines = Vec::with_capacity(state.lines.len());
    let mut demand_by_product = BTreeMap::new();
    let mut subtotal = 0;

    for entry in state.lines.values() {
        validate_id(entry.presentation_id)?;
        if !(1..=JS_MAX_SAFE_INTEGER).contains(&entry.entered_quantity) {
            return Err(invalid_quantity());
        }
        let presentation = load_presentation(connection, entry.presentation_id, true)?;
        let applied_unit_price = entry.unit_price_override.unwrap_or(presentation.price);
        if !(1..=JS_MAX_SAFE_INTEGER).contains(&applied_unit_price) {
            return Err(invalid_price());
        }
        let base_consumed = presentation_base_quantity(
            presentation.mode,
            entry.entered_quantity,
            presentation.base_quantity,
        )?;
        let current = demand_by_product
            .get(&presentation.product_id)
            .copied()
            .unwrap_or(0);
        demand_by_product.insert(
            presentation.product_id,
            checked_total(current, base_consumed, "CART_AMOUNT_OVERFLOW")?,
        );
        let computed_total = line_total(
            presentation.mode,
            entry.entered_quantity,
            presentation.base_quantity,
            applied_unit_price,
        )?;
        subtotal = checked_total(subtotal, computed_total, "CART_AMOUNT_OVERFLOW")?;
        lines.push(CartLine {
            presentation_id: presentation.id,
            presentation_name: presentation.name,
            presentation_barcode: presentation.barcode,
            product_id: presentation.product_id,
            product_name: presentation.product_name,
            sku: presentation.sku,
            base_unit: presentation.base_unit,
            mode: presentation.mode,
            base_quantity: presentation.base_quantity,
            catalog_unit_price: presentation.price,
            applied_unit_price,
            price_overridden: entry.unit_price_override.is_some(),
            entered_quantity: entry.entered_quantity,
            base_consumed,
            stock_base: presentation.stock,
            available: false,
            line_total: computed_total,
        });
    }

    for line in &mut lines {
        line.available = demand_by_product
            .get(&line.product_id)
            .is_some_and(|demand| *demand <= line.stock_base);
    }
    let (discount_type, discount_value, discount_amount) =
        discount_values(state.discount, subtotal)?;
    let total = if subtotal == 0 && matches!(state.discount, CartDiscount::None) {
        0
    } else {
        subtotal
            .checked_sub(discount_amount)
            .filter(|value| *value > 0)
            .ok_or_else(invalid_discount)?
    };
    let cart_fingerprint = fingerprint(
        lines.iter().map(|line| FingerprintLine {
            presentation_id: line.presentation_id,
            entered_quantity: line.entered_quantity,
            mode: line.mode,
            presentation_base_quantity: line.base_quantity,
            base_consumed: line.base_consumed,
            catalog_unit_price: line.catalog_unit_price,
            applied_unit_price: line.applied_unit_price,
            line_total: line.line_total,
        }),
        discount_type,
        discount_value,
        discount_amount,
        subtotal,
        total,
    );
    Ok(Cart {
        line_count: i64::try_from(lines.len()).map_err(|_| amount_overflow())?,
        lines,
        subtotal,
        discount_type,
        discount_value,
        discount_amount,
        total,
        cart_fingerprint,
    })
}

pub fn add(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
) -> ServiceResult<Cart> {
    change_quantity(connection, state, presentation_id, 1)
}

pub fn increment(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    if !state.lines.contains_key(&presentation_id) {
        return Err(item_not_found());
    }
    change_quantity(connection, state, presentation_id, 1)
}

pub fn set_quantity(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
    entered_quantity: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    if !(1..=JS_MAX_SAFE_INTEGER).contains(&entered_quantity) {
        return Err(invalid_quantity());
    }
    load_presentation(connection, presentation_id, true)?;
    mutate(connection, state, |candidate| {
        candidate
            .lines
            .entry(presentation_id)
            .and_modify(|entry| entry.entered_quantity = entered_quantity)
            .or_insert(CartEntry {
                presentation_id,
                entered_quantity,
                unit_price_override: None,
            });
        Ok(())
    })
}

pub fn set_unit_price(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
    value: &str,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    let unit_price = parse_money_text(value).map_err(|_| invalid_price())?;
    mutate(connection, state, |candidate| {
        let entry = candidate
            .lines
            .get_mut(&presentation_id)
            .ok_or_else(item_not_found)?;
        entry.unit_price_override = Some(unit_price);
        Ok(())
    })
}

pub fn reset_unit_price(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    mutate(connection, state, |candidate| {
        let entry = candidate
            .lines
            .get_mut(&presentation_id)
            .ok_or_else(item_not_found)?;
        entry.unit_price_override = None;
        Ok(())
    })
}

pub fn set_discount(
    connection: &Connection,
    state: &mut SessionCart,
    discount_type: &str,
    value: &str,
) -> ServiceResult<Cart> {
    if state.is_empty() {
        return Err(ServiceError::validation(
            "EMPTY_CART",
            "El carrito no tiene productos",
        ));
    }
    let discount = parse_discount(discount_type, value)?;
    mutate(connection, state, |candidate| {
        candidate.discount = discount;
        Ok(())
    })
}

pub fn quote_discount(
    connection: &Connection,
    state: &SessionCart,
    discount_type: &str,
    value: &str,
) -> ServiceResult<CartDiscountQuote> {
    if state.is_empty() {
        return Err(ServiceError::validation(
            "EMPTY_CART",
            "El carrito no tiene productos",
        ));
    }
    let mut candidate = state.clone();
    candidate.discount = parse_discount(discount_type, value)?;
    let cart = get(connection, &candidate)?;
    Ok(CartDiscountQuote {
        subtotal: cart.subtotal,
        discount_type: cart.discount_type,
        discount_value: cart.discount_value,
        discount_amount: cart.discount_amount,
        total: cart.total,
    })
}

fn parse_discount(discount_type: &str, value: &str) -> ServiceResult<CartDiscount> {
    let discount = match discount_type {
        "percentage" => {
            parse_percentage(value)?.map_or(CartDiscount::None, CartDiscount::Percentage)
        }
        "fixed" => {
            let amount =
                super::parse_money_text_allow_zero(value, true).map_err(|_| invalid_discount())?;
            if amount == 0 {
                CartDiscount::None
            } else {
                CartDiscount::Fixed(amount)
            }
        }
        _ => return Err(invalid_discount()),
    };
    Ok(discount)
}

pub fn remove_discount(connection: &Connection, state: &mut SessionCart) -> ServiceResult<Cart> {
    mutate(connection, state, |candidate| {
        candidate.discount = CartDiscount::None;
        Ok(())
    })
}

pub fn decrement(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    mutate(connection, state, |candidate| {
        let quantity = candidate
            .lines
            .get(&presentation_id)
            .ok_or_else(item_not_found)?
            .entered_quantity;
        if quantity == 1 {
            candidate.lines.remove(&presentation_id);
        } else if let Some(entry) = candidate.lines.get_mut(&presentation_id) {
            entry.entered_quantity -= 1;
        }
        if candidate.lines.is_empty() {
            candidate.discount = CartDiscount::None;
        }
        Ok(())
    })
}

pub fn remove(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    mutate(connection, state, |candidate| {
        if candidate.lines.remove(&presentation_id).is_none() {
            return Err(item_not_found());
        }
        if candidate.lines.is_empty() {
            candidate.discount = CartDiscount::None;
        }
        Ok(())
    })
}

pub fn clear(connection: &Connection, state: &mut SessionCart) -> ServiceResult<Cart> {
    mutate(connection, state, |candidate| {
        candidate.clear();
        Ok(())
    })
}

fn change_quantity(
    connection: &Connection,
    state: &mut SessionCart,
    presentation_id: i64,
    amount: i64,
) -> ServiceResult<Cart> {
    validate_id(presentation_id)?;
    load_presentation(connection, presentation_id, true)?;
    let quantity = state
        .lines
        .get(&presentation_id)
        .map(|entry| entry.entered_quantity)
        .unwrap_or(0)
        .checked_add(amount)
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(amount_overflow)?;
    mutate(connection, state, |candidate| {
        candidate
            .lines
            .entry(presentation_id)
            .and_modify(|entry| entry.entered_quantity = quantity)
            .or_insert(CartEntry {
                presentation_id,
                entered_quantity: quantity,
                unit_price_override: None,
            });
        Ok(())
    })
}

fn mutate(
    connection: &Connection,
    state: &mut SessionCart,
    operation: impl FnOnce(&mut SessionCart) -> ServiceResult<()>,
) -> ServiceResult<Cart> {
    let mut candidate = state.clone();
    operation(&mut candidate)?;
    let cart = get(connection, &candidate)?;
    if !candidate.is_empty() && cart.lines.iter().any(|line| !line.available) {
        return Err(ServiceError::conflict(
            "INSUFFICIENT_STOCK",
            "No hay stock suficiente para la cantidad solicitada",
        ));
    }
    *state = candidate;
    Ok(cart)
}

fn discount_values(
    discount: CartDiscount,
    subtotal: i64,
) -> ServiceResult<(CartDiscountType, i64, i64)> {
    match discount {
        CartDiscount::None => Ok((CartDiscountType::None, 0, 0)),
        CartDiscount::Percentage(basis_points) => {
            let amount = divide_half_up_positive(
                i128::from(subtotal) * i128::from(basis_points),
                10_000,
                "CART_AMOUNT_OVERFLOW",
            )?;
            if amount == 0 || amount >= subtotal {
                return Err(invalid_discount());
            }
            Ok((CartDiscountType::Percentage, basis_points, amount))
        }
        CartDiscount::Fixed(amount) if amount > 0 && amount < subtotal => {
            Ok((CartDiscountType::Fixed, amount, amount))
        }
        CartDiscount::Fixed(_) => Err(invalid_discount()),
    }
}

fn parse_percentage(value: &str) -> ServiceResult<Option<i64>> {
    let value = value.trim();
    if value.is_empty() || value.starts_with(['+', '-']) {
        return Err(invalid_discount());
    }
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.is_some_and(|v| v.len() > 2 || !v.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid_discount());
    }
    let whole: i64 = whole.parse().map_err(|_| invalid_discount())?;
    let fraction = match fraction.unwrap_or_default() {
        "" => 0,
        v if v.len() == 1 => v.parse::<i64>().map_err(|_| invalid_discount())? * 10,
        v => v.parse().map_err(|_| invalid_discount())?,
    };
    let basis_points = whole
        .checked_mul(100)
        .and_then(|v| v.checked_add(fraction))
        .ok_or_else(invalid_discount)?;
    match basis_points {
        0 => Ok(None),
        1..=9999 => Ok(Some(basis_points)),
        _ => Err(invalid_discount()),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FingerprintLine {
    pub presentation_id: i64,
    pub entered_quantity: i64,
    pub mode: PresentationMode,
    pub presentation_base_quantity: i64,
    pub base_consumed: i64,
    pub catalog_unit_price: i64,
    pub applied_unit_price: i64,
    pub line_total: i64,
}

pub(crate) fn fingerprint(
    items: impl IntoIterator<Item = FingerprintLine>,
    discount_type: CartDiscountType,
    discount_value: i64,
    discount_amount: i64,
    subtotal: i64,
    total: i64,
) -> String {
    let mut canonical = String::new();
    for line in items {
        canonical.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}:{}|",
            line.presentation_id,
            line.entered_quantity,
            line.mode.as_str(),
            line.presentation_base_quantity,
            line.base_consumed,
            line.catalog_unit_price,
            line.applied_unit_price,
            line.line_total
        ));
    }
    let discount_type = match discount_type {
        CartDiscountType::None => "none",
        CartDiscountType::Percentage => "percentage",
        CartDiscountType::Fixed => "fixed",
    };
    canonical.push_str(&format!(
        "{discount_type}:{discount_value}:{discount_amount}:{subtotal}:{total}"
    ));
    format!("cart-v5:{:x}", Sha256::digest(canonical.as_bytes()))
}

fn load_presentation(
    connection: &Connection,
    presentation_id: i64,
    require_active: bool,
) -> ServiceResult<Presentation> {
    let raw = connection.query_row("SELECT pp.id,pp.name,pp.barcode,p.id,p.name,p.sku,p.base_unit,pp.mode,pp.base_quantity,pp.price,p.stock,pp.is_active FROM product_presentations pp JOIN products p ON p.id=pp.product_id WHERE pp.id=?1", [presentation_id], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?,row.get::<_,i64>(8)?,row.get::<_,i64>(9)?,row.get::<_,i64>(10)?,row.get::<_,bool>(11)?))).optional()?.ok_or_else(|| ServiceError::not_found("PRESENTATION_NOT_FOUND", "La presentacion no existe"))?;
    if require_active && !raw.11 {
        return Err(ServiceError::conflict(
            "PRESENTATION_INACTIVE",
            "La presentacion ya no esta activa",
        ));
    }
    Ok(Presentation {
        id: raw.0,
        name: raw.1,
        barcode: raw.2,
        product_id: raw.3,
        product_name: raw.4,
        sku: raw.5,
        base_unit: BaseUnit::try_from(raw.6.as_str()).map_err(|_| invalid_presentation())?,
        mode: PresentationMode::try_from(raw.7.as_str()).map_err(|_| invalid_presentation())?,
        base_quantity: raw.8,
        price: raw.9,
        stock: raw.10,
    })
}

fn invalid_quantity() -> ServiceError {
    ServiceError::validation(
        "INVALID_CART_QUANTITY",
        "Todas las cantidades deben ser mayores que cero",
    )
}
fn invalid_price() -> ServiceError {
    ServiceError::validation(
        "INVALID_CART_UNIT_PRICE",
        "El precio debe ser un importe mayor que cero con hasta dos decimales",
    )
}
fn invalid_discount() -> ServiceError {
    ServiceError::validation(
        "INVALID_CART_DISCOUNT",
        "El descuento no es valido para el subtotal del carrito",
    )
}
fn invalid_presentation() -> ServiceError {
    ServiceError::internal(
        "INVALID_PRESENTATION_DATA",
        "La presentacion contiene datos invalidos",
    )
}
fn item_not_found() -> ServiceError {
    ServiceError::not_found(
        "CART_ITEM_NOT_FOUND",
        "La presentacion no esta en el carrito",
    )
}
fn amount_overflow() -> ServiceError {
    ServiceError::validation(
        "CART_AMOUNT_OVERFLOW",
        "Los totales del carrito exceden el limite permitido",
    )
}

#[cfg(test)]
mod tests {
    use crate::db::migrations;
    use rusqlite::Connection;

    fn database() -> (Connection, i64, i64) {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch("INSERT INTO brands(name,name_search_key) VALUES('M','m'); INSERT INTO categories(name,name_search_key) VALUES('C','c'); INSERT INTO products(name,name_search_key,price,stock,sku,sku_search_key,brand_id,category_id,base_unit) VALUES('Tornillo','tornillo',100,200,'T','t',1,1,'piece'); INSERT INTO product_presentations(product_id,name,mode,base_quantity,price,is_default) VALUES(1,'Pieza','fixed',1,100,1); INSERT INTO product_presentations(product_id,name,mode,base_quantity,price) VALUES(1,'Bolsa','fixed',100,8000);").unwrap();
        (connection, 1, 2)
    }

    #[test]
    fn overrides_discounts_and_candidate_failures_are_atomic() {
        let (connection, piece, bag) = database();
        let mut state = super::SessionCart::from([(piece, 2), (bag, 1)]);
        let original_catalog = connection
            .query_row(
                "SELECT price FROM product_presentations WHERE id=?1",
                [piece],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let cart = super::set_unit_price(&connection, &mut state, piece, "0.75").unwrap();
        assert_eq!(
            (
                cart.lines[0].catalog_unit_price,
                cart.lines[0].applied_unit_price,
                cart.lines[0].price_overridden
            ),
            (100, 75, true)
        );
        let discounted =
            super::set_discount(&connection, &mut state, "percentage", "10.00").unwrap();
        assert_eq!(
            (
                discounted.subtotal,
                discounted.discount_value,
                discounted.discount_amount,
                discounted.total
            ),
            (8150, 1000, 815, 7335)
        );
        let before = state.clone();
        assert_eq!(
            super::set_discount(&connection, &mut state, "fixed", "81.50")
                .unwrap_err()
                .code,
            "INVALID_CART_DISCOUNT"
        );
        assert_eq!(state, before);
        assert_eq!(
            connection
                .query_row(
                    "SELECT price FROM product_presentations WHERE id=?1",
                    [piece],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            original_catalog
        );
        assert_ne!(discounted.cart_fingerprint, cart.cart_fingerprint);
        assert!(discounted.cart_fingerprint.starts_with("cart-v5:"));
        assert_eq!(discounted.cart_fingerprint.len(), 72);
        assert!(!discounted.cart_fingerprint.contains("8150"));
    }

    #[test]
    fn v5_fingerprint_has_a_stable_sha256_canonical() {
        let fingerprint = super::fingerprint(
            [super::FingerprintLine {
                presentation_id: 1,
                entered_quantity: 2,
                mode: crate::models::product_presentation::PresentationMode::Fixed,
                presentation_base_quantity: 1,
                base_consumed: 2,
                catalog_unit_price: 100,
                applied_unit_price: 75,
                line_total: 150,
            }],
            crate::models::cart::CartDiscountType::None,
            0,
            0,
            150,
            150,
        );

        assert_eq!(
            fingerprint,
            "cart-v5:e4fb8276090d1a105176c9ee1c786be941943f10a502a28a750c1b01f80af842"
        );
    }

    #[test]
    fn percentage_rounding_limits_zero_and_aggregate_demand_are_enforced() {
        let (connection, piece, bag) = database();
        let mut state = super::SessionCart::from([(piece, 1)]);
        super::set_unit_price(&connection, &mut state, piece, "0.50").unwrap();
        assert_eq!(
            super::set_discount(&connection, &mut state, "percentage", "1.00")
                .unwrap()
                .discount_amount,
            1
        );
        assert_eq!(
            super::set_discount(&connection, &mut state, "percentage", "0.01")
                .unwrap_err()
                .code,
            "INVALID_CART_DISCOUNT"
        );
        assert_eq!(
            super::set_discount(&connection, &mut state, "percentage", "99.99")
                .unwrap_err()
                .code,
            "INVALID_CART_DISCOUNT"
        );
        assert_eq!(
            super::set_discount(&connection, &mut state, "percentage", "100")
                .unwrap_err()
                .code,
            "INVALID_CART_DISCOUNT"
        );
        assert_eq!(
            super::set_discount(&connection, &mut state, "fixed", "0")
                .unwrap()
                .discount_type,
            crate::models::cart::CartDiscountType::None
        );
        let mut demand = super::SessionCart::from([(piece, 101)]);
        let before = demand.clone();
        assert_eq!(
            super::set_quantity(&connection, &mut demand, bag, 1)
                .unwrap_err()
                .code,
            "INSUFFICIENT_STOCK"
        );
        assert_eq!(demand, before);
    }

    #[test]
    fn reset_clear_and_price_parse_preserve_full_state() {
        let (connection, piece, _) = database();
        let mut state = super::SessionCart::from([(piece, 2)]);
        super::set_unit_price(&connection, &mut state, piece, "2.00").unwrap();
        super::set_discount(&connection, &mut state, "fixed", "1.00").unwrap();
        let before = state.clone();
        assert_eq!(
            super::set_unit_price(&connection, &mut state, piece, "0")
                .unwrap_err()
                .code,
            "INVALID_CART_UNIT_PRICE"
        );
        assert_eq!(state, before);
        let reset = super::reset_unit_price(&connection, &mut state, piece).unwrap();
        assert!(!reset.lines[0].price_overridden);
        let empty = super::clear(&connection, &mut state).unwrap();
        assert!(empty.lines.is_empty());
        assert_eq!(
            empty.discount_type,
            crate::models::cart::CartDiscountType::None
        );
    }

    #[test]
    fn fixed_discount_that_becomes_invalid_preserves_price_and_discount() {
        let (connection, piece, _) = database();
        let mut state = super::SessionCart::from([(piece, 2)]);
        super::set_unit_price(&connection, &mut state, piece, "2.00").unwrap();
        super::set_discount(&connection, &mut state, "fixed", "3.00").unwrap();
        let before = state.clone();

        assert_eq!(
            super::reset_unit_price(&connection, &mut state, piece)
                .unwrap_err()
                .code,
            "INVALID_CART_DISCOUNT"
        );
        assert_eq!(state, before);
    }

    #[test]
    fn discount_quotes_are_non_mutating_for_percentage_fixed_and_removal() {
        let (connection, piece, _) = database();
        let mut state = super::SessionCart::from([(piece, 2)]);
        super::set_discount(&connection, &mut state, "percentage", "10").unwrap();
        let before = state.clone();

        let fixed = super::quote_discount(&connection, &state, "fixed", "0.50").unwrap();
        assert_eq!(
            (
                fixed.subtotal,
                fixed.discount_type,
                fixed.discount_value,
                fixed.discount_amount,
                fixed.total
            ),
            (
                200,
                crate::models::cart::CartDiscountType::Fixed,
                50,
                50,
                150
            )
        );
        let removed = super::quote_discount(&connection, &state, "fixed", "0").unwrap();
        assert_eq!(
            removed.discount_type,
            crate::models::cart::CartDiscountType::None
        );
        assert_eq!(removed.total, 200);
        assert_eq!(state, before);
    }

    #[test]
    fn discount_quote_uses_measured_line_totals_without_mutating_state() {
        let (connection, _, _) = database();
        connection
            .execute(
                "UPDATE products SET base_unit='gram', stock=2000 WHERE id=1",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO product_presentations(product_id,name,mode,base_quantity,price) VALUES(1,'Kilogramo','measured',1000,12000)", []).unwrap();
        let measured = connection.last_insert_rowid();
        let state = super::SessionCart::from([(measured, 250)]);

        let quote = super::quote_discount(&connection, &state, "percentage", "10").unwrap();
        assert_eq!(
            (quote.subtotal, quote.discount_amount, quote.total),
            (3000, 300, 2700)
        );
        assert_eq!(state.lines[&measured].entered_quantity, 250);
        assert_eq!(state.discount, super::CartDiscount::None);
    }
}
