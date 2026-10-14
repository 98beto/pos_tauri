pub mod auth;
pub mod brands;
pub mod cart;
pub mod categories;
pub mod inventory;
pub mod products;
pub mod sales;

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

#[allow(dead_code)] // Shared now; consumed by inventory/checkout in the next integration block.
pub fn proportional_cost_removal(
    cost_before: i64,
    outgoing_base_quantity: i64,
    stock_before: i64,
) -> ServiceResult<i64> {
    if cost_before < 0
        || outgoing_base_quantity <= 0
        || stock_before <= 0
        || outgoing_base_quantity > stock_before
    {
        return Err(ServiceError::validation(
            "INVALID_COST_REMOVAL",
            "No se puede retirar el costo solicitado",
        ));
    }
    if outgoing_base_quantity == stock_before {
        return Ok(cost_before);
    }
    divide_half_up_positive(
        i128::from(cost_before) * i128::from(outgoing_base_quantity),
        i128::from(stock_before),
        "COST_OVERFLOW",
    )
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
    fn proportional_cost_rounds_and_exhaustion_clears_all_cost() {
        assert_eq!(super::proportional_cost_removal(101, 1, 2).unwrap(), 51);
        assert_eq!(super::proportional_cost_removal(101, 2, 2).unwrap(), 101);
        assert_eq!(super::proportional_cost_removal(0, 1, 2).unwrap(), 0);
        assert_eq!(
            super::proportional_cost_removal(1, 0, 2).unwrap_err().code,
            "INVALID_COST_REMOVAL"
        );
    }
}
