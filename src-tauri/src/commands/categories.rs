use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::category::{Category, SaveCategoryInput},
    services::categories,
};

use super::{with_authenticated_connection, AuthSession};

#[tauri::command]
pub fn list_categories(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Category>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        categories::list(connection)
    })
}

#[tauri::command]
pub fn create_category(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: SaveCategoryInput,
) -> Result<Category, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        categories::create(connection, input)
    })
}

#[tauri::command]
pub fn update_category(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    id: i64,
    input: SaveCategoryInput,
) -> Result<Category, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        categories::update(connection, id, input)
    })
}
