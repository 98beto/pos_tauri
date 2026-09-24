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
    #[test]
    fn search_keys_use_full_unicode_case_folding_and_normalization() {
        assert_eq!(super::search_key("Straße"), super::search_key("STRASSE"));
        assert_eq!(super::search_key("ς"), super::search_key("Σ"));
        assert_eq!(super::search_key("Cafe\u{301}"), super::search_key("Café"));
        assert_eq!(super::search_key("ＡＢＣ"), super::search_key("abc"));
    }
}
