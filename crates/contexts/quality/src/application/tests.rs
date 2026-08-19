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
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId,
};

use super::*;
use crate::ports::{AuthorizationRequest, InspectionList, PortError, ReadProjection, StoredCommit};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "quality-test-actor".into(),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("quality-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "quality.test".into(),
            customer_visible: false,
            module: Some("quality".into()),
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
        active_scope: scope(),
        audit: AuditContext {
            purpose: "quality.test.query".into(),
            customer_visible: false,
            module: Some("quality".into()),
        },
        consistency: QueryConsistency::Eventual,
        query,
    }
}

fn aggregate() -> QualityInspection {
    let mut aggregate = QualityInspection::create(
        InspectionId::try_from_uuid(id(10)).unwrap(),
        scope(),
        InspectionKind::Final,
        id(11),
        Some("LOT-001".into()),
        5,
        at(),
    )
    .unwrap();
    aggregate.take_uncommitted_events();
    aggregate
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
    async fn ensure_quality_enabled(
        &self,
        _scope: &AuthorityScope,
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
    aggregate: Arc<Mutex<Option<QualityInspection>>>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &AuthorityScope,
        _inspection_id: InspectionId,
    ) -> Result<Option<QualityInspection>, PortError> {
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

    async fn commit(&self, request: QualityCommit) -> Result<StoredCommit, PortError> {
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
    view: InspectionView,
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for FakeReadRepository {
    async fn get(
        &self,
        _authority_scope: &AuthorityScope,
        _inspection_id: InspectionId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<InspectionView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: Some(self.view.clone()),
            metadata: projection_metadata(),
        })
    }

    async fn list(
        &self,
        _authority_scope: &AuthorityScope,
        _filter: &InspectionListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<InspectionList>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(ReadProjection {
            value: InspectionList {
                inspections: vec![self.view.clone()],
                next_cursor: None,
            },
            metadata: projection_metadata(),
        })
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
) -> QualityCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FixedClock,
> {
    QualityCommandHandler::new(
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
        CreateInspection {
            inspection_id: InspectionId::try_from_uuid(id(10)).unwrap(),
            scope: scope(),
            kind: InspectionKind::Final,
            product_id: id(11),
            lot_reference: None,
            quantity: 5,
        },
    );
    let first = block_on(handler.handle(envelope.clone())).unwrap();
    let replay = block_on(handler.handle(envelope)).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn reused_idempotency_identity_with_changed_payload_conflicts() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow,
        true,
    );
    let original = command_envelope(
        2,
        None,
        CreateInspection {
            inspection_id: InspectionId::try_from_uuid(id(20)).unwrap(),
            scope: scope(),
            kind: InspectionKind::Incoming,
            product_id: id(21),
            lot_reference: None,
            quantity: 1,
        },
    );
    block_on(handler.handle(original.clone())).unwrap();
    let mut changed = original;
    changed.command.quantity = 2;
    let error = block_on(handler.handle(changed)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn expected_version_is_checked_before_mutation_and_commit() {
    let uow = FakeUnitOfWork::default();
    let write = FakeWriteRepository {
        aggregate: Arc::new(Mutex::new(Some(aggregate()))),
        loads: Arc::new(AtomicUsize::new(0)),
    };
    let handler = command_handler(write, uow.clone(), true);
    let error = block_on(handler.handle(command_envelope(
        3,
        Some(99),
        StartInspection {
            inspection_id: InspectionId::try_from_uuid(id(10)).unwrap(),
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn authorization_denial_prevents_create_commit() {
    let uow = FakeUnitOfWork::default();
    let handler = command_handler(
        FakeWriteRepository {
            aggregate: Arc::new(Mutex::new(None)),
            loads: Arc::new(AtomicUsize::new(0)),
        },
        uow.clone(),
        false,
    );
    let error = block_on(handler.handle(command_envelope(
        4,
        None,
        CreateInspection {
            inspection_id: InspectionId::try_from_uuid(id(40)).unwrap(),
            scope: scope(),
            kind: InspectionKind::Final,
            product_id: id(41),
            lot_reference: None,
            quantity: 1,
        },
    )))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn query_handler_reads_projection_and_preserves_metadata() {
    let reads = Arc::new(AtomicUsize::new(0));
    let view = InspectionView::from(&aggregate());
    let handler = QualityQueryHandler::new(
        FakeReadRepository {
            view: view.clone(),
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
        GetInspection {
            inspection_id: view.inspection_id,
        },
    )))
    .unwrap();
    assert_eq!(result.value.inspection, Some(view));
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
