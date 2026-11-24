//! Bind a registered Commerce residual-read plan on the caller's transaction.
//!
//! The adapter only maps caller scope facts onto the exact registered SQL and
//! its ordered bind positions. It never opens a connection, commits, locks, or
//! changes a live TypeScript reader; authority stays with the retained caller.
//! A `None` branch scope binds SQL NULL so the retained unscoped predicate
//! keeps its live behavior.
use crate::public::native_read::{self, BindField, ReadPlan};
use erpx_server_shared::public::BusinessError;

/// Scope facts the live caller already resolved from its WorkContext.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    /// Product or warehouse id the retained guard/detail caller resolved.
    pub record_id: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// Both `(?::text IS NULL OR branch_id=?)` positions take this value;
    /// retained callers pass their single `branchId` twice.
    pub branch_id: Option<&'a str>,
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
        BindField::RecordId => "The Commerce read needs the product or warehouse id.",
        BindField::CompanyId => "The Commerce read needs the current Company.",
        BindField::BranchId => "The Commerce read needs a branch scope.",
    };
    BusinessError::invalid("COMMERCE_READ_SCOPE_REQUIRED", message)
}

fn bind_value<'a>(field: BindField, scope: &'a ReadScope<'_>) -> Option<String> {
    match field {
        BindField::RecordId => scope.record_id.map(str::to_owned),
        BindField::CompanyId => scope.company_id.map(str::to_owned),
        BindField::BranchId => scope.branch_id.map(str::to_owned),
    }
}

/// Bind `kind` using `scope`, preserving the exact registered SQL and order.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = native_read::plan(kind).ok_or_else(native_read::invalid)?;
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
    use crate::public::native_read as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            record_id: Some("product-1"),
            company_id: Some("company-1"),
            branch_id: Some("branch-1"),
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
                kinds::placeholder_count(bound.sql),
                "{kind}"
            );
        }
    }

    #[test]
    fn ordered_binds_match_the_retained_call_sites() {
        let active = bind(kinds::RD_PRODUCT_ACTIVE, &full_scope()).unwrap();
        assert_eq!(
            active.binds,
            vec![Some("product-1".to_owned()), Some("company-1".to_owned())]
        );
        let warehouse = bind(kinds::RD_WAREHOUSE_ACTIVE, &full_scope()).unwrap();
        assert_eq!(
            warehouse.binds,
            vec![
                Some("product-1".to_owned()),
                Some("company-1".to_owned()),
                Some("branch-1".to_owned()),
                Some("branch-1".to_owned()),
            ]
        );
        let identity = bind(kinds::RD_WAREHOUSE_IDENTITY, &full_scope()).unwrap();
        assert!(identity.binds.is_empty());
    }

    #[test]
    fn optional_branch_binds_null_and_keeps_the_unscoped_predicate() {
        let scope = ReadScope {
            company_id: Some("company-1"),
            ..ReadScope::default()
        };
        let rows = bind(kinds::RD_WAREHOUSE_PAGE_ROWS, &scope).unwrap();
        assert_eq!(rows.binds, vec![Some("company-1".to_owned()), None, None]);
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "COMMERCE_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_PRODUCT_ACTIVE, &ReadScope::default())
                .unwrap_err()
                .code,
            "COMMERCE_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_SETTINGS_BY_COMPANY, &ReadScope::default())
                .unwrap_err()
                .code,
            "COMMERCE_READ_SCOPE_REQUIRED"
        );
    }
}
