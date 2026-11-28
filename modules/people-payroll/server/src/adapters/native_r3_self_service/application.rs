//! Ports for the native R3 self-service summary read. No SQL and no HTTP here.
//! The next-shift fact is People-owned and executed in `postgres`; the remaining
//! sources belong to Operations, Leave, Overtime, Work and Records owners.

use super::domain::{NextShift, SummaryActor, SummaryResult};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type OwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// People-owned next Shift selection: published week, assigned engagement,
/// active shift, ending after `now`, earliest start. One row at most.
pub trait NextShiftRead: Send + Sync {
    fn next_shift<'a>(
        &'a self,
        company: &'a str,
        engagement: &'a str,
        now: i64,
    ) -> OwnerFuture<'a, Option<NextShift>>;
}

/// Composition-owned aggregate. It already receives the People-owned shift;
/// it must not re-read `finance_expenses` or `people_roster_*` tables itself.
/// The work count is the value the retired handler wrote to the response
/// header (`null` becomes the literal `unknown`).
pub trait SummaryAggregatePort: Send + Sync {
    fn summarize<'a>(
        &'a self,
        actor: &'a SummaryActor,
        next_shift: Option<&'a NextShift>,
        now: i64,
    ) -> OwnerFuture<'a, SummaryResult>;
}
