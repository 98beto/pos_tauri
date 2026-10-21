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
    pub purchase_unit_price_cents: Option<i64>,
    pub purchase_total_cents: Option<i64>,
    pub reason: String,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub quantity: i64,
    pub entered_quantity: Option<i64>,
    pub base_quantity: Option<i64>,
    pub stock_before: Option<i64>,
    pub stock_after: Option<i64>,
    pub supplier: Option<String>,
    pub document_reference: Option<String>,
    pub note: Option<String>,
    pub operation_token: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LookupInventoryMovementInput {
    pub operation_token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuotePurchaseInput {
    pub product_id: i64,
    #[serde(default)]
    pub presentation_id: Option<i64>,
    pub quantity: String,
    pub purchase_unit_price: String,
    pub supplier: String,
    pub document_reference: String,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePurchaseInput {
    pub product_id: i64,
    #[serde(default)]
    pub presentation_id: Option<i64>,
    pub quantity: String,
    pub purchase_unit_price: String,
    pub supplier: String,
    pub document_reference: String,
    #[serde(default)]
    pub note: Option<String>,
    pub operation_token: String,
    pub quote_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteStockCountInput {
    pub product_id: i64,
    pub counted_stock: String,
    pub adjustment_reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStockCountInput {
    pub product_id: i64,
    pub counted_stock: String,
    pub adjustment_reason: String,
    pub operation_token: String,
    pub quote_fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct PurchaseQuote {
    pub product_id: i64,
    pub product_name: String,
    pub product_sku: String,
    pub presentation_id: Option<i64>,
    pub presentation_name: Option<String>,
    pub capture_name: String,
    pub presentation_mode: Option<PresentationMode>,
    pub presentation_base_quantity: i64,
    pub entered_quantity: i64,
    pub base_quantity: i64,
    pub stock_before: i64,
    pub stock_after: i64,
    pub purchase_unit_price_cents: i64,
    pub purchase_total_cents: i64,
    pub purchase_price_description: String,
    pub base_unit: BaseUnit,
    pub supplier: String,
    pub document_reference: String,
    pub note: Option<String>,
    pub quote_fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct StockCountQuote {
    pub product_id: i64,
    pub product_name: String,
    pub product_sku: String,
    pub base_unit: BaseUnit,
    pub stock_before: i64,
    pub counted_stock: i64,
    pub difference: i64,
    #[serde(rename = "type")]
    pub movement_type: String,
    pub base_quantity: i64,
    pub adjustment_reason: String,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QuotePurchasePriceCorrectionInput {
    pub movement_id: i64,
    pub new_purchase_unit_price: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmPurchasePriceCorrectionInput {
    pub movement_id: i64,
    pub new_purchase_unit_price: String,
    pub quote_fingerprint: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PurchasePriceCorrectionQuote {
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
    pub previous_purchase_unit_price_cents: i64,
    pub new_purchase_unit_price_cents: i64,
    pub previous_purchase_total_cents: i64,
    pub new_purchase_total_cents: i64,
    pub quote_fingerprint: String,
}

#[derive(Debug, Serialize)]
pub struct InventoryView {
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
    fn movement_lookup_accepts_only_an_operation_token() {
        assert!(
            serde_json::from_value::<super::LookupInventoryMovementInput>(
                serde_json::json!({ "operation_token": "lost-response" })
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<super::LookupInventoryMovementInput>(serde_json::json!({
                "operation_token": "lost-response",
                "quote_fingerprint": "legacy-fingerprint"
            }))
            .is_err()
        );
    }

    #[test]
    fn purchase_and_stock_count_contracts_reject_fields_from_the_other_operation() {
        assert!(
            serde_json::from_value::<super::QuotePurchaseInput>(serde_json::json!({
                "product_id": 1,
                "quantity": "1",
                "purchase_unit_price": "1.00",
                "supplier": "Proveedor",
                "document_reference": "FAC-1",
                "counted_stock": "3"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<super::QuoteStockCountInput>(serde_json::json!({
                "product_id": 1,
                "counted_stock": "3",
                "adjustment_reason": "Conteo mensual",
                "supplier": "Proveedor"
            }))
            .is_err()
        );
    }

    #[test]
    fn movement_filter_rejects_retired_cost_status() {
        assert!(
            serde_json::from_value::<super::InventoryViewFilters>(serde_json::json!({
                "cost_status": "known"
            }))
            .is_err()
        );
    }
}
