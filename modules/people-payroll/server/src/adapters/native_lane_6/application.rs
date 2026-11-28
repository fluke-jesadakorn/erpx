//! Query port for the native lane-6 calendar-availability read. No SQL here:
//! the Postgres adapter owns the statement and the domain owns validation.

use super::domain::{LeaveDay, LeaveDayQuery};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
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

pub type PersonScopeFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader for the Company-scoped calendar person-scope presence probe.
/// It owns the byte-identical statement and the boolean projection; no SQL or
/// fallback lives in the TypeScript caller.
pub trait CalendarPersonScopeRead: Send + Sync {
    fn present<'a>(
        &'a self,
        company: &'a str,
        person: &'a str,
        scope_ids: &'a str,
    ) -> PersonScopeFuture<'a, bool>;
}

pub type CalendarPeopleFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader for the Company calendar-access people list. It owns the
/// byte-identical statement and the typed projection; no SQL lives in TS.
pub trait CalendarPeopleRead: Send + Sync {
    fn people<'a>(
        &'a self,
        company: &'a str,
        query: &'a super::domain::CalendarPeopleQuery,
    ) -> CalendarPeopleFuture<'a, Vec<super::domain::CalendarPerson>>;
}

/// Fail-closed readers used by the narrower test construction seams.
pub struct UnavailableCalendarPeopleRead;

impl CalendarPeopleRead for UnavailableCalendarPeopleRead {
    fn people<'a>(
        &'a self,
        _company: &'a str,
        _query: &'a super::domain::CalendarPeopleQuery,
    ) -> CalendarPeopleFuture<'a, Vec<super::domain::CalendarPerson>> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE",
                "The native People Payroll availability service is unavailable.",
            ))
        })
    }
}

pub struct UnavailableCalendarPersonScopeRead;

impl CalendarPersonScopeRead for UnavailableCalendarPersonScopeRead {
    fn present<'a>(
        &'a self,
        _company: &'a str,
        _person: &'a str,
        _scope_ids: &'a str,
    ) -> PersonScopeFuture<'a, bool> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE",
                "The native People Payroll availability service is unavailable.",
            ))
        })
    }
}
