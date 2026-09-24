use serde::Serialize;
use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Validation,
    Conflict,
    Authentication,
    NotFound,
    Database,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppError {
    pub category: ErrorCategory,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<&'static str>,
}

impl AppError {
    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Validation, code, message)
    }

    pub fn validation_field(
        code: &'static str,
        message: impl Into<String>,
        field: &'static str,
    ) -> Self {
        let mut error = Self::new(ErrorCategory::Validation, code, message);
        error.field = Some(field);
        error
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Conflict, code, message)
    }

    pub fn authentication(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Authentication, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::NotFound, code, message)
    }

    pub fn database() -> Self {
        Self::new(
            ErrorCategory::Database,
            "DATABASE_ERROR",
            "No se pudo completar la operacion en la base de datos",
        )
    }

    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Internal, code, message)
    }

    fn new(category: ErrorCategory, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            category,
            code,
            message: message.into(),
            field: None,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        #[cfg(debug_assertions)]
        eprintln!("SQLite error: {error}");
        #[cfg(not(debug_assertions))]
        let _ = error;
        Self::database()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn database_errors_serialize_without_internal_details() {
        let error = super::AppError::from(rusqlite::Error::InvalidQuery);
        let value = serde_json::to_value(error).unwrap();

        assert_eq!(value["category"], "database");
        assert_eq!(value["code"], "DATABASE_ERROR");
        assert!(!value.to_string().contains("InvalidQuery"));
        assert!(!value.to_string().contains("SQLite"));
    }
}
