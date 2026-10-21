use serde::{Deserialize, Serialize};

use super::cart::CartDiscountType;
use super::sale_detail::SaleDetail;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sale {
    pub id: i64,
    pub user_id: i64,
    pub cashier_name: String,
    pub cashier_email: String,
    pub checkout_token: String,
    pub cart_fingerprint: String,
    pub sale_date: String,
    pub subtotal: i64,
    pub discount_type: CartDiscountType,
    pub discount_value: i64,
    pub discount_amount: i64,
    pub total: i64,
    pub payment_method: String,
    pub cash_received: Option<i64>,
    pub change_amount: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleReceipt {
    pub sale: Sale,
    pub details: Vec<SaleDetail>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SalesViewFilters {
    pub search: Option<String>,
    pub payment_method: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SalesViewRow {
    pub id: i64,
    pub ticket_number: String,
    pub sale_date: String,
    pub payment_method: String,
    pub total: i64,
    pub line_count: i64,
    pub user_id: i64,
    pub user_name: String,
}

#[derive(Debug, Serialize)]
pub struct SalesViewStats {
    pub sale_count: i64,
    pub total_sold: i64,
    pub average_sale: i64,
    pub lines_sold: i64,
}

#[derive(Debug, Serialize)]
pub struct SalesView {
    pub items: Vec<SalesViewRow>,
    pub stats: SalesViewStats,
}

#[derive(Debug, Serialize)]
pub struct SaleHistoryLine {
    pub product_id: i64,
    pub presentation_id: Option<i64>,
    pub product_name: String,
    pub sku: String,
    pub presentation_name: Option<String>,
    pub presentation_barcode: Option<String>,
    pub base_unit: Option<String>,
    pub presentation_mode: Option<String>,
    pub presentation_base_quantity: Option<i64>,
    pub quantity: i64,
    pub catalog_unit_price: Option<i64>,
    pub unit_price: i64,
    pub entered_quantity: i64,
    pub base_consumed: i64,
    pub subtotal: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SaleHistoryDetail {
    pub id: i64,
    pub ticket_number: String,
    pub payment_method: String,
    pub sale_date: String,
    pub user_id: i64,
    pub user_name: String,
    pub user_email: String,
    pub lines: Vec<SaleHistoryLine>,
    pub line_count: i64,
    pub subtotal: i64,
    pub discount_type: CartDiscountType,
    pub discount_value: i64,
    pub discount_amount: i64,
    pub line_totals_verifiable: bool,
    pub total: i64,
    pub cash_received: Option<i64>,
    pub change_amount: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::SalesViewFilters;

    #[test]
    fn sales_view_filters_reject_unknown_fields() {
        let filters = r#"{"search":null,"payment_method":null,"unexpected":true}"#;

        assert!(serde_json::from_str::<SalesViewFilters>(filters).is_err());
    }
}
