use std::collections::BTreeSet;

use chrono::{Duration, TimeZone, Utc};
use erpx_kernel::TenantScope;
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn tenant() -> PlatformTenant {
    PlatformTenant::register(
        PlatformTenantId::try_from_uuid(id(1)).unwrap(),
        "ACME".into(),
        "Acme".into(),
        EnvironmentId::try_from_uuid(id(2)).unwrap(),
        id(3),
        id(4),
        "holding/acme".into(),
        at(),
    )
    .unwrap()
}

#[test]
fn tenant_lifecycle_rejects_unsafe_shortcut_and_retirement_from_active() {
    let mut tenant = tenant();
    assert_eq!(tenant.status(), TenantLifecycleState::Provisioning);
    assert_eq!(
        tenant.transition(TenantLifecycleAction::Suspend, at()),
        Err(PlatformAdministrationDomainError::InvalidTenantTransition)
    );
    tenant
        .transition(TenantLifecycleAction::Activate, at())
        .unwrap();
    assert_eq!(
        tenant.transition(TenantLifecycleAction::Retire, at()),
        Err(PlatformAdministrationDomainError::InvalidTenantTransition)
    );
}

#[test]
fn tenant_transition_increments_version_and_records_fact() {
    let mut tenant = tenant();
    tenant.take_uncommitted_events();
    tenant
        .transition(TenantLifecycleAction::Activate, at())
        .unwrap();
    assert_eq!(tenant.version(), 2);
    assert!(
        matches!(tenant.take_uncommitted_events().as_slice(), [DomainEvent::TenantChanged { state, .. }] if state.status == TenantLifecycleState::Active)
    );
}

#[test]
fn connector_requires_sha256_digest_and_nonempty_capabilities() {
    let result = ConnectorBinding::register(
        ConnectorBindingId::try_from_uuid(id(10)).unwrap(),
        ConnectorScope::Platform,
        ConnectorKind::N8n,
        "workflow".into(),
        "not-a-digest".into(),
        BTreeSet::new(),
        at(),
    );
    assert_eq!(
        result.unwrap_err(),
        PlatformAdministrationDomainError::InvalidConfigurationDigest
    );
}

#[test]
fn connector_cannot_reactivate_after_retirement() {
    let mut connector = ConnectorBinding::register(
        ConnectorBindingId::try_from_uuid(id(10)).unwrap(),
        ConnectorScope::Platform,
        ConnectorKind::N8n,
        "workflow".into(),
        "a".repeat(64),
        BTreeSet::from(["workflow.execute".into()]),
        at(),
    )
    .unwrap();
    connector
        .transition(ConnectorTransition::Retire, at())
        .unwrap();
    assert_eq!(
        connector.transition(ConnectorTransition::Activate, at()),
        Err(PlatformAdministrationDomainError::InvalidConnectorTransition)
    );
}

#[test]
fn admin_context_is_holding_scoped_and_bounded_to_thirty_minutes() {
    let result = TenantAdminContext::issue(
        AdminContextId::try_from_uuid(id(20)).unwrap(),
        PlatformTenantId::try_from_uuid(id(1)).unwrap(),
        TenantScope::Platform,
        "root".into(),
        id(21),
        id(22),
        "support".into(),
        at(),
        at() + Duration::minutes(31),
    );
    assert_eq!(
        result.unwrap_err(),
        PlatformAdministrationDomainError::InvalidAdminScope
    );

    let too_long = TenantAdminContext::issue(
        AdminContextId::try_from_uuid(id(20)).unwrap(),
        PlatformTenantId::try_from_uuid(id(1)).unwrap(),
        TenantScope::Holding { holding_id: id(4) },
        "root".into(),
        id(21),
        id(22),
        "support".into(),
        at(),
        at() + Duration::minutes(31),
    );
    assert_eq!(
        too_long.unwrap_err(),
        PlatformAdministrationDomainError::InvalidAdminContextLifetime
    );
}

#[test]
fn revoked_admin_context_never_becomes_active_again() {
    let mut context = TenantAdminContext::issue(
        AdminContextId::try_from_uuid(id(20)).unwrap(),
        PlatformTenantId::try_from_uuid(id(1)).unwrap(),
        TenantScope::Holding { holding_id: id(4) },
        "root".into(),
        id(21),
        id(22),
        "support".into(),
        at(),
        at() + Duration::minutes(30),
    )
    .unwrap();
    context.revoke(at() + Duration::minutes(1)).unwrap();
    assert_eq!(
        context.status_at(at() + Duration::minutes(2)),
        AdminContextStatus::Revoked
    );
    assert_eq!(
        context.revoke(at()),
        Err(PlatformAdministrationDomainError::AdminContextRevoked)
    );
}
