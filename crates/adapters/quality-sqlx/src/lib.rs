#![doc = "PostgreSQL command, query, and projection adapters for the Quality context."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{QualityCommandAdapter, QualityWriteRepository};
pub use projection::{QualityInspectionProjector, QualityOutboxPayload, QualitySnapshot};
pub use read::{QualityReadRepository, decode_cursor, encode_cursor};
pub use records::{
    QualityLifecycleRecordProjector, QualityLifecycleRecordReadAdapter,
    QualityLifecycleRecordSqlxAdapter, RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "quality";
pub const AGGREGATE_TYPE: &str = "quality_inspection";
pub const PROJECTION_NAME: &str = "quality.inspections";
pub const SNAPSHOT_EVENT_TYPE: &str = "quality.inspection_snapshot";
pub const EVENT_VERSION: u32 = 1;

/// Version-neutral adapter contract. The migration owner assigns revisions.
pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");

/// Version-neutral adapter contract. The migration owner assigns revisions.
pub const READ_DDL: &str = include_str!("../ddl/read.sql");
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
pub const COMPLETION_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
/// Typed relational Quality authority introduced after the shared CQRS
/// foundation. This schema stores scalar domain state, never an aggregate
/// snapshot or a generic JSON component document.
pub const NORMALIZED_AUTHORITY_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");

pub fn inspection_partition(inspection_id: erpx_quality::domain::InspectionId) -> String {
    format!("quality.inspection:{}", inspection_id.as_uuid().simple())
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partition_is_stable_and_aggregate_specific() {
        let id = erpx_quality::domain::InspectionId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            inspection_partition(id),
            "quality.inspection:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn ddl_declares_rebuildable_generation_rows_and_u64_quantities() {
        assert!(READ_DDL.contains("generation UUID NOT NULL"));
        assert!(READ_DDL.contains("NUMERIC(20, 0)"));
        assert!(READ_DDL.contains("quality.inspections"));
        assert!(WRITE_DDL.contains("inspection_authority"));
        assert!(WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(!WRITE_DDL.contains("aggregate_snapshots"));
        assert!(erpx_cqrs_sqlx::WRITE_DDL.contains("outbox_events"));
    }

    #[test]
    fn read_adapter_has_no_write_side_capability() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
    }

    #[test]
    fn inspection_commands_use_typed_relational_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("erpx_quality_write.inspection_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("authoritative_version"));
        assert!(!source.contains("load_snapshot"));
        let owned = schema_test::table_definition(
            NORMALIZED_AUTHORITY_DDL,
            "erpx_quality_write.inspection_authority",
        )
        .expect("quality inspection authority table");
        assert!(NORMALIZED_AUTHORITY_DDL.contains("inspection_authority"));
        assert!(!owned.contains("JSONB"));
        assert!(!owned.contains("aggregate_components"));
    }
}
