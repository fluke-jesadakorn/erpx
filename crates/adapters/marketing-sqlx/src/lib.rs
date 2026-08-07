#![doc = "PostgreSQL command, read, and projection adapters for Marketing."]

mod command;
mod projection;
mod read;

pub use command::{MarketingCommandAdapter, MarketingWriteRepository};
pub use projection::{MarketingLeadProjector, MarketingOutboxPayload, MarketingSnapshot};
pub use read::{MarketingReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "marketing";
pub const AGGREGATE_TYPE: &str = "lead_candidate";
pub const PROJECTION_NAME: &str = "marketing.lead_candidates";
pub const SNAPSHOT_EVENT_TYPE: &str = "marketing.lead_candidate_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);

pub fn lead_partition(id: erpx_marketing::LeadCandidateId) -> String {
    format!("marketing.lead:{}", id.as_uuid().simple())
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
        assert!(READ_DDL.contains("marketing.lead_candidates"));
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
