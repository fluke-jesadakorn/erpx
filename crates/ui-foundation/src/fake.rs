use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
};

use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};

use crate::{CommandGateway, QueryGateway, UiError, UiErrorKind};

#[derive(Debug)]
pub struct FakeCommandGateway<C, R> {
    responses: Mutex<VecDeque<Result<CommandReceipt<R>, UiError>>>,
    received: Mutex<Vec<CommandEnvelope<C>>>,
}

impl<C, R> FakeCommandGateway<C, R> {
    pub fn from_responses(
        responses: impl IntoIterator<Item = Result<CommandReceipt<R>, UiError>>,
    ) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn call_count(&self) -> usize {
        lock(&self.received).len()
    }

    pub fn received(&self) -> Vec<CommandEnvelope<C>>
    where
        C: Clone,
    {
        lock(&self.received).clone()
    }

    pub fn remaining_responses(&self) -> usize {
        lock(&self.responses).len()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<C, R> CommandGateway<C> for FakeCommandGateway<C, R>
where
    C: Send,
    R: Send,
{
    type Response = R;

    async fn execute(
        &self,
        envelope: CommandEnvelope<C>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        lock(&self.received).push(envelope);
        lock(&self.responses).pop_front().unwrap_or_else(|| {
            Err(UiError::bounded(
                UiErrorKind::Failed,
                "fake_response_exhausted",
                "The deterministic command fake has no response queued.",
                false,
                None,
            ))
        })
    }
}

#[derive(Debug)]
pub struct FakeQueryGateway<Q, R> {
    responses: Mutex<VecDeque<Result<QueryResult<R>, UiError>>>,
    received: Mutex<Vec<QueryEnvelope<Q>>>,
}

impl<Q, R> FakeQueryGateway<Q, R> {
    pub fn from_responses(
        responses: impl IntoIterator<Item = Result<QueryResult<R>, UiError>>,
    ) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            received: Mutex::new(Vec::new()),
        }
    }

    pub fn call_count(&self) -> usize {
        lock(&self.received).len()
    }

    pub fn received(&self) -> Vec<QueryEnvelope<Q>>
    where
        Q: Clone,
    {
        lock(&self.received).clone()
    }

    pub fn remaining_responses(&self) -> usize {
        lock(&self.responses).len()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<Q, R> QueryGateway<Q> for FakeQueryGateway<Q, R>
where
    Q: Send,
    R: Send,
{
    type Response = R;

    async fn execute(
        &self,
        envelope: QueryEnvelope<Q>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        lock(&self.received).push(envelope);
        lock(&self.responses).pop_front().unwrap_or_else(|| {
            Err(UiError::bounded(
                UiErrorKind::Failed,
                "fake_response_exhausted",
                "The deterministic query fake has no response queued.",
                false,
                None,
            ))
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fakes_fail_deterministically_without_transport_access() {
        let command = FakeCommandGateway::<(), ()>::from_responses([]);
        let query = FakeQueryGateway::<(), ()>::from_responses([]);
        assert_eq!(command.remaining_responses(), 0);
        assert_eq!(query.remaining_responses(), 0);
        assert_eq!(command.call_count(), 0);
        assert_eq!(query.call_count(), 0);
    }
}
