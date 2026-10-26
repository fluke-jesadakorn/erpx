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

pub type TransactionFuture<'a, Output, Error = BusinessError> =
    Pin<Box<dyn Future<Output = Result<Output, Error>> + Send + 'a>>;

/// A replay coordinator constructs fresh owner ports for every serializable attempt.
/// The callback may run again after a database conflict; external effects belong
/// in an outbox, and owned outputs cannot retain a transaction or leased port.
/// A retained cross-database authorization lease is a separate one-attempt contract.
pub trait UnitOfWork<Ports> {
    type Error: From<BusinessError> + Send + 'static;
    fn run<'a, Output, Work>(
        &'a mut self,
        work: Work,
    ) -> TransactionFuture<'a, Output, Self::Error>
    where
        Output: Send + 'static,
        Work: for<'tx> FnMut(&'tx mut Ports) -> TransactionFuture<'tx, Output, Self::Error>
            + Send
            + 'a;
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
