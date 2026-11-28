//! Public surface for the native handover acknowledgement POST. Composition
//! mounts the router and binds the workflow ledger; TypeScript consumers keep
//! a strict transport, never the retired `acknowledgeHandover` SQL.

pub use crate::adapters::native_r3_handover_ack_write::{
    AckWriteTarget, PgAckWriteStore, configured, router, router_for_holdings,
};
pub use crate::application::native_r3_handover_ack_write::{
    AckCommit, AckCommandLedger, AckReceipt, AckWriteHandler, AckWriteStore,
};
pub use crate::domain::native_r3_handover_ack_write::{
    ACK_INSERT_SQL, ACK_OPERATION, RECEIPT_SQL, RECIPIENT_GUARD_SQL, invalid_key,
    invalid_version, origin_forbidden, parse_version, recipient_required,
    source_changed, validate_key,
};
