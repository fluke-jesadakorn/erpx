//! Bind a registered Commercial read plan on the caller's own transaction.
//!
//! The adapter only maps caller scope facts onto the exact registered SQL and
//! its ordered bind positions. It never opens a connection, commits, locks, or
//! changes a live TypeScript reader; authority stays with the retained caller.
//! Dynamic fragments and advisory-lock statements are not bindable here.
use crate::public::native_read::{self, BindField, ReadPlan};
use erpx_platform_shared_server::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    pub company_id: Option<&'a str>,
    pub package_id: Option<&'a str>,
    pub event_id: Option<&'a str>,
    pub month: Option<&'a str>,
    pub holding_id: Option<&'a str>,
    /// Upper event-history bound, bound as a bigint literal.
    pub end: Option<i64>,
}

/// A plan ready for the caller to run on its existing SQL snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub sql: &'static str,
    pub binds: Vec<String>,
}

fn scope_required(_kind: &str, field: BindField) -> BusinessError {
    let message = match field {
        BindField::CompanyId => "The Commercial read needs the current Company.",
        BindField::PackageId => "The Commercial read needs the package id.",
        BindField::EventId => "The Commercial read needs the receipt id.",
        BindField::Month => "The Commercial projection read needs the month.",
        BindField::HoldingId => "The Commercial projection read needs the Holding id.",
        BindField::End => "The Commercial seat read needs the event history bound.",
    };
    BusinessError::invalid("COMMERCIAL_READ_SCOPE_REQUIRED", message)
}

fn bind_value<'a>(field: BindField, scope: &'a ReadScope<'_>) -> Option<String> {
    match field {
        BindField::CompanyId => scope.company_id.map(str::to_owned),
        BindField::PackageId => scope.package_id.map(str::to_owned),
        BindField::EventId => scope.event_id.map(str::to_owned),
        BindField::Month => scope.month.map(str::to_owned),
        BindField::HoldingId => scope.holding_id.map(str::to_owned),
        BindField::End => scope.end.map(|value| value.to_string()),
    }
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = native_read::plan(kind).ok_or_else(native_read::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        let value = bind_value(field, scope).ok_or_else(|| scope_required(plan.kind, field))?;
        binds.push(value);
    }
    Ok(BoundPlan {
        kind: plan.kind,
        sql: plan.sql,
        binds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::native_read as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            company_id: Some("company"),
            package_id: Some("base"),
            event_id: Some("receipt-1"),
            month: Some("2026-11"),
            holding_id: Some("holding"),
            end: Some(1_760_000_000_000),
        }
    }

    #[test]
    fn every_registered_kind_binds_without_losing_its_plan() {
        for kind in kinds::kinds() {
            let bound = bind(kind, &full_scope()).expect("bindable plan");
            let plan = kinds::plan(kind).unwrap();
            assert_eq!(bound.kind, kind);
            assert_eq!(bound.sql, plan.sql);
            assert_eq!(bound.binds.len(), bound.sql.matches('?').count(), "{kind}");
        }
    }

    #[test]
    fn ordered_binds_match_the_retained_call_sites() {
        let facts = bind(kinds::RD_PURCHASE_FACTS, &full_scope()).unwrap();
        assert_eq!(facts.binds, vec!["company".to_owned(), "company".to_owned()]);
        let current = bind(kinds::RD_CURRENT_GRANT, &full_scope()).unwrap();
        assert_eq!(current.binds, vec!["company".to_owned(), "base".to_owned()]);
        let header = bind(kinds::RD_SNAPSHOT_HEADER_LOCK, &full_scope()).unwrap();
        assert_eq!(header.binds, vec!["holding".to_owned(), "2026-11".to_owned()]);
        let events = bind(kinds::RD_SEAT_ASSIGNMENT_EVENTS, &full_scope()).unwrap();
        assert_eq!(events.binds, vec!["1760000000000".to_owned()]);
        let sweep = bind(kinds::RD_HOLDING_GRANT_PAIRS, &ReadScope::default()).unwrap();
        assert!(sweep.binds.is_empty());
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "COMMERCIAL_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_PURCHASE_FACTS, &ReadScope::default())
                .unwrap_err()
                .code,
            "COMMERCIAL_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_SEAT_SNAPSHOT, &ReadScope::default())
                .unwrap_err()
                .code,
            "COMMERCIAL_READ_SCOPE_REQUIRED"
        );
    }

    #[test]
    fn advisory_locks_are_not_bindable_read_plans() {
        for lock in kinds::ADVISORY_LOCKS {
            assert_eq!(
                bind(lock.name, &full_scope()).unwrap_err().code,
                "COMMERCIAL_READ_PLAN_INVALID"
            );
        }
    }
}
