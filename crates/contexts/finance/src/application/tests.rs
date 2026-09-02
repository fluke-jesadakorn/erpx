use std::{
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

use async_trait::async_trait;
use chrono::{NaiveDate, TimeZone, Utc};
use erpx_cqrs::{
    CONTRACT_VERSION, CommandEnvelope, CommandHandler, EventPosition, FreshnessState,
    IdempotencyKey, ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency,
    QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_integration_contracts::{
    BoundedContext, CostCenterRef, CurrencyCode, DimensionSet, IntegrationEvent, MoneyAmount,
    RecordRef, ReferenceCode, customer, finance, organization, sales,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, HumanDecisionProof, QueryId, SessionId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{
        AccountCategory, CustomerInvoiceStatus, JournalLineId, NormalBalance, PeriodStatus,
        RevenueScheduleStatus,
    },
    ports::{ReadProjection, SubledgerView},
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
        session_id: Some(SessionId::try_from_uuid(id(3)).unwrap()),
        represented_principal_ref: None,
        support_capability_id: None,
    }
}
fn account(value: u128, code: &str, category: AccountCategory) -> Account {
    Account {
        account_id: AccountId::try_from_uuid(id(value)).unwrap(),
        code: code.into(),
        name: code.into(),
        category,
        normal_balance: if category == AccountCategory::Asset {
            NormalBalance::Debit
        } else {
            NormalBalance::Credit
        },
        parent_account_id: None,
        status: AccountStatus::Active,
    }
}
fn chart() -> Chart {
    Chart::create(
        ChartId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "MAIN".into(),
        "Main".into(),
        "THB".into(),
        vec![
            account(11, "1100", AccountCategory::Asset),
            account(12, "4100", AccountCategory::Revenue),
        ],
        at(),
    )
    .unwrap()
}
fn period() -> AccountingPeriod {
    AccountingPeriod::create(
        PeriodId::try_from_uuid(id(20)).unwrap(),
        scope(),
        "2026-08".into(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        at(),
    )
    .unwrap()
}
fn period_decision(principal_ref: &str, command_id: CommandId) -> HumanDecisionProof {
    HumanDecisionProof::try_new(actor(principal_ref), scope(), command_id.to_string(), at())
        .unwrap()
}
fn closing_period(requester: &str) -> AccountingPeriod {
    let mut value = period();
    value.begin_close(requester, at()).unwrap();
    value.take_uncommitted_events();
    value
}
fn closed_period(closer: &str) -> AccountingPeriod {
    let mut value = closing_period("requester");
    let command_id = CommandId::try_from_uuid(id(990)).unwrap();
    value
        .close_with_decision(period_decision(closer, command_id), command_id, at())
        .unwrap();
    value.take_uncommitted_events();
    value
}
fn journal() -> Journal {
    Journal::create(
        JournalId::try_from_uuid(id(30)).unwrap(),
        scope(),
        ChartId::try_from_uuid(id(10)).unwrap(),
        PeriodId::try_from_uuid(id(20)).unwrap(),
        "J-30".into(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
        "THB".into(),
        None,
        JournalSource::Manual {
            reference: "MAN-30".into(),
        },
        "posting".into(),
        vec![
            JournalLine {
                line_id: JournalLineId::try_from_uuid(id(31)).unwrap(),
                account_id: AccountId::try_from_uuid(id(11)).unwrap(),
                debit_minor: 500,
                credit_minor: 0,
                currency: "THB".into(),
                memo: "cash".into(),
            },
            JournalLine {
                line_id: JournalLineId::try_from_uuid(id(32)).unwrap(),
                account_id: AccountId::try_from_uuid(id(12)).unwrap(),
                debit_minor: 0,
                credit_minor: 500,
                currency: "THB".into(),
                memo: "revenue".into(),
            },
        ],
        at(),
    )
    .unwrap()
}
fn billing_fact() -> sales::BillingEligibilityFact {
    sales::BillingEligibilityFact {
        billing_event: sales::BillingEventRef::try_from_uuid(id(39)).unwrap(),
        fulfillment: sales::FulfillmentRef::try_from_uuid(id(38)).unwrap(),
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        billable_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        tax_amount: MoneyAmount::new(35, CurrencyCode::try_new("THB").unwrap()),
        eligible_on: chrono::NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
        dimensions: dimensions(),
    }
}

fn dimensions() -> DimensionSet {
    let mut value =
        DimensionSet::for_legal_entity(organization::CompanyRef::try_from_uuid(id(2)).unwrap());
    value.business_unit = Some(organization::BusinessUnitRef::try_from_uuid(id(30)).unwrap());
    value.cost_center = Some(CostCenterRef::try_from_uuid(id(31)).unwrap());
    value
}

fn invoice_fact() -> finance::CustomerInvoicePostedFact {
    finance::CustomerInvoicePostedFact {
        invoice: finance::CustomerInvoiceRef::try_from_uuid(id(49)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        invoice_code: ReferenceCode::try_new("INV-49").unwrap(),
        source: RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 2)
            .unwrap(),
        net_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        tax_amount: MoneyAmount::new(35, CurrencyCode::try_new("THB").unwrap()),
        receivable_amount: MoneyAmount::new(535, CurrencyCode::try_new("THB").unwrap()),
        dimensions: dimensions(),
        posted_at: at(),
    }
}

fn draft_invoice() -> CustomerInvoice {
    draft_invoice_by("preparer")
}

fn draft_invoice_by(prepared_by: &str) -> CustomerInvoice {
    CustomerInvoice::create_from_billing(
        CustomerInvoiceId::try_from_uuid(id(48)).unwrap(),
        scope(),
        "INV-48".into(),
        RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 1).unwrap(),
        billing_fact(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
        prepared_by.into(),
        at(),
    )
    .unwrap()
}

fn arrangement_fact() -> sales::RevenueArrangementFact {
    sales::RevenueArrangementFact {
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        legal_contract: None,
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        dimensions: dimensions(),
        obligations: vec![sales::PerformanceObligationFact {
            obligation: sales::PerformanceObligationRef::try_from_uuid(id(42)).unwrap(),
            line: sales::SalesOrderLineRef::try_from_uuid(id(43)).unwrap(),
            model: sales::RevenueModel::Goods,
            allocated_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
            service_starts_on: None,
            service_ends_on: None,
        }],
    }
}

fn active_revenue_schedule() -> RevenueSchedule {
    RevenueSchedule::create_from_arrangement(
        RevenueScheduleId::try_from_uuid(id(70)).unwrap(),
        scope(),
        RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 1).unwrap(),
        arrangement_fact(),
        at(),
    )
    .unwrap()
}

fn fulfillment_fact() -> sales::FulfillmentAcceptedFact {
    sales::FulfillmentAcceptedFact {
        fulfillment: sales::FulfillmentRef::try_from_uuid(id(71)).unwrap(),
        arrangement: sales::RevenueArrangementRef::try_from_uuid(id(37)).unwrap(),
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        obligation: sales::PerformanceObligationRef::try_from_uuid(id(42)).unwrap(),
        kind: sales::FulfillmentKind::GoodsDelivery,
        accepted_amount: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        accepted_at: at(),
        dimensions: dimensions(),
    }
}

fn command_envelope(
    sequence: u128,
    expected: Option<u64>,
    command: FinanceCommand,
) -> CommandEnvelope<FinanceCommand> {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        command_id: CommandId::try_from_uuid(id(100 + sequence)).unwrap(),
        idempotency_key: IdempotencyKey::try_new(format!("finance-{sequence}")).unwrap(),
        correlation_id: CorrelationId::try_from_uuid(id(200 + sequence)).unwrap(),
        causation_id: None,
        actor: actor("controller"),
        active_scope: scope(),
        audit: AuditContext {
            purpose: "finance.test".into(),
            customer_visible: false,
            module: Some("finance".into()),
        },
        expected_aggregate_version: expected,
        command,
    }
}

#[derive(Clone)]
struct FakeAuthorization(bool);
#[async_trait]
impl AuthorizationPort for FakeAuthorization {
    async fn authorize(&self, _: AuthorizationRequest<'_>) -> Result<(), PortError> {
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
    async fn ensure_finance_enabled(&self, _: &AuthorityScope, _: bool) -> Result<(), PortError> {
        if self.0 {
            Ok(())
        } else {
            Err(PortError::EntitlementDenied)
        }
    }
}

#[derive(Clone, Default)]
struct FakeWrite {
    chart: Option<Chart>,
    period: Option<AccountingPeriod>,
    journal: Option<Journal>,
    subledger: Option<SubledgerItem>,
    invoice: Option<CustomerInvoice>,
    revenue_schedule: Option<RevenueSchedule>,
    loads: Arc<AtomicUsize>,
}
#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_chart(&self, _: &AuthorityScope, _: ChartId) -> Result<Option<Chart>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.chart.clone())
    }
    async fn load_period(
        &self,
        _: &AuthorityScope,
        _: PeriodId,
    ) -> Result<Option<AccountingPeriod>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.period.clone())
    }
    async fn load_journal(
        &self,
        _: &AuthorityScope,
        _: JournalId,
    ) -> Result<Option<Journal>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.journal.clone())
    }
    async fn load_subledger(
        &self,
        _: &AuthorityScope,
        _: SubledgerId,
    ) -> Result<Option<SubledgerItem>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.subledger.clone())
    }
    async fn load_customer_invoice(
        &self,
        _: &AuthorityScope,
        _: CustomerInvoiceId,
    ) -> Result<Option<CustomerInvoice>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.invoice.clone())
    }
    async fn load_revenue_schedule(
        &self,
        _: &AuthorityScope,
        _: RevenueScheduleId,
    ) -> Result<Option<RevenueSchedule>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.revenue_schedule.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    commits: Arc<AtomicUsize>,
    receipt_lookups: Arc<AtomicUsize>,
    event_types: Arc<Mutex<Vec<Vec<&'static str>>>>,
    references: Arc<Mutex<Vec<Vec<ReferenceVersion>>>>,
}
#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(&self, _: &CommandIdentity) -> Result<Option<StoredCommit>, PortError> {
        self.receipt_lookups.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
    async fn commit(&self, request: FinanceCommit) -> Result<StoredCommit, PortError> {
        self.commits.fetch_add(1, Ordering::SeqCst);
        self.event_types.lock().unwrap().push(
            request
                .integration_events
                .iter()
                .map(IntegrationEvent::event_type)
                .collect(),
        );
        self.references
            .lock()
            .unwrap()
            .push(request.reference_versions);
        Ok(StoredCommit {
            command_id: request.identity.command_id,
            correlation_id: request.correlation_id,
            aggregate: request.aggregate,
            committed_events: vec![EventPosition {
                event_id: EventId::try_from_uuid(id(900)).unwrap(),
                global_sequence: 1,
            }],
            committed_at: at(),
            replayed: false,
        })
    }
}

fn handler(
    write: FakeWrite,
    uow: FakeUnitOfWork,
    authorized: bool,
) -> FinanceCommandHandler<FakeWrite, FakeUnitOfWork, FakeAuthorization, FakeEntitlement, FixedClock>
{
    FinanceCommandHandler::new(
        write,
        uow,
        FakeAuthorization(authorized),
        FakeEntitlement(true),
        FixedClock::new(at()),
    )
}

#[test]
fn denied_create_never_reaches_atomic_commit() {
    let uow = FakeUnitOfWork::default();
    let error = block_on(handler(FakeWrite::default(), uow.clone(), false).handle(
        command_envelope(
            1,
            None,
            FinanceCommand::CreateReceivableFromInvoice(CreateReceivableFromInvoice {
                subledger_id: SubledgerId::try_from_uuid(id(50)).unwrap(),
                scope: scope(),
                fact: invoice_fact(),
            }),
        ),
    ))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Denied);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn mutating_command_requires_exact_expected_version() {
    let uow = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWrite {
                chart: Some(chart()),
                ..FakeWrite::default()
            },
            uow.clone(),
            true,
        )
        .handle(command_envelope(
            2,
            Some(99),
            FinanceCommand::AddAccount(AddAccount {
                chart_id: ChartId::try_from_uuid(id(10)).unwrap(),
                account: account(13, "5100", AccountCategory::Expense),
            }),
        )),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn period_decisions_reject_non_humans_before_receipt_or_period_access() {
    let commands = [
        FinanceCommand::ClosePeriod(ClosePeriod {
            period_id: PeriodId::try_from_uuid(id(20)).unwrap(),
        }),
        FinanceCommand::ReopenPeriod(ReopenPeriod {
            period_id: PeriodId::try_from_uuid(id(20)).unwrap(),
        }),
    ];
    for command in commands {
        for kind in [
            ActorKind::Agent,
            ActorKind::Service,
            ActorKind::SystemWorker,
        ] {
            let loads = Arc::new(AtomicUsize::new(0));
            let uow = FakeUnitOfWork::default();
            let mut envelope = command_envelope(31, Some(2), command.clone());
            envelope.actor.kind = kind;

            let error = block_on(
                handler(
                    FakeWrite {
                        loads: loads.clone(),
                        ..FakeWrite::default()
                    },
                    uow.clone(),
                    true,
                )
                .handle(envelope),
            )
            .unwrap_err();

            assert_eq!(error.category(), ErrorCategory::Denied);
            assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
            assert_eq!(loads.load(Ordering::SeqCst), 0);
            assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn closing_period_derives_human_approver_and_enforces_requester_separation() {
    let period = closing_period("controller");
    let requester_uow = FakeUnitOfWork::default();
    let requester_error = block_on(
        handler(
            FakeWrite {
                period: Some(period.clone()),
                ..FakeWrite::default()
            },
            requester_uow.clone(),
            true,
        )
        .handle(command_envelope(
            32,
            Some(period.version()),
            FinanceCommand::ClosePeriod(ClosePeriod {
                period_id: period.id(),
            }),
        )),
    )
    .unwrap_err();
    assert_eq!(requester_error.category(), ErrorCategory::Invalid);
    assert_eq!(requester_uow.commits.load(Ordering::SeqCst), 0);

    let approver_uow = FakeUnitOfWork::default();
    let mut envelope = command_envelope(
        33,
        Some(period.version()),
        FinanceCommand::ClosePeriod(ClosePeriod {
            period_id: period.id(),
        }),
    );
    envelope.actor.principal_ref = "approver".into();
    let receipt = block_on(
        handler(
            FakeWrite {
                period: Some(period),
                ..FakeWrite::default()
            },
            approver_uow.clone(),
            true,
        )
        .handle(envelope),
    )
    .unwrap();
    let FinanceMutationResult::Period(closed) = receipt.response else {
        panic!("expected period response")
    };
    assert_eq!(closed.status, PeriodStatus::Closed);
    assert_eq!(closed.closed_by.as_deref(), Some("approver"));
    assert_eq!(approver_uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn reopening_period_derives_human_actor_and_enforces_closer_separation() {
    let period = closed_period("approver");
    let closer_uow = FakeUnitOfWork::default();
    let mut closer_envelope = command_envelope(
        34,
        Some(period.version()),
        FinanceCommand::ReopenPeriod(ReopenPeriod {
            period_id: period.id(),
        }),
    );
    closer_envelope.actor.principal_ref = "approver".into();
    let closer_error = block_on(
        handler(
            FakeWrite {
                period: Some(period.clone()),
                ..FakeWrite::default()
            },
            closer_uow.clone(),
            true,
        )
        .handle(closer_envelope),
    )
    .unwrap_err();
    assert_eq!(closer_error.category(), ErrorCategory::Invalid);
    assert_eq!(closer_uow.commits.load(Ordering::SeqCst), 0);

    let controller_uow = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(
            FakeWrite {
                period: Some(period.clone()),
                ..FakeWrite::default()
            },
            controller_uow.clone(),
            true,
        )
        .handle(command_envelope(
            35,
            Some(period.version()),
            FinanceCommand::ReopenPeriod(ReopenPeriod {
                period_id: period.id(),
            }),
        )),
    )
    .unwrap();
    let FinanceMutationResult::Period(open) = receipt.response else {
        panic!("expected period response")
    };
    assert_eq!(open.status, PeriodStatus::Open);
    assert_eq!(open.close_requested_by, None);
    assert_eq!(open.closed_by, None);
    assert_eq!(controller_uow.commits.load(Ordering::SeqCst), 1);
}

#[test]
fn beginning_period_close_preserves_existing_agent_behavior() {
    let period = period();
    let uow = FakeUnitOfWork::default();
    let mut envelope = command_envelope(
        36,
        Some(period.version()),
        FinanceCommand::BeginPeriodClose(BeginPeriodClose {
            period_id: period.id(),
        }),
    );
    envelope.actor.principal_ref = "close-workflow".into();
    envelope.actor.kind = ActorKind::Agent;
    let receipt = block_on(
        handler(
            FakeWrite {
                period: Some(period),
                ..FakeWrite::default()
            },
            uow,
            true,
        )
        .handle(envelope),
    )
    .unwrap();
    let FinanceMutationResult::Period(closing) = receipt.response else {
        panic!("expected period response")
    };
    assert_eq!(closing.status, PeriodStatus::Closing);
    assert_eq!(
        closing.close_requested_by.as_deref(),
        Some("close-workflow")
    );
}

#[test]
fn posting_fences_chart_and_period_and_publishes_owner_fact() {
    let uow = FakeUnitOfWork::default();
    block_on(
        handler(
            FakeWrite {
                chart: Some(chart()),
                period: Some(period()),
                journal: Some(journal()),
                ..FakeWrite::default()
            },
            uow.clone(),
            true,
        )
        .handle(command_envelope(
            3,
            Some(1),
            FinanceCommand::PostJournal(PostJournal {
                journal_id: JournalId::try_from_uuid(id(30)).unwrap(),
                expected_chart_version: 1,
                expected_period_version: 1,
            }),
        )),
    )
    .unwrap();
    assert_eq!(
        uow.event_types.lock().unwrap().as_slice(),
        &[vec!["finance.journal_posted"]]
    );
    let references = uow.references.lock().unwrap();
    assert_eq!(references[0].len(), 2);
    assert_eq!(references[0][0].aggregate_type, "chart");
    assert_eq!(references[0][1].aggregate_type, "period");
}

#[test]
fn posting_rejects_non_human_actor_before_receipt_or_journal_access() {
    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        let loads = Arc::new(AtomicUsize::new(0));
        let uow = FakeUnitOfWork::default();
        let mut envelope = command_envelope(
            30,
            Some(1),
            FinanceCommand::PostJournal(PostJournal {
                journal_id: JournalId::try_from_uuid(id(30)).unwrap(),
                expected_chart_version: 1,
                expected_period_version: 1,
            }),
        );
        envelope.actor.kind = kind;
        let error = block_on(
            handler(
                FakeWrite {
                    loads: loads.clone(),
                    ..FakeWrite::default()
                },
                uow.clone(),
                true,
            )
            .handle(envelope),
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn customer_invoice_posting_rejects_non_humans_before_receipt_or_invoice_access() {
    for kind in [
        ActorKind::Agent,
        ActorKind::Service,
        ActorKind::SystemWorker,
    ] {
        let loads = Arc::new(AtomicUsize::new(0));
        let uow = FakeUnitOfWork::default();
        let mut envelope = command_envelope(
            37,
            Some(1),
            FinanceCommand::PostCustomerInvoice(PostCustomerInvoice {
                invoice_id: CustomerInvoiceId::try_from_uuid(id(48)).unwrap(),
            }),
        );
        envelope.actor.kind = kind;

        let error = block_on(
            handler(
                FakeWrite {
                    loads: loads.clone(),
                    ..FakeWrite::default()
                },
                uow.clone(),
                true,
            )
            .handle(envelope),
        )
        .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Denied);
        assert_eq!(uow.receipt_lookups.load(Ordering::SeqCst), 0);
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn customer_invoice_posting_derives_human_poster_and_enforces_preparer_separation() {
    let invoice = draft_invoice_by("controller");
    let uow = FakeUnitOfWork::default();
    let error = block_on(
        handler(
            FakeWrite {
                invoice: Some(invoice.clone()),
                ..FakeWrite::default()
            },
            uow.clone(),
            true,
        )
        .handle(command_envelope(
            38,
            Some(invoice.version()),
            FinanceCommand::PostCustomerInvoice(PostCustomerInvoice {
                invoice_id: invoice.id(),
            }),
        )),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Invalid);
    assert_eq!(uow.commits.load(Ordering::SeqCst), 0);
}

#[test]
fn posting_customer_invoice_publishes_invoice_fact_before_ar_creation() {
    let uow = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(
            FakeWrite {
                invoice: Some(draft_invoice()),
                ..FakeWrite::default()
            },
            uow.clone(),
            true,
        )
        .handle(command_envelope(
            6,
            Some(1),
            FinanceCommand::PostCustomerInvoice(PostCustomerInvoice {
                invoice_id: CustomerInvoiceId::try_from_uuid(id(48)).unwrap(),
            }),
        )),
    )
    .unwrap();
    let FinanceMutationResult::CustomerInvoice(value) = receipt.response else {
        panic!("expected customer invoice response");
    };
    assert_eq!(value.status, CustomerInvoiceStatus::Posted);
    assert_eq!(value.posted_by.as_deref(), Some("controller"));
    assert_eq!(value.source, draft_invoice().source().clone());
    assert_eq!(
        uow.event_types.lock().unwrap().as_slice(),
        &[vec!["finance.customer_invoice_posted"]]
    );
}

#[test]
fn recognizing_fulfillment_publishes_revenue_without_invoice_or_ar_event() {
    let uow = FakeUnitOfWork::default();
    let receipt = block_on(
        handler(
            FakeWrite {
                revenue_schedule: Some(active_revenue_schedule()),
                ..FakeWrite::default()
            },
            uow.clone(),
            true,
        )
        .handle(command_envelope(
            7,
            Some(1),
            FinanceCommand::RecognizeRevenueFromFulfillment(RecognizeRevenueFromFulfillment {
                schedule_id: RevenueScheduleId::try_from_uuid(id(70)).unwrap(),
                source: RecordRef::try_new(BoundedContext::Sales, "revenue_arrangement", id(37), 2)
                    .unwrap(),
                fact: fulfillment_fact(),
            }),
        )),
    )
    .unwrap();
    let FinanceMutationResult::RevenueSchedule(value) = receipt.response else {
        panic!("expected revenue schedule response");
    };
    assert_eq!(value.status, RevenueScheduleStatus::Completed);
    assert_eq!(value.recognized_total_minor, 500);
    assert_eq!(
        uow.event_types.lock().unwrap().as_slice(),
        &[vec!["finance.revenue_recognized"]]
    );
}

#[test]
fn stable_fingerprint_excludes_actor_and_retry_time() {
    let command = FinanceCommand::CreateReceivableFromInvoice(CreateReceivableFromInvoice {
        subledger_id: SubledgerId::try_from_uuid(id(50)).unwrap(),
        scope: scope(),
        fact: invoice_fact(),
    });
    let first = command.fingerprint();
    let second = command.clone().fingerprint();
    assert_eq!(first, second);
}

#[derive(Clone)]
struct NotReadyRead {
    reads: Arc<AtomicUsize>,
}
#[async_trait]
impl ReadRepository for NotReadyRead {
    async fn get_chart(
        &self,
        _: &AuthorityScope,
        _: ChartId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ChartView>>, PortError> {
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
    async fn list_charts(
        &self,
        _: &AuthorityScope,
        _: &ChartListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<ChartList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_period(
        &self,
        _: &AuthorityScope,
        _: PeriodId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PeriodView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_journal(
        &self,
        _: &AuthorityScope,
        _: JournalId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<JournalView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_journals(
        &self,
        _: &AuthorityScope,
        _: &JournalListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<JournalList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_subledger(
        &self,
        _: &AuthorityScope,
        _: SubledgerId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubledgerView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_customer_invoice(
        &self,
        _: &AuthorityScope,
        _: CustomerInvoiceId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<CustomerInvoiceView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_revenue_schedule(
        &self,
        _: &AuthorityScope,
        _: RevenueScheduleId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RevenueScheduleView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_subledger(
        &self,
        _: &AuthorityScope,
        _: &SubledgerListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<SubledgerList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_trial_balance(
        &self,
        _: &AuthorityScope,
        _: ChartId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<TrialBalanceView>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_bank_transactions(
        &self,
        _: &AuthorityScope,
        _: &BankTransactionListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<BankTransactionList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_statutory_tax_report(
        &self,
        _: &AuthorityScope,
        _: &str,
        _: &str,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<StatutoryTaxReport>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_financial_statements(
        &self,
        _: &AuthorityScope,
        _: ChartId,
        _: PeriodId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<crate::domain::FinancialStatements>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_aging(
        &self,
        _: &AuthorityScope,
        _: &crate::domain::SubledgerKind,
        _: &NaiveDate,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<crate::domain::AgingReport>, PortError> {
        Err(PortError::Internal)
    }
}

#[test]
fn read_your_writes_not_ready_never_falls_back_to_write_store() {
    let reads = Arc::new(AtomicUsize::new(0));
    let target = ProjectionTarget {
        event_id: EventId::try_from_uuid(id(901)).unwrap(),
        global_sequence: 7,
    };
    let error = block_on(
        FinanceQueryHandler::new(
            NotReadyRead {
                reads: reads.clone(),
            },
            FakeAuthorization(true),
            FakeEntitlement(true),
        )
        .handle(QueryEnvelope {
            contract_version: CONTRACT_VERSION,
            query_id: QueryId::try_from_uuid(id(500)).unwrap(),
            correlation_id: CorrelationId::try_from_uuid(id(501)).unwrap(),
            actor: actor("reader"),
            active_scope: scope(),
            audit: AuditContext {
                purpose: "finance.test.query".into(),
                customer_visible: false,
                module: Some("finance".into()),
            },
            consistency: QueryConsistency::ReadYourWrites(ReadYourWrites {
                projection: "finance.charts".into(),
                partition: "company:2".into(),
                target,
                timeout_millis: 10,
            }),
            query: FinanceQuery::GetChart(GetChart {
                chart_id: ChartId::try_from_uuid(id(10)).unwrap(),
            }),
        }),
    )
    .unwrap_err();
    assert!(matches!(error, CqrsError::NotReady(value) if value.target == target));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

#[test]
fn query_metadata_is_projection_owned() {
    let metadata = ProjectionMetadata {
        checkpoint: None,
        freshness: FreshnessState::Current,
        lag_millis: Some(0),
    };
    assert_eq!(metadata.freshness, FreshnessState::Current);
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
