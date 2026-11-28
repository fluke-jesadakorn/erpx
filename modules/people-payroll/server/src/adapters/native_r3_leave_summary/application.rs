//! Query port and handler for the native leave-summary read. No SQL lives here:
//! the owner reader executes the statements and the domain owns the row shape,
//! the UTC clock derivation and the bounded identity checks.

use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

use super::domain::{
    BalanceView, LeaveSummary, LeaveSummaryFacts, NO_POLICY_MESSAGE, NO_STAFF_MESSAGE, PolicyFact,
    SummaryPolicy, invalid, utc_date, utc_year, valid_id,
};

pub type SummaryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// All call-site values of the retired `leaveSummary` read. The Company and
/// account identity are isolated context values; `now_ms` is the owner clock
/// instant from which the retired date and year derivations are made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveSummaryQuery {
    pub company_id: String,
    pub user_id: String,
    pub now_ms: i64,
    pub today: String,
    pub year: i32,
}

impl LeaveSummaryQuery {
    pub fn try_new(company: &str, user: &str, now_ms: i64) -> Result<Self, BusinessError> {
        if !valid_id(company) || !valid_id(user) || now_ms < 0 {
            return Err(invalid());
        }
        Ok(Self {
            company_id: company.to_owned(),
            user_id: user.to_owned(),
            now_ms,
            today: utc_date(now_ms),
            year: utc_year(now_ms),
        })
    }
}

/// Owner reader injected by composition. The router transports only bounded
/// identities and one bounded clock instant; no SQL and no TypeScript fallback
/// is part of this port.
pub trait LeaveSummaryRead: Send + Sync {
    fn read<'a>(
        &'a self,
        query: &'a LeaveSummaryQuery,
    ) -> SummaryFuture<'a, LeaveSummaryFacts>;
}

fn setup_payload(as_of: i64, message: &str) -> LeaveSummary {
    LeaveSummary {
        as_of,
        setup_required: true,
        setup_message: Some(message.to_owned()),
        policy: None,
        balances: vec![],
        upcoming: vec![],
        pending: vec![],
        history: vec![],
        holidays: vec![],
    }
}

/// Read-only handler: the query already validated at construction, so this
/// only dispatches to the owner reader and shapes the exact retired envelope.
pub async fn leave_summary<R: LeaveSummaryRead + ?Sized>(
    reader: &R,
    query: &LeaveSummaryQuery,
) -> Result<LeaveSummary, BusinessError> {
    if !valid_id(&query.company_id) || !valid_id(&query.user_id) {
        return Err(invalid());
    }
    let facts = reader.read(query).await?;
    if !facts.staff_linked {
        return Ok(setup_payload(query.now_ms, NO_STAFF_MESSAGE));
    }

    let assigned = facts.policy;
    let policy = assigned.as_ref().map(|row| SummaryPolicy {
        id: row.policy_version_id.clone(),
        name: row.policy_name.clone(),
        version: row.policy_version,
        timezone: row.timezone.clone(),
    });
    // The retired `leaveSummary` reads types only when a policy is assigned;
    // without one there are no balances even if a reader supplied stale types.
    let balances = if assigned.is_some() {
        facts
            .types
            .into_iter()
            .map(|leave_type| {
                let balance = facts
                    .balances
                    .iter()
                    .find(|item| item.leave_type_id == leave_type.id)
                    .cloned()
                    .unwrap_or_default();
                BalanceView {
                    leave_type,
                    available: balance.available,
                    reserved: balance.reserved,
                    used: balance.used,
                    accrued: balance.accrued,
                }
            })
            .collect()
    } else {
        vec![]
    };
    let holidays = if assigned.is_some() {
        facts.holidays
    } else {
        vec![]
    };
    let upcoming = facts
        .requests
        .iter()
        .filter(|row| {
            row.state == "approved"
                && row.end_date.as_deref().unwrap_or(&row.start_date) >= query.today.as_str()
        })
        .cloned()
        .collect();
    let pending = facts
        .requests
        .iter()
        .filter(|row| row.state == "pending_approval")
        .cloned()
        .collect();

    Ok(LeaveSummary {
        as_of: query.now_ms,
        setup_required: assigned.is_none(),
        setup_message: if assigned.is_none() {
            Some(NO_POLICY_MESSAGE.to_owned())
        } else {
            None
        },
        policy,
        balances,
        upcoming,
        pending,
        history: facts.requests,
        holidays,
    })
}

/// The retired `policyFor` ambiguity invariant: two assignments with different
/// ids for the same engagement and date must fail closed.
pub fn assert_unambiguous(policy: Option<&PolicyFact>, second: Option<&PolicyFact>) -> Result<(), BusinessError> {
    if let (Some(first), Some(other)) = (policy, second) {
        if first.assignment_id != other.assignment_id {
            return Err(super::domain::policy_ambiguous());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_r3_leave_summary::domain::{
        Balance, HolidayRow, fixture_row, fixture_type, policy_ambiguous,
    };
    use std::sync::{Arc, Mutex};

    struct Probe {
        facts: LeaveSummaryFacts,
        fail: bool,
        seen: Arc<Mutex<Vec<LeaveSummaryQuery>>>,
    }

    impl LeaveSummaryRead for Probe {
        fn read<'a>(
            &'a self,
            query: &'a LeaveSummaryQuery,
        ) -> SummaryFuture<'a, LeaveSummaryFacts> {
            let seen = self.seen.clone();
            let query = query.clone();
            let facts = self.facts.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                if fail {
                    return Err(crate::adapters::native_r3_leave_summary::domain::unavailable());
                }
                Ok(facts)
            })
        }
    }

    fn facts() -> LeaveSummaryFacts {
        LeaveSummaryFacts {
            staff_linked: true,
            policy: Some(PolicyFact {
                assignment_id: "assignment-1".into(),
                policy_version_id: "policy-1".into(),
                policy_name: "Standard".into(),
                policy_version: 1,
                timezone: "Asia/Bangkok".into(),
                holiday_calendar_id: Some("calendar-1".into()),
            }),
            types: vec![fixture_type()],
            balances: vec![Balance {
                leave_type_id: "type-1".into(),
                available: 8.0,
                reserved: 0.0,
                used: 2.0,
                accrued: 0.0,
            }],
            requests: vec![fixture_row()],
            holidays: vec![HolidayRow {
                holiday_date: "2026-11-25".into(),
                name: "Holiday".into(),
            }],
        }
    }

    #[tokio::test]
    async fn handler_dispatches_once_and_shapes_the_envelope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            facts: facts(),
            fail: false,
            seen: seen.clone(),
        };
        // 2025-11-11 is before the 2026-11-02 request, so it is upcoming.
        let query = LeaveSummaryQuery::try_new("acme-co", "usr-1", 1_763_000_000_000).unwrap();
        let summary = leave_summary(&reader, &query).await.unwrap();
        assert_eq!(summary.as_of, 1_763_000_000_000);
        assert!(!summary.setup_required);
        assert_eq!(summary.policy.as_ref().unwrap().name, "Standard");
        assert_eq!(summary.balances[0].available, 8.0);
        assert_eq!(summary.upcoming.len(), 1);
        assert!(summary.pending.is_empty());
        assert_eq!(summary.history.len(), 1);
        assert_eq!(summary.holidays.len(), 1);
        assert_eq!(seen.lock().unwrap().as_slice(), &[query]);
    }

    #[tokio::test]
    async fn staff_link_fallback_is_fixed_and_empty() {
        let mut data = facts();
        data.staff_linked = false;
        let reader = Probe {
            facts: data,
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let query = LeaveSummaryQuery::try_new("acme-co", "usr-1", 1_763_000_000_000).unwrap();
        let summary = leave_summary(&reader, &query).await.unwrap();
        assert!(summary.setup_required);
        assert_eq!(summary.setup_message.as_deref(), Some(NO_STAFF_MESSAGE));
        assert!(summary.history.is_empty());
        assert!(summary.holidays.is_empty());
    }

    #[tokio::test]
    async fn missing_policy_keeps_requests_but_drops_types_and_holidays() {
        let mut data = facts();
        data.policy = None;
        data.holidays.clear();
        let reader = Probe {
            facts: data,
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let query = LeaveSummaryQuery::try_new("acme-co", "usr-1", 1_763_000_000_000).unwrap();
        let summary = leave_summary(&reader, &query).await.unwrap();
        assert!(summary.setup_required);
        assert_eq!(summary.setup_message.as_deref(), Some(NO_POLICY_MESSAGE));
        assert!(summary.balances.is_empty());
        assert_eq!(summary.history.len(), 1);
    }

    #[tokio::test]
    async fn invalid_queries_never_reach_the_reader() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Probe {
            facts: LeaveSummaryFacts::default(),
            fail: false,
            seen: seen.clone(),
        };
        assert!(LeaveSummaryQuery::try_new("bad company", "usr-1", 1_763_000_000_000).is_err());
        assert!(LeaveSummaryQuery::try_new("acme-co", "usr-1", -1).is_err());
        let bad = LeaveSummaryQuery {
            company_id: "bad company".into(),
            user_id: "usr-1".into(),
            now_ms: 0,
            today: "1970-01-01".into(),
            year: 1970,
        };
        assert!(leave_summary(&reader, &bad).await.is_err());
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn overlapping_assignments_fail_closed() {
        let first = PolicyFact {
            assignment_id: "a1".into(),
            policy_version_id: "p1".into(),
            policy_name: "P".into(),
            policy_version: 1,
            timezone: "UTC".into(),
            holiday_calendar_id: None,
        };
        let mut second = first.clone();
        second.assignment_id = "a2".into();
        assert_eq!(
            assert_unambiguous(Some(&first), Some(&second)).unwrap_err().code,
            policy_ambiguous().code
        );
        assert!(assert_unambiguous(Some(&first), Some(&first)).is_ok());
    }
}
