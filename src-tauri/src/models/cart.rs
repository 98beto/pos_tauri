use serde::{Deserialize, Serialize};

use super::{product::BaseUnit, product_presentation::PresentationMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CartDiscountType {
    None,
    Percentage,
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CartLine {
    pub presentation_id: i64,
    pub presentation_name: String,
    pub presentation_barcode: Option<String>,
    pub product_id: i64,
    pub product_name: String,
    pub sku: String,
    pub base_unit: BaseUnit,
    pub mode: PresentationMode,
    pub base_quantity: i64,
    pub catalog_unit_price: i64,
    pub applied_unit_price: i64,
    pub price_overridden: bool,
    pub entered_quantity: i64,
    pub base_consumed: i64,
    pub stock_base: i64,
    pub available: bool,
    pub line_total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cart {
    pub lines: Vec<CartLine>,
    pub line_count: i64,
    pub subtotal: i64,
    pub discount_type: CartDiscountType,
    pub discount_value: i64,
    pub discount_amount: i64,
    pub total: i64,
    pub cart_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CartDiscountQuote {
    pub subtotal: i64,
    pub discount_type: CartDiscountType,
    pub discount_value: i64,
    pub discount_amount: i64,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashChangeQuote {
    pub cash_received: i64,
    pub change: i64,
    pub total: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutCartInput {
    pub checkout_token: String,
    pub cart_fingerprint: String,
    pub payment_method: String,
    pub cash_received: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::CheckoutCartInput;

    #[test]
    fn checkout_input_rejects_unknown_fields() {
        let input = r#"{
            "checkout_token":"token",
            "cart_fingerprint":"cart-v4",
            "payment_method":"tarjeta",
            "cash_received":null,
            "unexpected":true
        }"#;

        assert!(serde_json::from_str::<CheckoutCartInput>(input).is_err());
    }
}
