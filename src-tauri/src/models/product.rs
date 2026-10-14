use serde::{Deserialize, Serialize};

use super::product_presentation::{
    CreateProductPresentationInput, ProductPresentation, UpdateProductPresentationInput,
};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseUnit {
    Piece,
    Gram,
    Milliliter,
}

impl BaseUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Piece => "piece",
            Self::Gram => "gram",
            Self::Milliliter => "milliliter",
        }
    }
}

impl TryFrom<&str> for BaseUnit {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "piece" => Ok(Self::Piece),
            "gram" => Ok(Self::Gram),
            "milliliter" => Ok(Self::Milliliter),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: i64,
    pub stock: i64,
    pub stock_base: i64,
    pub base_unit: BaseUnit,
    pub inventory_cost_cents: Option<i64>,
    pub inventory_cost_status: &'static str,
    pub average_cost_cents: Option<i64>,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub created_at: String,
    pub updated_at: String,
    pub presentations: Vec<ProductPresentation>,
    pub default_presentation: ProductPresentation,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProductInput {
    pub name: String,
    pub price: String,
    pub stock: i64,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductInput {
    pub name: String,
    pub price: String,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProductWithPresentationsInput {
    pub name: String,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub base_unit: BaseUnit,
    pub initial_stock: i64,
    pub initial_purchase_price_per_base: Option<String>,
    pub presentations: Vec<CreateProductPresentationInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductWithPresentationsInput {
    pub name: String,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub base_unit: BaseUnit,
    pub presentations: Vec<UpdateProductPresentationInput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductCodeMatchKind {
    Sku,
    Barcode,
}

#[derive(Debug, Serialize)]
pub struct ProductCodeMatch {
    pub product: Product,
    pub match_kind: ProductCodeMatchKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_id: Option<i64>,
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
    pub stock_base: i64,
    pub base_unit: BaseUnit,
    pub inventory_cost_cents: Option<i64>,
    pub inventory_cost_status: &'static str,
    pub average_cost_cents: Option<i64>,
    pub sku: String,
    pub brand_id: i64,
    pub brand_name: String,
    pub category_id: i64,
    pub category_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub presentations: Vec<ProductPresentation>,
    pub default_presentation: ProductPresentation,
}

#[derive(Debug, Serialize)]
pub struct ProductViewStats {
    pub product_count: i64,
    pub piece_stock: i64,
    pub gram_stock: i64,
    pub milliliter_stock: i64,
    pub brand_count: i64,
    pub inventory_value: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductView {
    pub items: Vec<ProductViewRow>,
    pub stats: ProductViewStats,
}

#[derive(Clone, Debug, Serialize)]
pub struct InventoryProductRow {
    pub id: i64,
    pub name: String,
    pub sku: String,
    pub stock: i64,
    pub base_unit: BaseUnit,
    pub inventory_cost_cents: Option<i64>,
    pub inventory_cost_status: &'static str,
    pub average_cost_cents: Option<i64>,
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
    pub stock_base: i64,
    pub base_unit: BaseUnit,
    pub cart_quantity: i64,
    pub available_stock: i64,
    pub brand_id: i64,
    pub brand_name: String,
    pub category_id: i64,
    pub category_name: String,
    pub presentations: Vec<ProductPresentation>,
    pub default_presentation: ProductPresentation,
}

#[cfg(test)]
mod tests {
    use super::{
        CreateProductInput, CreateProductWithPresentationsInput, UpdateProductInput,
        UpdateProductWithPresentationsInput,
    };

    #[test]
    fn legacy_and_presentation_payloads_are_unambiguous() {
        let incomplete_create = serde_json::json!({
            "name": "Producto",
            "price": "1.00",
            "stock": 0,
            "sku": "SKU",
            "brand_id": 1,
            "category_id": 1,
            "presentations": []
        });
        assert!(serde_json::from_value::<CreateProductInput>(incomplete_create.clone()).is_err());
        assert!(
            serde_json::from_value::<CreateProductWithPresentationsInput>(incomplete_create)
                .is_err()
        );

        let incomplete_update = serde_json::json!({
            "name": "Producto",
            "price": "1.00",
            "sku": "SKU",
            "brand_id": 1,
            "category_id": 1,
            "presentations": []
        });
        assert!(serde_json::from_value::<UpdateProductInput>(incomplete_update.clone()).is_err());
        assert!(
            serde_json::from_value::<UpdateProductWithPresentationsInput>(incomplete_update)
                .is_err()
        );
    }
}
