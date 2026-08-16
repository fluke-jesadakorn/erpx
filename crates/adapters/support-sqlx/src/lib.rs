#![doc = "PostgreSQL command, read, and projection adapters for Support."]
#![forbid(unsafe_code)]

mod command;
mod platform_case;
mod projection;
mod read;

pub use command::{SupportCommandAdapter, SupportWriteRepository};
pub use platform_case::{PlatformCaseProjector, PlatformCaseReadAdapter, PlatformCaseSqlxAdapter};
pub use projection::{
    CapabilityProjector, IncidentProjector, SessionProjector, SupportOutboxPayload,
    SupportSnapshot, TicketProjector,
};
pub use read::{SupportReadRepository, decode_cursor, encode_cursor};

pub const CONTEXT_NAME: &str = "support";
pub const TICKET_AGGREGATE_TYPE: &str = "support_ticket";
pub const INCIDENT_AGGREGATE_TYPE: &str = "support_incident";
pub const SESSION_AGGREGATE_TYPE: &str = "support_session";
pub const CAPABILITY_AGGREGATE_TYPE: &str = "support_capability";
pub const TICKET_PROJECTION: &str = "support.tickets";
pub const INCIDENT_PROJECTION: &str = "support.incidents";
pub const SESSION_PROJECTION: &str = "support.sessions";
pub const CAPABILITY_PROJECTION: &str = "support.capabilities";
pub const TICKET_SNAPSHOT_EVENT: &str = "support.ticket_snapshot";
pub const INCIDENT_SNAPSHOT_EVENT: &str = "support.incident_snapshot";
pub const SESSION_SNAPSHOT_EVENT: &str = "support.session_snapshot";
pub const CAPABILITY_SNAPSHOT_EVENT: &str = "support.capability_snapshot";
pub const AUTHORITY_EFFECT_EVENT: &str = "support.authority_effect";
pub const EVENT_VERSION: u32 = 1;

#[cfg(test)]
const WRITE_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0062_wave8_dependent_context_guards.sql"
);
#[cfg(test)]
const NORMALIZED_AUTHORITY_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_write/0081_current_normalized_authority.sql"
);
#[cfg(test)]
const READ_DDL: &str = include_str!(
    "../../../migration/migrations_tenant_read/0054_wave8_dependent_context_projections.sql"
);

pub fn ticket_partition(id: erpx_support::SupportTicketId) -> String {
    format!("support.ticket:{}", id.as_uuid().simple())
}
pub fn incident_partition(id: erpx_support::IncidentId) -> String {
    format!("support.incident:{}", id.as_uuid().simple())
}
pub fn session_partition(id: erpx_support::SessionId) -> String {
    format!("support.session:{}", id.as_uuid().simple())
}
pub fn capability_partition(id: erpx_support::CapabilityId) -> String {
    format!("support.capability:{}", id.as_uuid().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn partitions_and_frozen_schema_are_stable() {
        let ticket = erpx_support::SupportTicketId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let capability = erpx_support::CapabilityId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert!(ticket_partition(ticket).starts_with("support.ticket:"));
        assert!(capability_partition(capability).starts_with("support.capability:"));
        assert!(WRITE_DDL.contains("secret_hash_sha256 BYTEA NOT NULL"));
        assert!(READ_DDL.contains(TICKET_PROJECTION));
        assert!(!READ_DDL.contains("capability_secret"));
    }

    #[test]
    fn read_adapter_has_no_write_snapshot_fallback() {
        let source = include_str!("read.rs");
        assert!(!source.contains(concat!("Write", "Pool")));
        assert!(!source.contains(concat!("aggregate_", "snapshots")));
        assert!(source.contains("ticket_customer_message_projection"));
        assert!(source.contains("ticket_internal_message_projection"));
    }

    #[test]
    fn context_guards_share_the_atomic_command_transaction() {
        let source = include_str!("command.rs");
        assert!(source.contains("commit_normalized"));
        assert!(source.contains("impl AtomicCommandGuard for SupportAtomicGuard"));
        assert!(!source.contains("aggregate_state"));
        assert!(source.contains("audit: AuditWrite"));
        assert!(source.contains("events,"));
        assert!(!source.contains(concat!("load_", "snapshot")));
        assert!(!source.contains(concat!("commit_", "guarded")));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("BEGIN SUPPORT_NORMALIZED_AUTHORITY"));
        assert!(NORMALIZED_AUTHORITY_DDL.contains("FORCE ROW LEVEL SECURITY"));
    }
}
