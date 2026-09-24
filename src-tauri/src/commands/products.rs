use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::product::{
        CreateProductInput, Product, ProductSelectorFilters, ProductSelectorRow, ProductView,
        ProductViewFilters, UpdateProductInput,
    },
    services::products,
};

use super::{with_authenticated_connection, AuthSession};

#[tauri::command]
pub fn get_products_view(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    filters: ProductViewFilters,
) -> Result<ProductView, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        products::view(connection, filters)
    })
}

#[tauri::command]
pub fn list_product_selector(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    filters: ProductSelectorFilters,
) -> Result<Vec<ProductSelectorRow>, AppError> {
    let state = session
        .0
        .lock()
        .map_err(|_| AppError::internal("SESSION_LOCK_ERROR", "No se pudo acceder al carrito"))?;
    state.authenticated_user()?;
    let connection = database.0.lock().map_err(|_| {
        AppError::internal(
            "DATABASE_LOCK_ERROR",
            "No se pudo acceder a la base de datos",
        )
    })?;
    products::selector(&connection, &state.cart, filters)
}

#[tauri::command]
pub fn list_products(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<Product>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        products::list(connection)
    })
}

#[tauri::command]
pub fn find_product_by_sku(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    sku: String,
) -> Result<Option<Product>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        products::find_by_sku(connection, &sku)
    })
}

#[tauri::command]
pub fn create_product(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CreateProductInput,
) -> Result<Product, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        products::create(connection, input)
    })
}

#[tauri::command]
pub fn update_product(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    id: i64,
    input: UpdateProductInput,
) -> Result<Product, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        products::update(connection, id, input)
    })
}
