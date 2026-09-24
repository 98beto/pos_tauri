use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Brand {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveBrandInput {
    pub name: String,
}
