//! Target-neutral CRM UI models. Core and wire mapping lives in the gateway.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmLeadItem {
    pub lead_id: String,
    pub version: u64,
    pub scope: String,
    pub marketing_candidate_id: String,
    pub customer_id: Option<String>,
    pub source: String,
    pub qualification_score_basis_points: u16,
    pub accepted_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmOpportunityItem {
    pub opportunity_id: String,
    pub version: u64,
    pub scope: String,
    pub accepted_lead_id: String,
    pub customer_id: String,
    pub name: String,
    pub expected_value_minor: i64,
    pub currency: String,
    pub stage: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmOpportunityItems {
    pub opportunities: Vec<CrmOpportunityItem>,
    pub next_cursor: Option<String>,
}
