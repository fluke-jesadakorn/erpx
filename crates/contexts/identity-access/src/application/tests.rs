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
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{IdentityAccessAggregate, PrincipalStatus},
    ports::{
        AggregateKey, AuthorizationRequest, IdentityAccessCommit, PrincipalList, ReadProjection,
        RoleList,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> TenantScope {
    TenantScope::Holding { holding_id: id(1) }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "identity-access-test-actor".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("identity-access-test-{sequence}"))
            .unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "identity_access.test".into(),
            customer_visible: false,
            module: Some("identity_access".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

fn query_envelope<Q>(sequence: u128, query: Q) -> QueryEnvelope<Q> {
    QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300 + sequence)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(400 + sequence)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "identity_access.test.query".into(),
            customer_visible: false,
            module: Some("identity_access".into()),
        },
        consistency: QueryConsistency::Eventual,
        query,
    }
}

fn principal() -> AccessPrincipal {
    let mut principal = AccessPrincipal::create(
        PrincipalId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "Test principal".into(),
        at(),
    )
    .unwrap();
    principal.activate(at()).unwrap();
    principal.take_uncommitted_events();
    principal
}

#[derive(Clone)]
struct FakeAuthorization {
    allow: bool,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.allow {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeEntitlement {
    allow: bool,
}

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_identity_access_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.allow {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone)]
struct FakeWriteRepository {
    aggregate: Arc<Mutex<Option<IdentityAccessAggregate>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        _key: AggregateKey,
    ) -> Result<Option<IdentityAccessAggregate>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.aggregate.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, StoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let receipts = self.receipts.lock().unwrap();
        for (stored_identity, receipt) in receipts.iter() {
            let reused = stored_identity.command_id == identity.command_id
                || stored_identity.idempotency_key == identity.idempotency_key;
            if reused {
                return if stored_identity == identity {
                    Ok(Some(receipt.clone()))
                } else {
                    Err(PortError::IdempotencyConflict)
                };
            }
        }
        Ok(None)
    }

    async fn commit(&self, request: IdentityAccessCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: vec![EventPosition {
                event_id: EventId::try_from_uuid(id(900)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(),
            replayed: false,
        };
        self.receipts
            .lock()
            .unwrap()
            .push((request.identity, stored.clone()));
        Ok(stored)
    }
}

#[derive(Clone)]
struct FakeReadRepository {
    principal: PrincipalView,
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get_principal(
        &self,
        _authority_scope: &TenantScope,
        _principal_id: PrincipalId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PrincipalView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: Some(self.principal.clone()),
            metadata: projection_metadata(),
        })
    }

    async fn list_principals(
        &self,
        _authority_scope: &TenantScope,
        _filter: &PrincipalListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PrincipalList>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: PrincipalList {
                principals: vec![self.principal.clone()],
                next_cursor: None,
            },
            metadata: projection_metadata(),
        })
    }

    async fn get_role(
        &self,
        _authority_scope: &TenantScope,
        _role_id: RoleId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RoleView>>, PortError> {
        unreachable!("principal query does not read roles")
    }

    async fn list_roles(
        &self,
        _authority_scope: &TenantScope,
        _filter: &RoleListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RoleList>, PortError> {
        unreachable!("principal query does not read roles")
    }
}

fn projection_metadata() -> ProjectionMetadata {
    ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: Some(0),
    }
}

fn command_handler(
    write: FakeWriteRepository,
    uow: FakeUnitOfWork,
    allow: bool,
) -> IdentityAccessCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FixedClock,
> {
    IdentityAccessCommandHandler::new(
        write,
        uow,
        FakeAuthorization {
            allow,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
        FixedClock::new(at()),
    )
}

#[test]
fn create_replays_stable_identity_without_a_second_commit() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        true,
    );
    let envelope = command_envelope(
        1,
        None,
        CreatePrincipal {
            principal_id: PrincipalId::try_from_uuid(id(10)).unwrap(),
            scope: scope(),
            display_name: "Ada Lovelace".into(),
        },
    );
    let first = block_on(handler.handle(envelope.clone())).unwrap();
    let replay = block_on(handler.handle(envelope)).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn changed_payload_with_reused_idempotency_identity_conflicts() {
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        FakeUnitOfWork::default(),
        true,
    );
    let original = command_envelope(
        2,
        None,
        CreatePrincipal {
            principal_id: PrincipalId::try_from_uuid(id(20)).unwrap(),
            scope: scope(),
            display_name: "Original".into(),
        },
    );
    block_on(handler.handle(original.clone())).unwrap();
    let mut changed = original;
    changed.command.display_name = "Changed".into();
    let error = block_on(handler.handle(changed)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn expected_version_conflict_prevents_mutation_and_commit() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(Some(IdentityAccessAggregate::Principal(
                principal(),
            )))),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(
        3,
        Some(99),
        SuspendPrincipal {
            principal_id: PrincipalId::try_from_uuid(id(10)).unwrap(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn authority_scope_and_authorization_both_fail_closed() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        true,
    );
    let error = block_on(handler.handle(command_envelope(
        4,
        None,
        CreatePrincipal {
            principal_id: PrincipalId::try_from_uuid(id(40)).unwrap(),
            scope: TenantScope::Holding { holding_id: id(99) },
            display_name: "Outside authority".into(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);

    let denied = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        false,
    );
    let error = block_on(denied.handle(command_envelope(
        5,
        None,
        CreatePrincipal {
            principal_id: PrincipalId::try_from_uuid(id(41)).unwrap(),
            scope: scope(),
            display_name: "Denied".into(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn query_uses_projection_contract_and_preserves_metadata() {
    let reads = Arc::new(AtomicUsize::new(0));
    let view = PrincipalView::from(&principal());
    assert_eq!(view.status, PrincipalStatus::Active);
    let handler = IdentityAccessQueryHandler::new(
        FakeReadRepository {
            principal: view.clone(),
            reads: reads.clone(),
        },
        FakeAuthorization {
            allow: true,
            calls: Arc::new(AtomicUsize::new(0)),
        },
        FakeEntitlement { allow: true },
    );
    let result = block_on(handler.handle(query_envelope(
        1,
        GetPrincipal {
            principal_id: view.principal_id,
        },
    )))
    .unwrap();
    assert_eq!(result.value.principal, Some(view));
    assert_eq!(
        result.metadata.projection.freshness,
        FreshnessState::Current
    );
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

struct ThreadWaker;

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {}
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWaker));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
