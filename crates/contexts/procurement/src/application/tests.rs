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
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::PurchaseOrderLineId,
    ports::{ReadProjection, StoredCommit},
};

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
        principal_ref: "procurement-test".into(),
        kind: ActorKind::TenantUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn line() -> PurchaseOrderLine {
    PurchaseOrderLine {
        line_id: PurchaseOrderLineId::try_from_uuid(id(12)).unwrap(),
        item_code: "ITEM-1".into(),
        ordered_quantity: 2,
        unit_price_minor: 100,
        currency: "THB".into(),
        destination_code: "WH-1".into(),
    }
}

fn create_command() -> CreatePurchaseOrder {
    CreatePurchaseOrder {
        purchase_order_id: PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
        scope: scope(),
        order_code: "PO-1".into(),
        supplier_id: SupplierId::try_from_uuid(id(11)).unwrap(),
        currency: "THB".into(),
        lines: vec![line()],
    }
}

fn aggregate() -> PurchaseOrder {
    let command = create_command();
    let mut aggregate = PurchaseOrder::create(
        command.purchase_order_id,
        command.scope,
        command.order_code,
        command.supplier_id,
        command.currency,
        command.lines,
        at(),
    )
    .unwrap();
    aggregate.take_uncommitted_events();
    aggregate
}

fn command_envelope<C>(sequence: u128, expected: Option<u64>, command: C) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("procurement-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "procurement.test".into(),
            customer_visible: false,
            module: Some("procurement".into()),
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
    async fn ensure_procurement_enabled(
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
struct FakeWrite(Arc<Mutex<Option<PurchaseOrder>>>);

#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load(
        &self,
        _authority_scope: &TenantScope,
        _purchase_order_id: PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>, PortError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct FakeUow {
    receipts: Arc<Mutex<Vec<(CommandIdentity, StoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUow {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        for (stored_identity, receipt) in self.receipts.lock().unwrap().iter() {
            if stored_identity.command_id == identity.command_id
                || stored_identity.idempotency_key == identity.idempotency_key
            {
                return if stored_identity == identity {
                    Ok(Some(receipt.clone()))
                } else {
                    Err(PortError::IdempotencyConflict)
                };
            }
        }
        Ok(None)
    }

    async fn commit(&self, request: ProcurementCommit) -> Result<StoredCommit, PortError> {
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

fn handler(
    write: Option<PurchaseOrder>,
    uow: FakeUow,
    authorized: bool,
    entitled: bool,
) -> ProcurementCommandHandler<FakeWrite, FakeUow, FakeAuthorization, FakeEntitlement, FixedClock> {
    ProcurementCommandHandler::new(
        FakeWrite(Arc::new(Mutex::new(write))),
        uow,
        FakeAuthorization(authorized),
        FakeEntitlement(entitled),
        FixedClock::new(at()),
    )
}

#[test]
fn stable_identity_replays_and_changed_payload_conflicts() {
    let uow = FakeUow::default();
    let handler = handler(None, uow.clone(), true, true);
    let envelope = command_envelope(1, None, create_command());
    assert!(!block_on(handler.handle(envelope.clone())).unwrap().replayed);
    assert!(block_on(handler.handle(envelope.clone())).unwrap().replayed);
    let mut changed = envelope;
    changed.command.order_code = "PO-CHANGED".into();
    assert_eq!(
        block_on(handler.handle(changed)).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn authorization_and_entitlement_are_both_command_guards() {
    for (authorized, entitled) in [(false, true), (true, false)] {
        let uow = FakeUow::default();
        let error = block_on(
            handler(None, uow.clone(), authorized, entitled).handle(command_envelope(
                2,
                None,
                create_command(),
            )),
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn expected_version_is_required_and_checked_before_release() {
    let uow = FakeUow::default();
    let command = ReleasePurchaseOrder {
        purchase_order_id: PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
    };
    let missing = block_on(
        handler(Some(aggregate()), uow.clone(), true, true)
            .handle(command_envelope(3, None, command)),
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::Invalid);
    let conflict = block_on(
        handler(Some(aggregate()), uow.clone(), true, true).handle(command_envelope(
            4,
            Some(99),
            command,
        )),
    )
    .unwrap_err();
    assert_eq!(conflict.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct NotReadyRead(Arc<AtomicUsize>);

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get(
        &self,
        _authority_scope: &TenantScope,
        _purchase_order_id: PurchaseOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PurchaseOrderView>>, PortError> {
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
        _filter: &PurchaseOrderListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PurchaseOrderList>, PortError> {
        unreachable!()
    }
}

#[test]
fn read_your_writes_not_ready_is_returned_without_write_fallback() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = ProcurementQueryHandler::new(
        NotReadyRead(reads.clone()),
        FakeAuthorization(true),
        FakeEntitlement(true),
    );
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(999)).unwrap(),
        global_sequence: 12,
    };
    let error = block_on(handler.handle(QueryEnvelope {
        contract_version: CONTRACT_VERSION,
        query_id: QueryId::try_from_uuid(id(300)).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(301)).unwrap(),
        actor: actor(),
        tenant_scope: scope(),
        audit: AuditContext {
            purpose: "procurement.test.query".into(),
            customer_visible: false,
            module: Some("procurement".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "procurement.orders".into(),
            partition: "holding-1".into(),
            target,
            timeout_millis: 25,
        }),
        query: GetPurchaseOrder {
            purchase_order_id: PurchaseOrderId::try_from_uuid(id(10)).unwrap(),
        },
    }))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotReady);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
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
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
