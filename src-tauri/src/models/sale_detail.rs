use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleDetail {
    pub id: i64,
    pub product_id: i64,
    pub sale_id: i64,
    pub product_name: String,
    pub sku: String,
    pub quantity: i64,
    pub unit_price: i64,
    pub created_at: String,
    pub updated_at: String,
}
