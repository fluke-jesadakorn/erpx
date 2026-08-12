#![doc = "PostgreSQL command, read, and projection adapters for CRM."]

mod command;
mod projection;
mod read;

pub use command::{CrmCommandAdapter, CrmWriteRepository};
pub use projection::{CrmLeadProjector, CrmOpportunityProjector, CrmOutboxPayload, CrmSnapshot};
pub use read::{CrmReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "crm";
pub const LEAD_AGGREGATE_TYPE: &str = "crm_lead";
pub const OPPORTUNITY_AGGREGATE_TYPE: &str = "opportunity";
pub const LEAD_PROJECTION: &str = "crm.leads";
pub const OPPORTUNITY_PROJECTION: &str = "crm.opportunities";
pub const LEAD_SNAPSHOT_EVENT: &str = "crm.lead_snapshot";
pub const OPPORTUNITY_SNAPSHOT_EVENT: &str = "crm.opportunity_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);
#[cfg(test)]
pub const FORECAST_WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0067_crm_opportunity_forecast_guards.sql"
);
#[cfg(test)]
pub const FORECAST_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0059_crm_opportunity_forecast_projection.sql"
);

pub fn lead_partition(id: erpx_crm::CrmLeadId) -> String {
    format!("crm.lead:{}", id.as_uuid().simple())
}
pub fn opportunity_partition(id: erpx_crm::OpportunityId) -> String {
    format!("crm.opportunity:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    #[test]
    fn partitions_and_schema_are_stable() {
        let lead = erpx_crm::CrmLeadId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let opportunity = erpx_crm::OpportunityId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert!(lead_partition(lead).starts_with("crm.lead:"));
        assert!(opportunity_partition(opportunity).starts_with("crm.opportunity:"));
        assert!(WRITE_DDL.contains("'discovery'"));
        assert!(READ_DDL.contains("crm.opportunities"));
        assert!(FORECAST_WRITE_DDL.contains("pipeline_category"));
        assert!(FORECAST_READ_DDL.contains("forecast_cockpit"));
    }
    #[test]
    fn reads_have_no_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
