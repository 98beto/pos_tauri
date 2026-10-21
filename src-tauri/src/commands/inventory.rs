use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::inventory_movement::{
        ConfirmPurchasePriceCorrectionInput, CreatePurchaseInput, CreateStockCountInput,
        InventoryMovement, InventoryProductSearchResult, InventoryView, InventoryViewFilters,
        LookupInventoryMovementInput, ProductInventoryDetail, ProductInventoryDetailFilters,
        PurchasePriceCorrectionQuote, PurchaseQuote, QuotePurchaseInput,
        QuotePurchasePriceCorrectionInput, QuoteStockCountInput, StockCountQuote,
    },
    services::inventory,
};

use super::{with_authenticated_connection, AuthSession};

#[tauri::command]
pub fn lookup_inventory_movement(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: LookupInventoryMovementInput,
) -> Result<Option<InventoryMovement>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::lookup_movement(connection, input)
    })
}

#[tauri::command]
pub fn quote_purchase(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: QuotePurchaseInput,
) -> Result<PurchaseQuote, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::quote_purchase(connection, input)
    })
}

#[tauri::command]
pub fn create_purchase(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CreatePurchaseInput,
) -> Result<InventoryMovement, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::create_purchase(connection, input)
    })
}

#[tauri::command]
pub fn quote_stock_count(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: QuoteStockCountInput,
) -> Result<StockCountQuote, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::quote_stock_count(connection, input)
    })
}

#[tauri::command]
pub fn create_stock_count(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CreateStockCountInput,
) -> Result<InventoryMovement, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::create_stock_count(connection, input)
    })
}

#[tauri::command]
pub fn quote_purchase_price_correction(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: QuotePurchasePriceCorrectionInput,
) -> Result<PurchasePriceCorrectionQuote, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::quote_purchase_price_correction(connection, input)
    })
}

#[tauri::command]
pub fn confirm_purchase_price_correction(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: ConfirmPurchasePriceCorrectionInput,
) -> Result<PurchasePriceCorrectionQuote, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::confirm_purchase_price_correction(connection, input)
    })
}

#[tauri::command]
pub fn search_inventory_products(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    search: Option<String>,
    page: i64,
) -> Result<InventoryProductSearchResult, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::search_products(connection, search, page)
    })
}

#[tauri::command]
pub fn get_product_inventory_detail(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> Result<ProductInventoryDetail, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::product_detail(connection, product_id, filters)
    })
}

#[tauri::command]
pub fn get_inventory_view(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    filters: InventoryViewFilters,
) -> Result<InventoryView, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::view(connection, filters)
    })
}

#[tauri::command]
pub fn list_inventory_movements(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
) -> Result<Vec<InventoryMovement>, AppError> {
    with_authenticated_connection(&database, &session, |connection, _| {
        inventory::list(connection)
    })
}

#[cfg(test)]
mod tests {
    use crate::models::inventory_movement::{QuotePurchaseInput, QuoteStockCountInput};

    #[test]
    fn command_inputs_keep_purchase_and_count_contracts_separate() {
        let purchase = serde_json::json!({
            "product_id": 1,
            "quantity": "1",
            "purchase_unit_price": "1.00",
            "supplier": "Proveedor",
            "document_reference": "FAC-1"
        });
        assert!(serde_json::from_value::<QuotePurchaseInput>(purchase.clone()).is_ok());
        assert!(serde_json::from_value::<QuoteStockCountInput>(purchase).is_err());
    }
}
