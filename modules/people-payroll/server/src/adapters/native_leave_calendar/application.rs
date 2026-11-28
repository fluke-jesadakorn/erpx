//! Query port and handler for the native leave-calendar month read. No SQL
//! lives here: the owner reader executes the statements and the domain owns the
//! row shape, the month bounds and the value validation.

use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

use super::domain::{
    HolidayRow, LeaveCalendarFacts, LeaveCalendarMonth, LeaveRequestRow, invalid, month_bounds,
    month_invalid, valid_id, valid_month,
};

pub type CalendarFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// All call-site values of the retired `leaveCalendarMonth` read: the Company
/// and the account identity are isolated context values, the month is the
/// bounded request query, and `start`/`end` are derived exactly as the retired
/// application derived them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveCalendarQuery {
    pub company_id: String,
    pub user_id: String,
    pub month: String,
    pub start: String,
    pub end: String,
}

impl LeaveCalendarQuery {
    pub fn try_new(company: &str, user: &str, month: &str) -> Result<Self, BusinessError> {
        if !valid_id(company) || !valid_id(user) {
            return Err(invalid());
        }
        if !valid_month(month) {
            return Err(month_invalid());
        }
        let (start, end) = month_bounds(month);
        Ok(Self {
            company_id: company.to_owned(),
            user_id: user.to_owned(),
            month: month.to_owned(),
            start,
            end,
        })
    }
}

/// Owner reader injected by composition. The axum handler transports only
/// bounded identities and one bounded month; no SQL and no TypeScript fallback
/// is part of this port.
pub trait LeaveCalendarRead: Send + Sync {
    fn read<'a>(
        &'a self,
        query: &'a LeaveCalendarQuery,
    ) -> CalendarFuture<'a, LeaveCalendarFacts>;
}

/// Read-only handler: the query already validated at construction, so this only
/// dispatches to the owner reader and shapes the exact retired envelope.
pub async fn leave_calendar_month<R: LeaveCalendarRead + ?Sized>(
    reader: &R,
    query: &LeaveCalendarQuery,
) -> Result<LeaveCalendarMonth, BusinessError> {
    if !valid_month(&query.month) {
        return Err(month_invalid());
    }
    let facts = reader.read(query).await?;
    Ok(LeaveCalendarMonth {
        month: query.month.clone(),
        requests: facts.requests,
        holidays: facts.holidays,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_leave_calendar::domain::fixture_row;
    use std::sync::{Arc, Mutex};

    struct Probe {
        seen: Arc<Mutex<Vec<LeaveCalendarQuery>>>,
        facts: LeaveCalendarFacts,
    }

    impl LeaveCalendarRead for Probe {
        fn read<'a>(
            &'a self,
            query: &'a LeaveCalendarQuery,
        ) -> CalendarFuture<'a, LeaveCalendarFacts> {
            let seen = self.seen.clone();
            let query = query.clone();
            let facts = self.facts.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                Ok(facts)
            })
        }
    }

    #[tokio::test]
    async fn handler_dispatches_once_and_shapes_the_envelope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            seen: seen.clone(),
            facts: LeaveCalendarFacts {
                requests: vec![fixture_row()],
                holidays: vec![HolidayRow {
                    holiday_date: "2026-11-25".into(),
                    name: "Holiday".into(),
                }],
            },
        };
        let query = LeaveCalendarQuery::try_new("acme-co", "usr-1", "2026-11").unwrap();
        assert_eq!(query.start, "2026-11-01");
        assert_eq!(query.end, "2026-11-30");
        let month = leave_calendar_month(&reader, &query).await.unwrap();
        assert_eq!(month.month, "2026-11");
        assert_eq!(month.requests, vec![fixture_row()]);
        assert_eq!(month.holidays.len(), 1);
        assert_eq!(seen.lock().unwrap().as_slice(), &[query]);
    }

    #[tokio::test]
    async fn invalid_queries_never_reach_the_reader() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            seen: seen.clone(),
            facts: LeaveCalendarFacts::default(),
        };
        assert!(LeaveCalendarQuery::try_new("bad company", "usr-1", "2026-11").is_err());
        assert!(LeaveCalendarQuery::try_new("acme-co", "usr-1", "2026-13").is_err());
        // The handler re-checks the month even if a query is constructed
        // directly, before it reaches the owner reader.
        let bad_query = LeaveCalendarQuery {
            company_id: "acme-co".into(),
            user_id: "usr-1".into(),
            month: "2026-13".into(),
            start: "2026-13-01".into(),
            end: "2026-13-31".into(),
        };
        assert!(leave_calendar_month(&reader, &bad_query).await.is_err());
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_facts_are_the_retired_staff_link_fallback() {
        let facts = LeaveCalendarFacts::default();
        assert!(facts.requests.is_empty());
        assert!(facts.holidays.is_empty());
        let _: Vec<LeaveRequestRow> = facts.requests;
    }
}
