#![doc = "PostgreSQL command, read, and projection adapters for Sales."]

mod command;
mod projection;
mod read;

pub use command::{SalesCommandAdapter, SalesWriteRepository};
pub use projection::{
    SalesOrderProjector, SalesOutboxPayload, SalesQuoteProjector, SalesRevenueArrangementProjector,
    SalesSnapshot,
};
pub use read::{SalesReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "sales";
pub const QUOTE_AGGREGATE_TYPE: &str = "quote";
pub const ORDER_AGGREGATE_TYPE: &str = "sales_order";
pub const ARRANGEMENT_AGGREGATE_TYPE: &str = "revenue_arrangement";
pub const QUOTE_PROJECTION: &str = "sales.quotes";
pub const ORDER_PROJECTION: &str = "sales.orders";
pub const ARRANGEMENT_PROJECTION: &str = "sales.revenue_arrangements";
pub const QUOTE_SNAPSHOT_EVENT: &str = "sales.quote_snapshot";
pub const ORDER_SNAPSHOT_EVENT: &str = "sales.order_snapshot";
pub const ARRANGEMENT_SNAPSHOT_EVENT: &str = "sales.revenue_arrangement_snapshot";
pub const EVENT_VERSION: u32 = 1;
#[cfg(test)]
pub const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0061_wave7_transactional_context_guards.sql"
);
#[cfg(test)]
pub const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0053_wave7_transactional_context_projections.sql"
);
#[cfg(test)]
pub const REVENUE_WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0064_unified_revenue_arrangement_guards.sql"
);
#[cfg(test)]
pub const REVENUE_READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0056_unified_revenue_arrangement_projection.sql"
);

pub fn quote_partition(id: erpx_sales::QuoteId) -> String {
    format!("sales.quote:{}", id.as_uuid().simple())
}

pub fn order_partition(id: erpx_sales::SalesOrderId) -> String {
    format!("sales.order:{}", id.as_uuid().simple())
}

pub fn arrangement_partition(id: erpx_sales::RevenueArrangementId) -> String {
    format!("sales.arrangement:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn frozen_schema_and_partitions_are_stable() {
        let quote = erpx_sales::QuoteId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let order = erpx_sales::SalesOrderId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        let arrangement =
            erpx_sales::RevenueArrangementId::try_from_uuid(Uuid::from_u128(3)).unwrap();
        assert!(quote_partition(quote).starts_with("sales.quote:"));
        assert!(order_partition(order).starts_with("sales.order:"));
        assert!(arrangement_partition(arrangement).starts_with("sales.arrangement:"));
        assert!(WRITE_DDL.contains("erpx_sales_write.quote_guards"));
        assert!(READ_DDL.contains("sales.orders"));
        assert!(REVENUE_WRITE_DDL.contains("revenue_arrangement_guards"));
        assert!(REVENUE_READ_DDL.contains("sales.revenue_arrangements"));
    }

    #[test]
    fn read_adapter_has_no_write_store_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
    }
}
