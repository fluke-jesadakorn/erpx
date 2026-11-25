//! Query port for the native lane-6 calendar-availability read. No SQL here:
//! the Postgres adapter owns the statement and the domain owns validation.

use super::domain::{LeaveDay, LeaveDayQuery};
use erpx_server_shared::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type LeaveDayFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader behind the axum router. Composition injects the Pg-backed
/// implementation; the HTTP layer transports only bounded request facts.
pub trait CalendarAvailabilityRead: Send + Sync {
    fn leave_days<'a>(
        &'a self,
        company: &'a str,
        query: &'a LeaveDayQuery,
    ) -> LeaveDayFuture<'a, Vec<LeaveDay>>;
}
