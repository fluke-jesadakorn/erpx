//! Bind a registered Customer Authority portal read plan on the caller's own
//! transaction. The adapter only maps caller scope facts onto the exact
//! registered SQL and its ordered bind positions. It never opens a connection,
//! commits, locks or changes a live TypeScript reader; authority stays with the
//! retained caller until G4 retires it. Dynamic fragments and the support
//! source lease are not bindable here.
use crate::public::portal_read::{self, BindField, ReadPlan};
use erpx_platform_shared_server::public::BusinessError;

/// Scope facts the live caller already resolved from its session/context.
#[derive(Clone, Debug, Default)]
pub struct ReadScope<'a> {
    pub binding_id: Option<&'a str>,
    pub request_id: Option<&'a str>,
    /// A missing user id is a deliberate SQL NULL for staff threads.
    pub user_id: Option<&'a str>,
    pub email: Option<&'a str>,
    pub company_id: Option<&'a str>,
    /// Missing branch/department are deliberate SQL NULLs for work-scope predicates.
    pub branch_id: Option<&'a str>,
    pub department_id: Option<&'a str>,
}

/// A plan ready for the caller to run on its existing SQL snapshot. `None` in
/// `binds` is a deliberate SQL NULL bind, never an omitted position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundPlan {
    pub kind: &'static str,
    pub sql: &'static str,
    pub binds: Vec<Option<String>>,
}

fn scope_required(field: BindField) -> BusinessError {
    let message = match field {
        BindField::BindingId => "The portal read needs the customer binding id.",
        BindField::RequestId => "The portal read needs the support request id.",
        BindField::UserId => "The portal read needs the portal user id.",
        BindField::Email => "The portal read needs the verified customer email.",
        BindField::CompanyId => "The portal read needs the current Company.",
        BindField::BranchId => "The portal read needs the work-scope branch.",
        BindField::DepartmentId => "The portal read needs the work-scope department.",
    };
    BusinessError::invalid("CUSTOMER_AUTHORITY_READ_SCOPE_REQUIRED", message)
}

fn bind_value<'a>(field: BindField, scope: &'a ReadScope<'_>) -> Option<String> {
    match field {
        BindField::BindingId => scope.binding_id.map(str::to_owned),
        BindField::RequestId => scope.request_id.map(str::to_owned),
        BindField::UserId => scope.user_id.map(str::to_owned),
        BindField::Email => scope.email.map(str::to_owned),
        BindField::CompanyId => scope.company_id.map(str::to_owned),
        BindField::BranchId => scope.branch_id.map(str::to_owned),
        BindField::DepartmentId => scope.department_id.map(str::to_owned),
    }
}

/// `kind` must be an owner data-read plan; fragments and source leases are
/// rejected. A nullable field with no scope value is bound as SQL NULL.
pub fn bind(kind: &str, scope: &ReadScope<'_>) -> Result<BoundPlan, BusinessError> {
    let plan: &'static ReadPlan = portal_read::plan(kind).ok_or_else(portal_read::invalid)?;
    let mut binds = Vec::with_capacity(plan.binds.len());
    for &field in plan.binds {
        match bind_value(field, scope) {
            Some(value) => binds.push(Some(value)),
            None if field.nullable() => binds.push(None),
            None => return Err(scope_required(field)),
        }
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
    use crate::public::portal_read as kinds;

    fn full_scope<'a>() -> ReadScope<'a> {
        ReadScope {
            binding_id: Some("binding-1"),
            request_id: Some("request-1"),
            user_id: Some("customer"),
            email: Some("customer@example.invalid"),
            company_id: Some("company"),
            branch_id: Some("branch"),
            department_id: Some("department"),
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
        let response = bind(kinds::RD_SUPPORT_LATEST_RESPONSE, &full_scope()).unwrap();
        assert_eq!(response.binds, vec![Some("request-1".to_owned())]);
        let threads = bind(kinds::RD_SUPPORT_THREADS, &full_scope()).unwrap();
        assert_eq!(
            threads.binds,
            vec![
                Some("binding-1".to_owned()),
                Some("customer".to_owned()),
                Some("customer".to_owned()),
            ]
        );
        let staff = bind(kinds::RD_SUPPORT_STAFF_BINDING, &full_scope()).unwrap();
        assert_eq!(
            staff.binds,
            vec![
                Some("binding-1".to_owned()),
                Some("company".to_owned()),
                Some("branch".to_owned()),
                Some("branch".to_owned()),
                Some("department".to_owned()),
                Some("department".to_owned()),
            ]
        );
        let lock = bind(kinds::RD_BINDING_LOCK, &full_scope()).unwrap();
        assert_eq!(
            lock.binds,
            vec![
                Some("binding-1".to_owned()),
                Some("customer@example.invalid".to_owned()),
            ]
        );
    }

    #[test]
    fn staff_threads_bind_null_user_and_null_work_scope() {
        let threads = bind(
            kinds::RD_SUPPORT_THREADS,
            &ReadScope {
                binding_id: Some("binding-1"),
                ..ReadScope::default()
            },
        )
        .unwrap();
        assert_eq!(
            threads.binds,
            vec![Some("binding-1".to_owned()), None, None],
            "staff threads bind the fixed binding id and SQL NULL for the user filter"
        );
        let staff = bind(
            kinds::RD_SUPPORT_STAFF_BINDING,
            &ReadScope {
                binding_id: Some("binding-1"),
                company_id: Some("company"),
                ..ReadScope::default()
            },
        )
        .unwrap();
        assert_eq!(
            staff.binds,
            vec![
                Some("binding-1".to_owned()),
                Some("company".to_owned()),
                None,
                None,
                None,
                None,
            ]
        );
    }

    #[test]
    fn required_scope_and_unknown_kind_are_validation_errors() {
        assert_eq!(
            bind("bogus.read", &full_scope()).unwrap_err().code,
            "CUSTOMER_AUTHORITY_READ_PLAN_INVALID"
        );
        assert_eq!(
            bind(kinds::RD_SUPPORT_LATEST_RESPONSE, &ReadScope::default())
                .unwrap_err()
                .code,
            "CUSTOMER_AUTHORITY_READ_SCOPE_REQUIRED"
        );
        assert_eq!(
            bind(kinds::RD_BINDING_LOCK, &ReadScope::default())
                .unwrap_err()
                .code,
            "CUSTOMER_AUTHORITY_READ_SCOPE_REQUIRED"
        );
    }

    #[test]
    fn fragments_and_source_leases_are_not_bindable_read_plans() {
        for name in kinds::fragments() {
            assert_eq!(
                bind(name, &full_scope()).unwrap_err().code,
                "CUSTOMER_AUTHORITY_READ_PLAN_INVALID"
            );
        }
        for entry in kinds::LOCKS {
            assert_eq!(
                bind(entry.name, &full_scope()).unwrap_err().code,
                "CUSTOMER_AUTHORITY_READ_PLAN_INVALID"
            );
        }
    }
}
