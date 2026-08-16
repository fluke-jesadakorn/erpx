#![doc = "PostgreSQL command, read, and projection adapters for Legal."]
#![forbid(unsafe_code)]

mod command;
mod legal_case;
mod projection;
mod read;
mod signature_projection;

pub use command::{LegalCommandAdapter, LegalWriteRepository};
pub use legal_case::{LegalCaseProjector, LegalCaseReadAdapter, LegalCaseSqlxAdapter};
pub use projection::{
    LegalAccessHistoryProjector, LegalContractProjector, LegalDocumentProjector,
    LegalMatterProjector, LegalOutboxPayload, LegalSnapshot,
};
pub use read::{LegalReadRepository, decode_cursor, encode_cursor};
pub use signature_projection::{
    SIGNATURE_PROJECTION, SignatureEnvelopeProjector, SignatureEnvelopeReadAdapter,
};

pub const CONTEXT_NAME: &str = "legal";
pub const CONTRACT_AGGREGATE_TYPE: &str = "legal_contract";
pub const DOCUMENT_AGGREGATE_TYPE: &str = "legal_document";
pub const ACCESS_AGGREGATE_TYPE: &str = "legal_access_entry";
pub const MATTER_AGGREGATE_TYPE: &str = "legal_matter";
pub const CONTRACT_PROJECTION: &str = "legal.contracts";
pub const DOCUMENT_PROJECTION: &str = "legal.documents";
pub const ACCESS_PROJECTION: &str = "legal.access_history";
pub const MATTER_PROJECTION: &str = "legal.matters";
pub const CONTRACT_SNAPSHOT_EVENT: &str = "legal.contract_snapshot";
pub const DOCUMENT_SNAPSHOT_EVENT: &str = "legal.document_snapshot";
pub const ACCESS_SNAPSHOT_EVENT: &str = "legal.access_history_snapshot";
pub const MATTER_SNAPSHOT_EVENT: &str = "legal.matter_snapshot";
pub const EVENT_VERSION: u32 = 1;
#[cfg(test)]
const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
#[cfg(test)]
const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);

#[cfg(test)]
const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0062_wave8_dependent_context_guards.sql"
);
#[cfg(test)]
const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0054_wave8_dependent_context_projections.sql"
);
#[cfg(test)]
const MATTER_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0068_legal_matter_graph_guards.sql");
#[cfg(test)]
const MATTER_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0060_legal_matter_graph_projection.sql"
);

pub fn contract_partition(id: erpx_legal::LegalContractId) -> String {
    format!("legal.contract:{}", id.as_uuid().simple())
}
pub fn document_partition(id: erpx_legal::LegalDocumentId) -> String {
    format!("legal.document:{}", id.as_uuid().simple())
}
pub fn access_partition(id: erpx_legal::AccessEntryId) -> String {
    format!("legal.access:{}", id.as_uuid().simple())
}
pub fn matter_partition(id: erpx_legal::LegalMatterId) -> String {
    format!("legal.matter:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partitions_and_frozen_schema_are_stable() {
        let contract = erpx_legal::LegalContractId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let access = erpx_legal::AccessEntryId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert!(contract_partition(contract).starts_with("legal.contract:"));
        assert!(access_partition(access).starts_with("legal.access:"));
        assert!(WRITE_DDL.contains("legal_access_entry_immutable"));
        assert!(READ_DDL.contains(CONTRACT_PROJECTION));
        assert!(READ_DDL.contains(DOCUMENT_PROJECTION));
        assert!(READ_DDL.contains(ACCESS_PROJECTION));
        assert!(MATTER_WRITE_DDL.contains("legal_matter_link_immutable"));
        assert!(MATTER_READ_DDL.contains(MATTER_PROJECTION));
    }

    #[test]
    fn reads_have_no_write_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn append_only_children_are_never_deleted_or_updated() {
        let source = include_str!("command.rs");
        assert!(!source.contains(concat!(
            "DELETE FROM erpx_legal_write.document_",
            "version_guards"
        )));
        assert!(!source.contains(concat!(
            "UPDATE erpx_legal_write.document_",
            "version_guards"
        )));
        assert!(!source.contains(concat!("UPDATE erpx_legal_write.access_", "entry_guards")));
        assert!(!source.contains(concat!(
            "DELETE FROM erpx_legal_write.access_",
            "entry_guards"
        )));
    }

    #[test]
    fn legal_hold_completion_is_normalized_tenant_bound_and_rebuildable() {
        assert!(COMPLETION_WRITE_DDL.contains("legal_hold_records"));
        assert!(COMPLETION_READ_DDL.contains("legal_hold_projection"));
        assert!(COMPLETION_WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(COMPLETION_READ_DDL.contains("(tenant_id,projection_name,generation)"));
        assert!(!COMPLETION_WRITE_DDL.contains("legal_hold_snapshot"));
    }
}
