use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, FreshnessState,
    IdempotencyKey, ProjectionMetadata, QueryConsistency, QueryEnvelope, QueryHandler,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, EventId, FixedClock,
    QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{EnvironmentId, PlatformTenant, TenantLifecycleState},
    ports::{AuthorizationGrant, ReadProjection},
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn tenant_id() -> PlatformTenantId {
    PlatformTenantId::try_from_uuid(id(1)).unwrap()
}

fn tenant() -> PlatformTenant {
    PlatformTenant::register(
        tenant_id(),
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

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "platform-root".into(),
        kind: ActorKind::PlatformUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("platform-administration-{sequence}"))
            .unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: TenantScope::Platform,
        audit: AuditContext {
            purpose: "platform_administration.test".into(),
            customer_visible: false,
            module: Some("platform_administration".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn query_envelope<Q>(query: Q) -> QueryEnvelope<Q> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(301)).unwrap(),
        actor: actor(),
        tenant_scope: TenantScope::Platform,
        audit: AuditContext {
            purpose: "platform_administration.test.query".into(),
            customer_visible: false,
            module: Some("platform_administration".into()),
        },
        consistency: QueryConsistency::Eventual,
        query,
    }
}

#[derive(Clone)]
struct FakeAuthorization {
    mfa: bool,
    scope: TenantScope,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(
        &self,
        _request: AuthorizationRequest<'_>,
    ) -> Result<AuthorizationGrant, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(AuthorizationGrant {
            authorized_scope: self.scope.clone(),
            mfa_verified: self.mfa,
        })
    }
}

#[derive(Clone)]
struct FakeEntitlement {
    allow: bool,
}

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_platform_administration_enabled(&self, _write: bool) -> Result<(), PortError> {
        if self.allow {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeTenantAuthority {
    binding_checks: Arc<AtomicUsize>,
    scope_checks: Arc<AtomicUsize>,
}

#[async_trait]
impl TenantAuthorityPort for FakeTenantAuthority {
    async fn ensure_binding_available(
        &self,
        _customer_id: Uuid,
        _root_holding_id: Uuid,
        _holding_ref: &str,
    ) -> Result<(), PortError> {
        self.binding_checks.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn ensure_tenant_scope(
        &self,
        _tenant_id: PlatformTenantId,
        _target_scope: &TenantScope,
    ) -> Result<(), PortError> {
        self.scope_checks.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakeWriteRepository {
    tenant: Arc<Mutex<Option<PlatformTenant>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load_tenant(
        &self,
        _scope: &TenantScope,
        _id: PlatformTenantId,
    ) -> Result<Option<PlatformTenant>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.tenant.lock().unwrap().clone())
    }
    async fn load_connector(
        &self,
        _scope: &TenantScope,
        _id: ConnectorBindingId,
    ) -> Result<Option<ConnectorBinding>, PortError> {
        Ok(None)
    }
    async fn load_admin_context(
        &self,
        _scope: &TenantScope,
        _id: AdminContextId,
    ) -> Result<Option<TenantAdminContext>, PortError> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipt: Arc<Mutex<Option<StoredCommit>>>,
    commits: Arc<Mutex<Vec<PlatformAdministrationCommit>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        _identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        Ok(self.receipt.lock().unwrap().clone())
    }
    async fn commit(
        &self,
        request: PlatformAdministrationCommit,
    ) -> Result<StoredCommit, PortError> {
        let aggregate = request.aggregate.clone();
        let command_id = request.identity.command_id;
        let correlation_id = request.correlation_id;
        self.commits.lock().unwrap().push(request);
        Ok(StoredCommit {
            command_id,
            correlation_id,
            aggregate,
            committed_events: vec![EventPosition {
                event_id: EventId::try_from_uuid(id(500)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(),
            replayed: false,
        })
    }
}

fn handler(
    write: FakeWriteRepository,
    unit: FakeUnitOfWork,
    mfa: bool,
) -> PlatformAdministrationCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FakeTenantAuthority,
    FixedClock,
> {
    PlatformAdministrationCommandHandler::new(
        write,
        unit,
        FakeAuthorization {
            mfa,
            scope: TenantScope::Platform,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        FakeTenantAuthority::default(),
        FixedClock::new(at()),
    )
}

#[test]
fn high_risk_command_requires_mfa_before_write_store_access() {
    let write = FakeWriteRepository {
        tenant: Arc::new(Mutex::new(Some(tenant()))),
        loads: Arc::new(AtomicUsize::new(0)),
    };
    let loads = write.loads.clone();
    let result = block_on(handler(write, FakeUnitOfWork::default(), false).handle(
        command_envelope(
            1,
            Some(1),
            TransitionPlatformTenant {
                tenant_id: tenant_id(),
                action: TenantLifecycleAction::Activate,
            },
        ),
    ));
    assert_eq!(
        result.unwrap_err().category(),
        erpx_kernel::ErrorCategory::Denied
    );
    assert_eq!(
        loads.load(Ordering::SeqCst),
        1,
        "scope is derived from authoritative aggregate before authorization"
    );
}

#[test]
fn expected_version_is_mandatory_and_checked_before_commit() {
    let write = FakeWriteRepository {
        tenant: Arc::new(Mutex::new(Some(tenant()))),
        loads: Arc::new(AtomicUsize::new(0)),
    };
    let unit = FakeUnitOfWork::default();
    let commits = unit.commits.clone();
    let result = block_on(handler(write, unit, true).handle(command_envelope(
        2,
        None,
        TransitionPlatformTenant {
            tenant_id: tenant_id(),
            action: TenantLifecycleAction::Activate,
        },
    )));
    assert_eq!(
        result.unwrap_err().category(),
        erpx_kernel::ErrorCategory::Invalid
    );
    assert!(commits.lock().unwrap().is_empty());
}

#[test]
fn command_commit_carries_snapshot_receipt_audit_and_typed_outbox_fact() {
    let unit = FakeUnitOfWork::default();
    let commits = unit.commits.clone();
    let command = RegisterPlatformTenant {
        tenant_id: tenant_id(),
        code: "ACME".into(),
        name: "Acme".into(),
        environment_id: EnvironmentId::try_from_uuid(id(2)).unwrap(),
        customer_id: id(3),
        root_holding_id: id(4),
        holding_ref: "holding/acme".into(),
    };
    let receipt = block_on(
        handler(FakeWriteRepository::default(), unit, true)
            .handle(command_envelope(3, None, command)),
    )
    .unwrap();
    assert_eq!(receipt.aggregate.version, 1);
    let commits = commits.lock().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].integration_events.len(), 1);
    assert_eq!(
        commits[0].audit.module.as_deref(),
        Some("platform_administration")
    );
}

#[test]
fn stable_fingerprint_excludes_correlation_and_audit_retry_metadata() {
    let unit_a = FakeUnitOfWork::default();
    let unit_b = FakeUnitOfWork::default();
    let command = RegisterPlatformTenant {
        tenant_id: tenant_id(),
        code: "ACME".into(),
        name: "Acme".into(),
        environment_id: EnvironmentId::try_from_uuid(id(2)).unwrap(),
        customer_id: id(3),
        root_holding_id: id(4),
        holding_ref: "holding/acme".into(),
    };
    let first = command_envelope(30, None, command.clone());
    let mut retry = command_envelope(30, None, command);
    retry.correlation_id = CorrelationId::try_from_uuid(id(999)).unwrap();
    retry.audit.purpose = "platform_administration.retry".into();
    block_on(handler(FakeWriteRepository::default(), unit_a.clone(), true).handle(first)).unwrap();
    block_on(handler(FakeWriteRepository::default(), unit_b.clone(), true).handle(retry)).unwrap();
    let first = unit_a.commits.lock().unwrap()[0]
        .identity
        .fingerprint
        .clone();
    let retry = unit_b.commits.lock().unwrap()[0]
        .identity
        .fingerprint
        .clone();
    assert_eq!(first, retry);
}

#[test]
fn stable_idempotency_replay_skips_write_repository_and_commit() {
    let stored = StoredCommit {
        command_id: CommandId::try_from_uuid(id(101)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(201)).unwrap(),
        aggregate: PlatformAggregate::Tenant(tenant()),
        committed_events: vec![],
        committed_at: at(),
        replayed: false,
    };
    let unit = FakeUnitOfWork {
        receipt: Arc::new(Mutex::new(Some(stored))),
        commits: Arc::new(Mutex::new(vec![])),
    };
    let write = FakeWriteRepository::default();
    let loads = write.loads.clone();
    let result = block_on(handler(write, unit.clone(), true).handle(command_envelope(
        1,
        Some(1),
        TransitionPlatformTenant {
            tenant_id: tenant_id(),
            action: TenantLifecycleAction::Activate,
        },
    )))
    .unwrap();
    assert!(result.replayed);
    assert_eq!(loads.load(Ordering::SeqCst), 0);
    assert!(unit.commits.lock().unwrap().is_empty());
}

#[derive(Clone)]
struct FakeReadRepository {
    tenant: TenantView,
    calls: Arc<AtomicUsize>,
    error: Option<PortError>,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get_tenant(
        &self,
        _scope: &TenantScope,
        _id: PlatformTenantId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<TenantView>>, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.error.clone() {
            return Err(error);
        }
        Ok(ReadProjection {
            value: Some(self.tenant.clone()),
            metadata: projection_metadata(),
        })
    }
    async fn list_tenants(
        &self,
        _scope: &TenantScope,
        _filter: &PageFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<TenantView>>, PortError> {
        unreachable!()
    }
    async fn get_connector(
        &self,
        _scope: &TenantScope,
        _id: ConnectorBindingId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ConnectorView>>, PortError> {
        unreachable!()
    }
    async fn list_connectors(
        &self,
        _scope: &TenantScope,
        _filter: &PageFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Page<ConnectorView>>, PortError> {
        unreachable!()
    }
    async fn get_admin_context(
        &self,
        _scope: &TenantScope,
        _id: AdminContextId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<AdminContextView>>, PortError> {
        unreachable!()
    }
}

fn projection_metadata() -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: Some(0),
    }
}

#[test]
fn query_uses_projection_repository_and_returns_projection_metadata() {
    let calls = Arc::new(AtomicUsize::new(0));
    let read = FakeReadRepository {
        tenant: TenantView::from(&tenant()),
        calls: calls.clone(),
        error: None,
    };
    let handler = PlatformAdministrationQueryHandler::new(
        read,
        FakeAuthorization {
            mfa: false,
            scope: TenantScope::Platform,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let result = block_on(handler.handle(query_envelope(GetPlatformTenant {
        tenant_id: tenant_id(),
    })))
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        result.value.tenant.unwrap().status,
        TenantLifecycleState::Provisioning
    );
    assert_eq!(
        result.metadata.projection.freshness,
        FreshnessState::Current
    );
}

struct NoopWake;
impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
