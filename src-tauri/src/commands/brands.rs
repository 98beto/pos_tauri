use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::brand::{Brand, SaveBrandInput},
    services::brands,
};

use super::{with_authenticated_connection, AuthSession};

#[tauri::command]
pub fn list_brands(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Brand>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        brands::list(connection)
    })
}

#[tauri::command]
pub fn create_brand(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: SaveBrandInput,
) -> Result<Brand, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        brands::create(connection, input)
    })
}

#[tauri::command]
pub fn update_brand(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    id: i64,
    input: SaveBrandInput,
) -> Result<Brand, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        brands::update(connection, id, input)
    })
}
