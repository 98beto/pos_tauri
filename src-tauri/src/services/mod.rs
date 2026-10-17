pub mod auth;
pub mod brands;
pub mod cart;
pub mod categories;
pub mod inventory;
pub mod products;
pub mod sales;

use std::collections::BTreeSet;
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;

pub use crate::error::AppError as ServiceError;

pub type ServiceResult<T> = Result<T, ServiceError>;

pub const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
pub const SEARCH_MAX_LENGTH: usize = 100;

pub fn parse_money_text(value: &str) -> ServiceResult<i64> {
    parse_money_text_allow_zero(value, false)
}

pub fn parse_money_text_allow_zero(value: &str, allow_zero: bool) -> ServiceResult<i64> {
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
    let whole = whole.parse::<i128>().map_err(|_| invalid_money())?;
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i128>().map_err(|_| invalid_money())? * 10,
        _ => fraction.parse::<i128>().map_err(|_| invalid_money())?,
    };
    let cents = whole
        .checked_mul(100)
        .and_then(|amount| amount.checked_add(fraction))
        .filter(|amount| *amount <= i128::from(JS_MAX_SAFE_INTEGER))
        .ok_or_else(invalid_money)?;
    if cents == 0 && !allow_zero {
        return Err(invalid_money());
    }
    i64::try_from(cents).map_err(|_| invalid_money())
}

fn invalid_money() -> ServiceError {
    ServiceError::validation(
        "INVALID_MONEY",
        "El importe debe ser un decimal valido con hasta dos decimales",
    )
}

pub fn checked_multiply(left: i64, right: i64, code: &'static str) -> ServiceResult<i64> {
    if left < 0 || right < 0 {
        return Err(ServiceError::validation(
            code,
            "Los valores no pueden ser negativos",
        ));
    }
    let result = i128::from(left) * i128::from(right);
    i64::try_from(result)
        .ok()
        .filter(|value| *value <= JS_MAX_SAFE_INTEGER)
        .ok_or_else(|| ServiceError::validation(code, "El resultado excede el limite permitido"))
}

pub fn divide_half_up_positive(
    numerator: i128,
    denominator: i128,
    code: &'static str,
) -> ServiceResult<i64> {
    if numerator < 0 || denominator <= 0 {
        return Err(ServiceError::validation(
            code,
            "La division requiere valores positivos",
        ));
    }
    let rounded = numerator
        .checked_add(denominator / 2)
        .map(|value| value / denominator)
        .filter(|value| *value <= i128::from(JS_MAX_SAFE_INTEGER))
        .ok_or_else(|| ServiceError::validation(code, "El resultado excede el limite permitido"))?;
    i64::try_from(rounded)
        .map_err(|_| ServiceError::validation(code, "El resultado excede el limite permitido"))
}

#[allow(dead_code)] // Shared commercial primitive retained for checkout and correction replay.
pub fn commercial_value(
    base_quantity: i64,
    price_cents: i64,
    price_base_quantity: i64,
    code: &'static str,
) -> ServiceResult<i64> {
    if base_quantity < 0 || price_cents < 0 || price_base_quantity <= 0 {
        return Err(ServiceError::validation(
            code,
            "La cantidad y el precio no pueden ser negativos y la base debe ser positiva",
        ));
    }
    divide_half_up_positive(
        i128::from(base_quantity) * i128::from(price_cents),
        i128::from(price_base_quantity),
        code,
    )
}

#[allow(dead_code)] // Shared production primitive for the later inventory-cost integration block.
pub fn fixed_entry_movement_cost(
    entered_quantity: i64,
    purchase_price_cents: i64,
) -> ServiceResult<i64> {
    if entered_quantity <= 0 || purchase_price_cents <= 0 {
        return Err(ServiceError::validation(
            "INVALID_PURCHASE_COST",
            "La cantidad y el precio de compra deben ser positivos",
        ));
    }
    checked_multiply(
        entered_quantity,
        purchase_price_cents,
        "PURCHASE_COST_OVERFLOW",
    )
}

#[allow(dead_code)] // Shared production primitive for the later inventory-cost integration block.
pub fn measured_entry_movement_cost(
    entered_quantity: i64,
    purchase_price_cents: i64,
    presentation_base_quantity: i64,
) -> ServiceResult<i64> {
    if entered_quantity <= 0 || purchase_price_cents <= 0 || presentation_base_quantity <= 0 {
        return Err(ServiceError::validation(
            "INVALID_PURCHASE_COST",
            "La cantidad, el precio de compra y la base deben ser positivos",
        ));
    }
    divide_half_up_positive(
        i128::from(entered_quantity) * i128::from(purchase_price_cents),
        i128::from(presentation_base_quantity),
        "PURCHASE_COST_OVERFLOW",
    )
}

#[allow(dead_code)] // Shared production primitive for the later inventory-cost integration block.
pub fn direct_entry_movement_cost(
    base_quantity: i64,
    purchase_price_per_base_cents: i64,
) -> ServiceResult<i64> {
    fixed_entry_movement_cost(base_quantity, purchase_price_per_base_cents)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Shared production result for the later checkout/replay blocks.
pub struct CostWithdrawal {
    pub movement_cost_cents: Option<i64>,
    pub inventory_cost_after_cents: Option<i64>,
}

#[allow(dead_code)] // Shared production primitive for the later checkout/replay blocks.
pub fn proportional_cost_withdrawal(
    inventory_cost_before_cents: Option<i64>,
    stock_before: i64,
    withdrawn_base_quantity: i64,
) -> ServiceResult<CostWithdrawal> {
    if stock_before <= 0
        || withdrawn_base_quantity <= 0
        || withdrawn_base_quantity > stock_before
        || inventory_cost_before_cents
            .is_some_and(|cost| !(0..=JS_MAX_SAFE_INTEGER).contains(&cost))
    {
        return Err(ServiceError::validation(
            "INVALID_COST_WITHDRAWAL",
            "El saldo y la cantidad retirada no son validos",
        ));
    }
    let stock_after = stock_before - withdrawn_base_quantity;
    match inventory_cost_before_cents {
        None => Ok(CostWithdrawal {
            movement_cost_cents: None,
            inventory_cost_after_cents: (stock_after == 0).then_some(0),
        }),
        Some(cost) => {
            let removed = if stock_after == 0 {
                cost
            } else {
                divide_half_up_positive(
                    i128::from(cost) * i128::from(withdrawn_base_quantity),
                    i128::from(stock_before),
                    "COST_WITHDRAWAL_OVERFLOW",
                )?
            };
            Ok(CostWithdrawal {
                movement_cost_cents: Some(removed),
                inventory_cost_after_cents: Some(cost - removed),
            })
        }
    }
}

#[allow(dead_code)] // Compatibility inside the cost domain, not an IPC compatibility field.
pub fn proportional_cost_removal(
    inventory_cost_before_cents: i64,
    withdrawn_base_quantity: i64,
    stock_before: i64,
) -> ServiceResult<i64> {
    proportional_cost_withdrawal(
        Some(inventory_cost_before_cents),
        stock_before,
        withdrawn_base_quantity,
    )?
    .movement_cost_cents
    .ok_or_else(|| ServiceError::internal("UNKNOWN_COST", "El costo retirado es desconocido"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Shared production input for the later checkout/replay blocks.
pub struct CostAllocationLine {
    pub stable_id: i64,
    pub base_quantity: i64,
}

#[allow(dead_code)] // Shared production primitive for the later checkout/replay blocks.
pub fn allocate_grouped_cost(
    total_cost_cents: i64,
    lines: &[CostAllocationLine],
) -> ServiceResult<Vec<i64>> {
    let mut stable_ids = BTreeSet::new();
    if !(0..=JS_MAX_SAFE_INTEGER).contains(&total_cost_cents)
        || lines.is_empty()
        || lines.iter().any(|line| {
            line.stable_id <= 0
                || line.stable_id > JS_MAX_SAFE_INTEGER
                || line.base_quantity <= 0
                || line.base_quantity > JS_MAX_SAFE_INTEGER
                || !stable_ids.insert(line.stable_id)
        })
    {
        return Err(ServiceError::validation(
            "INVALID_COST_ALLOCATION",
            "El costo y las lineas a distribuir no son validos",
        ));
    }
    let total_base = lines
        .iter()
        .try_fold(0_i128, |total, line| {
            total.checked_add(i128::from(line.base_quantity))
        })
        .ok_or_else(|| {
            ServiceError::validation(
                "COST_ALLOCATION_OVERFLOW",
                "La distribucion excede el limite permitido",
            )
        })?;
    if total_base > i128::from(JS_MAX_SAFE_INTEGER) {
        return Err(ServiceError::validation(
            "COST_ALLOCATION_OVERFLOW",
            "La distribucion excede el limite permitido",
        ));
    }
    let mut allocated = 0_i64;
    let mut result = Vec::with_capacity(lines.len());
    let mut remainders = Vec::with_capacity(lines.len());
    for (position, line) in lines.iter().enumerate() {
        let numerator = i128::from(total_cost_cents) * i128::from(line.base_quantity);
        let floor = i64::try_from(numerator / total_base).map_err(|_| {
            ServiceError::validation(
                "COST_ALLOCATION_OVERFLOW",
                "La distribucion excede el limite permitido",
            )
        })?;
        allocated = checked_total(allocated, floor, "COST_ALLOCATION_OVERFLOW")?;
        result.push(floor);
        remainders.push((numerator % total_base, line.stable_id, position));
    }
    remainders.sort_by_key(|(remainder, stable_id, _)| (std::cmp::Reverse(*remainder), *stable_id));
    for &(_, _, position) in remainders
        .iter()
        .take(usize::try_from(total_cost_cents - allocated).unwrap_or(usize::MAX))
    {
        result[position] += 1;
    }
    Ok(result)
}

pub fn validate_presentation_mode(
    base_unit: crate::models::product::BaseUnit,
    mode: crate::models::product_presentation::PresentationMode,
) -> ServiceResult<()> {
    use crate::models::{product::BaseUnit, product_presentation::PresentationMode};

    let valid = matches!(
        (base_unit, mode),
        (BaseUnit::Piece, PresentationMode::Fixed)
            | (BaseUnit::Gram, PresentationMode::Measured)
            | (BaseUnit::Milliliter, PresentationMode::Measured)
            | (BaseUnit::Centimeter, PresentationMode::Measured)
    );
    if valid {
        Ok(())
    } else {
        Err(ServiceError::validation(
            "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT",
            "Pieza requiere una presentacion fija y las unidades medidas requieren una presentacion medida",
        ))
    }
}

#[allow(dead_code)] // Shared now; consumed by cart/checkout in the next integration block.
pub fn presentation_base_quantity(
    mode: crate::models::product_presentation::PresentationMode,
    entered_quantity: i64,
    base_quantity: i64,
) -> ServiceResult<i64> {
    if entered_quantity <= 0 || base_quantity <= 0 {
        return Err(ServiceError::validation(
            "INVALID_QUANTITY",
            "La cantidad debe ser positiva",
        ));
    }
    match mode {
        crate::models::product_presentation::PresentationMode::Fixed => {
            checked_multiply(entered_quantity, base_quantity, "QUANTITY_OVERFLOW")
        }
        crate::models::product_presentation::PresentationMode::Measured => Ok(entered_quantity),
    }
}

#[allow(dead_code)] // Shared now; consumed by cart/checkout in the next integration block.
pub fn line_total(
    mode: crate::models::product_presentation::PresentationMode,
    entered_quantity: i64,
    base_quantity: i64,
    price_cents: i64,
) -> ServiceResult<i64> {
    if entered_quantity <= 0 || base_quantity <= 0 || price_cents <= 0 {
        return Err(ServiceError::validation(
            "INVALID_LINE",
            "La cantidad, base y precio deben ser positivos",
        ));
    }
    let total = match mode {
        crate::models::product_presentation::PresentationMode::Fixed => {
            checked_multiply(entered_quantity, price_cents, "LINE_TOTAL_OVERFLOW")?
        }
        crate::models::product_presentation::PresentationMode::Measured => divide_half_up_positive(
            i128::from(entered_quantity) * i128::from(price_cents),
            i128::from(base_quantity),
            "LINE_TOTAL_OVERFLOW",
        )?,
    };
    if total == 0 {
        return Err(ServiceError::validation(
            "ZERO_LINE_TOTAL",
            "El total de la linea no puede redondear a cero",
        ));
    }
    Ok(total)
}

pub fn validate_id(id: i64) -> ServiceResult<()> {
    if !(1..=JS_MAX_SAFE_INTEGER).contains(&id) {
        return Err(ServiceError::validation(
            "INVALID_ID",
            "El identificador esta fuera del rango permitido",
        ));
    }
    Ok(())
}

pub fn normalize_search(search: Option<String>) -> ServiceResult<String> {
    let search = search_key(&search.unwrap_or_default());
    if search.chars().count() > SEARCH_MAX_LENGTH {
        return Err(ServiceError::validation(
            "SEARCH_TOO_LONG",
            "La busqueda debe tener como maximo 100 caracteres",
        ));
    }
    Ok(search)
}

pub fn normalize_persisted(value: &str) -> String {
    value.trim().nfc().collect()
}

pub fn search_key(value: &str) -> String {
    value.trim().nfkc().case_fold().nfkc().collect()
}

pub fn normalize_uppercase(value: &str) -> String {
    normalize_persisted(value)
        .chars()
        .flat_map(char::to_uppercase)
        .collect::<String>()
        .nfc()
        .collect()
}

pub fn checked_total(current: i64, value: i64, code: &'static str) -> ServiceResult<i64> {
    current
        .checked_add(value)
        .filter(|total| (0..=JS_MAX_SAFE_INTEGER).contains(total))
        .ok_or_else(|| {
            ServiceError::validation(
                code,
                "El total excede el limite permitido por la aplicacion",
            )
        })
}

#[cfg(test)]
mod tests {
    use crate::models::product_presentation::PresentationMode;

    #[test]
    fn search_keys_use_full_unicode_case_folding_and_normalization() {
        assert_eq!(super::search_key("Straße"), super::search_key("STRASSE"));
        assert_eq!(super::search_key("ς"), super::search_key("Σ"));
        assert_eq!(super::search_key("Cafe\u{301}"), super::search_key("Café"));
        assert_eq!(super::search_key("ＡＢＣ"), super::search_key("abc"));
    }

    #[test]
    fn money_and_checked_arithmetic_enforce_safe_limits() {
        assert_eq!(super::parse_money_text("12.3").unwrap(), 1230);
        assert_eq!(
            super::parse_money_text("90071992547409.91").unwrap(),
            super::JS_MAX_SAFE_INTEGER
        );
        for value in [
            "",
            "0",
            "1.",
            ".1",
            "-1",
            "+1",
            "1.001",
            "90071992547409.92",
        ] {
            assert_eq!(
                super::parse_money_text(value).unwrap_err().code,
                "INVALID_MONEY"
            );
        }
        assert_eq!(super::checked_multiply(3, 4, "X").unwrap(), 12);
        assert_eq!(
            super::checked_multiply(super::JS_MAX_SAFE_INTEGER, 2, "X")
                .unwrap_err()
                .code,
            "X"
        );
    }

    #[test]
    fn presentation_conversion_and_totals_round_half_up_and_reject_zero() {
        assert_eq!(
            super::presentation_base_quantity(PresentationMode::Fixed, 2, 100).unwrap(),
            200
        );
        assert_eq!(
            super::presentation_base_quantity(PresentationMode::Measured, 250, 1000).unwrap(),
            250
        );
        assert_eq!(
            super::line_total(PresentationMode::Measured, 250, 1000, 12_000).unwrap(),
            3_000
        );
        assert_eq!(
            super::line_total(PresentationMode::Measured, 1, 2, 1).unwrap(),
            1
        );
        assert_eq!(
            super::line_total(PresentationMode::Measured, 1, 3, 1)
                .unwrap_err()
                .code,
            "ZERO_LINE_TOTAL"
        );
    }

    #[test]
    fn commercial_values_round_half_up_and_may_be_zero() {
        assert_eq!(
            super::commercial_value(250, 12_000, 1_000, "X").unwrap(),
            3_000
        );
        assert_eq!(
            super::commercial_value(500, 4_000, 1_000, "X").unwrap(),
            2_000
        );
        assert_eq!(super::commercial_value(75, 2_500, 100, "X").unwrap(), 1_875);
        assert_eq!(super::commercial_value(50, 1_000, 100, "X").unwrap(), 500);
        assert_eq!(super::commercial_value(10, 2_500, 25, "X").unwrap(), 1_000);
        assert_eq!(super::commercial_value(1, 1, 3, "X").unwrap(), 0);
        assert_eq!(super::commercial_value(1, 1, 2, "X").unwrap(), 1);
        assert_eq!(
            super::commercial_value(super::JS_MAX_SAFE_INTEGER, 2, 1, "X")
                .unwrap_err()
                .code,
            "X"
        );
    }

    #[test]
    fn validates_the_mode_matrix_for_every_base_unit() {
        use crate::models::product::BaseUnit;

        for (unit, valid_mode, invalid_mode) in [
            (
                BaseUnit::Piece,
                PresentationMode::Fixed,
                PresentationMode::Measured,
            ),
            (
                BaseUnit::Gram,
                PresentationMode::Measured,
                PresentationMode::Fixed,
            ),
            (
                BaseUnit::Milliliter,
                PresentationMode::Measured,
                PresentationMode::Fixed,
            ),
            (
                BaseUnit::Centimeter,
                PresentationMode::Measured,
                PresentationMode::Fixed,
            ),
        ] {
            assert!(super::validate_presentation_mode(unit, valid_mode).is_ok());
            assert_eq!(
                super::validate_presentation_mode(unit, invalid_mode)
                    .unwrap_err()
                    .code,
                "INVALID_PRESENTATION_MODE_FOR_BASE_UNIT"
            );
        }
        assert_eq!(BaseUnit::Gram.standard_base_quantity(), Some(1_000));
        assert_eq!(BaseUnit::Milliliter.standard_base_quantity(), Some(1_000));
        assert_eq!(BaseUnit::Centimeter.standard_base_quantity(), Some(100));
        assert_eq!(BaseUnit::Piece.standard_base_quantity(), None);
    }

    #[test]
    fn entry_cost_helpers_cover_fixed_measured_and_direct_capture() {
        assert_eq!(super::fixed_entry_movement_cost(3, 250).unwrap(), 750);
        assert_eq!(
            super::measured_entry_movement_cost(250, 12_000, 1_000).unwrap(),
            3_000
        );
        assert_eq!(super::measured_entry_movement_cost(1, 1, 2).unwrap(), 1);
        assert_eq!(super::direct_entry_movement_cost(75, 10).unwrap(), 750);
        assert_eq!(
            super::direct_entry_movement_cost(super::JS_MAX_SAFE_INTEGER, 2)
                .unwrap_err()
                .code,
            "PURCHASE_COST_OVERFLOW"
        );
    }

    #[test]
    fn withdrawals_preserve_unknown_cost_and_zero_on_depletion() {
        assert_eq!(
            super::proportional_cost_withdrawal(Some(1_001), 250, 103).unwrap(),
            super::CostWithdrawal {
                movement_cost_cents: Some(412),
                inventory_cost_after_cents: Some(589),
            }
        );
        assert_eq!(
            super::proportional_cost_withdrawal(None, 4, 1).unwrap(),
            super::CostWithdrawal {
                movement_cost_cents: None,
                inventory_cost_after_cents: None,
            }
        );
        assert_eq!(
            super::proportional_cost_withdrawal(None, 3, 3).unwrap(),
            super::CostWithdrawal {
                movement_cost_cents: None,
                inventory_cost_after_cents: Some(0),
            }
        );
    }

    #[test]
    fn grouped_cost_allocation_is_exact_and_deterministic() {
        let lines = [
            super::CostAllocationLine {
                stable_id: 2,
                base_quantity: 1,
            },
            super::CostAllocationLine {
                stable_id: 1,
                base_quantity: 1,
            },
            super::CostAllocationLine {
                stable_id: 3,
                base_quantity: 2,
            },
        ];
        let allocated = super::allocate_grouped_cost(3, &lines).unwrap();
        assert_eq!(allocated, vec![1, 1, 1]);
        assert_eq!(allocated.iter().sum::<i64>(), 3);
        assert_eq!(
            super::allocate_grouped_cost(1, &lines[..2]).unwrap(),
            vec![0, 1]
        );
        assert_eq!(
            super::allocate_grouped_cost(
                1,
                &[
                    super::CostAllocationLine {
                        stable_id: 1,
                        base_quantity: 1
                    },
                    super::CostAllocationLine {
                        stable_id: 1,
                        base_quantity: 1
                    },
                ],
            )
            .unwrap_err()
            .code,
            "INVALID_COST_ALLOCATION"
        );
    }
}
