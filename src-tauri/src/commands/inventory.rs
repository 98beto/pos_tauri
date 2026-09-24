use tauri::State;

use crate::{
    db::Database,
    error::AppError,
    models::inventory_movement::{
        CreateInventoryMovementInput, InventoryMovement, InventoryView, InventoryViewFilters,
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

#[tauri::command]
pub fn create_inventory_movement(
    database: State<'_, Database>,
    session: State<'_, AuthSession>,
    input: CreateInventoryMovementInput,
) -> Result<InventoryMovement, AppError> {
    create_authenticated(&database, &session, input)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;

    use crate::{db::Database, models::inventory_movement::CreateInventoryMovementInput};

    #[test]
    fn create_requires_an_authenticated_session() {
        let database = Database(Mutex::new(Connection::open_in_memory().unwrap()));
        let session = super::AuthSession(Mutex::new(Default::default()));

        let error = super::create_authenticated(
            &database,
            &session,
            CreateInventoryMovementInput {
                product_id: 1,
                reason: "compra".into(),
                movement_type: "entrada".into(),
                quantity: "1".into(),
                operation_token: "command-auth-test".into(),
            },
        )
        .unwrap_err();

        assert_eq!(error.code, "SESSION_REQUIRED");
    }
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
