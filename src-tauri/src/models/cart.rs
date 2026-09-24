use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CartLine {
    pub product_id: i64,
    pub sku: String,
    pub name: String,
    pub unit_price: i64,
    pub quantity: i64,
    pub stock: i64,
    pub available: bool,
    pub subtotal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cart {
    pub lines: Vec<CartLine>,
    pub item_count: i64,
    pub subtotal: i64,
    pub total: i64,
    pub cart_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashChangeQuote {
    pub cash_received: i64,
    pub change: i64,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
pub struct CheckoutCartInput {
    pub checkout_token: String,
    pub cart_fingerprint: String,
    pub payment_method: String,
    pub cash_received: Option<String>,
}
