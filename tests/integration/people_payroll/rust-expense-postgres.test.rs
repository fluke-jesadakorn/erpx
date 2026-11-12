//! Native expense claims on migrated PostgreSQL.
//!
//! Proves serializable owner SQL, command receipts/audit/outbox, claim version
//! CAS, Company/branch isolation, the retained v2 stage machine, decisions,
//! Treasury payment preparation/confirmation/cancellation and reversal.
//! Organization/Workflow/Files/Finance ports are deliberately owner-local fakes;
//! their real composition is mounted by the lead, and the fail-closed defaults
//! are exercised by the sibling unit target.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::expense::{
    ExpenseApprovalDecision, ExpenseApprovalInstance, ExpenseApprovalPort, ExpenseApprovalView,
    ExpenseCommandFuture, ExpenseEvidenceFacts, ExpenseEvidenceOwner, ExpenseEvidencePort,
    ExpenseEvidenceRemoval, ExpenseFileBinding, ExpensePayable, ExpensePaymentSettlement,
    ExpensePaymentSettlementRequest, ExpensePostingRequest, ExpensePostingSettlement,
    ExpenseReversalRequest, ExpenseReversalSettlement, ExpenseRoute, ExpenseRoutePort,
    ExpenseRouteStep, ExpenseRow, ExpenseSettlementPort, ExpenseStaffFacts, ExpenseStaffPort,
    NoExpenseApprovalPort, PgExpenseCommand, PgExpenseReader, cancel_expense,
    cancel_expense_payment, confirm_expense_payment, decide_expense, get_expense,
    prepare_expense_payment, reverse_expense, save_expense, save_expense_accounting,
    save_expense_payment, submit_expense,
};
use erpx_server_people_payroll::public::workforce::WorkforceCommandActor;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn owner() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "owner-assignment".into(),
        role: "employee".into(),
        permissions: vec!["expense.prepare".into(), "expense.read".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn reviewer() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "reviewer".into(),
        assignment_id: "reviewer-assignment".into(),
        permissions: vec!["expense.approve".into(), "expense.read".into()],
        ..owner()
    }
}

fn payer() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "payer".into(),
        assignment_id: "payer-assignment".into(),
        permissions: vec!["expense.pay".into(), "expense.read".into()],
        ..owner()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed_org(pool: &DatabasePool, company: &str) {
    for (id, kind, slug, parent, branch) in [
        ("company", "company", "company", None, None),
        ("branch", "branch", "bangkok", Some("company"), None),
        (
            "department",
            "department",
            "engineering",
            Some("branch"),
            Some("branch"),
        ),
    ] {
        sqlx::query(
            "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,\
             branch_id,status) VALUES($1,$2,$3,$3,$4,$5,$6,'active') ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(kind)
        .bind(slug)
        .bind(parent)
        .bind(company)
        .bind(branch)
        .execute(pool.raw())
        .await
        .unwrap();
    }
}

async fn seed_worker(pool: &DatabasePool, company: &str) {
    sqlx::query(
        "INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,\
         updated_at) VALUES('person',$1,'Somchai Jaidee','TH','seed',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,\
         employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
         VALUES('engagement','person',$1,'branch','department','employee','salaried','active',\
         '2026-01-01','Engineer','THB','seed',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_finance(pool: &DatabasePool, company: &str) {
    sqlx::query(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,status,version,created_by,\
         created_at,updated_at) VALUES('chart',$1,'Chart','THB','active',1,'seed',0,0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    for (id, code, name, category) in [
        ("account", "5100", "Expense", "expense"),
        ("payable", "2100", "Payable", "liability"),
    ] {
        sqlx::query(
            "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,\
             is_control,status,version,created_at,updated_at) \
             VALUES($1,'chart',$2,$3,$4,$5,'THB',0,'active',1,0,0) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(company)
        .bind(code)
        .bind(name)
        .bind(category)
        .execute(pool.raw())
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at) \
         VALUES('period',$1,'Feb 2026','2026-02-01','2026-02-28','open',0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,\
         payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) \
         VALUES('party',$1,'supplier','SUP-1','Supplier','THB',NULL,30,0,'active',1,0,0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,\
         currency,amount_minor,outstanding_minor,status,version,created_by,created_at,updated_at) \
         VALUES('payment-1',$1,'party','payment','PAY-1','2026-02-04','THB',120000,0,'settled',1,\
         'payer',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,\
         currency,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,\
         posted_at,created_at,updated_at) \
         VALUES('journal',$1,'chart','period','JRN-1','2026-02-03','THB','expense_claim',\
         'expense-posted','Expense','posted',0,1,'reviewer','reviewer',0,0,0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,\
         currency,source_type,source_id,memo,status,is_opening,version,created_by,posted_by,\
         posted_at,created_at,updated_at) \
         VALUES('journal-reversal',$1,'chart','period','JRN-REV-1','2026-02-04','THB',\
         'expense_reversal','expense-posted','Reversal','posted',0,1,'reviewer','reviewer',0,0,0) \
         ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,\
         due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,\
         debit_account_id,credit_account_id,source_type,source_id,status,version,created_by,\
         created_at,updated_at) \
         VALUES('invoice',$1,'party','payable','EXP-1','2026-02-03','2026-02-03','THB',120000,0,\
         120000,120000,'account','payable','expense_claim','expense-posted','posted',1,'reviewer',\
         0,0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_posted_expense(pool: &DatabasePool, company: &str, id: &str) {
    sqlx::query(
        "INSERT INTO finance_expenses(id,company_id,employee_id,engagement_id,branch_id,\
         department_id,claimant_user_id,claimant_name,expense_date,merchant,currency,amount_minor,\
         tax_minor,category_account_id,receipt_reference,business_purpose,prepared_by,status,\
         workflow_version,workflow_stage,version,created_by,created_at,updated_at,approved_by,\
         submitted_at,posted_at,invoice_id,journal_id,accounting_json) \
         VALUES($1,$2,'person','engagement','branch','department','owner','Somchai Jaidee',\
         '2026-02-01','Bookshop','THB',120000,0,'account','RCPT-1','Team reference books',\
         'owner','posted',2,'payment',3,'owner',0,0,'reviewer',0,0,'invoice','journal',\
         '{\"categoryAccountId\":\"account\",\"periodId\":\"period\",\"entryDate\":\"2026-02-03\",\"preparedBy\":\"accountant\"}')",
    )
    .bind(id)
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn staff() -> ExpenseStaffFacts {
    ExpenseStaffFacts {
        person_id: "person".into(),
        engagement_id: "engagement".into(),
        engagement_version: 1,
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        legal_name: "Somchai Jaidee".into(),
        worker_type: "employee".into(),
        level_active: true,
        start_date: "2026-01-01".into(),
        end_date: None,
    }
}

struct FakeStaff(ExpenseStaffFacts);

impl ExpenseStaffPort for FakeStaff {
    fn active_staff<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseStaffFacts>, BusinessError> {
        Box::pin(async move { Ok(Some(self.0.clone())) })
    }
}

struct FakeRoute;

impl ExpenseRoutePort for FakeRoute {
    fn route<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a [String],
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseRoute, BusinessError> {
        Box::pin(async {
            Ok(ExpenseRoute {
                steps: vec![ExpenseRouteStep {
                    stage: "department_approval".into(),
                }],
                blocked_reason: None,
            })
        })
    }
}

struct FakeApproval {
    final_decision: bool,
    next_stage: Option<String>,
    can_decide: bool,
    payers: Vec<String>,
    opens: usize,
    decisions: usize,
    closes: usize,
}

impl Default for FakeApproval {
    fn default() -> Self {
        Self {
            final_decision: true,
            next_stage: Some("accounts_review".into()),
            can_decide: true,
            payers: vec!["payer".into()],
            opens: 0,
            decisions: 0,
            closes: 0,
        }
    }
}

impl ExpenseApprovalPort for FakeApproval {
    fn open<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a ExpenseRoute,
        _: i64,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalInstance, BusinessError> {
        Box::pin(async move {
            self.opens += 1;
            Ok(ExpenseApprovalInstance {
                instance_id: "approval".into(),
            })
        })
    }
    fn prepare_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> ExpenseCommandFuture<'a, ExpenseApprovalDecision, BusinessError> {
        Box::pin(async move {
            Ok(ExpenseApprovalDecision {
                instance_id: "approval".into(),
                final_decision: self.final_decision,
                next_stage: self.next_stage.clone(),
            })
        })
    }
    fn apply_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.decisions += 1;
            Ok(())
        })
    }
    fn close<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.closes += 1;
            Ok(())
        })
    }
    fn view<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a WorkforceCommandActor,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseApprovalView>, BusinessError> {
        Box::pin(async move {
            Ok(Some(ExpenseApprovalView {
                can_view: true,
                can_decide: self.can_decide,
                can_override: false,
                blocked_reason: None,
            }))
        })
    }
    fn payers<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Vec<String>, BusinessError> {
        Box::pin(async move { Ok(self.payers.clone()) })
    }
}

struct FakeEvidence {
    facts: ExpenseEvidenceFacts,
}

impl Default for FakeEvidence {
    fn default() -> Self {
        Self {
            facts: ExpenseEvidenceFacts {
                claim_files: 1,
                claim_pending: 0,
                payment_slips: 1,
                payment_pending: 0,
            },
        }
    }
}

impl ExpenseEvidencePort for FakeEvidence {
    fn facts<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, ExpenseEvidenceFacts, BusinessError> {
        Box::pin(async move { Ok(self.facts.clone()) })
    }
    fn binding<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: ExpenseEvidenceOwner<'a>,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpenseFileBinding>, BusinessError> {
        Box::pin(async { Ok(None) })
    }
    fn remove<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpenseEvidenceRemoval<'a>,
    ) -> ExpenseCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

struct FakeSettlement {
    payable: Option<ExpensePayable>,
    outstanding: i64,
    posts: usize,
    confirms: usize,
    reverses: usize,
}

impl Default for FakeSettlement {
    fn default() -> Self {
        Self {
            payable: Some(ExpensePayable {
                outstanding_minor: 120_000,
                total_minor: 120_000,
            }),
            outstanding: 0,
            posts: 0,
            confirms: 0,
            reverses: 0,
        }
    }
}

impl ExpenseSettlementPort for FakeSettlement {
    fn payable<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> ExpenseCommandFuture<'a, Option<ExpensePayable>, BusinessError> {
        Box::pin(async move { Ok(self.payable.clone()) })
    }
    fn post<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpensePostingRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePostingSettlement, BusinessError> {
        Box::pin(async move {
            self.posts += 1;
            Ok(ExpensePostingSettlement {
                journal_id: "journal".into(),
                invoice_id: "invoice".into(),
            })
        })
    }
    fn confirm_payment<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpensePaymentSettlementRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpensePaymentSettlement, BusinessError> {
        Box::pin(async move {
            self.confirms += 1;
            Ok(ExpensePaymentSettlement {
                payment_id: "payment-1".into(),
                outstanding_minor: self.outstanding,
            })
        })
    }
    fn reverse<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a ExpenseReversalRequest<'a>,
    ) -> ExpenseCommandFuture<'a, ExpenseReversalSettlement, BusinessError> {
        Box::pin(async move {
            self.reverses += 1;
            Ok(ExpenseReversalSettlement {
                reversal_journal_id: "journal-reversal".into(),
            })
        })
    }
}

fn draft_body() -> Value {
    json!({
        "expenseDate": "2026-02-01",
        "merchant": "Bookshop",
        "receiptReference": "RCPT-1",
        "businessPurpose": "Team reference books",
        "currency": "THB",
        "amount": "1200.00",
        "tax": "0.00",
        "categoryAccountId": "account"
    })
}

async fn count(pool: &DatabasePool, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

async fn expense_state(pool: &DatabasePool, id: &str) -> (String, String, i64, Option<String>) {
    let row = sqlx::query(
        "SELECT status,workflow_stage,version,approved_by FROM finance_expenses WHERE id=$1",
    )
    .bind(id)
    .fetch_one(pool.raw())
    .await
    .unwrap();
    use sqlx::Row;
    (
        row.try_get("status").unwrap(),
        row.try_get("workflow_stage").unwrap(),
        row.try_get("version").unwrap(),
        row.try_get("approved_by").unwrap(),
    )
}

#[tokio::test]
async fn create_is_atomic_idempotent_and_company_scoped() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        let created = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    save_expense(
                        &actor,
                        None,
                        &draft_body(),
                        &key("expense-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(created.id, "expense-expense-create-1");
        assert_eq!(created.status, "draft");
        assert_eq!(created.version, 1);
        assert_eq!(count(&pool, "finance_expenses").await, 1);
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            assert_eq!(count(&pool, table).await, 1, "{table}");
        }
        let replay = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    save_expense(
                        &actor,
                        None,
                        &draft_body(),
                        &key("expense-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(replay.id, created.id);
        assert_eq!(count(&pool, "finance_expenses").await, 1);
        let conflict = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    save_expense(
                        &actor,
                        None,
                        &json!({
                            "expenseDate": "2026-02-02",
                            "merchant": "Other",
                            "receiptReference": "RCPT-2",
                            "businessPurpose": "Different purpose",
                            "currency": "THB",
                            "amount": "10.00",
                            "tax": "0.00",
                            "categoryAccountId": "account"
                        }),
                        &key("expense-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut reader = PgExpenseReader::new(connection);
                    reader
                        .expense_row(
                            &CompanyId::new("foreign").unwrap(),
                            "expense-expense-create-1",
                            "owner",
                            true,
                            None,
                            None,
                        )
                        .await
                })
            })
            .await
            .unwrap();
        assert!(error.is_none());
    })
    .await;
}

#[tokio::test]
async fn submit_and_stage_decisions_commit_placement_and_history() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = owner();
                let mut command = PgExpenseCommand::new(connection, &actor);
                save_expense(
                    &actor,
                    None,
                    &draft_body(),
                    &key("stage-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let submitted = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    submit_expense(
                        &actor,
                        "expense-stage-create-1",
                        &json!({"action": "submit", "expectedVersion": 1}),
                        &key("stage-submit-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::default(),
                        &mut FakeEvidence::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(submitted.status, "in_review");
        assert_eq!(submitted.stage.as_deref(), Some("department_approval"));
        assert_eq!(submitted.version, 2);
        let row = sqlx::query(
            "SELECT employee_id,engagement_id,submitted_at FROM finance_expenses \
             WHERE id='expense-stage-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(row.get::<String, _>("employee_id"), "person");
        assert_eq!(row.get::<String, _>("engagement_id"), "engagement");
        assert!(row.get::<Option<i64>, _>("submitted_at").is_some());

        let approved = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = reviewer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    decide_expense(
                        &actor,
                        "expense-stage-create-1",
                        &json!({"expectedVersion": 2, "reason": "Department approved"}),
                        &key("stage-approve-1"),
                        true,
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(approved.stage.as_deref(), Some("accounts_review"));
        assert_eq!(approved.version, 3);

        let accounting = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    save_expense_accounting(
                        &actor,
                        "expense-stage-create-1",
                        &json!({
                            "expectedVersion": 3,
                            "categoryAccountId": "account",
                            "tax": "200.00",
                            "entryDate": "2026-02-03",
                            "periodId": "period",
                            "note": "Net of VAT"
                        }),
                        &key("stage-accounting-1"),
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(accounting.version, 4);
        let row = sqlx::query(
            "SELECT amount_minor,tax_minor,accounting_json::text accounting_json \
             FROM finance_expenses WHERE id='expense-stage-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.get::<i64, _>("amount_minor"), 100_000);
        assert_eq!(row.get::<i64, _>("tax_minor"), 20_000);
        assert!(
            row.get::<String, _>("accounting_json")
                .contains("preparedBy")
        );

        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = reviewer();
                let mut command = PgExpenseCommand::new(connection, &actor);
                decide_expense(
                    &actor,
                    "expense-stage-create-1",
                    &json!({"expectedVersion": 4, "reason": "Accounts reviewed"}),
                    &key("stage-approve-2"),
                    true,
                    &mut command,
                    &mut FakeApproval {
                        next_stage: Some("accounts_approval".into()),
                        ..FakeApproval::default()
                    },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let final_state = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = reviewer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    decide_expense(
                        &actor,
                        "expense-stage-create-1",
                        &json!({"expectedVersion": 5, "reason": "Accounts approved"}),
                        &key("stage-approve-3"),
                        true,
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(final_state.status, "approved");
        assert_eq!(final_state.stage.as_deref(), Some("gl_posting"));
        assert_eq!(final_state.version, 6);
        let (status, stage, version, approved_by) =
            expense_state(&pool, "expense-stage-create-1").await;
        assert_eq!(status, "approved");
        assert_eq!(stage, "gl_posting");
        assert_eq!(version, 6);
        assert_eq!(approved_by.as_deref(), Some("reviewer"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM expense_decisions WHERE expense_id='expense-stage-create-1' \
                 AND action='approve'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            3
        );
    })
    .await;
}

#[tokio::test]
async fn cancel_writes_the_cancelled_claim_and_decision() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = owner();
                let mut command = PgExpenseCommand::new(connection, &actor);
                save_expense(
                    &actor,
                    None,
                    &draft_body(),
                    &key("cancel-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let cancelled = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    cancel_expense(
                        &actor,
                        "expense-cancel-create-1",
                        &json!({
                            "action": "cancel",
                            "expectedVersion": 1,
                            "reason": "No longer needed"
                        }),
                        &key("cancel-1"),
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(cancelled.status, "rejected");
        assert_eq!(cancelled.stage.as_deref(), Some("cancelled"));
        let row = sqlx::query(
            "SELECT cancelled_at,rejection_reason FROM finance_expenses \
             WHERE id='expense-cancel-create-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert!(row.get::<Option<i64>, _>("cancelled_at").is_some());
        assert_eq!(
            row.get::<Option<String>, _>("rejection_reason").as_deref(),
            Some("No longer needed")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM expense_decisions WHERE expense_id='expense-cancel-create-1' \
                 AND action='cancel'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn payment_preparation_confirmation_settles_the_claim() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        seed_posted_expense(&pool, "company", "expense-posted").await;
        let uow = PgUnitOfWork::new(pool.clone());
        let prepared = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = payer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    prepare_expense_payment(
                        &actor,
                        "expense-posted",
                        &json!({"expectedVersion": 3}),
                        &key("payment-prepare-1"),
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(prepared.draft_id, "expense-payment-payment-prepare-1");
        assert_eq!(count(&pool, "expense_payment_drafts").await, 1);
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = payer();
                let mut command = PgExpenseCommand::new(connection, &actor);
                save_expense_payment(
                    &actor,
                    "expense-posted",
                    "expense-payment-payment-prepare-1",
                    &json!({
                        "expectedVersion": 1,
                        "date": "2026-02-04",
                        "amount": "1200.00",
                        "reference": "TRX-1"
                    }),
                    &key("payment-save-1"),
                    &mut command,
                    &mut FakeApproval::default(),
                    &mut FakeEvidence::default(),
                    &mut FakeSettlement::default(),
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let confirmed = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = payer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    confirm_expense_payment(
                        &actor,
                        "expense-payment-payment-prepare-1",
                        &json!({
                            "expectedVersion": 2,
                            "date": "2026-02-04",
                            "amount": "1200.00",
                            "reference": "TRX-1"
                        }),
                        &key("payment-confirm-1"),
                        &mut command,
                        &mut FakeApproval::default(),
                        &mut FakeEvidence::default(),
                        &mut FakeSettlement::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(confirmed.payment_id.as_deref(), Some("payment-1"));
        assert_eq!(confirmed.outstanding_minor, Some(0));
        let (status, stage, version, _) = expense_state(&pool, "expense-posted").await;
        assert_eq!(status, "reimbursed");
        assert_eq!(stage, "completed");
        assert_eq!(version, 4);
        let draft = sqlx::query(
            "SELECT status,payment_id,amount_minor FROM expense_payment_drafts \
             WHERE id='expense-payment-payment-prepare-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        use sqlx::Row;
        assert_eq!(draft.get::<String, _>("status"), "confirmed");
        assert_eq!(draft.get::<String, _>("payment_id"), "payment-1");
        assert_eq!(draft.get::<i64, _>("amount_minor"), 120_000);
    })
    .await;
}

#[tokio::test]
async fn payment_cancellation_requires_the_native_preparation() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        seed_posted_expense(&pool, "company", "expense-posted").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = payer();
                let mut command = PgExpenseCommand::new(connection, &actor);
                prepare_expense_payment(
                    &actor,
                    "expense-posted",
                    &json!({"expectedVersion": 3}),
                    &key("payment-cancel-prepare-1"),
                    &mut command,
                    &mut FakeApproval::default(),
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let cancelled = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = payer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    cancel_expense_payment(
                        &actor,
                        "expense-posted",
                        "expense-payment-payment-cancel-prepare-1",
                        &json!({
                            "expectedVersion": 1,
                            "expenseVersion": 3,
                            "reason": "Bank details invalid",
                            "paymentNotExecuted": true
                        }),
                        &key("payment-cancel-1"),
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(cancelled.version, 2);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM expense_payment_drafts \
                 WHERE id='expense-payment-payment-cancel-prepare-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "cancelled"
        );
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = payer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    cancel_expense_payment(
                        &actor,
                        "expense-posted",
                        "expense-payment-payment-cancel-prepare-1",
                        &json!({
                            "expectedVersion": 2,
                            "expenseVersion": 3,
                            "reason": "Bank details invalid",
                            "paymentNotExecuted": true
                        }),
                        &key("payment-cancel-2"),
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "EXPENSE_PAYMENT_NOT_CANCELLABLE");
    })
    .await;
}

#[tokio::test]
async fn reversal_requires_no_settlement_and_writes_the_reversal_journal() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        seed_posted_expense(&pool, "company", "expense-posted").await;
        let uow = PgUnitOfWork::new(pool.clone());
        let partially_paid = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = reviewer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    reverse_expense(
                        &actor,
                        "expense-posted",
                        &json!({
                            "expectedVersion": 3,
                            "periodId": "period",
                            "entryDate": "2026-02-04",
                            "reason": "Duplicate claim"
                        }),
                        &key("reversal-1"),
                        &mut command,
                        &mut FakeSettlement {
                            payable: Some(ExpensePayable {
                                outstanding_minor: 60_000,
                                total_minor: 120_000,
                            }),
                            ..FakeSettlement::default()
                        },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(partially_paid).code, "EXPENSE_HAS_SETTLEMENT");
        let reversed = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = reviewer();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    reverse_expense(
                        &actor,
                        "expense-posted",
                        &json!({
                            "expectedVersion": 3,
                            "periodId": "period",
                            "entryDate": "2026-02-04",
                            "reason": "Duplicate claim"
                        }),
                        &key("reversal-2"),
                        &mut command,
                        &mut FakeSettlement::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(reversed.status, "rejected");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT reversal_journal_id FROM finance_expenses WHERE id='expense-posted'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "journal-reversal"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM expense_decisions WHERE expense_id='expense-posted' \
                 AND action='cancel'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn read_row_and_default_ports_fail_closed() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_org(&pool, "company").await;
        seed_worker(&pool, "company").await;
        seed_finance(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = owner();
                let mut command = PgExpenseCommand::new(connection, &actor);
                save_expense(
                    &actor,
                    None,
                    &draft_body(),
                    &key("read-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let row: Option<ExpenseRow> = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut reader = PgExpenseReader::new(connection);
                    reader
                        .expense_row(
                            &CompanyId::new("foreign").unwrap(),
                            "expense-read-create-1",
                            "owner",
                            true,
                            None,
                            None,
                        )
                        .await
                })
            })
            .await
            .unwrap();
        assert!(row.is_none());
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    submit_expense(
                        &actor,
                        "expense-read-create-1",
                        &json!({"action": "submit", "expectedVersion": 1}),
                        &key("read-submit-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut NoExpenseApprovalPort,
                        &mut FakeEvidence::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "EXPENSE_APPROVAL_PORT_UNAVAILABLE");
        let (status, stage, version, _) = expense_state(&pool, "expense-read-create-1").await;
        assert_eq!(status, "draft");
        assert_eq!(stage, "draft");
        assert_eq!(version, 1);
        let detail = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = owner();
                    let mut command = PgExpenseCommand::new(connection, &actor);
                    get_expense(
                        &actor,
                        "expense-read-create-1",
                        &mut command,
                        &mut FakeApproval::default(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(detail.expense.id, "expense-read-create-1");
        assert!(detail.permitted_actions.contains(&"save".to_owned()));
        assert!(detail.permitted_actions.contains(&"cancel".to_owned()));
    })
    .await;
}
