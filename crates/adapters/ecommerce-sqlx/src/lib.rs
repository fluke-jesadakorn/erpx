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
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const READ_DDL: &str = include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
const CHANNEL_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
const AGENT_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const AGENT_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
const PROJECTS_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const PROJECTS_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");
#[cfg(test)]
const SHARED_WRITE_DDL: &str =
    include_str!("../../../migration/migrations_holding_write/0001_baseline.sql");
#[cfg(test)]
const SHARED_READ_DDL: &str =
    include_str!("../../../migration/migrations_holding_read/0001_baseline.sql");

pub fn aggregate_partition(aggregate_type: &str, id: uuid::Uuid) -> String {
    format!("ecommerce.{aggregate_type}:{}", id.simple())
}

#[cfg(test)]
#[path = "../../schema_test.rs"]
mod schema_test;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_schema_is_normalized_atomic_and_holding_bound() {
        let owned = schema_test::select_schema_statements(WRITE_DDL, &["erpx_ecommerce_write"]);
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
        assert!(!owned.contains("aggregate_guards"));
        assert!(!owned.contains("state JSONB"));
        assert!(WRITE_DDL.contains("ENABLE ROW LEVEL SECURITY"));
        assert!(WRITE_DDL.contains("FORCE ROW LEVEL SECURITY"));
        assert!(WRITE_DDL.contains("current_setting('app.holding_id', true)"));
        assert!(owned.contains("CHECK ((order_receipt IS NULL) = (order_ref IS NULL))"));
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
    fn every_new_holding_table_is_explicitly_bound_and_rls_listed() {
        for (ddl, schemas) in [
            (AGENT_WRITE_DDL, &["erpx_agent_write"][..]),
            (AGENT_READ_DDL, &["erpx_agent_read"][..]),
            (PROJECTS_WRITE_DDL, &["erpx_projects_write"][..]),
            (PROJECTS_READ_DDL, &["erpx_projects_read"][..]),
            (WRITE_DDL, &["erpx_ecommerce_write"][..]),
            (READ_DDL, &["erpx_ecommerce_read"][..]),
            (SHARED_WRITE_DDL, &["erpx_shared_write"][..]),
            (SHARED_READ_DDL, &["erpx_shared_read"][..]),
        ] {
            let owned = schema_test::select_schema_statements(ddl, schemas);
            let tables = owned
                .lines()
                .filter_map(|line| line.trim().strip_prefix("CREATE TABLE "))
                .map(|line| line.strip_prefix("IF NOT EXISTS ").unwrap_or(line))
                .filter_map(|line| line.split_whitespace().next())
                .collect::<Vec<_>>();
            assert!(!tables.is_empty(), "holding schema has no tables");
            for qualified in tables {
                let table = qualified.rsplit('.').next().expect("qualified table");
                assert!(
                    ddl.contains(&format!("'{table}'"))
                        || ddl.contains(&format!("'{qualified}'"))
                        || ddl
                            .contains(&format!("ALTER TABLE {qualified} FORCE ROW LEVEL SECURITY")),
                    "{qualified} is absent from the RLS protection inventory"
                );
            }
            assert!(ddl.contains("ENABLE ROW LEVEL SECURITY"));
            assert!(ddl.contains("FORCE ROW LEVEL SECURITY"));
            assert!(ddl.contains("current_setting('app.holding_id', true)"));
        }
    }
}
