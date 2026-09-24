pub mod auth;
pub mod brands;
pub mod cart;
pub mod categories;
pub mod inventory;
pub mod products;
pub mod sales;

use crate::{
    db::Database,
    error::AppError,
    models::user::User,
    services::{ServiceError, ServiceResult},
};
use rusqlite::Connection;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub struct SessionState {
    pub(super) current_user: Option<User>,
    pub(super) cart: BTreeMap<i64, i64>,
}

impl SessionState {
    fn clear(&mut self) {
        // Keep logout cleanup centralized as more per-session state is added.
        self.current_user = None;
        self.cart.clear();
    }

    fn set_current_user(&mut self, user: User) {
        self.clear();
        self.current_user = Some(user);
    }

    fn authenticated_user(&self) -> Result<&User, AppError> {
        self.current_user
            .as_ref()
            .ok_or_else(|| AppError::authentication("SESSION_REQUIRED", "Debes iniciar sesion"))
    }
}

pub struct AuthSession(pub Mutex<SessionState>);

fn with_connection<T>(
    database: &Database,
    operation: impl FnOnce(&mut Connection) -> ServiceResult<T>,
) -> Result<T, AppError> {
    let mut connection = database.0.lock().map_err(|_| {
        AppError::internal(
            "DATABASE_LOCK_ERROR",
            "No se pudo acceder a la base de datos",
        )
    })?;
    operation(&mut connection).map_err(|error: ServiceError| error)
}

fn with_authenticated_connection<T>(
    database: &Database,
    session: &AuthSession,
    operation: impl FnOnce(&mut Connection, &User) -> ServiceResult<T>,
) -> Result<T, AppError> {
    let state = session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo comprobar la sesion"))?;
    let user = state.authenticated_user()?.clone();
    drop(state);
    with_connection(database, |connection| operation(connection, &user))
}

#[cfg(test)]
mod tests {
    use crate::models::user::User;

    #[test]
    fn clearing_session_removes_authenticated_user() {
        let mut state = super::SessionState {
            current_user: Some(User {
                id: 1,
                first_name: "Admin".into(),
                last_name: "POS".into(),
                email: "admin@example.com".into(),
                created_at: "2026-01-01".into(),
                updated_at: "2026-01-01".into(),
            }),
            cart: [(7, 2)].into(),
        };

        state.clear();

        assert!(state.current_user.is_none());
        assert!(state.cart.is_empty());
    }

    #[test]
    fn unauthenticated_state_returns_session_required() {
        let state = super::SessionState::default();

        let error = state.authenticated_user().unwrap_err();

        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn login_and_logout_replace_user_and_clear_cart() {
        let mut state = super::SessionState {
            current_user: None,
            cart: [(7, 2)].into(),
        };
        let user = User {
            id: 1,
            first_name: "Admin".into(),
            last_name: "POS".into(),
            email: "admin@example.com".into(),
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
        };

        state.set_current_user(user.clone());
        assert_eq!(state.authenticated_user().unwrap(), &user);
        assert!(state.cart.is_empty());

        state.cart.insert(9, 1);
        state.clear();
        assert_eq!(
            state.authenticated_user().unwrap_err().code,
            "SESSION_REQUIRED"
        );
        assert!(state.cart.is_empty());
    }
}
