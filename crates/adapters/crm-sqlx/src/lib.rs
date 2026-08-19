#![doc = "PostgreSQL command, read, and projection adapters for CRM."]

mod command;
mod projection;
mod qualified_lead;
mod read;
mod records;

pub use command::{CrmCommandAdapter, CrmWriteRepository};
pub use projection::{CrmLeadProjector, CrmOpportunityProjector, CrmOutboxPayload, CrmSnapshot};
pub use qualified_lead::{MarketingQualifiedLeadEnvelope, SqlxQualifiedLeadResolver};
pub use read::{CrmReadRepository, decode_cursor, encode_cursor};
pub use records::{
    CrmRecordProjector, CrmRecordReadAdapter, CrmRecordSqlxAdapter, RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "crm";
pub const LEAD_AGGREGATE_TYPE: &str = "crm_lead";
pub const OPPORTUNITY_AGGREGATE_TYPE: &str = "opportunity";
pub const LEAD_PROJECTION: &str = "crm.leads";
pub const OPPORTUNITY_PROJECTION: &str = "crm.opportunities";
pub const LEAD_SNAPSHOT_EVENT: &str = "crm.lead_snapshot";
pub const OPPORTUNITY_SNAPSHOT_EVENT: &str = "crm.opportunity_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql"),
    "\n",
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql")
);
pub const READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql"),
    "\n",
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql")
);
pub const NORMALIZED_AUTHORITY_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
pub const FORECAST_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
pub const FORECAST_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

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
    #[test]
    fn commands_use_complete_typed_crm_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("lead_authority"));
        assert!(source.contains("opportunity_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("marketing_source_version BIGINT NOT NULL"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("opportunity_name TEXT NOT NULL"));
    }
}
