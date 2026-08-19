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
    CostCenterRef, CurrencyCode, DimensionSet, IntegrationEvent, MoneyAmount, crm, customer,
    organization, sales,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, AuthorityScope, CommandId, CorrelationId,
    ErrorCategory, EventId, FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{QuoteStatus, RevenueArrangementStatus, SalesOrderLineId, SalesOrderStatus},
    ports::{
        AuthorizationRequest, CommandIdentity, PortError, ReadProjection, SalesCommit, StoredCommit,
    },
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
        .single()
        .unwrap()
}

fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "sales-test-actor".into(),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn opportunity(stage: crm::OpportunityStage) -> crm::OpportunityFact {
    crm::OpportunityFact {
        opportunity: crm::OpportunityRef::try_from_uuid(id(3)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(4)).unwrap(),
        stage,
        expected_value: MoneyAmount::new(2_000, CurrencyCode::try_new("THB").unwrap()),
    }
}

fn line() -> SalesOrderLine {
    SalesOrderLine {
        line_id: SalesOrderLineId::try_from_uuid(id(11)).unwrap(),
        item_code: "ITEM-11".into(),
        description: "Item".into(),
        quantity: 2,
        unit_price_minor: 900,
        tax_minor: 200,
        currency: "THB".into(),
    }
}

fn create_quote() -> CreateQuote {
    CreateQuote {
        quote_id: QuoteId::try_from_uuid(id(10)).unwrap(),
        scope: scope(),
        opportunity: opportunity(crm::OpportunityStage::Proposal),
        quote_code: "QT-10".into(),
        currency: "THB".into(),
        lines: vec![line()],
        valid_until: at(12),
    }
}

fn accepted_quote() -> Quote {
    let mut quote = Quote::create_from_opportunity(
        QuoteId::try_from_uuid(id(10)).unwrap(),
        scope(),
        opportunity(crm::OpportunityStage::Won),
        "QT-10".into(),
        "THB".into(),
        vec![line()],
        at(12),
        at(9),
    )
    .unwrap();
    quote.take_uncommitted_events();
    quote.send(at(9)).unwrap();
    quote.accept(at(9)).unwrap();
    quote.take_uncommitted_events();
    quote
}

fn draft_order() -> SalesOrder {
    SalesOrder::create_from_quote(
        SalesOrderId::try_from_uuid(id(20)).unwrap(),
        &accepted_quote(),
        "SO-20".into(),
        at(9),
    )
    .unwrap()
}

fn confirmed_order() -> SalesOrder {
    let mut order = draft_order();
    order.confirm(at(9)).unwrap();
    order.take_uncommitted_events();
    order
}

fn dimensions() -> DimensionSet {
    let mut value =
        DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(id(2)).unwrap());
    value.business_unit = Some(organization::BusinessUnitRef::try_from_uuid(id(30)).unwrap());
    value.cost_center = Some(CostCenterRef::try_from_uuid(id(31)).unwrap());
    value
}

fn arrangement() -> RevenueArrangement {
    let mut value = RevenueArrangement::activate_from_order(
        RevenueArrangementId::try_from_uuid(id(40)).unwrap(),
        &confirmed_order(),
        None,
        dimensions(),
        vec![PerformanceObligationDefinition {
            obligation_id: PerformanceObligationId::try_from_uuid(id(41)).unwrap(),
            line_id: SalesOrderLineId::try_from_uuid(id(11)).unwrap(),
            model: sales::RevenueModel::Goods,
            service_starts_on: None,
            service_ends_on: None,
        }],
        at(9),
    )
    .unwrap();
    value.take_uncommitted_events();
    value
}

fn command_envelope(
    sequence: u128,
    expected: Option<u64>,
    command: SalesCommand,
) -> CommandEnvelope<SalesCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("sales-test-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor(),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "sales.test".into(),
            customer_visible: false,
            module: Some("sales".into()),
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
    async fn ensure_sales_enabled(
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
struct FakeWriteRepository {
    quote: Option<Quote>,
    order: Option<SalesOrder>,
    arrangement: Option<RevenueArrangement>,
    loads: Arc<AtomicUsize>,
}

#[async_trait]
impl WriteRepository for FakeWriteRepository {
    async fn load_quote(
        &self,
        _authority_scope: &AuthorityScope,
        _quote_id: QuoteId,
    ) -> Result<Option<Quote>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.quote.clone())
    }

    async fn load_order(
        &self,
        _authority_scope: &AuthorityScope,
        _sales_order_id: SalesOrderId,
    ) -> Result<Option<SalesOrder>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.order.clone())
    }

    async fn load_revenue_arrangement(
        &self,
        _authority_scope: &AuthorityScope,
        _arrangement_id: RevenueArrangementId,
    ) -> Result<Option<RevenueArrangement>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.arrangement.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    receipts: Arc<Mutex<Vec<(CommandIdentity, StoredCommit)>>>,
    commits: Arc<AtomicUsize>,
    event_types: Arc<Mutex<Vec<Vec<&'static str>>>>,
}

#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
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

    async fn commit(&self, request: SalesCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.event_types.lock().unwrap().push(
            request
                .integration_events
                .iter()
                .map(IntegrationEvent::event_type)
                .collect(),
        );
        let stored = StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: vec![EventPosition {
                event_id: EventId::try_from_uuid(id(900)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(9),
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
    write: FakeWriteRepository,
    unit_of_work: FakeUnitOfWork,
    authorization: bool,
    entitlement: bool,
) -> SalesCommandHandler<
    FakeWriteRepository,
    FakeUnitOfWork,
    FakeAuthorization,
    FakeEntitlement,
    FixedClock,
> {
    SalesCommandHandler::new(
        write,
        unit_of_work,
        FakeAuthorization(authorization),
        FakeEntitlement(entitlement),
        FixedClock::new(at(9)),
    )
}

#[test]
fn stable_command_identity_replays_without_loading_or_committing_again() {
    let write = FakeWriteRepository::default();
    let unit_of_work = FakeUnitOfWork::default();
    let handler = handler(write.clone(), unit_of_work.clone(), true, true);
    let command = SalesCommand::CreateQuote(create_quote());
    let first = block_on(handler.handle(command_envelope(1, None, command.clone()))).unwrap();
    let replay = block_on(handler.handle(command_envelope(1, None, command))).unwrap();
    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 1);
    assert_eq!(write.loads.load(Ordering::SeqCst), 0);
}

#[test]
fn authorization_and_entitlement_gate_atomic_commit() {
    for (authorization, entitlement) in [(false, true), (true, false)] {
        let unit_of_work = FakeUnitOfWork::default();
        let error = block_on(
            handler(
                FakeWriteRepository::default(),
                unit_of_work.clone(),
                authorization,
                entitlement,
            )
            .handle(command_envelope(
                2,
                None,
                SalesCommand::CreateQuote(create_quote()),
            )),
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn expected_version_is_checked_before_quote_mutation_commit() {
    let unit_of_work = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWriteRepository {
                quote: Some(accepted_quote()),
                ..FakeWriteRepository::default()
            },
            unit_of_work.clone(),
            true,
            true,
        )
        .handle(command_envelope(
            3,
            Some(99),
            SalesCommand::RejectQuote(RejectQuote {
                quote_id: QuoteId::try_from_uuid(id(10)).unwrap(),
            }),
        )),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn create_order_requires_exact_accepted_quote_version() {
    let quote = accepted_quote();
    assert_eq!(quote.status(), QuoteStatus::Accepted);
    let unit_of_work = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWriteRepository {
                quote: Some(quote),
                ..FakeWriteRepository::default()
            },
            unit_of_work.clone(),
            true,
            true,
        )
        .handle(command_envelope(
            4,
            None,
            SalesCommand::CreateSalesOrder(CreateSalesOrder {
                sales_order_id: SalesOrderId::try_from_uuid(id(20)).unwrap(),
                quote_id: QuoteId::try_from_uuid(id(10)).unwrap(),
                expected_quote_version: 2,
                order_code: "SO-20".into(),
            }),
        )),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(unit_of_work.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn confirmed_order_publishes_only_sales_owned_fact() {
    let unit_of_work = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(
            FakeWriteRepository {
                order: Some(draft_order()),
                ..FakeWriteRepository::default()
            },
            unit_of_work.clone(),
            true,
            true,
        )
        .handle(command_envelope(
            5,
            Some(1),
            SalesCommand::ConfirmSalesOrder(ConfirmSalesOrder {
                sales_order_id: SalesOrderId::try_from_uuid(id(20)).unwrap(),
            }),
        )),
    )
    .unwrap();
    let SalesMutationResult::SalesOrder(order) = receipt.response else {
        panic!("expected sales order response");
    };
    assert_eq!(order.status, SalesOrderStatus::Confirmed);
    assert_eq!(
        unit_of_work.event_types.lock().unwrap().as_slice(),
        &[vec!["sales.order_changed"]]
    );
}

#[test]
fn accepted_fulfillment_publishes_billing_eligibility_but_no_finance_event() {
    let unit_of_work = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(
            FakeWriteRepository {
                arrangement: Some(arrangement()),
                ..FakeWriteRepository::default()
            },
            unit_of_work.clone(),
            true,
            true,
        )
        .handle(command_envelope(
            6,
            Some(1),
            SalesCommand::AcceptRevenueFulfillment(AcceptRevenueFulfillment {
                arrangement_id: RevenueArrangementId::try_from_uuid(id(40)).unwrap(),
                fulfillment_id: FulfillmentId::try_from_uuid(id(42)).unwrap(),
                billing_event_id: BillingEventId::try_from_uuid(id(43)).unwrap(),
                obligation_id: PerformanceObligationId::try_from_uuid(id(41)).unwrap(),
                kind: sales::FulfillmentKind::GoodsDelivery,
                accepted_minor: 1_800,
                tax_minor: 200,
                eligible_on: at(9).date_naive(),
            }),
        )),
    )
    .unwrap();
    let SalesMutationResult::RevenueArrangement(value) = receipt.response else {
        panic!("expected revenue arrangement response");
    };
    assert_eq!(value.status, RevenueArrangementStatus::Completed);
    assert_eq!(
        unit_of_work.event_types.lock().unwrap().as_slice(),
        &[vec!["sales.fulfillment_accepted", "sales.billing_eligible"]]
    );
}

#[derive(Clone)]
struct NotReadyRead {
    reads: Arc<AtomicUsize>,
}

#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_quote(
        &self,
        _authority_scope: &AuthorityScope,
        _quote_id: QuoteId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<QuoteView>>, PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
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

    async fn get_order(
        &self,
        _authority_scope: &AuthorityScope,
        _sales_order_id: SalesOrderId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SalesOrderView>>, PortError> {
        Err(PortError::Internal)
    }

    async fn get_revenue_arrangement(
        &self,
        _authority_scope: &AuthorityScope,
        _arrangement_id: RevenueArrangementId,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RevenueArrangementView>>, PortError> {
        Err(PortError::Internal)
    }

    async fn list_orders(
        &self,
        _authority_scope: &AuthorityScope,
        _filter: &SalesOrderListFilter,
        _consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SalesOrderList>, PortError> {
        Err(PortError::Internal)
    }
}

#[test]
fn read_your_writes_not_ready_never_falls_back_to_write_store() {
    let reads = Arc::new(AtomicUsize::new(0));
    let handler = SalesQueryHandler::new(
        NotReadyRead {
            reads: reads.clone(),
        },
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
        active_scope: scope(),
        audit: AuditContext {
            purpose: "sales.test.query".into(),
            customer_visible: false,
            module: Some("sales".into()),
        },
        consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
            projection: "sales.quotes".into(),
            partition: "company:2".into(),
            target,
            timeout_millis: 10,
        }),
        query: SalesQuery::GetQuote(GetQuote {
            quote_id: QuoteId::try_from_uuid(id(10)).unwrap(),
        }),
    }))
    .unwrap_err();
    assert!(matches!(error, CqrsError::NotReady(value) if value.target == target));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn core_manifest_has_no_crm_runtime_or_framework_dependency() {
    let manifest = include_str!("../../Cargo.toml");
    assert!(!manifest.contains("erpx-crm"));
    for forbidden in ["sqlx", "tonic", "prost", "dioxus"] {
        assert!(
            !manifest
                .lines()
                .any(|line| line.trim_start().starts_with(forbidden)),
            "Sales core must not depend on {forbidden}"
        );
    }
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
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
