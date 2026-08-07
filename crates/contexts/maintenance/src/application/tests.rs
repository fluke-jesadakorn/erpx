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
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, IdempotencyKey,
    ProjectionNotReady, ProjectionTarget, QueryConsistency, QueryEnvelope, QueryHandler,
    ReadYourWrites,
};
use erpx_integration_contracts::{
    AggregateKind, DisplayName, IntegrationEventEnvelope, ReferenceCode, assets,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AggregateId, AuditContext, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId, TenantScope,
};
use uuid::Uuid;

use super::*;
use crate::ports::{AuthorizationRequest, PortError, ReadProjection};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}

fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "maintenance-test-actor".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn asset_fact() -> assets::AssetConditionFact {
    assets::AssetConditionFact {
        asset: assets::AssetRef::try_from_uuid(id(10)).unwrap(),
        asset_code: ReferenceCode::try_new("AST-10").unwrap(),
        name: DisplayName::try_new("Packing line").unwrap(),
        location: assets::AssetLocationRef::try_from_uuid(id(11)).unwrap(),
        condition: assets::AssetCondition::OutOfService,
    }
}

fn request_envelope() -> IntegrationEventEnvelope<assets::Event> {
    IntegrationEventEnvelope::try_new(
        EventId::try_from_uuid(id(20)).unwrap(),
        scope(),
        AggregateKind::try_new("asset").unwrap(),
        AggregateId::try_from_uuid(id(10)).unwrap(),
        3,
        CorrelationId::try_from_uuid(id(21)).unwrap(),
        None,
        at(),
        assets::Event::MaintenanceRequested {
            asset: assets::AssetRef::try_from_uuid(id(10)).unwrap(),
            request_code: ReferenceCode::try_new("REQ-100").unwrap(),
        },
    )
    .unwrap()
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("maintenance-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: Some(EventId::try_from_uuid(id(20)).unwrap()),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "maintenance.test".into(),
            customer_visible: false,
            module: Some("maintenance".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

#[derive(Clone)]
struct FakeAuthorization(bool);

#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _request: AuthorizationRequest<'_>) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::Denied)
        }
    }
}

#[derive(Clone)]
struct FakeEntitlement(bool);

#[async_trait]
impl EntitlementPort for FakeEntitlement {
    async fn ensure_maintenance_enabled(
        &self,
        _scope: &TenantScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone)]
struct FakeAssetFacts {
    fact: assets::AssetConditionFact,
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl AssetFactPort for FakeAssetFacts {
    async fn get_at_least(
        &self,
        _asset: assets::AssetRef,
        _source_aggregate_version: u64,
    ) -> Result<assets::AssetConditionFact, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.fact.clone())
    }
}

#[derive(Clone)]
struct FakeWriteRepository(Option<WorkOrder>);

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        _work_order_id: WorkOrderId,
    ) -> Result<Option<WorkOrder>, PortError> {
        Ok(self.0.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, MaintenanceStoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<MaintenanceStoredCommit>, PortError> {
        for (stored_identity, receipt) in self.receipts.lock().unwrap().iter() {
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

    async fn commit(
        &self,
        request: MaintenanceCommit,
    ) -> Result<MaintenanceStoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = MaintenanceStoredCommit {
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
struct NotReadyRead(Arc<AtomicUsize>);

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get(
        &self,
        _authority_scope: &TenantScope,
        _work_order_id: WorkOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<WorkOrderView>>, PortError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let QueryConsistency::ReadYourWrites(requirement) = consistency else {
            return Err(PortError::Internal);
        };
        Err(PortError::NotReady(ProjectionNotReady {
            projection: requirement.projection.clone(),
            partition: requirement.partition.clone(),
            target: requirement.target,
            observed: None,
        }))
    }

    async fn list(
        &self,
        _authority_scope: &TenantScope,
        _filter: &WorkOrderListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<WorkOrderList>, PortError> {
        Err(PortError::Internal)
    }
}

fn handler(
    write: FakeWriteRepository,
    uow: FakeUnitOfWork,
    authorization: bool,
    entitlement: bool,
    fact_reads: Arc<AtomicUsize>,
) -> MaintenanceCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FakeAssetFacts,
    FixedClock,
> {
    MaintenanceCommandHandler::new(
        write,
        uow,
        FakeAuthorization(authorization),
        FakeEntitlement(entitlement),
        FakeAssetFacts {
            fact: asset_fact(),
            reads: fact_reads,
        },
        FixedClock::new(at()),
    )
}

fn open_command() -> OpenWorkOrder {
    OpenWorkOrder {
        work_order_id: WorkOrderId::try_from_uuid(id(30)).unwrap(),
        request: request_envelope(),
        title: "Replace drive belt".into(),
    }
}

fn aggregate() -> WorkOrder {
    let mut value = WorkOrder::open(
        WorkOrderId::try_from_uuid(id(30)).unwrap(),
        scope(),
        MaintenanceRequest::try_new(
            EventId::try_from_uuid(id(20)).unwrap(),
            3,
            assets::AssetRef::try_from_uuid(id(10)).unwrap(),
            "REQ-100",
        )
        .unwrap(),
        asset_fact(),
        "Replace drive belt",
        at(),
    )
    .unwrap();
    value.take_uncommitted_events();
    value
}

#[test]
fn source_event_and_stable_identity_open_once() {
    let uow = FakeUnitOfWork::default();
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = handler(
        FakeWriteRepository(None),
        uow.clone(),
        true,
        true,
        reads.clone(),
    );
    let first = block_on(handler.handle(command_envelope(1, None, open_command()))).unwrap();
    let replay = block_on(handler.handle(command_envelope(1, None, open_command()))).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn wrong_causation_rejects_before_assets_fact_read_or_commit() {
    let uow = FakeUnitOfWork::default();
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = handler(
        FakeWriteRepository(None),
        uow.clone(),
        true,
        true,
        reads.clone(),
    );
    let mut envelope = command_envelope(2, None, open_command());
    envelope.causation_id = Some(EventId::try_from_uuid(id(99)).unwrap());
    let error = block_on(handler.handle(envelope)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn authorization_and_entitlement_gate_source_fact_consumption() {
    for (authorization, entitlement) in [(false, true), (true, false)] {
        let uow = FakeUnitOfWork::default();
        let reads = Arc::new(AtomicUsize::new(0));
        let error = block_on(
            handler(
                FakeWriteRepository(None),
                uow.clone(),
                authorization,
                entitlement,
                reads.clone(),
            )
            .handle(command_envelope(3, None, open_command())),
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn expected_version_is_checked_before_atomic_commit() {
    let uow = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWriteRepository(Some(aggregate())),
            uow.clone(),
            true,
            true,
            Arc::new(AtomicUsize::new(0)),
        )
        .handle(command_envelope(
            4,
            Some(99),
            ScheduleWorkOrder {
                work_order_id: WorkOrderId::try_from_uuid(id(30)).unwrap(),
                scheduled_for: at() + chrono::Duration::hours(1),
            },
        )),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn read_your_writes_not_ready_never_falls_back_to_work_order_store() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = MaintenanceQueryHandler::new(
        NotReadyRead(reads.clone()),
        FakeAuthorization(true),
        FakeEntitlement(true),
    );
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(901)).unwrap(),
        global_sequence: 7,
    };
    let error = block_on(handler.handle(QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(500)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(501)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "maintenance.test.query".into(),
            customer_visible: false,
            module: Some("maintenance".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "maintenance.work_order".into(),
            partition: "company:2".into(),
            target,
            timeout_millis: 10,
        }),
        query: GetWorkOrder {
            work_order_id: WorkOrderId::try_from_uuid(id(30)).unwrap(),
        },
    }))
    .unwrap_err();
    assert!(matches!(error, CqrsError::NotReady(value) if value.target == target));
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
