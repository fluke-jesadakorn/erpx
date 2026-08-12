//! Target-neutral Marketing UI models. Core and wire mapping lives in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingLeadItem {
    pub lead_id: String,
    pub version: u64,
    pub scope: String,
    pub campaign_id: String,
    pub audience_member_id: String,
    pub source: String,
    pub existing_customer_id: Option<String>,
    pub score_basis_points: Option<u16>,
    pub status: String,
    pub engagement_count: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingLeadItems {
    pub candidates: Vec<MarketingLeadItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingCampaignBudgetItem {
    pub budget_id: String,
    pub version: u64,
    pub scope: String,
    pub campaign_id: String,
    pub code: String,
    pub name: String,
    pub period: String,
    pub currency: String,
    pub requested_minor: i64,
    pub approved_minor: Option<i64>,
    pub reserved_minor: i64,
    pub available_minor: i64,
    pub allocation_count: usize,
    pub active_spend_request_count: usize,
    pub approval_request_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}
