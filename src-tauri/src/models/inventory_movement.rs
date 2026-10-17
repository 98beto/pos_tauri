use serde::{Deserialize, Serialize};

use super::product::{BaseUnit, InventoryProductRow};
use super::product_presentation::PresentationMode;

#[derive(Clone, Debug, Serialize)]
pub struct InventoryMovement {
    pub id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub product_name_snapshot: String,
    pub sku_snapshot: String,
    pub sale_id: Option<i64>,
    pub ticket_reference: Option<String>,
    pub record_version: i64,
    pub origin: String,
    pub presentation_id: Option<i64>,
    pub presentation_name: Option<String>,
    pub presentation_barcode: Option<String>,
    pub base_unit: Option<BaseUnit>,
    pub presentation_mode: Option<String>,
    pub presentation_base_quantity: Option<i64>,
    pub unit_price_cents: Option<i64>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: i64,
    pub entered_quantity: Option<i64>,
    pub base_quantity: Option<i64>,
    pub stock_before: Option<i64>,
    pub stock_after: Option<i64>,
    pub movement_cost_cents: Option<i64>,
    pub inventory_cost_before_cents: Option<i64>,
    pub inventory_cost_after_cents: Option<i64>,
    pub supplier: Option<String>,
    pub document_reference: Option<String>,
    pub note: Option<String>,
    pub operation_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInventoryMovementInput {
    pub product_id: i64,
    #[serde(default)]
    pub presentation_id: Option<i64>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: String,
    #[serde(default)]
    pub unit_price: Option<String>,
    #[serde(default)]
    pub supplier: Option<String>,
    #[serde(default)]
    pub document_reference: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub operation_token: String,
    #[serde(default)]
    pub quote_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteInventoryMovementInput {
    pub product_id: i64,
    #[serde(default)]
    pub presentation_id: Option<i64>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: String,
    #[serde(default)]
    pub unit_price: Option<String>,
    #[serde(default)]
    pub supplier: Option<String>,
    #[serde(default)]
    pub document_reference: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InventoryMovementQuote {
    pub product_id: i64,
    pub product_name: String,
    pub product_sku: String,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub presentation_id: Option<i64>,
    pub presentation_name: Option<String>,
    pub capture_name: String,
    pub presentation_mode: Option<PresentationMode>,
    pub presentation_base_quantity: i64,
    pub entered_quantity: i64,
    pub entered_quantity_description: String,
    pub base_quantity: i64,
    pub base_quantity_description: String,
    pub unit_price_cents: Option<i64>,
    pub unit_price_description: Option<String>,
    pub total_cost_cents: Option<i64>,
    pub base_unit: BaseUnit,
    pub quote_fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryProductSearchResult {
    pub items: Vec<InventoryMovementProduct>,
    pub page: i64,
    pub total: i64,
    pub page_size: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct InventoryMovementProduct {
    pub id: i64,
    pub name: String,
    pub sku: String,
    pub stock: i64,
    pub base_unit: BaseUnit,
    pub inventory_cost_cents: Option<i64>,
    pub inventory_cost_status: &'static str,
    pub average_cost_cents: Option<i64>,
    pub presentations: Vec<InventoryMovementPresentation>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InventoryMovementPresentation {
    pub id: i64,
    pub product_id: i64,
    pub name: String,
    pub mode: PresentationMode,
    pub base_quantity: i64,
    pub is_active: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryViewFilters {
    pub product_search: Option<String>,
    pub product_page: Option<i64>,
    pub base_unit: Option<BaseUnit>,
    pub cost_status: Option<String>,
    pub movement_search: Option<String>,
    pub movement_page: Option<i64>,
    pub reason: Option<String>,
    #[serde(rename = "type")]
    pub movement_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductInventoryDetailFilters {
    pub movement_search: Option<String>,
    pub reason: Option<String>,
    #[serde(rename = "type")]
    pub movement_type: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct InventoryViewStats {
    pub product_count: i64,
    pub piece_stock: i64,
    pub gram_stock: i64,
    pub milliliter_stock: i64,
    pub centimeter_stock: i64,
    pub inventory_value_cents: i64,
    pub products_without_cost: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuotePurchaseCostCorrectionInput {
    pub movement_id: i64,
    pub new_unit_price: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPurchaseCostCorrectionInput {
    pub movement_id: i64,
    pub new_unit_price: String,
    pub quote_fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PurchaseCostCorrectionQuote {
    pub movement_id: i64,
    pub product_id: i64,
    pub product_name: String,
    pub product_sku: String,
    pub base_unit: BaseUnit,
    pub capture_name: String,
    pub presentation_id: Option<i64>,
    pub presentation_name: Option<String>,
    pub presentation_mode: Option<PresentationMode>,
    pub presentation_base_quantity: Option<i64>,
    pub entered_quantity: i64,
    pub base_quantity: i64,
    pub previous_unit_price_cents: i64,
    pub new_unit_price_cents: i64,
    pub previous_movement_cost_cents: i64,
    pub new_movement_cost_cents: i64,
    pub current_inventory_cost_cents: Option<i64>,
    pub corrected_inventory_cost_cents: Option<i64>,
    pub affected_movement_count: i64,
    pub affected_sale_count: i64,
    pub first_affected_at: String,
    pub last_affected_at: String,
    pub quote_fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryView {
    /// Legacy alias for the current movement page.
    pub items: Vec<InventoryMovement>,
    pub products: Vec<InventoryMovementProduct>,
    pub movements: Vec<InventoryMovement>,
    pub product_page: i64,
    pub product_total: i64,
    pub movement_page: i64,
    pub movement_total: i64,
    pub page_size: i64,
    pub stats: InventoryViewStats,
}

#[derive(Debug, Serialize)]
pub struct ProductInventoryDetail {
    pub product: InventoryProductRow,
    pub last_movement_at: Option<String>,
    pub movements: Vec<InventoryMovement>,
    pub page: i64,
    pub total: i64,
    pub page_size: i64,
}

#[cfg(test)]
mod tests {
    #[test]
    fn create_input_defaults_optional_v4_fields() {
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
        assert_eq!(input.presentation_id, None);
        assert_eq!(input.supplier, None);
        assert_eq!(input.quote_fingerprint, None);

        let serialized = serde_json::to_value(input).unwrap();
        assert_eq!(serialized["type"], "salida");
        assert!(serialized.get("movement_type").is_none());
    }

    #[test]
    fn inventory_view_filter_contracts_are_independent() {
        let filters: super::InventoryViewFilters = serde_json::from_value(serde_json::json!({
            "product_search": "producto",
            "product_page": 2,
            "base_unit": "gram",
            "movement_search": "factura",
            "movement_page": 3,
            "reason": "compra",
            "type": "entrada"
        }))
        .unwrap();
        assert_eq!(filters.product_search.as_deref(), Some("producto"));
        assert_eq!(filters.product_page, Some(2));
        assert_eq!(filters.movement_search.as_deref(), Some("factura"));
        assert_eq!(filters.movement_page, Some(3));

        assert!(
            serde_json::from_value::<super::InventoryViewFilters>(serde_json::json!({
                "search": "contrato anterior"
            }))
            .is_err()
        );
    }
}
