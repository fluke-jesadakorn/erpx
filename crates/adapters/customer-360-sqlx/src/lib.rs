#![doc = "PostgreSQL projections and composed read adapter for Customer 360."]
#![forbid(unsafe_code)]

mod owned_records;
mod projection;
mod read;

pub use owned_records::{
    Customer360OwnedRecordProjector, Customer360OwnedRecordReadAdapter,
    Customer360RecordSqlxAdapter,
};
pub use projection::{ControlCustomerProjector, HoldingActivityProjector};
pub use read::Customer360ReadRepository;

pub use erpx_customer_360::{CONTROL_CUSTOMERS_PROJECTION_NAME, HOLDING_ACTIVITY_PROJECTION_NAME};

pub const CONTROL_PROFILE_EVENT: &str = "customer.profile_changed";
pub const SUBSCRIPTION_EVENT: &str = "commercial.subscription_changed";
pub const OPPORTUNITY_EVENT: &str = "crm.opportunity_changed";
pub const SALES_ORDER_EVENT: &str = "sales.order_changed";
pub const SUPPORT_CASE_EVENT: &str = "support.case_changed";
pub const EVENT_VERSION: u32 = 1;

pub const READ_DDL: &str = include_str!("../../../migration/migrations_read/0001_baseline.sql");

pub fn control_partition(customer: erpx_integration_contracts::customer::CustomerRef) -> String {
    format!(
        "customer_authority.customer:{}",
        customer.as_uuid().simple()
    )
}

#[cfg(test)]
mod tests {
    use erpx_integration_contracts::customer::CustomerRef;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn frozen_schema_and_projection_names_are_source_qualified() {
        assert!(READ_DDL.contains(CONTROL_CUSTOMERS_PROJECTION_NAME));
        assert!(READ_DDL.contains(HOLDING_ACTIVITY_PROJECTION_NAME));
        assert!(READ_DDL.contains("projection_source_checkpoints"));
        assert!(READ_DDL.contains("source_key"));
        assert!(READ_DDL.contains("customer_profile_projection"));
        assert!(READ_DDL.contains("support_case_projection"));
    }

    #[test]
    fn control_partition_matches_the_customer_authority_owner() {
        let customer = CustomerRef::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            control_partition(customer),
            "customer_authority.customer:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
        assert!(source.contains("customer_profile_projection"));
        assert!(source.contains("holding_activity"));
    }
}
