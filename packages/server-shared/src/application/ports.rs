use crate::domain::identity::{RecordId, TimestampMillis};

/// Wall-clock source owned by the caller; no shared port opens a transaction.
pub trait Clock {
    fn now(&self) -> TimestampMillis;
}

/// Deterministic identifier source owned by the caller.
pub trait IdentifierSource {
    fn next(&mut self) -> RecordId;
}

// Transaction, command-receipt, audit and outbox witnesses are not declared here.
// The adopted single implementations are `erpx_server_runtime::public::postgres::
// PgUnitOfWork` (one Holding transaction, named session locks, bounded replay) and
// `erpx_server_workflow::public::commands::PgCommandLedger` (receipt, owner effects
// and outbox retain in the caller's transaction). Declaring trait copies here
// created a second, unimplemented authority surface with no production consumer.
