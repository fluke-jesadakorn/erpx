//! Query port for the native PMS property-readiness read. The port is pure;
//! the sqlx execution lives in `postgres.rs` and the transport in `http.rs`.

use super::domain::{PropertyReadinessFacts, RoomSummaryFacts};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type PropertyReadinessFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the Pg implementation; tests inject a
/// deterministic probe. The company and property are exact, already-validated
/// identities; no caller SQL reaches this port.
pub trait PropertyReadinessRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> PropertyReadinessFuture<'a, Option<PropertyReadinessFacts>>;
}

pub type RoomSummaryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Bounded room-list filters. Every value is already validated by the inbound
/// adapter; no caller-supplied SQL reaches this port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomSummaryQuery {
    pub property: String,
    pub status: String,
    pub q: String,
    pub building: String,
    pub floor: String,
    pub type_id: String,
}

/// Owner reader port for the room-summary family.
pub trait RoomSummaryRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a RoomSummaryQuery,
    ) -> RoomSummaryFuture<'a, Option<RoomSummaryFacts>>;
}
