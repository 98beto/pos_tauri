use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::sale::{Sale, SaleHistoryDetail, SalesView, SalesViewFilters},
    services::sales,
};

use super::{with_authenticated_connection, AuthSession};

#[tauri::command]
pub fn get_sales_view(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    filters: SalesViewFilters,
) -> Result<SalesView, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        sales::view(connection, filters)
    })
}

#[tauri::command]
pub fn get_sale_history_detail(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    sale_id: i64,
) -> Result<SaleHistoryDetail, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        sales::history_detail(connection, sale_id)
    })
}

#[tauri::command]
pub fn list_sales(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Sale>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| sales::list(connection))
}
