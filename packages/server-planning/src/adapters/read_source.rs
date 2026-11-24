//! Bind a registered Planning read-source plan on the caller's transaction.
//!
//! The adapter only maps caller scope facts onto the exact registered SQL and
//! its ordered bind positions. It never opens a connection, commits, locks or
//! changes a live TypeScript reader. The retained caller remains authoritative
//! until a later G4 retirement task switches it. A `None` branch scope binds
//! SQL NULL so the retained unscoped predicate keeps working.
use crate::public::read_source::{self, BindField, ReadPlan};
use erpx_server_shared::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    pub record_id: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// Both `(?::text IS NULL OR branch_id=?)` positions take this value;
    /// retained callers pass their single `branchId` twice.
    pub branch_id: Option<&'a str>,
    pub specification_id: Option<&'a str>,
    pub resource_id: Option<&'a str>,
    pub work_date: Option<&'a str>,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
    pub version: Option<i64>,
    pub user_id: Option<&'a str>,
    pub minutes: Option<i64>,
    pub available_minutes: Option<i64>,
}

/// A plan ready for the caller to run on its existing SQL snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub category: &'static str,
    pub sql: &'static str,
    /// Ordered bind values; `None` is a SQL NULL branch scope.
    pub binds: Vec<Option<String>>,
}

fn scope_required(field: BindField) -> BusinessError {
    let message = match field {
        BindField::RecordId => "The Planning read needs a scenario or recommendation id.",
        BindField::CompanyId => "The Planning read needs the current Company.",
        BindField::BranchId => "The Planning read needs a branch scope.",
        BindField::SpecificationId => "The Planning read needs a specification id.",
        BindField::ResourceId => "The Planning read needs a resource id.",
        BindField::WorkDate => "The Planning read needs a work date.",
        BindField::From => "The Planning executive read needs a lower date bound.",
        BindField::To => "The Planning executive read needs an upper date bound.",
        BindField::Version => "The Planning read needs a version.",
        BindField::UserId => "The Planning read needs the acting user id.",
        BindField::Minutes => "The Planning read needs the reserved minutes.",
        BindField::AvailableMinutes => "The Planning read needs the available minutes.",
    };
    BusinessError::invalid("PLANNING_READ_SCOPE_REQUIRED", message)
}

fn bind_value<'a>(field: BindField, scope: &'a ReadScope<'_>) -> Option<String> {
    match field {
        BindField::RecordId => scope.record_id.map(str::to_owned),
        BindField::CompanyId => scope.company_id.map(str::to_owned),
        BindField::BranchId => scope.branch_id.map(str::to_owned),
        BindField::SpecificationId => scope.specification_id.map(str::to_owned),
        BindField::ResourceId => scope.resource_id.map(str::to_owned),
        BindField::WorkDate => scope.work_date.map(str::to_owned),
        BindField::From => scope.from.map(str::to_owned),
        BindField::To => scope.to.map(str::to_owned),
        BindField::Version => scope.version.map(|value| value.to_string()),
        BindField::UserId => scope.user_id.map(str::to_owned),
        BindField::Minutes => scope.minutes.map(|value| value.to_string()),
        BindField::AvailableMinutes => scope.available_minutes.map(|value| value.to_string()),
    }
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = read_source::plan(kind).ok_or_else(read_source::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        match bind_value(field, scope) {
            Some(value) => binds.push(Some(value)),
            None if field == BindField::BranchId => binds.push(None),
            None => return Err(scope_required(field)),
        }
    }
    Ok(BoundPlan {
        kind: plan.kind,
        category: plan.category,
        sql: plan.sql,
        binds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::read_source as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            record_id: Some("record-1"),
            company_id: Some("company-1"),
            branch_id: Some("branch-1"),
            specification_id: Some("spec-1"),
            resource_id: Some("resource-1"),
            work_date: Some("2026-11-24"),
            from: Some("2026-01-01"),
            to: Some("2026-12-31"),
            version: Some(3),
            user_id: Some("planner"),
            minutes: Some(30),
            available_minutes: Some(480),
        }
    }

    #[test]
    fn every_registered_kind_binds_without_losing_its_plan() {
        for kind in kinds::kinds() {
            let bound = bind(kind, &full_scope()).expect("bindable plan");
            let plan = kinds::plan(kind).unwrap();
            assert_eq!(bound.kind, kind);
            assert_eq!(bound.sql, plan.sql);
            assert_eq!(bound.category, plan.category);
            assert_eq!(
                bound.binds.len(),
                read_source::placeholder_count(bound.sql),
                "{kind}"
            );
        }
    }

    #[test]
    fn ordered_binds_match_the_retained_call_sites() {
        let scenarios = bind(kinds::RD_SCENARIOS, &full_scope()).unwrap();
        assert_eq!(
            scenarios.binds,
            vec![
                Some("company-1".to_owned()),
                Some("branch-1".to_owned()),
                Some("branch-1".to_owned()),
            ]
        );
        let executive = bind(kinds::RD_EXECUTIVE_SHORTAGE, &full_scope()).unwrap();
        assert_eq!(
            executive.binds,
            vec![
                Some("record-1".to_owned()),
                Some("company-1".to_owned()),
                Some("branch-1".to_owned()),
                Some("branch-1".to_owned()),
                Some("2026-01-01".to_owned()),
                Some("2026-12-31".to_owned()),
            ]
        );
        let capacity = bind(kinds::RD_GUARD_CAPACITY_RESERVATION, &full_scope()).unwrap();
        assert_eq!(
            capacity.binds,
            vec![
                Some("resource-1".to_owned()),
                Some("2026-11-24".to_owned()),
                Some("30".to_owned()),
                Some("480".to_owned()),
            ]
        );
        let calendar = bind(kinds::RD_GUARD_CALENDAR_VERSION, &full_scope()).unwrap();
        assert_eq!(
            calendar.binds,
            vec![
                Some("resource-1".to_owned()),
                Some("2026-11-24".to_owned()),
                Some("3".to_owned()),
            ]
        );
    }

    #[test]
    fn optional_branch_binds_null_and_keeps_the_unscoped_predicate() {
        let scope = ReadScope {
            company_id: Some("company-1"),
            ..ReadScope::default()
        };
        let bound = bind(kinds::RD_SCENARIOS, &scope).unwrap();
        assert_eq!(bound.binds[1], None);
        assert_eq!(bound.binds[2], None);
        assert!(bound.sql.contains("(?::text IS NULL OR p.branch_id=?)"));
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "PLANNING_READ_SOURCE_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_SCENARIOS, &ReadScope::default())
                .unwrap_err()
                .code,
            "PLANNING_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_GUARD_NO_ACTIVE_ALLOCATION, &full_scope())
                .unwrap()
                .binds,
            vec![Some("resource-1".to_owned())]
        );
    }
}
