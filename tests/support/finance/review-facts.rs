//! Disposable fixture authority only. This is not a production Identity,
//! Commercial, release or foreign-source assembly.
use erpx_server_finance::public::{FinanceActor, JournalError, journal_review::*};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis, UserId,
};
use erpx_server_workflow::public::commands::{CommandActor, CommandField, WorkAuthority};
use sqlx::{PgConnection, Row};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{Mutex, Notify};
pub type Error = PgTransactionError<JournalError>;
type WorkflowError = PgTransactionError<BusinessError>;
pub struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_800_000_000_000)
    }
}
pub struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
pub fn actor() -> FinanceActor {
    FinanceActor {
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        user_id: UserId::new("reviewer").unwrap(),
        assignment_id: AssignmentId::new("assignment").unwrap(),
    }
}
pub fn post() -> JournalReviewInput {
    JournalReviewInput::Post {
        expected_version: 1,
    }
}
pub fn reverse() -> JournalReviewInput {
    JournalReviewInput::Reverse {
        period_id: "period".into(),
        entry_date: "2026-01-10".into(),
        memo: None,
    }
}
pub async fn invoke(
    c: &mut PgConnection,
    policy: &Mutex<Policy>,
    journal: &str,
    key: &str,
    input: JournalReviewInput,
) -> Result<JournalReviewResult, Error> {
    let actor = actor();
    let mut ids = Ids;
    let mut ports = PgJournalReview::new(
        actor.holding_id.clone(),
        actor.company_id.clone(),
        c,
        policy,
        &FixedClock,
        &mut ids,
    )?;
    review_journal(
        &mut ports,
        &actor,
        &JournalReviewCommand {
            key: erpx_server_shared::public::CommandKey::new(key).unwrap(),
            journal_id: RecordId::new(journal).unwrap(),
            input,
        },
    )
    .await
}
pub struct RetryGate {
    pub ready: Notify,
    pub release: Notify,
    pub attempts: AtomicUsize,
}
impl RetryGate {
    pub fn new() -> Self {
        Self {
            ready: Notify::new(),
            release: Notify::new(),
            attempts: AtomicUsize::new(0),
        }
    }
}
#[derive(Default)]
pub struct Policy {
    pub mutate_after_event: bool,
    pub revoke_after_event: bool,
    pub gate: Option<Arc<RetryGate>>,
    pub calls: Vec<FinanceJournalAccessPurpose>,
}
fn denied() -> WorkflowError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FINANCE_SOURCE_ACCESS_CHANGED",
        "The disposable current Finance source authority denied this request.",
    )
    .into()
}
impl PgFinanceJournalReviewPolicy for Policy {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &FinanceActor,
        _: &JournalReviewCommand,
    ) -> Result<(), WorkflowError> {
        sqlx::query("LOCK TABLE finance_review_test_authority IN SHARE MODE")
            .execute(c)
            .await?;
        Ok(())
    }
    async fn authorize_current(
        &mut self,
        c: &mut PgConnection,
        expected: &FinanceActor,
        command: &JournalReviewCommand,
        _: i64,
    ) -> Result<CommandActor, WorkflowError> {
        let row=sqlx::query("SELECT active,revision,can_write FROM finance_review_test_authority WHERE id='current'").fetch_one(&mut *c).await?;
        if self.revoke_after_event
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM finance_financial_events WHERE journal_id=$1)",
            )
            .bind(command.journal_id.as_str())
            .fetch_one(&mut *c)
            .await?
        {
            return Err(denied());
        }
        if !row.try_get::<bool, _>("active")? || !row.try_get::<bool, _>("can_write")? {
            return Err(denied());
        }
        if expected.company_id.as_str() != "company" || expected.holding_id.as_str() != "holding" {
            return Err(denied());
        }
        Ok(CommandActor {
            user_id: expected.user_id.clone(),
            company_id: expected.company_id.clone(),
            assignment_id: expected.assignment_id.clone(),
            role: "finance_approver".into(),
            branch_id: CommandField::Null,
            department_id: CommandField::Null,
            authority: CommandField::Value(WorkAuthority {
                session_id: "finance-session".into(),
                revision: row.try_get("revision")?,
                assignment_version: 1,
            }),
            iam: CommandField::Omitted,
            assignment_scope_kind: "holding".into(),
            assignment_scope_node_id: None,
        })
    }
    async fn reviewer(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        journal: Option<&ReviewJournal>,
        _: i64,
    ) -> Result<JournalReviewerFacts, WorkflowError> {
        let row=sqlx::query("SELECT can_read,can_approve,same_person FROM finance_review_test_authority WHERE id='current'").fetch_one(c).await?;
        Ok(JournalReviewerFacts {
            can_read: row.try_get("can_read")?,
            can_approve: row.try_get("can_approve")?,
            same_person: journal.is_some_and(|j| j.created_by == "reviewer")
                || row.try_get::<bool, _>("same_person")?,
        })
    }
    async fn ready(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: i64,
    ) -> Result<(), WorkflowError> {
        if let Some(gate) = &self.gate
            && gate.attempts.fetch_add(1, Ordering::SeqCst) == 0
        {
            let _: i64 = sqlx::query_scalar("SELECT value FROM finance_retry_probe WHERE id=1")
                .fetch_one(c)
                .await?;
            gate.ready.notify_one();
            gate.release.notified().await;
        }
        Ok(())
    }
    async fn require_source_access(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        command: &JournalReviewCommand,
        purpose: FinanceJournalAccessPurpose,
        _: i64,
    ) -> Result<(), WorkflowError> {
        self.calls.push(purpose);
        let row=sqlx::query("SELECT history,settle_existing,source_current FROM finance_review_test_authority WHERE id='current'").fetch_one(&mut *c).await?;
        let allowed = match purpose {
            FinanceJournalAccessPurpose::BaselineRead
            | FinanceJournalAccessPurpose::ExactReplayHistory => {
                row.try_get::<bool, _>("history")?
            }
            FinanceJournalAccessPurpose::SettleExisting => {
                row.try_get::<bool, _>("settle_existing")?
            }
        };
        if !allowed || !row.try_get::<bool, _>("source_current")? {
            return Err(denied());
        }
        if self.mutate_after_event
            && purpose == FinanceJournalAccessPurpose::SettleExisting
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM finance_financial_events WHERE journal_id=$1)",
            )
            .bind(command.journal_id.as_str())
            .fetch_one(&mut *c)
            .await?
        {
            sqlx::query(
                // The DB already forbids memo changes on posted journals.
                // Permitted metadata still must match the final owned image.
                "UPDATE finance_journals SET updated_at=updated_at+1 WHERE id=$1",
            )
            .bind(command.journal_id.as_str())
            .execute(c)
            .await?;
        }
        Ok(())
    }
}
pub async fn setup(pool: &DatabasePool, books: bool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE finance_review_test_authority(id text PRIMARY KEY,active boolean NOT NULL DEFAULT true,revision bigint NOT NULL DEFAULT 1,can_write boolean NOT NULL DEFAULT true,can_read boolean NOT NULL DEFAULT true,can_approve boolean NOT NULL DEFAULT true,same_person boolean NOT NULL DEFAULT false,history boolean NOT NULL DEFAULT true,settle_existing boolean NOT NULL DEFAULT true,source_current boolean NOT NULL DEFAULT true);INSERT INTO finance_review_test_authority(id)VALUES('current');
    INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('chart','company','Primary','THB','maker',0,0),('book-chart','company','Secondary','THB','maker',0,0);
    INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at)VALUES('period','company','January','2026-01-01','2026-01-31',0);
    INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at)VALUES('cash','company','chart','100','Cash','asset',0,0),('sales','company','chart','400','Sales','revenue',0,0),('book-cash','company','book-chart','100','Cash','asset',0,0),('book-sales','company','book-chart','400','Sales','revenue',0,0);
    INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('old-rate','company','THB','2026-01-01',2,'maker',1)").execute(pool.raw()).await.unwrap();
    if books {
        sqlx::raw_sql("INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at)VALUES('rule','company','chart','book-chart','2026-01-01','maker','Reviewed book','[]','synthetic-opening-hash','period',0);INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)VALUES('rule','cash','book-cash'),('rule','sales','book-sales');UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer',reviewed_at=1 WHERE id='rule'").execute(pool.raw()).await.unwrap();
    }
    seed_journal(pool, "journal", false).await;
}
pub async fn seed_journal(pool: &DatabasePool, id: &str, opening: bool) {
    sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,memo,is_opening,status,created_by,created_at,updated_at)VALUES($1,'company','chart','period',$1,'2026-01-01','THB',1000000,$2,'Original journal',$3,'draft','preparer',0,0)").bind(id).bind(if opening{"opening_import"}else{"manual"}).bind(i64::from(opening)).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal)VALUES($1||'-0',$1,'cash',10000,0,10000,0,0),($1||'-1',$1,'sales',0,10000,0,10000,1)").bind(id).execute(pool.raw()).await.unwrap();
}
