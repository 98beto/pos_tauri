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
    Centimeter,
}

impl BaseUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Piece => "piece",
            Self::Gram => "gram",
            Self::Milliliter => "milliliter",
            Self::Centimeter => "centimeter",
        }
    }

    #[allow(dead_code)] // Shared domain rule; the UI consumes it now and later Rust blocks will reuse it.
    pub fn standard_base_quantity(self) -> Option<i64> {
        match self {
            Self::Piece => None,
            Self::Gram | Self::Milliliter => Some(1_000),
            Self::Centimeter => Some(100),
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
            "centimeter" => Ok(Self::Centimeter),
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
pub struct CreateProductWithPresentationsInput {
    pub name: String,
    pub sku: String,
    pub brand_id: i64,
    pub category_id: i64,
    pub base_unit: BaseUnit,
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
    pub centimeter_stock: i64,
    pub brand_count: i64,
    pub inventory_value_cents: i64,
    pub products_without_cost: i64,
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
