use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::user::{InitializeOwnerInput, LoginInput, User},
    services::auth,
};

use super::{with_connection, AuthSession};

#[tauri::command]
pub fn owner_exists(database: State<'_, Database>) -> Result<bool, AppError> {
    with_connection(&database, |connection| auth::owner_exists(connection))
}

#[tauri::command]
pub fn initialize_owner(
    database: State<'_, Database>,
    input: InitializeOwnerInput,
) -> Result<User, AppError> {
    with_connection(&database, |connection| {
        auth::initialize_owner(connection, input)
    })
}

#[tauri::command]
pub fn login(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: LoginInput,
) -> Result<User, AppError> {
    let user = with_connection(&database, |connection| auth::login(connection, input))?;
    set_current_user(&session, user.clone())?;
    Ok(user)
}

#[tauri::command]
pub fn logout(session: State<'_, AuthSession>) -> Result<(), AppError> {
    let mut state = session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo cerrar la sesion"))?;
    state.clear();
    Ok(())
}

#[tauri::command]
pub fn is_authenticated(session: State<'_, AuthSession>) -> Result<bool, AppError> {
    session
        .0
        .lock()
        .map(|state| state.current_user.is_some())
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo comprobar la sesion"))
}

#[tauri::command]
pub fn current_user(session: State<'_, AuthSession>) -> Result<User, AppError> {
    session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo comprobar la sesion"))?
        .current_user
        .clone()
        .ok_or_else(|| AppError::authentication("SESSION_REQUIRED", "Debes iniciar sesion"))
}

fn set_current_user(session: &State<'_, AuthSession>, user: User) -> Result<(), AppError> {
    let mut state = session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo actualizar la sesion"))?;
    state.set_current_user(user);
    Ok(())
}
