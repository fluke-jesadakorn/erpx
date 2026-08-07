#![doc = "PostgreSQL command, query, and projection adapters for the Quality context."]

mod command;
mod projection;
mod read;

pub use command::{QualityCommandAdapter, QualityWriteRepository};
pub use projection::{QualityInspectionProjector, QualityOutboxPayload, QualitySnapshot};
pub use read::{QualityReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "quality";
pub const AGGREGATE_TYPE: &str = "quality_inspection";
pub const PROJECTION_NAME: &str = "quality.inspections";
pub const SNAPSHOT_EVENT_TYPE: &str = "quality.inspection_snapshot";
pub const EVENT_VERSION: u32 = 1;

/// Version-neutral adapter contract. The migration owner assigns revisions.
pub const WRITE_DDL: &str = include_str!("../ddl/write.sql");

/// Version-neutral adapter contract. The migration owner assigns revisions.
pub const READ_DDL: &str = include_str!("../ddl/read.sql");

pub fn inspection_partition(inspection_id: erpx_quality::domain::InspectionId) -> String {
    format!("quality.inspection:{}", inspection_id.as_uuid().simple())
}

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
        assert!(WRITE_DDL.contains("aggregate_snapshots"));
        assert!(WRITE_DDL.contains("outbox_events"));
    }

    #[test]
    fn read_adapter_has_no_write_side_capability() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
    }
}
