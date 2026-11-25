//! Query port and handler for the native settlement event-history read.

use super::domain::{
    SettlementEventFact, SettlementEventsRequest, SettlementEventsResponse,
};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type SettlementEventsFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait SettlementEventRead: Send + Sync {
    fn history<'a>(
        &'a self,
        request: &'a SettlementEventsRequest,
    ) -> SettlementEventsFuture<'a, Vec<SettlementEventFact>>;
}

impl SettlementEventRead for std::sync::Arc<dyn SettlementEventRead> {
    fn history<'a>(
        &'a self,
        request: &'a SettlementEventsRequest,
    ) -> SettlementEventsFuture<'a, Vec<SettlementEventFact>> {
        (**self).history(request)
    }
}

pub struct ReadSettlementEvents<R: SettlementEventRead>(R);

impl<R: SettlementEventRead> ReadSettlementEvents<R> {
    pub fn new(reader: R) -> Self {
        Self(reader)
    }

    pub fn reader(&self) -> &R {
        &self.0
    }

    pub async fn execute(
        &self,
        request: SettlementEventsRequest,
    ) -> Result<SettlementEventsResponse, BusinessError> {
        request.validate()?;
        Ok(self.0.history(&request).await?.into())
    }
}
