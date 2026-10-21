use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaleDetail {
    pub id: i64,
    pub product_id: i64,
    pub sale_id: i64,
    pub record_version: i64,
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
    pub entered_quantity: Option<i64>,
    pub base_quantity: Option<i64>,
    pub line_total: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}
