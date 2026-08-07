#![doc = "PostgreSQL command, query, and projection adapters for Assets."]

mod command;
mod projection;
mod read;

pub use command::{AssetsCommandAdapter, AssetsWriteRepository};
pub use projection::{AssetOutboxPayload, AssetProjector, AssetSnapshot};
pub use read::{AssetsReadRepository, decode_cursor, encode_cursor};

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
}
