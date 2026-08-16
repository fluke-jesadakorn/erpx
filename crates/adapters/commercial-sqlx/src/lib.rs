#![doc = "PostgreSQL command, query, and projection adapters for Commercial."]

mod command;
mod projection;
mod read;
mod records;

pub use command::{CommercialCommandAdapter, CommercialWriteRepository};
pub use projection::{
    CatalogPlanProjector, CommercialOutboxPayload, CommercialSnapshot, EntitlementProjector,
    SubscriptionProjector,
};
pub use read::{CommercialReadRepository, decode_cursor, encode_cursor};
pub use records::{
    CommercialLifecycleRecordProjector, CommercialLifecycleRecordReadAdapter,
    CommercialLifecycleRecordSqlxAdapter,
    RECORD_PROJECTION_NAME as LIFECYCLE_RECORD_PROJECTION_NAME,
    RECORD_SNAPSHOT_EVENT_TYPE as LIFECYCLE_RECORD_SNAPSHOT_EVENT_TYPE,
};

pub const CONTEXT_NAME: &str = "commercial";
pub const CATALOG_PLAN_AGGREGATE: &str = "catalog_plan";
pub const SUBSCRIPTION_AGGREGATE: &str = "subscription";
pub const ENTITLEMENT_AGGREGATE: &str = "entitlement";
pub const CATALOG_PLAN_PROJECTION: &str = "commercial.catalog_plans";
pub const SUBSCRIPTION_PROJECTION: &str = "commercial.subscriptions";
pub const ENTITLEMENT_PROJECTION: &str = "commercial.entitlements";
pub const CATALOG_PLAN_SNAPSHOT_EVENT: &str = "commercial.catalog_plan_snapshot";
pub const SUBSCRIPTION_SNAPSHOT_EVENT: &str = "commercial.subscription_snapshot";
pub const ENTITLEMENT_SNAPSHOT_EVENT: &str = "commercial.entitlement_snapshot";
pub const EVENT_VERSION: u32 = 1;
pub const COMPLETION_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0080_current_record_completion.sql");
pub const COMPLETION_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0071_current_record_completion_projection.sql"
);

/// Control-write foundation plus the authoritative platform catalog guard.
pub const CONTROL_WRITE_DDL: &str = concat!(
    include_str!(
        "../../../migration/migrations/0075_cqrs_control_foundation_and_commercial_catalog.sql"
    ),
    "\n",
    include_str!("../../../migration/migrations/0082_current_normalized_authority.sql")
);
/// Control-read foundation plus the authoritative platform catalog projection.
pub const CONTROL_READ_DDL: &str = include_str!(
    "../../../migration/migrations_read/0015_cqrs_control_foundation_and_commercial_catalog.sql"
);

/// Tenant-write Commercial guards before the catalog cleanup migration.
pub const TENANT_WRITE_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0058_wave4_context_guards.sql");
/// Removes the obsolete tenant-local catalog write table.
pub const TENANT_WRITE_CATALOG_CLEANUP_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0059_move_commercial_catalog_to_control.sql"
);
pub const TENANT_NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);
/// Final ordered tenant-write Commercial DDL composition.
pub const TENANT_WRITE_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_write/0058_wave4_context_guards.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_write/0059_move_commercial_catalog_to_control.sql"
    ),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
    )
);

/// Tenant-read Commercial projections before the catalog cleanup migration.
pub const TENANT_READ_CONTEXT_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0050_wave4_context_projections.sql");
/// Removes the obsolete tenant-local catalog read table.
pub const TENANT_READ_CATALOG_CLEANUP_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0051_move_commercial_catalog_to_control.sql"
);
/// Final ordered tenant-read Commercial DDL composition.
pub const TENANT_READ_DDL: &str = concat!(
    include_str!("../../../migration/migrations_tenant_read/0050_wave4_context_projections.sql"),
    "\n",
    include_str!(
        "../../../migration/migrations_tenant_read/0051_move_commercial_catalog_to_control.sql"
    )
);

// Compatibility aliases retain the original tenant-plane meaning. New
// composition code should choose an explicitly capability-scoped constant.
pub const WRITE_DDL: &str = TENANT_WRITE_DDL;
pub const READ_DDL: &str = TENANT_READ_DDL;

pub fn catalog_plan_partition(id: erpx_commercial::CatalogPlanId) -> String {
    format!("commercial.catalog_plan:{}", id.as_uuid().simple())
}

pub fn subscription_partition(id: erpx_commercial::SubscriptionId) -> String {
    format!("commercial.subscription:{}", id.as_uuid().simple())
}

pub fn entitlement_partition(id: erpx_commercial::EntitlementId) -> String {
    format!("commercial.entitlement:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn partitions_are_stable_and_aggregate_specific() {
        let raw = Uuid::from_u128(42);
        assert_eq!(
            catalog_plan_partition(erpx_commercial::CatalogPlanId::try_from_uuid(raw).unwrap()),
            "commercial.catalog_plan:0000000000000000000000000000002a"
        );
        assert_ne!(
            subscription_partition(erpx_commercial::SubscriptionId::try_from_uuid(raw).unwrap()),
            entitlement_partition(erpx_commercial::EntitlementId::try_from_uuid(raw).unwrap())
        );
    }

    #[test]
    fn read_adapter_has_no_write_capability_or_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }

    #[test]
    fn ddl_points_to_capability_separated_canonical_migrations() {
        assert!(CONTROL_WRITE_DDL.contains("catalog_plan_guards"));
        assert!(!CONTROL_WRITE_DDL.contains("subscription_guards"));
        assert!(!CONTROL_WRITE_DDL.contains("entitlement_guards"));
        assert!(CONTROL_READ_DDL.contains("catalog_plan_projection"));
        assert!(!CONTROL_READ_DDL.contains("subscription_projection"));
        assert!(!CONTROL_READ_DDL.contains("entitlement_projection"));

        assert!(TENANT_WRITE_CONTEXT_DDL.contains("subscription_guards"));
        assert!(TENANT_WRITE_CONTEXT_DDL.contains("entitlement_guards"));
        assert!(TENANT_READ_CONTEXT_DDL.contains("subscription_projection"));
        assert!(TENANT_READ_CONTEXT_DDL.contains("entitlement_projection"));

        assert!(
            TENANT_WRITE_CATALOG_CLEANUP_DDL
                .contains("DROP TABLE IF EXISTS erpx_commercial_write.catalog_plan_guards")
        );
        assert!(
            TENANT_READ_CATALOG_CLEANUP_DDL
                .contains("DROP TABLE IF EXISTS erpx_commercial_read.catalog_plan_projection")
        );
        assert!(TENANT_WRITE_DDL.contains(TENANT_WRITE_CATALOG_CLEANUP_DDL));
        assert!(TENANT_WRITE_DDL.ends_with(TENANT_NORMALIZED_AUTHORITY_DDL));
        assert!(TENANT_NORMALIZED_AUTHORITY_DDL.contains("subscription_authority"));
        assert!(TENANT_NORMALIZED_AUTHORITY_DDL.contains("entitlement_authority"));
        assert!(TENANT_READ_DDL.ends_with(TENANT_READ_CATALOG_CLEANUP_DDL));
        assert_eq!(WRITE_DDL, TENANT_WRITE_DDL);
        assert_eq!(READ_DDL, TENANT_READ_DDL);
    }
}
