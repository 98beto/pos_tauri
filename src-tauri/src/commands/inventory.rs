use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::inventory_movement::{
        ConfirmPurchaseCostCorrectionInput, CreateInventoryMovementInput, InventoryMovement,
        InventoryMovementQuote, InventoryProductSearchResult, InventoryView, InventoryViewFilters,
        ProductInventoryDetail, ProductInventoryDetailFilters, PurchaseCostCorrectionQuote,
        QuoteInventoryMovementInput, QuotePurchaseCostCorrectionInput,
    },
    services::inventory,
};

use super::{with_authenticated_connection, AuthSession};

fn create_authenticated(
    database: &Database,
    session: &AuthSession,
    input: CreateInventoryMovementInput,
) -> Result<InventoryMovement, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::create(connection, input)
    })
}

fn detail_authenticated(
    database: &Database,
    session: &AuthSession,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> Result<ProductInventoryDetail, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::product_detail(connection, product_id, filters)
    })
}

fn view_authenticated(
    database: &Database,
    session: &AuthSession,
    filters: InventoryViewFilters,
) -> Result<InventoryView, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::view(connection, filters)
    })
}

fn quote_authenticated(
    database: &Database,
    session: &AuthSession,
    input: QuoteInventoryMovementInput,
) -> Result<InventoryMovementQuote, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::quote(connection, input)
    })
}

fn search_products_authenticated(
    database: &Database,
    session: &AuthSession,
    search: Option<String>,
    page: i64,
) -> Result<InventoryProductSearchResult, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::search_products(connection, search, page)
    })
}

fn quote_correction_authenticated(
    database: &Database,
    session: &AuthSession,
    input: QuotePurchaseCostCorrectionInput,
) -> Result<PurchaseCostCorrectionQuote, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::quote_purchase_cost_correction(connection, input)
    })
}

fn confirm_correction_authenticated(
    database: &Database,
    session: &AuthSession,
    input: ConfirmPurchaseCostCorrectionInput,
) -> Result<PurchaseCostCorrectionQuote, AppError> {
    with_authenticated_connection(database, session, |connection, _| {
        inventory::confirm_purchase_cost_correction(connection, input)
    })
}

#[tauri::command]
pub fn create_inventory_movement(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CreateInventoryMovementInput,
) -> Result<InventoryMovement, AppError> {
    create_authenticated(&database, &session, input)
}

#[tauri::command]
pub fn quote_inventory_movement(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: QuoteInventoryMovementInput,
) -> Result<InventoryMovementQuote, AppError> {
    quote_authenticated(&database, &session, input)
}

#[tauri::command]
pub fn search_inventory_products(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    search: Option<String>,
    page: i64,
) -> Result<InventoryProductSearchResult, AppError> {
    search_products_authenticated(&database, &session, search, page)
}

#[tauri::command]
pub fn quote_purchase_cost_correction(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: QuotePurchaseCostCorrectionInput,
) -> Result<PurchaseCostCorrectionQuote, AppError> {
    quote_correction_authenticated(&database, &session, input)
}

#[tauri::command]
pub fn confirm_purchase_cost_correction(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: ConfirmPurchaseCostCorrectionInput,
) -> Result<PurchaseCostCorrectionQuote, AppError> {
    confirm_correction_authenticated(&database, &session, input)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use crate::{
        db::Database,
        models::inventory_movement::{
            ConfirmPurchaseCostCorrectionInput, CreateInventoryMovementInput,
            QuoteInventoryMovementInput, QuotePurchaseCostCorrectionInput,
        },
    };

    #[test]
    fn create_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));

        let error = super::create_authenticated(
            &database,
            &session,
            CreateInventoryMovementInput {
                product_id: 1,
                presentation_id: None,
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "1".into(),
                unit_price: Some("1.00".into()),
                supplier: None,
                document_reference: None,
                note: None,
                operation_token: "command-auth-test".into(),
                quote_fingerprint: Some("inventory-quote-v3:test".into()),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn product_detail_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));

        let error =
            super::detail_authenticated(&database, &session, 1, Default::default()).unwrap_err();

        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn inventory_view_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));

        let error = super::view_authenticated(&database, &session, Default::default()).unwrap_err();

        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn quote_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));
        let error = super::quote_authenticated(
            &database,
            &session,
            QuoteInventoryMovementInput {
                product_id: 1,
                presentation_id: None,
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "1".into(),
                unit_price: Some("1.00".into()),
                supplier: Some("Proveedor".into()),
                document_reference: Some("FAC-1".into()),
                note: Some("Nota".into()),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn product_search_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));
        let error = super::search_products_authenticated(&database, &session, None, 1).unwrap_err();
        assert_eq!(error.code, "SESSION_REQUIRED");
    }

    #[test]
    fn purchase_cost_correction_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));
        assert_eq!(
            super::quote_correction_authenticated(
                &database,
                &session,
                QuotePurchaseCostCorrectionInput {
                    movement_id: 1,
                    new_unit_price: "1.00".into(),
                },
            )
            .unwrap_err()
            .code,
            "SESSION_REQUIRED"
        );
        assert_eq!(
            super::confirm_correction_authenticated(
                &database,
                &session,
                ConfirmPurchaseCostCorrectionInput {
                    movement_id: 1,
                    new_unit_price: "1.00".into(),
                    quote_fingerprint: "purchase-cost-correction-v1:test".into(),
                },
            )
            .unwrap_err()
            .code,
            "SESSION_REQUIRED"
        );
    }
}

#[tauri::command]
pub fn get_product_inventory_detail(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    product_id: i64,
    filters: ProductInventoryDetailFilters,
) -> Result<ProductInventoryDetail, AppError> {
    detail_authenticated(&database, &session, product_id, filters)
}

#[tauri::command]
pub fn get_inventory_view(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    filters: InventoryViewFilters,
) -> Result<InventoryView, AppError> {
    view_authenticated(&database, &session, filters)
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
