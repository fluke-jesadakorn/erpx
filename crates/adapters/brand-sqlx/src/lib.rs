#![doc = "PostgreSQL command, query, and projection adapters for Brand."]

mod command;
mod projection;
mod read;

pub use command::{BrandCommandAdapter, BrandParentAuthorityRepository, BrandWriteRepository};
pub use projection::{
    BrandHistoryProjector, BrandOutboxPayload, BrandProfileProjector, BrandResolvedProjector,
    BrandSnapshot,
};
pub use read::{BrandReadRepository, decode_history_cursor, encode_history_cursor};

pub const CONTEXT_NAME: &str = "brand";
pub const AGGREGATE_TYPE: &str = "brand_profile";
pub const PROFILE_PROJECTION: &str = "brand.profiles";
pub const RESOLVED_PROJECTION: &str = "brand.resolved";
pub const HISTORY_PROJECTION: &str = "brand.history";
pub const SNAPSHOT_EVENT_TYPE: &str = "brand.profile_snapshot";
pub const EVENT_VERSION: u32 = 1;

pub const CONTROL_WRITE_FOUNDATION_DDL: &str = include_str!(
    "../../../migration/migrations/0075_cqrs_control_foundation_and_commercial_catalog.sql"
);
pub const CONTROL_WRITE_CONTEXT_DDL: &str = concat!(
    include_str!("../../../migration/migrations/0076_wave5_control_context_guards.sql"),
    "\n",
    include_str!("../../../migration/migrations/0082_current_normalized_authority.sql")
);
pub const CONTROL_WRITE_DDL: &str = concat!(
    include_str!(
        "../../../migration/migrations/0075_cqrs_control_foundation_and_commercial_catalog.sql"
    ),
    "\n",
    include_str!("../../../migration/migrations/0076_wave5_control_context_guards.sql"),
    "\n",
    include_str!("../../../migration/migrations/0082_current_normalized_authority.sql")
);
pub const CONTROL_READ_FOUNDATION_DDL: &str = include_str!(
    "../../../migration/migrations_read/0015_cqrs_control_foundation_and_commercial_catalog.sql"
);
pub const CONTROL_READ_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_read/0016_wave5_control_context_projections.sql");
pub const CONTROL_READ_DDL: &str = concat!(
    include_str!(
        "../../../migration/migrations_read/0015_cqrs_control_foundation_and_commercial_catalog.sql"
    ),
    "\n",
    include_str!("../../../migration/migrations_read/0016_wave5_control_context_projections.sql")
);
pub const WRITE_DDL: &str = CONTROL_WRITE_DDL;
pub const READ_DDL: &str = CONTROL_READ_DDL;

pub fn profile_partition(id: erpx_brand::BrandProfileId) -> String {
    format!("brand.profile:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partition_is_stable_and_profile_specific() {
        let id = erpx_brand::BrandProfileId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            profile_partition(id),
            "brand.profile:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn canonical_ddl_contains_foundation_and_brand_tables() {
        assert!(CONTROL_WRITE_FOUNDATION_DDL.contains("command_receipts"));
        assert!(!CONTROL_WRITE_FOUNDATION_DDL.contains("aggregate_snapshots"));
        assert!(CONTROL_WRITE_CONTEXT_DDL.contains("erpx_brand_write.profile_guards"));
        assert!(CONTROL_READ_FOUNDATION_DDL.contains("projection_generations"));
        assert!(CONTROL_READ_CONTEXT_DDL.contains("erpx_brand_read.profile_projection"));
        assert!(CONTROL_READ_DDL.ends_with(CONTROL_READ_CONTEXT_DDL));
    }

    #[test]
    fn read_adapter_has_no_write_capability_or_snapshot_fallback() {
        let production = include_str!("read.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("read adapter has production source");
        for forbidden in [
            concat!("Write", "Pool"),
            concat!("aggregate_", "snapshots"),
            concat!("erpx_brand_", "write"),
        ] {
            assert!(!production.contains(forbidden));
        }
    }
}
