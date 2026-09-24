use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: i64,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateProductInput {
    pub name: String,
    pub price: String,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProductInput {
    pub name: String,
    pub price: String,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductViewFilters {
    pub search: Option<String>,
    pub brand_id: Option<i64>,
    pub category_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ProductViewRow {
    pub id: i64,
    pub name: String,
    pub price: i64,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub brand_name: String,
    pub category_id: i64,
    pub category_name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ProductViewStats {
    pub product_count: i64,
    pub inventory_units: i64,
    pub brand_count: i64,
    pub inventory_value: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductView {
    pub items: Vec<ProductViewRow>,
    pub stats: ProductViewStats,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProductSelectorFilters {
    pub search: Option<String>,
    pub category_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ProductSelectorRow {
    pub id: i64,
    pub name: String,
    pub sku: String,
    pub price: i64,
    pub stock: i64,
    pub cart_quantity: i64,
    pub available_stock: i64,
    pub brand_id: i64,
    pub brand_name: String,
    pub category_id: i64,
    pub category_name: String,
}
