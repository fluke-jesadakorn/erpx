//! Target-neutral Finance view models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceItem {
    pub id: String,
    pub version: u64,
    pub scope: String,
    pub code: String,
    pub kind: String,
    pub status: String,
    pub currency: Option<String>,
    pub amount_minor: Option<i64>,
    pub updated_at: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceItems {
    pub heading: String,
    pub items: Vec<FinanceItem>,
    pub next_cursor: Option<String>,
}
