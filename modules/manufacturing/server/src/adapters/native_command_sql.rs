//! Bind a registered caller-unit read plan on the caller's own transaction.
//!
//! A plan carries exact owner SQL and an ordered bind list. This adapter only
//! maps caller scope facts onto those positions; it never opens a connection,
//! commits, locks, or emits a status marker. A missing required scope is a
//! planning error, not a runtime 503, and a `None` branch scope binds SQL NULL
//! so the caller's existing unscoped predicate keeps working. Live TypeScript
//! reads stay authoritative until the per-caller cutover in
//! `tests/unit/manufacturing/native-read-plan.spec.md`.
use crate::public::native_command::{self, BindField, ReadPlan};
use erpx_platform_shared_server::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    pub id: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// `None` binds SQL NULL and preserves `(?::text IS NULL OR branch_id=?)`.
    pub branch_id: Option<&'a str>,
    pub specification_id: Option<&'a str>,
    pub order_id: Option<&'a str>,
    pub version: Option<i64>,
}

/// A plan ready for the caller to run on its existing SQL snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub caller: &'static str,
    pub sql: &'static str,
    /// Ordered bind values; `None` is a SQL NULL branch scope.
    pub binds: Vec<Option<String>>,
}

fn scope_required(_kind: &str, field: BindField) -> BusinessError {
    let message = match field {
        BindField::Id => "The Manufacturing read needs a record id.",
        BindField::CompanyId => "The Manufacturing read needs the current Company.",
        BindField::BranchId => "The Manufacturing read needs a branch scope.",
        BindField::SpecificationId => "The Manufacturing read needs a specification id.",
        BindField::OrderId => "The Manufacturing read needs an order id.",
        BindField::Version => "The Manufacturing read needs a specification version.",
    };
    BusinessError::invalid("MANUFACTURING_READ_SCOPE_REQUIRED", message)
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = native_command::plan(kind).ok_or_else(native_command::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        let value = match field {
            BindField::Version => scope.version.map(|version| version.to_string()),
            BindField::BranchId => scope.branch_id.map(str::to_owned),
            BindField::Id => scope.id.map(str::to_owned),
            BindField::CompanyId => scope.company_id.map(str::to_owned),
            BindField::SpecificationId => scope.specification_id.map(str::to_owned),
            BindField::OrderId => scope.order_id.map(str::to_owned),
        };
        match value {
            Some(value) => binds.push(Some(value)),
            None if field == BindField::BranchId => binds.push(None),
            None => return Err(scope_required(plan.kind, field)),
        }
    }
    Ok(BoundPlan {
        kind: plan.kind,
        caller: plan.caller,
        sql: plan.sql,
        binds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::public::native_command as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            id: Some("record-1"),
            company_id: Some("company-1"),
            branch_id: Some("branch-1"),
            specification_id: Some("spec-1"),
            order_id: Some("order-1"),
            version: Some(3),
        }
    }

    #[test]
    fn every_registered_kind_binds_without_losing_its_plan() {
        for kind in kinds::kinds() {
            let bound = bind(kind, &full_scope()).expect("bindable plan");
            let plan = kinds::plan(kind).unwrap();
            assert_eq!(bound.kind, kind);
            assert_eq!(bound.sql, plan.sql);
            assert_eq!(bound.caller, plan.caller);
            assert_eq!(bound.binds.len(), bound.sql.matches('?').count(), "{kind}");
            assert!(
                bound.binds.iter().all(Option::is_some),
                "{kind}: full scope"
            );
        }
    }

    #[test]
    fn optional_branch_binds_null_and_keeps_the_unscoped_predicate() {
        let scope = ReadScope {
            company_id: Some("company-1"),
            branch_id: None,
            ..ReadScope::default()
        };
        let bound = bind(kinds::RD_ORDERS, &scope).unwrap();
        assert_eq!(bound.binds, vec![Some("company-1".to_owned()), None, None,]);
        assert!(bound.sql.contains("(?::text IS NULL OR o.branch_id=?)"));
        let detail = bind(
            kinds::RD_ORDER_DETAIL,
            &ReadScope {
                id: Some("order-1"),
                branch_id: None,
                ..scope
            },
        )
        .unwrap();
        assert_eq!(detail.binds.len(), 4);
        assert!(detail.binds[2].is_none() && detail.binds[3].is_none());
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "MANUFACTURING_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_SPECIFICATION, &ReadScope::default())
                .unwrap_err()
                .code,
            "MANUFACTURING_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_ORDER_EVENTS, &ReadScope::default())
                .unwrap_err()
                .code,
            "MANUFACTURING_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(
                kinds::RD_SPECIFICATION_VERSION,
                &ReadScope {
                    id: Some("spec-1"),
                    company_id: Some("company-1"),
                    ..ReadScope::default()
                }
            )
            .unwrap_err()
            .code,
            "MANUFACTURING_READ_SCOPE_REQUIRED"
        );
    }

    #[test]
    fn bind_order_matches_live_caller_order() {
        let bound = bind(kinds::RD_ORDER_DETAIL, &full_scope()).unwrap();
        assert_eq!(
            bound.binds,
            vec![
                Some("record-1".to_owned()),
                Some("company-1".to_owned()),
                Some("branch-1".to_owned()),
                Some("branch-1".to_owned()),
            ]
        );
        let operations = bind(kinds::RD_ORDER_OPERATIONS, &full_scope()).unwrap();
        assert_eq!(
            operations.binds,
            vec![Some("order-1".to_owned()), Some("spec-1".to_owned()),]
        );
        let versioned = bind(kinds::RD_SPECIFICATION_VERSION, &full_scope()).unwrap();
        assert_eq!(
            versioned.binds,
            vec![
                Some("record-1".to_owned()),
                Some("company-1".to_owned()),
                Some("3".to_owned()),
            ]
        );
    }
}
