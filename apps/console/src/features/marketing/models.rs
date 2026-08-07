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
