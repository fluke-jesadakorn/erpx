#![doc = "PostgreSQL command, read, and projection adapters for Legal."]
#![forbid(unsafe_code)]

mod command;
mod projection;
mod read;

pub use command::{LegalCommandAdapter, LegalWriteRepository};
pub use projection::{
    LegalAccessHistoryProjector, LegalContractProjector, LegalDocumentProjector,
    LegalOutboxPayload, LegalSnapshot,
};
pub use read::{LegalReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "legal";
pub const CONTRACT_AGGREGATE_TYPE: &str = "legal_contract";
pub const DOCUMENT_AGGREGATE_TYPE: &str = "legal_document";
pub const ACCESS_AGGREGATE_TYPE: &str = "legal_access_entry";
pub const CONTRACT_PROJECTION: &str = "legal.contracts";
pub const DOCUMENT_PROJECTION: &str = "legal.documents";
pub const ACCESS_PROJECTION: &str = "legal.access_history";
pub const CONTRACT_SNAPSHOT_EVENT: &str = "legal.contract_snapshot";
pub const DOCUMENT_SNAPSHOT_EVENT: &str = "legal.document_snapshot";
pub const ACCESS_SNAPSHOT_EVENT: &str = "legal.access_history_snapshot";
pub const EVENT_VERSION: u32 = 1;

#[cfg(test)]
const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0062_wave8_dependent_context_guards.sql"
);
#[cfg(test)]
const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0054_wave8_dependent_context_projections.sql"
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
}
