#![doc = "PostgreSQL command, read, and projection adapters for Marketing."]

mod command;
mod projection;
mod read;

pub use command::{MarketingCommandAdapter, MarketingWriteRepository};
pub use projection::{
    MarketingBudgetProjector, MarketingLeadProjector, MarketingOutboxPayload, MarketingSnapshot,
};
pub use read::{MarketingReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "marketing";
pub const LEAD_AGGREGATE_TYPE: &str = "lead_candidate";
pub const BUDGET_AGGREGATE_TYPE: &str = "campaign_budget";
pub const LEAD_PROJECTION_NAME: &str = "marketing.lead_candidates";
pub const BUDGET_PROJECTION_NAME: &str = "marketing.campaign_budgets";
pub const LEAD_SNAPSHOT_EVENT_TYPE: &str = "marketing.lead_candidate_snapshot";
pub const BUDGET_SNAPSHOT_EVENT_TYPE: &str = "marketing.campaign_budget_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);
#[cfg(test)]
pub const BUDGET_WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0069_marketing_campaign_budget_guards.sql"
);
#[cfg(test)]
pub const BUDGET_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0061_marketing_campaign_budget_projection.sql"
);

pub fn lead_partition(id: erpx_marketing::LeadCandidateId) -> String {
    format!("marketing.lead:{}", id.as_uuid().simple())
}

pub fn budget_partition(id: erpx_marketing::CampaignBudgetId) -> String {
    format!("marketing.budget:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partition_and_frozen_schema_are_stable() {
        let id = erpx_marketing::LeadCandidateId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            lead_partition(id),
            "marketing.lead:0000000000000000000000000000002a"
        );
        assert!(WRITE_DDL.contains("erpx_marketing_write.lead_candidate_guards"));
        assert!(READ_DDL.contains(LEAD_PROJECTION_NAME));
        assert!(BUDGET_WRITE_DDL.contains("campaign_budget_guards"));
        assert!(BUDGET_READ_DDL.contains(BUDGET_PROJECTION_NAME));
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
