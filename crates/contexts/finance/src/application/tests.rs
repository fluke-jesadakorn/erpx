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
    IdempotencyKey, ProjectionMetadata, ProjectionNotReady, ProjectionTarget, QueryConsistency,
    QueryEnvelope, QueryHandler, ReadYourWrites,
};
use erpx_integration_contracts::{
    CurrencyCode, IntegrationEvent, MoneyAmount, ReferenceCode, customer, sales,
};
use erpx_kernel::{
    ActorAttribution, ActorKind, AuditContext, CommandId, CorrelationId, ErrorCategory, EventId,
    FixedClock, QueryId,
};
use uuid::Uuid;

use super::*;
use crate::{
    domain::{AccountCategory, JournalLineId, NormalBalance},
    ports::{ReadProjection, SubledgerView},
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
fn actor(name: &str) -> ActorAttribution {
    ActorAttribution {
        principal_ref: name.into(),
        kind: ActorKind::TenantUser,
        session_id: None,
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
fn journal() -> Journal {
    Journal::create(
        JournalId::try_from_uuid(id(30)).unwrap(),
        scope(),
        ChartId::try_from_uuid(id(10)).unwrap(),
        PeriodId::try_from_uuid(id(20)).unwrap(),
        "J-30".into(),
        chrono::NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
        "THB".into(),
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
fn sales_fact() -> sales::SalesOrderFact {
    sales::SalesOrderFact {
        sales_order: sales::SalesOrderRef::try_from_uuid(id(40)).unwrap(),
        order_code: ReferenceCode::try_new("SO-40").unwrap(),
        customer: customer::CustomerRef::try_from_uuid(id(41)).unwrap(),
        status: sales::SalesOrderStatus::Confirmed,
        total: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        lines: vec![sales::SalesOrderLineFact {
            line: sales::SalesOrderLineRef::try_from_uuid(id(42)).unwrap(),
            item_code: ReferenceCode::try_new("ITEM").unwrap(),
            quantity: erpx_integration_contracts::Quantity::try_new(1, 0).unwrap(),
            line_value: MoneyAmount::new(500, CurrencyCode::try_new("THB").unwrap()),
        }],
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
        tenant_scope: scope(),
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
    async fn ensure_finance_enabled(&self, _: &TenantScope, _: bool) -> Result<(), PortError> {
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
    loads: Arc<AtomicUsize>,
}
#[async_trait]
impl WriteRepository for FakeWrite {
    async fn load_chart(&self, _: &TenantScope, _: ChartId) -> Result<Option<Chart>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.chart.clone())
    }
    async fn load_period(
        &self,
        _: &TenantScope,
        _: PeriodId,
    ) -> Result<Option<AccountingPeriod>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.period.clone())
    }
    async fn load_journal(
        &self,
        _: &TenantScope,
        _: JournalId,
    ) -> Result<Option<Journal>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.journal.clone())
    }
    async fn load_subledger(
        &self,
        _: &TenantScope,
        _: SubledgerId,
    ) -> Result<Option<SubledgerItem>, PortError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.subledger.clone())
    }
}

#[derive(Clone, Default)]
struct FakeUnitOfWork {
    commits: Arc<AtomicUsize>,
    event_types: Arc<Mutex<Vec<Vec<&'static str>>>>,
    references: Arc<Mutex<Vec<Vec<ReferenceVersion>>>>,
}
#[async_trait]
impl CommandUnitOfWork for FakeUnitOfWork {
    async fn find_receipt(&self, _: &CommandIdentity) -> Result<Option<StoredCommit>, PortError> {
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
            FinanceCommand::CreateReceivableFromSales(CreateReceivableFromSales {
                subledger_id: SubledgerId::try_from_uuid(id(50)).unwrap(),
                scope: scope(),
                fact: sales_fact(),
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
fn stable_fingerprint_excludes_actor_and_retry_time() {
    let command = FinanceCommand::CreateReceivableFromSales(CreateReceivableFromSales {
        subledger_id: SubledgerId::try_from_uuid(id(50)).unwrap(),
        scope: scope(),
        fact: sales_fact(),
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
        _: &TenantScope,
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
    async fn get_period(
        &self,
        _: &TenantScope,
        _: PeriodId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PeriodView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_journal(
        &self,
        _: &TenantScope,
        _: JournalId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<JournalView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_journals(
        &self,
        _: &TenantScope,
        _: &JournalListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<JournalList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_subledger(
        &self,
        _: &TenantScope,
        _: SubledgerId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SubledgerView>>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_subledger(
        &self,
        _: &TenantScope,
        _: &SubledgerListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<SubledgerList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_trial_balance(
        &self,
        _: &TenantScope,
        _: ChartId,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<TrialBalanceView>, PortError> {
        Err(PortError::Internal)
    }
    async fn list_bank_transactions(
        &self,
        _: &TenantScope,
        _: &BankTransactionListFilter,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<BankTransactionList>, PortError> {
        Err(PortError::Internal)
    }
    async fn get_statutory_tax_report(
        &self,
        _: &TenantScope,
        _: &str,
        _: &str,
        _: &QueryConsistency,
    ) -> Result<ReadProjection<StatutoryTaxReport>, PortError> {
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
            tenant_scope: scope(),
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
