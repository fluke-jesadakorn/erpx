#![doc = "PostgreSQL command, query, and projection adapters for Assets."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{AssetsCommandAdapter, AssetsWriteRepository};
pub use projection::{AssetOutboxPayload, AssetProjector, AssetSnapshot};
pub use read::{AssetsReadRepository, decode_cursor, encode_cursor};
pub use records::{
    AssetLifecycleRecordProjector, AssetLifecycleRecordReadAdapter,
    AssetLifecycleRecordSqlxAdapter, RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "assets";
pub const AGGREGATE_TYPE: &str = "asset";
pub const PROJECTION_NAME: &str = "assets.assets";
pub const SNAPSHOT_EVENT_TYPE: &str = "assets.asset_snapshot";
pub const EVENT_VERSION: u32 = 1;

pub const TENANT_WRITE_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql");
pub const TENANT_WRITE_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql");
pub const TENANT_READ_FOUNDATION_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql");
pub const TENANT_READ_CONTEXT_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
);
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
pub const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);
pub const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);
pub const WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_write/0057_cqrs_foundation.sql"),
    "\n",
    include_str!("../../../migration/migrations_tenant_write/0060_wave6_paired_context_guards.sql")
);
pub const READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_read/0048_cqrs_foundation.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_read/0052_wave6_paired_context_projections.sql"
    )
);

pub fn asset_partition(asset_id: erpx_assets::AssetId) -> String {
    format!("assets.asset:{}", asset_id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partition_is_stable_and_context_owned() {
        let id = erpx_assets::AssetId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            asset_partition(id),
            "assets.asset:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn ddl_matches_the_frozen_guard_and_rebuild_contracts() {
        assert!(WRITE_DDL.contains("erpx_assets_write.asset_guards"));
        assert!(READ_DDL.contains("erpx_assets_read.asset_projection"));
        assert!(READ_DDL.contains("generation UUID NOT NULL"));
    }

    #[test]
    fn read_adapter_has_no_write_capability() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains("aggregate_snapshots"));
    }

    #[test]
    fn asset_commands_use_typed_tenant_authority() {
        let source = include_str!("command.rs");
        assert!(source.contains("erpx_assets_write.asset_authority"));
        assert!(source.contains("commit_normalized"));
        assert!(!source.contains("load_snapshot"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("asset_name TEXT NOT NULL"));
        assert!(!NORMALIZED_AUTHORITY_DDL.contains("aggregate_components"));
    }
}
