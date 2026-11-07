use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenCashInput {
    pub operation_token: String,
    pub opening_amount: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CashMovementInput {
    pub operation_token: String,
    pub session_id: i64,
    pub amount: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseCashInput {
    pub operation_token: String,
    pub session_id: i64,
    pub counted_amount: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashBreakdown {
    pub cash_sales: i64,
    pub card_sales: i64,
    pub transfer_sales: i64,
    pub sale_count: i64,
    pub income: i64,
    pub withdrawals: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashSession {
    pub id: i64,
    pub status: String,
    pub historical: bool,
    pub opening_amount: Option<i64>,
    pub opened_by: Option<i64>,
    pub opening_operator: Option<String>,
    pub opened_at: String,
    pub closed_by: Option<i64>,
    pub closing_operator: Option<String>,
    pub closed_at: Option<String>,
    pub expected_amount: Option<i64>,
    pub counted_amount: Option<i64>,
    pub difference: Option<i64>,
    pub breakdown: CashBreakdown,
}

#[derive(Debug, Serialize)]
pub struct CashOperation {
    pub id: i64,
    pub session_id: i64,
    pub kind: String,
    pub amount: i64,
    pub reason: String,
    pub user_id: i64,
    pub operator: String,
    pub operation_token: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CashSessionDetail {
    pub session: CashSession,
    pub operations: Vec<CashOperation>,
}
