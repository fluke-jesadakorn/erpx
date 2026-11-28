//! Query port for the native PMS property-readiness read. The port is pure;
//! the sqlx execution lives in `postgres.rs` and the transport in `http.rs`.

use super::domain::{CalendarRecord, ImportBatchRow, PropertyReadinessFacts, RoomSummaryFacts};
use erpx_platform_shared_server::public::BusinessError;
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

pub type CalendarFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Bounded inclusive calendar window for one Property.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarQuery {
    pub property: String,
    pub from: String,
    pub to: String,
}

pub trait CalendarRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        query: &'a CalendarQuery,
    ) -> CalendarFuture<'a, Option<Vec<CalendarRecord>>>;
}

pub type ImportBatchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port for the bounded cutover-import batch list.
pub trait ImportBatchRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        property: &'a str,
    ) -> ImportBatchFuture<'a, Option<Vec<ImportBatchRow>>>;
}
