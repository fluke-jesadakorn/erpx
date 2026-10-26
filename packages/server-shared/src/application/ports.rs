use std::{future::Future, pin::Pin};

use crate::domain::{
    error::BusinessError,
    identity::{RecordId, TimestampMillis},
};

pub trait Clock {
    fn now(&self) -> TimestampMillis;
}

pub trait IdentifierSource {
    fn next(&mut self) -> RecordId;
}

pub type TransactionFuture<'a, Output> =
    Pin<Box<dyn Future<Output = Result<Output, BusinessError>> + Send + 'a>>;

/// An adapter constructs all owner ports against one transaction. A nested
/// owner receives those ports, never a pool, SQL statement or fresh connection.
/// Owned outputs cannot retain a borrowed transaction after this boundary ends.
pub trait UnitOfWork<Ports> {
    fn run<'a, Output, Work>(&'a mut self, work: Work) -> TransactionFuture<'a, Output>
    where
        Output: Send + 'static,
        Work: for<'tx> FnOnce(&'tx mut Ports) -> TransactionFuture<'tx, Output> + Send + 'a;
}

pub trait CommandReceipts<Identity, Output> {
    fn replay(
        &mut self,
        identity: &Identity,
    ) -> impl Future<Output = Result<Option<Output>, BusinessError>> + Send;
    fn save(
        &mut self,
        identity: &Identity,
        result: &Output,
    ) -> impl Future<Output = Result<(), BusinessError>> + Send;
}

pub trait AuditSink<Event> {
    fn append(&mut self, event: Event) -> impl Future<Output = Result<(), BusinessError>> + Send;
}

pub trait Outbox<Event> {
    fn enqueue(&mut self, event: Event) -> impl Future<Output = Result<(), BusinessError>> + Send;
}

pub trait Authorization<Actor> {
    fn require(
        &mut self,
        actor: &Actor,
        permission: &str,
    ) -> impl Future<Output = Result<(), BusinessError>> + Send;
}
