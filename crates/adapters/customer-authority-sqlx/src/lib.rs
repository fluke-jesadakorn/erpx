#![doc = "PostgreSQL command, query, and projection adapters for Customer Authority."]

mod authority_records;
mod command;
mod portal_binding;
mod projection;
mod read;

pub use authority_records::{
    CustomerAuthorityRecordProjector, CustomerAuthorityRecordReadAdapter,
    CustomerAuthorityRecordSqlxAdapter,
};
pub use command::{CustomerAuthorityCommandAdapter, CustomerAuthorityWriteRepository};
pub use portal_binding::{CustomerPortalBindingSqlxAdapter, SqlxCustomerRecordAuthority};
pub use projection::{
    CustomerAuthorityOutboxPayload, CustomerAuthorityProjector, CustomerAuthoritySnapshot,
};
pub use read::CustomerAuthorityReadRepository;

pub const CONTEXT_NAME: &str = "customer_authority";
pub const AGGREGATE_TYPE: &str = "customer_account";
pub const PROJECTION_NAME: &str = "customer_authority.customers";
pub const SNAPSHOT_EVENT_TYPE: &str = "customer_authority.customer_snapshot";
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
pub const CONTEXT_WRITE_EVIDENCE_DDL: &str = include_str!("../ddl/write.sql");
pub const CONTEXT_READ_EVIDENCE_DDL: &str = include_str!("../ddl/read.sql");
pub const WRITE_DDL: &str = CONTROL_WRITE_DDL;
pub const READ_DDL: &str = CONTROL_READ_DDL;

pub fn customer_partition(customer_id: erpx_customer_authority::CustomerId) -> String {
    format!(
        "customer_authority.customer:{}",
        customer_id.as_uuid().simple()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partition_is_stable_and_customer_specific() {
        let customer_id =
            erpx_customer_authority::CustomerId::try_from_uuid(Uuid::from_u128(42)).unwrap();
        assert_eq!(
            customer_partition(customer_id),
            "customer_authority.customer:0000000000000000000000000000002a"
        );
    }

    #[test]
    fn canonical_migrations_declare_control_foundation_guards_and_projections() {
        assert!(CONTROL_WRITE_FOUNDATION_DDL.contains("erpx_cqrs_write.command_receipts"));
        assert!(!CONTROL_WRITE_FOUNDATION_DDL.contains("aggregate_snapshots"));
        assert!(CONTROL_WRITE_CONTEXT_DDL.contains("erpx_customer_authority_write.account_guards"));
        assert!(CONTROL_WRITE_CONTEXT_DDL.contains("authority_link_guards"));
        assert!(CONTROL_READ_FOUNDATION_DDL.contains("erpx_cqrs_read.projection_generations"));
        assert!(
            CONTROL_READ_CONTEXT_DDL.contains("erpx_customer_authority_read.customer_projection")
        );
        assert!(CONTROL_READ_CONTEXT_DDL.contains("authority_link_projection"));
        assert!(CONTROL_WRITE_DDL.starts_with(CONTROL_WRITE_FOUNDATION_DDL));
        assert!(CONTROL_WRITE_DDL.ends_with(CONTROL_WRITE_CONTEXT_DDL));
        assert!(CONTROL_READ_DDL.starts_with(CONTROL_READ_FOUNDATION_DDL));
        assert!(CONTROL_READ_DDL.ends_with(CONTROL_READ_CONTEXT_DDL));
        assert_eq!(WRITE_DDL, CONTROL_WRITE_DDL);
        assert_eq!(READ_DDL, CONTROL_READ_DDL);
        assert!(CONTEXT_WRITE_EVIDENCE_DDL.contains("account_guards"));
        assert!(CONTEXT_READ_EVIDENCE_DDL.contains("customer_projection"));
    }

    #[test]
    fn read_adapter_has_no_write_capability_or_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains("WritePool"));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
        assert!(!source.contains("registration_number"));
        assert!(!source.contains("tax_identifier"));
    }
}
