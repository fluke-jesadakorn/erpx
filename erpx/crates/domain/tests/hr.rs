use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::hr::{EmploymentScope, HrError, LeaveRequest, LeaveStatus};
use uuid::Uuid;

fn context(actor: &str, scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: actor.into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "exercise HR leave transition boundary",
    )
    .expect("valid command context")
}

fn employment_scope() -> EmploymentScope {
    EmploymentScope::new(
        Uuid::from_u128(1),
        Uuid::from_u128(2),
        Some(Uuid::from_u128(3)),
    )
    .expect("valid employment scope")
}

fn leave(owner: &CommandContext, scope: EmploymentScope) -> LeaveRequest {
    LeaveRequest::new(
        Uuid::from_u128(10),
        Uuid::from_u128(11),
        scope,
        chrono::NaiveDate::from_ymd_opt(2026, 2, 1).expect("valid date"),
        chrono::NaiveDate::from_ymd_opt(2026, 2, 3).expect("valid date"),
        "annual leave",
        owner,
    )
    .expect("valid leave request")
}

#[test]
fn leave_submission_requires_scope_authorization() {
    let scope = employment_scope();
    let owner = context("owner", scope.tenant_scope(), "leave-owner");
    let outsider = context(
        "outsider",
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(99),
        },
        "leave-outsider",
    );
    let mut request = leave(&owner, scope);
    let before = request.clone();

    assert_eq!(request.submit(&outsider), Err(HrError::ScopeNotContained));
    assert_eq!(request, before);
    assert_eq!(request.status, LeaveStatus::Draft);
}

#[test]
fn leave_version_overflow_is_rejected_without_mutation() {
    let scope = employment_scope();
    let owner = context("owner", scope.tenant_scope(), "leave-overflow");
    let mut request = leave(&owner, scope);
    request.version = u64::MAX;
    let before = request.clone();

    assert_eq!(request.submit(&owner), Err(HrError::InvalidVersion));
    assert_eq!(request, before);
    assert_eq!(request.status, LeaveStatus::Draft);
}
