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
fn connector_activation_requires_matching_successful_preflight_evidence() {
    let digest = "a".repeat(64);
    let capabilities = BTreeSet::from(["workflow.execute".into()]);
    let mut connector = ConnectorBinding::register(
        ConnectorBindingId::try_from_uuid(id(11)).unwrap(),
        ConnectorScope::Platform,
        ConnectorKind::N8n,
        "workflow".into(),
        digest.clone(),
        capabilities.clone(),
        at(),
    )
    .unwrap();
    assert_eq!(
        connector.transition(ConnectorTransition::Activate, at()),
        Err(PlatformAdministrationDomainError::ConnectorNotReady)
    );
    connector
        .observe_readiness(
            ConnectorReadinessEvidence::try_new(
                "preflight-1".into(),
                digest,
                capabilities,
                true,
                None,
                at() + Duration::seconds(1),
            )
            .unwrap(),
        )
        .unwrap();
    connector
        .transition(ConnectorTransition::Activate, at() + Duration::seconds(2))
        .unwrap();
    assert_eq!(connector.state(), ConnectorState::Active);
}

#[test]
fn connector_rejects_stale_mismatched_or_replayed_readiness() {
    let mut connector = ConnectorBinding::register(
        ConnectorBindingId::try_from_uuid(id(12)).unwrap(),
        ConnectorScope::Platform,
        ConnectorKind::Minio,
        "objects".into(),
        "a".repeat(64),
        BTreeSet::from(["object.write".into()]),
        at(),
    )
    .unwrap();
    let mismatched = ConnectorReadinessEvidence::try_new(
        "preflight-mismatch".into(),
        "b".repeat(64),
        BTreeSet::from(["object.write".into()]),
        true,
        None,
        at() + Duration::seconds(1),
    )
    .unwrap();
    assert_eq!(
        connector.observe_readiness(mismatched),
        Err(PlatformAdministrationDomainError::ReadinessConfigurationMismatch)
    );
    let failed = ConnectorReadinessEvidence::try_new(
        "preflight-failed".into(),
        "a".repeat(64),
        BTreeSet::from(["object.write".into()]),
        false,
        Some("provider_unavailable".into()),
        at() + Duration::seconds(2),
    )
    .unwrap();
    connector.observe_readiness(failed.clone()).unwrap();
    assert_eq!(
        connector.observe_readiness(failed),
        Err(PlatformAdministrationDomainError::StaleReadinessEvidence)
    );
    assert_eq!(
        connector.transition(ConnectorTransition::Activate, at() + Duration::seconds(3)),
        Err(PlatformAdministrationDomainError::ConnectorNotReady)
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

#[test]
fn branch_pair_topology_never_falls_back_to_holding_route() {
    let mut tenant = tenant();
    tenant
        .transition(TenantLifecycleAction::Activate, at())
        .unwrap();
    let mut routes = DataPlaneRouteCatalog::new(
        tenant.id(),
        tenant.root_holding_id(),
        DataPlaneDeploymentMode::CustomerHosted,
        DataPlaneTopology::HoldingAndBranchPairs,
    )
    .unwrap();
    routes
        .reserve(
            DataPlaneRouteTarget::Holding,
            "profile:holding-write".into(),
            "profile:holding-read".into(),
            at(),
        )
        .unwrap();
    routes
        .begin_provisioning(DataPlaneRouteTarget::Holding, at() + Duration::seconds(1))
        .unwrap();
    routes
        .verify(
            DataPlaneRouteTarget::Holding,
            ROUTING_SCHEMA_GENERATION,
            ROUTING_SCHEMA_GENERATION,
            "evidence:holding-1".into(),
            at() + Duration::seconds(2),
        )
        .unwrap();
    routes
        .activate(DataPlaneRouteTarget::Holding, at() + Duration::seconds(3))
        .unwrap();
    let branch_scope = TenantScope::Branch {
        holding_id: tenant.root_holding_id(),
        company_id: id(30),
        branch_id: id(31),
    };
    assert_eq!(
        routes.resolve(&tenant, &branch_scope),
        Err(PlatformAdministrationDomainError::RouteUnavailable)
    );
}

#[test]
fn route_activation_requires_generation_two_evidence_and_explicit_retry() {
    let tenant = tenant();
    let branch = DataPlaneRouteTarget::Branch { branch_id: id(41) };
    let mut routes = DataPlaneRouteCatalog::new(
        tenant.id(),
        tenant.root_holding_id(),
        DataPlaneDeploymentMode::ErpxManaged,
        DataPlaneTopology::HoldingAndBranchPairs,
    )
    .unwrap();
    assert!(
        routes
            .reserve(
                branch,
                "profile:branch-write".into(),
                "profile:branch-read".into(),
                at(),
            )
            .unwrap()
    );
    assert!(
        !routes
            .reserve(
                branch,
                "profile:branch-write".into(),
                "profile:branch-read".into(),
                at(),
            )
            .unwrap(),
        "identical reservation is idempotent"
    );
    routes
        .begin_provisioning(branch, at() + Duration::seconds(1))
        .unwrap();
    assert_eq!(
        routes.verify(
            branch,
            1,
            1,
            "evidence:branch-legacy".into(),
            at() + Duration::seconds(2),
        ),
        Err(PlatformAdministrationDomainError::UnsupportedRouteSchemaGeneration)
    );
    routes
        .mark_failed(
            branch,
            "migration_failed".into(),
            at() + Duration::seconds(3),
        )
        .unwrap();
    assert_eq!(
        routes.verify(
            branch,
            2,
            2,
            "evidence:branch-2".into(),
            at() + Duration::seconds(4),
        ),
        Err(PlatformAdministrationDomainError::InvalidRouteTransition)
    );
    routes
        .retry_provisioning(branch, at() + Duration::seconds(5))
        .unwrap();
    routes
        .verify(
            branch,
            2,
            2,
            "evidence:branch-3".into(),
            at() + Duration::seconds(6),
        )
        .unwrap();
}
