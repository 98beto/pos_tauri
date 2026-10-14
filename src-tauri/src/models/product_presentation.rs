use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentationMode {
    Fixed,
    Measured,
}

impl PresentationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Measured => "measured",
        }
    }
}

impl TryFrom<&str> for PresentationMode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "fixed" => Ok(Self::Fixed),
            "measured" => Ok(Self::Measured),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProductPresentation {
    pub id: i64,
    pub product_id: i64,
    pub name: String,
    pub mode: PresentationMode,
    pub base_quantity: i64,
    pub price: i64,
    pub barcode: Option<String>,
    pub is_default: bool,
    pub is_active: bool,
    pub is_conversion_locked: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProductPresentationInput {
    pub name: String,
    pub mode: PresentationMode,
    pub base_quantity: i64,
    pub price: String,
    pub barcode: Option<String>,
    pub is_default: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProductPresentationInput {
    pub id: Option<i64>,
    pub name: String,
    pub mode: PresentationMode,
    pub base_quantity: i64,
    pub price: String,
    pub barcode: Option<String>,
    pub is_default: bool,
    pub is_active: bool,
}

fn default_true() -> bool {
    true
}
