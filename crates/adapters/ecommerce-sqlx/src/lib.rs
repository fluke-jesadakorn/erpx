#![doc = "PostgreSQL CQRS adapters for E-commerce."]
#![forbid(unsafe_code)]

mod directory;
mod repository;
mod runner_projection;

pub use repository::{SqlxCommerceReadRepository, SqlxCommerceWriteRepository};
pub use runner_projection::{
    CommerceChannelProjector, CommerceJourneyProjector, CommerceListingProjector, CommerceSnapshot,
};

pub const CONTEXT_NAME: &str = "ecommerce";
pub const CHANNEL_PROJECTION: &str = "ecommerce.channels";
pub const LISTING_PROJECTION: &str = "ecommerce.listings";
pub const JOURNEY_PROJECTION: &str = "ecommerce.journeys";
pub const CHANNEL_SNAPSHOT_EVENT: &str = "ecommerce.channel.snapshot.v1";
pub const LISTING_SNAPSHOT_EVENT: &str = "ecommerce.listing.snapshot.v1";
pub const JOURNEY_SNAPSHOT_EVENT: &str = "ecommerce.journey.snapshot.v1";
pub const WEB_ONLY: bool = true;
const _: () = assert!(WEB_ONLY);

#[cfg(test)]
const WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0076_ecommerce_talent.sql");
#[cfg(test)]
const READ_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0067_ecommerce_talent_projection.sql");
#[cfg(test)]
const CHANNEL_READ_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0072_ecommerce_channel_projection.sql");
#[cfg(test)]
const AGENT_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0073_agent_orchestration.sql");
#[cfg(test)]
const AGENT_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0064_agent_orchestration_projection.sql"
);
#[cfg(test)]
const PROJECTS_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0074_projects.sql");
#[cfg(test)]
const PROJECTS_READ_DDL: &str =
    include_str!("../../../migration/migrations_tenant_read/0065_projects_projection.sql");
#[cfg(test)]
const SHARED_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_tenant_write/0078_shared_enterprise_services.sql");
#[cfg(test)]
const SHARED_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0069_shared_enterprise_services_projection.sql"
);

pub fn aggregate_partition(aggregate_type: &str, id: uuid::Uuid) -> String {
    format!("ecommerce.{aggregate_type}:{}", id.simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_schema_is_normalized_atomic_and_tenant_bound() {
        for table in [
            "channels",
            "listings",
            "carts",
            "checkouts",
            "returns",
            "command_receipts",
            "audit_log",
            "outbox",
        ] {
            assert!(WRITE_DDL.contains(&format!("'erpx_ecommerce_write','{table}'")));
        }
        assert!(!WRITE_DDL.contains("aggregate_guards"));
        assert!(!WRITE_DDL.contains("state JSONB"));
        assert!(WRITE_DDL.contains("ENABLE ROW LEVEL SECURITY"));
        assert!(WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(WRITE_DDL.contains("current_setting('app.tenant_id', true)"));
        assert!(WRITE_DDL.contains("CHECK ((order_receipt IS NULL) = (order_ref IS NULL))"));
    }

    #[test]
    fn read_schema_is_disposable_owner_scoped_and_rls_protected() {
        assert!(READ_DDL.contains(LISTING_PROJECTION));
        assert!(READ_DDL.contains(JOURNEY_PROJECTION));
        assert!(READ_DDL.contains("owner_principal_id"));
        assert!(READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(CHANNEL_READ_DDL.contains("erpx_ecommerce_read.channel_projection"));
        assert!(CHANNEL_READ_DDL.contains(CHANNEL_PROJECTION));
        assert!(CHANNEL_READ_DDL.contains("FORCE ROW LEVEL SECURITY"));
    }

    #[test]
    fn commerce_is_web_only_and_partitions_are_stable() {
        let id = uuid::Uuid::from_u128(1);
        assert_eq!(
            aggregate_partition("commerce_checkout", id),
            "ecommerce.commerce_checkout:00000000000000000000000000000001"
        );
    }

    #[test]
    fn every_new_tenant_table_is_explicitly_bound_and_rls_listed() {
        for ddl in [
            AGENT_WRITE_DDL,
            AGENT_READ_DDL,
            PROJECTS_WRITE_DDL,
            PROJECTS_READ_DDL,
            WRITE_DDL,
            READ_DDL,
            SHARED_WRITE_DDL,
            SHARED_READ_DDL,
        ] {
            let tables = ddl
                .lines()
                .filter_map(|line| line.trim().strip_prefix("CREATE TABLE "))
                .filter_map(|line| line.split_whitespace().next())
                .collect::<Vec<_>>();
            assert!(!tables.is_empty(), "tenant schema has no tables");
            for qualified in tables {
                let table = qualified.rsplit('.').next().expect("qualified table");
                assert!(
                    ddl.contains(&format!("'{table}'")),
                    "{qualified} is absent from the RLS protection inventory"
                );
            }
            assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(ddl.contains("FORCE ROW LEVEL SECURITY"));
            assert!(ddl.contains("current_setting('app.tenant_id', true)"));
        }
    }
}
