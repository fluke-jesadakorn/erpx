use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, FreshnessState,
    IdempotencyKey, ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency,
    QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId, SessionId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{PaymentId, PaymentMethod, RefundStatus, SaleLineId, SaleStatus},
    ports::{ReadProjection, RefundCommit, RefundStoredCommit, SaleCommit, SaleStoredCommit},
};

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
fn actor(name: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: name.into(),
        kind: ActorKind::HoldingUser,
        session_id: Some(SessionId::try_from_uuid(id(99)).unwrap()),
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn line() -> SaleLine {
    SaleLine {
        line_id: SaleLineId::try_from_uuid(id(10)).unwrap(),
        item_reference: "SKU-1".into(),
        description: "Item".into(),
        quantity: 2,
        unit_price_minor: 100,
        tax_minor: 14,
    }
}
fn create_sale() -> CreateSale {
    CreateSale {
        sale_id: SaleId::try_from_uuid(id(20)).unwrap(),
        scope: scope(),
        receipt_code: "R-1".into(),
        register_id: RegisterId::try_from_uuid(id(21)).unwrap(),
        customer_id: None,
        currency: "THB".into(),
        lines: vec![line()],
    }
}
fn sale() -> Sale {
    let command = create_sale();
    let mut sale = Sale::create(
        command.sale_id,
        command.scope,
        command.receipt_code,
        command.register_id,
        command.customer_id,
        command.currency,
        command.lines,
        at(),
    )
    .unwrap();
    sale.take_uncommitted_events();
    sale
}
fn completed_sale() -> Sale {
    let mut value = sale();
    value.open(at()).unwrap();
    value
        .record_payment(
            Payment {
                payment_id: PaymentId::try_from_uuid(id(30)).unwrap(),
                amount_minor: 214,
                method: PaymentMethod::Cash,
                received_at: at(),
            },
            at(),
        )
        .unwrap();
    value.complete(at()).unwrap();
    value.take_uncommitted_events();
    value
}

fn command_envelope<C>(
    sequence: u128,
    expected: Option<u64>,
    principal: &str,
    command: C,
) -> CommandEnvelope<C> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("pos-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(principal),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "pos.test".into(),
            customer_visible: false,
            module: Some("pos".into()),
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
    async fn ensure_pos_enabled(
        &self,
        _scope: &AuthorityScope,
        _write: bool,
    ) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeWrite {
    sale: Arc<Mutex<Option<Sale>>>,
    refund: Arc<Mutex<Option<PosRefund>>>,
    refund_loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_sale(
        &self,
        _authority_scope: &AuthorityScope,
        _sale_id: SaleId,
    ) -> Result<Option<Sale>, PortError> {
        Ok(self.sale.lock().unwrap().clone())
    }

    async fn load_refund(
        &self,
        _authority_scope: &AuthorityScope,
        _refund_id: RefundId,
    ) -> Result<Option<PosRefund>, PortError> {
        self.refund_loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.refund.lock().unwrap().clone())
    }
}

#[derive(Clone, Default)]
struct FakeSaleUow {
    receipts: Arc<Mutex<Vec<(CommandIdentity, SaleStoredCommit)>>>,
    commits: Arc<AtomicUsize>,
}

#[async_trait]
impl SaleUnitOfWork for FakeSaleUow {
    async fn find_sale_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<SaleStoredCommit>, PortError> {
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

    async fn commit_sale(&self, request: SaleCommit) -> Result<SaleStoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        let stored = SaleStoredCommit {
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

#[derive(Clone, Default)]
struct FakeRefundUow {
    commits: Arc<AtomicUsize>,
    receipt_lookups: Arc<AtomicUsize>,
}

#[async_trait]
impl RefundUnitOfWork for FakeRefundUow {
    async fn find_refund_receipt(
        &self,
        _identity: &CommandIdentity,
    ) -> Result<Option<RefundStoredCommit>, PortError> {
        self.receipt_lookups.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    async fn commit_refund(&self, request: RefundCommit) -> Result<RefundStoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        Ok(RefundStoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: vec![],
            committed_at: at(),
            replayed: false,
        })
    }
}

#[tokio::test]
async fn stable_sale_identity_replays_and_changed_payload_conflicts() {
    let uow = FakeSaleUow::default();
    let handler = PosSaleCommandHandler::new(
        FakeWrite::default(),
        uow.clone(),
        FakeAuthorization(true),
        FakeEntitlement(true),
        FixedClock::new(at()),
    );
    let envelope = command_envelope(1, None, "cashier", PosSaleCommand::Create(create_sale()));
    assert!(!handler.handle(envelope.clone()).await.unwrap().replayed);
    assert!(handler.handle(envelope.clone()).await.unwrap().replayed);
    let mut changed = envelope;
    let PosSaleCommand::Create(command) = &mut changed.command else {
        unreachable!()
    };
    command.receipt_code = "R-CHANGED".into();
    assert_eq!(
        handler.handle(changed).await.unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn expected_version_and_access_guards_run_before_sale_commit() {
    let write = FakeWrite {
        sale: Arc::new(Mutex::new(Some(sale()))),
        ..Default::default()
    };
    for (authorized, expected, category) in [
        (false, Some(1), ErrorCategory::Denied),
        (true, None, ErrorCategory::Invalid),
        (true, Some(99), ErrorCategory::Conflict),
    ] {
        let uow = FakeSaleUow::default();
        let handler = PosSaleCommandHandler::new(
            write.clone(),
            uow.clone(),
            FakeAuthorization(authorized),
            FakeEntitlement(true),
            FixedClock::new(at()),
        );
        let error = handler
            .handle(command_envelope(
                10 + if authorized { 1 } else { 0 },
                expected,
                "cashier",
                PosSaleCommand::Open(OpenSale {
                    sale_id: SaleId::try_from_uuid(id(20)).unwrap(),
                }),
            ))
            .await
            .unwrap_err();
        assert_eq!(error.category(), category);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn refund_request_uses_authoritative_completed_sale_and_self_approval_never_commits() {
    let refund_id = RefundId::try_from_uuid(id(40)).unwrap();
    let request = RequestRefund {
        refund_id,
        sale_id: SaleId::try_from_uuid(id(20)).unwrap(),
        lines: vec![RefundLine {
            sale_line_id: line().line_id,
            quantity: 1,
            amount_minor: 107,
        }],
        reason: "customer return".into(),
    };
    let uow = FakeRefundUow::default();
    let write = FakeWrite {
        sale: Arc::new(Mutex::new(Some(completed_sale()))),
        ..Default::default()
    };
    let handler = PosRefundCommandHandler::new(
        write,
        uow.clone(),
        FakeAuthorization(true),
        FakeEntitlement(true),
        FixedClock::new(at()),
    );
    let receipt = handler
        .handle(command_envelope(
            20,
            None,
            "requester",
            PosRefundCommand::Request(request),
        ))
        .await
        .unwrap();
    assert_eq!(receipt.response.refund.status, RefundStatus::Requested);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);

    let requested = PosRefund::request(
        refund_id,
        &completed_sale(),
        vec![RefundLine {
            sale_line_id: line().line_id,
            quantity: 1,
            amount_minor: 107,
        }],
        "customer return".into(),
        "requester".into(),
        at(),
    )
    .unwrap();
    let uow = FakeRefundUow::default();
    let write = FakeWrite {
        refund: Arc::new(Mutex::new(Some(requested))),
        ..Default::default()
    };
    let handler = PosRefundCommandHandler::new(
        write,
        uow.clone(),
        FakeAuthorization(true),
        FakeEntitlement(true),
        FixedClock::new(at()),
    );
    let error = handler
        .handle(command_envelope(
            21,
            Some(1),
            "requester",
            PosRefundCommand::Approve(ApproveRefund { refund_id }),
        ))
        .await
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn refund_review_rejects_non_humans_before_receipt_or_aggregate_access() {
    let refund_id = RefundId::try_from_uuid(id(40)).unwrap();
    let requested = PosRefund::request(
        refund_id,
        &completed_sale(),
        vec![RefundLine {
            sale_line_id: line().line_id,
            quantity: 1,
            amount_minor: 107,
        }],
        "customer return".into(),
        "requester".into(),
        at(),
    )
    .unwrap();

    for (offset, command) in [
        PosRefundCommand::Approve(ApproveRefund { refund_id }),
        PosRefundCommand::Reject(RejectRefund { refund_id }),
    ]
    .into_iter()
    .enumerate()
    {
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            let refund_loads = Arc::new(AtomicUsize::new(0));
            let uow = FakeRefundUow::default();
            let handler = PosRefundCommandHandler::new(
                FakeWrite {
                    refund: Arc::new(Mutex::new(Some(requested.clone()))),
                    refund_loads: refund_loads.clone(),
                    ..Default::default()
                },
                uow.clone(),
                FakeAuthorization(true),
                FakeEntitlement(true),
                FixedClock::new(at()),
            );
            let mut request = command_envelope(
                30 + u128::try_from(offset).unwrap(),
                Some(requested.version()),
                "reviewer",
                command.clone(),
            );
            request.actor.kind = kind;

            let error = handler.handle(request).await.unwrap_err();
            assert_eq!(error.category(), ErrorCategory::Denied);
            assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
            assert_eq!(refund_loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test]
async fn refund_approval_derives_reviewer_from_the_authenticated_human() {
    let refund_id = RefundId::try_from_uuid(id(40)).unwrap();
    let requested = PosRefund::request(
        refund_id,
        &completed_sale(),
        vec![RefundLine {
            sale_line_id: line().line_id,
            quantity: 1,
            amount_minor: 107,
        }],
        "customer return".into(),
        "requester".into(),
        at(),
    )
    .unwrap();
    let uow = FakeRefundUow::default();
    let handler = PosRefundCommandHandler::new(
        FakeWrite {
            refund: Arc::new(Mutex::new(Some(requested.clone()))),
            ..Default::default()
        },
        uow.clone(),
        FakeAuthorization(true),
        FakeEntitlement(true),
        FixedClock::new(at()),
    );

    let receipt = handler
        .handle(command_envelope(
            40,
            Some(requested.version()),
            "reviewer",
            PosRefundCommand::Approve(ApproveRefund { refund_id }),
        ))
        .await
        .unwrap();
    assert_eq!(receipt.response.refund.status, RefundStatus::Approved);
    assert_eq!(
        receipt.response.refund.reviewer_principal_ref.as_deref(),
        Some("reviewer")
    );
    assert_eq!(uow.commits.load(Ordering::SeqCst), 1);
}

#[derive(Clone)]
struct NotReadyRead(Arc<AtomicUsize>);

#[async_trait]
impl PosReadRepository for NotReadyRead {
    async fn get_sale(
        &self,
        _authority_scope: &AuthorityScope,
        _sale_id: SaleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SaleView>>, PortError> {
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

    async fn list_sales(
        &self,
        _authority_scope: &AuthorityScope,
        _filter: &SaleListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SaleList>, PortError> {
        Ok(ReadProjection {
            value: SaleList {
                sales: vec![],
                next_cursor: None,
            },
            metadata: ProjectionMetadata {
                checkpoint: None,
                freshness: FreshnessState::Current,
                lag_millis: None,
            },
        })
    }

    async fn get_refund(
        &self,
        _authority_scope: &AuthorityScope,
        _refund_id: RefundId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RefundView>>, PortError> {
        unreachable!()
    }

    async fn list_refunds(
        &self,
        _authority_scope: &AuthorityScope,
        _filter: &RefundListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RefundList>, PortError> {
        unreachable!()
    }
}

#[tokio::test]
async fn read_your_writes_not_ready_is_returned_without_write_fallback() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = PosQueryHandler::new(
        NotReadyRead(reads.clone()),
        FakeAuthorization(true),
        FakeEntitlement(true),
    );
    let error = handler
        .handle(QueryEnvelope {
            contract_version: CONTRACT_VERSION,
            query_id: QueryId::try_from_uuid(id(300)).unwrap(),
            correlation_id: CorrelationId::try_from_uuid(id(301)).unwrap(),
            actor: actor("cashier"),
            active_scope: scope(),
            audit: AuditContext {
                purpose: "pos.test.query".into(),
                customer_visible: false,
                module: Some("pos".into()),
            },
            consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: "pos.sales".into(),
                partition: "holding-1".into(),
                target: ProjectionTarget {
                    event_id: EventId::try_from_uuid(id(999)).unwrap(),
                    global_sequence: 12,
                },
                timeout_millis: 25,
            }),
            query: PosQuery::GetSale(GetSale {
                sale_id: SaleId::try_from_uuid(id(20)).unwrap(),
            }),
        })
        .await
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotReady);
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn core_manifest_and_production_sources_keep_adapter_boundaries() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for forbidden in ["sqlx", "tonic", "prost", "dioxus"] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden core dependency: {forbidden}"
        );
    }
    for relative in [
        "src/lib.rs",
        "src/domain/mod.rs",
        "src/application/mod.rs",
        "src/ports/mod.rs",
        "src/integration/mod.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).unwrap();
        for forbidden in ["erpx_sales", "erpx_finance", "erpx_domain"] {
            assert!(
                !source.contains(forbidden),
                "cross-context import in {relative}: {forbidden}"
            );
        }
    }
}

#[test]
fn view_conversion_preserves_sale_projection_facts() {
    let value = sale();
    let view = SaleView::from(&value);
    assert_eq!(view.status, SaleStatus::Draft);
    assert_eq!(view.total_minor, 214);
}
