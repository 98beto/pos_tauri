use super::{with_authenticated_connection, AuthSession};
use crate::{db::Database, error::AppError, models::cash::*, services::cash};
use tauri::State;

#[tauri::command]
pub fn get_cash_state(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Option<CashSession>, AppError> {
    with_authenticated_connection(&database, &session, |c, _| {
        let tx = c.transaction()?;
        cash::state(&tx)
    })
}
#[tauri::command]
pub fn list_cash_sessions(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<CashSession>, AppError> {
    with_authenticated_connection(&database, &session, |c, _| {
        let tx = c.transaction()?;
        cash::list(&tx)
    })
}
#[tauri::command]
pub fn get_cash_session_detail(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    session_id: i64,
) -> Result<CashSessionDetail, AppError> {
    with_authenticated_connection(&database, &session, |c, _| {
        let tx = c.transaction()?;
        cash::detail(&tx, session_id)
    })
}
#[tauri::command]
pub fn open_cash_session(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: OpenCashInput,
) -> Result<CashSessionDetail, AppError> {
    with_authenticated_connection(&database, &session, |c, u| cash::open(c, u.id, input))
}
#[tauri::command]
pub fn cash_income(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CashMovementInput,
) -> Result<CashSessionDetail, AppError> {
    with_authenticated_connection(&database, &session, |c, u| {
        cash::movement(c, u.id, input, false)
    })
}
#[tauri::command]
pub fn cash_withdraw(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CashMovementInput,
) -> Result<CashSessionDetail, AppError> {
    with_authenticated_connection(&database, &session, |c, u| {
        cash::movement(c, u.id, input, true)
    })
}
#[tauri::command]
pub fn close_cash_session(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CloseCashInput,
) -> Result<CashSessionDetail, AppError> {
    with_authenticated_connection(&database, &session, |c, u| cash::close(c, u.id, input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{commands::SessionState, models::user::User};
    use std::sync::Mutex;

    #[test]
    fn cash_authentication_cannot_be_bypassed_and_logout_does_not_close_cash() {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&mut c).unwrap();
        c.execute_batch("INSERT INTO users(first_name,last_name,email,password_hash) VALUES('Ada','POS','ada@local','hash');").unwrap();
        let database = Database(Mutex::new(c));
        let auth = AuthSession(Mutex::new(SessionState::default()));
        let denied = with_authenticated_connection(
            &database,
            &auth,
            |_, _| -> crate::services::ServiceResult<()> {
                panic!("unauthenticated operation invoked")
            },
        );
        assert_eq!(denied.unwrap_err().code, "SESSION_REQUIRED");
        auth.0.lock().unwrap().set_current_user(User {
            id: 1,
            first_name: "Ada".into(),
            last_name: "POS".into(),
            email: "ada@local".into(),
            created_at: "date".into(),
            updated_at: "date".into(),
        });
        let opened = with_authenticated_connection(&database, &auth, |c, u| {
            cash::open(
                c,
                u.id,
                OpenCashInput {
                    operation_token: "auth-open".into(),
                    opening_amount: 0,
                },
            )
        })
        .unwrap();
        auth.0.lock().unwrap().clear();
        assert_eq!(
            with_authenticated_connection(&database, &auth, |c, _| cash::state(c))
                .unwrap_err()
                .code,
            "SESSION_REQUIRED"
        );
        assert_eq!(
            cash::state(&database.0.lock().unwrap()).unwrap(),
            Some(opened.session)
        );
    }
}
