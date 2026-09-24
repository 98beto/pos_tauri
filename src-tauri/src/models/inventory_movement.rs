use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct InventoryMovement {
    pub id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub sale_id: Option<i64>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateInventoryMovementInput {
    pub product_id: i64,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: String,
    pub operation_token: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct InventoryViewFilters {
    pub search: Option<String>,
    pub reason: Option<String>,
    #[serde(rename = "type")]
    pub movement_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InventoryViewRow {
    pub id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub sale_id: Option<i64>,
    pub ticket_reference: Option<String>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryViewStats {
    pub global_total_entries: i64,
    pub global_total_exits: i64,
    pub global_current_stock: i64,
    pub global_out_of_stock_products: i64,
}

#[derive(Debug, Serialize)]
pub struct InventoryView {
    pub items: Vec<InventoryViewRow>,
    pub stats: InventoryViewStats,
}

#[cfg(test)]
mod tests {
    #[test]
    fn create_input_uses_type_at_the_ipc_boundary() {
        let input: super::CreateInventoryMovementInput =
            serde_json::from_value(serde_json::json!({
                "product_id": 7,
                "reason": "ajuste",
                "type": "salida",
                "quantity": "2",
                "operation_token": "550e8400-e29b-41d4-a716-446655440000"
            }))
            .unwrap();
        assert_eq!(input.movement_type, "salida");
        assert_eq!(input.quantity, "2");

        let serialized = serde_json::to_value(input).unwrap();
        assert_eq!(serialized["type"], "salida");
        assert!(serialized.get("movement_type").is_none());
    }
}
