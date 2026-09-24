use serde::{Deserialize, Serialize};

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
    pub item_count: i64,
    pub user_id: i64,
    pub user_name: String,
}

#[derive(Debug, Serialize)]
pub struct SalesViewStats {
    pub sale_count: i64,
    pub total_sold: i64,
    pub average_sale: i64,
    pub items_sold: i64,
}

#[derive(Debug, Serialize)]
pub struct SalesView {
    pub items: Vec<SalesViewRow>,
    pub stats: SalesViewStats,
}

#[derive(Debug, Serialize)]
pub struct SaleHistoryLine {
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub quantity: i64,
    pub unit_price: i64,
    pub subtotal: i64,
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
    pub item_count: i64,
    pub total: i64,
    pub cash_received: Option<i64>,
    pub change_amount: Option<i64>,
}
