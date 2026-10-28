//! Disposable policy ports only; all origin rows come from the actual Workflow
//! ledger and canonical native Finance prepare/persist operations, never fake
//! receipt INSERTs. This does not stand in for service Identity composition.
#[path = "journal-facts.rs"]
pub mod preparation;
#[path = "review-facts.rs"]
#[allow(dead_code)] // The shared fixture also supports retry-specific suites.
pub mod review;
use erpx_server_finance::public::{journal_review::*, *};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, RecordId, UserId};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
use std::sync::Arc;
use tokio::sync::Mutex;
pub type Error = PgTransactionError<BusinessError>;

pub fn command(id: &str, key: &str) -> JournalReviewCommand {
    JournalReviewCommand {
        key: CommandKey::new(key).unwrap(),
        journal_id: RecordId::new(id).unwrap(),
        input: review::post(),
    }
}
pub fn convert(error: PgTransactionError<JournalError>) -> Error {
    match error {
        PgTransactionError::Business(e) => BusinessError::new(
            e.kind,
            e.code,
            "The actual fixture Finance operation failed.",
        )
        .into(),
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
    }
}
pub fn denied<T>(result: Result<T, Error>, expected: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, expected),
        _ => panic!("expected {expected}"),
    }
}

struct CreationAuthority {
    actor: FinanceActor,
    command: JournalReviewCommand,
    policy: review::Policy,
}
impl PgCommandAuthority for CreationAuthority {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_journal_review_sources(c).await?;
        self.policy
            .lock_sources(c, &self.actor, &self.command)
            .await
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        self.policy
            .authorize_current(c, &self.actor, &self.command, 0)
            .await
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        self.policy
            .require_source_access(
                c,
                a,
                &self.command,
                if disposition == CommandDisposition::Replay {
                    FinanceJournalAccessPurpose::ExactReplayHistory
                } else {
                    FinanceJournalAccessPurpose::SettleExisting
                },
                0,
            )
            .await
    }
}
struct Creation {
    actor: FinanceActor,
    draft: JournalDraft,
    plan: Option<JournalPreparation>,
}
impl PgOwnerCommandEffects for Creation {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let rules = embedded_currency_rules().unwrap();
        let ordering = FinanceCollation::english().unwrap();
        let mut authority = preparation::JournalFacts::default();
        let mut facts = PgJournalPreparation::new(
            c,
            &self.actor,
            &mut authority,
            &review::FixedClock,
            &rules,
            &ordering,
        )
        .map_err(convert)?;
        let plan = prepare_journal(
            &mut facts,
            &self.actor,
            &self.draft,
            &CommandKey::new(&request.key).unwrap(),
            JournalStatus::Draft,
        )
        .await
        .map_err(convert)?;
        let output = WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(plan.result())?,
            event_scope: None,
        };
        self.plan = Some(plan);
        Ok(output)
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            c,
        )
        .persist(self.plan.as_ref().unwrap())
        .await
        .map_err(convert)
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<(), Error> {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*)FROM finance_journal_lines WHERE journal_id=$1")
                .bind(&request.record_id)
                .fetch_one(c)
                .await?;
        assert_eq!(count, 2);
        Ok(())
    }
}

pub async fn create(
    pool: &DatabasePool,
    key: &str,
    opening: bool,
    secondary: bool,
) -> Result<(), Error> {
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),move|c|{
        let key=key.clone();Box::pin(async move{
            let mut actor=review::actor();actor.user_id=UserId::new("preparer").unwrap();
            let chart=if secondary{"book-chart"}else{"chart"};
            let draft=JournalDraft{chart_id:chart.into(),period_id:"period".into(),entry_date:"2026-01-01".into(),currency:"THB".into(),fx_rate:None,is_opening:opening,memo:"Canonical native source".into(),source_type:if opening{"opening_import"}else{"manual"}.into(),source_id:opening.then(||chart.into()),
            lines:serde_json::from_value(serde_json::json!([{"accountId":if secondary{"book-cash"}else{"cash"},"debit":"100.00","credit":"0"},{"accountId":if secondary{"book-sales"}else{"sales"},"debit":"0","credit":"100.00"}])).unwrap()};
            let input=CommandJson::from_serializable(&serde_json::json!({"chartId":chart,"periodId":"period","entryDate":"2026-01-01","currency":"THB","memo":draft.memo,"sourceType":draft.source_type,"sourceId":draft.source_id,"isOpening":opening,"lines":[{"accountId":draft.lines[0].account_id,"debit":"100.00","credit":"0"},{"accountId":draft.lines[1].account_id,"debit":"0","credit":"100.00"}]}))?;
            let id=format!("journal-{key}");
            let request=WorkflowRequest{key:key.clone(),operation:"finance.journal.create".into(),record_id:id.clone(),input};
            let mut policy=CreationAuthority{actor:actor.clone(),command:command(&id,&key),policy:review::Policy::default()};
            let mut effects=Creation{actor:actor.clone(),draft,plan:None};let mut ids=review::Ids;
            let mut ledger=PgCommandLedger::new(actor.company_id,c,&mut policy,&review::FixedClock,&mut ids);
            assert!(!ledger.execute(&request,&mut effects).await?.replayed);Ok(())
        })
    }).await
}

pub async fn inspect(pool: &DatabasePool, id: &str) -> Result<CurrentJournalReviewScope, Error> {
    let id = id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let id = id.clone();
            Box::pin(async move {
                lock_journal_review_sources(c).await?;
                inspect_journal_review_source(c, &review::actor(), &command(&id, "scope-command"))
                    .await
            })
        })
        .await
}

pub fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIXTURE_ROLLBACK",
        "Rollback the controlled source mutation.",
    )
    .into()
}

pub async fn run(
    pool: &DatabasePool,
    policy: Arc<Mutex<review::Policy>>,
    journal: &str,
    key: &str,
    input: JournalReviewInput,
) -> Result<JournalReviewResult, PgTransactionError<JournalError>> {
    let journal = journal.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let policy = policy.clone();
            let journal = journal.clone();
            let key = key.clone();
            let input = input.clone();
            Box::pin(async move { review::invoke(c, policy.as_ref(), &journal, &key, input).await })
        })
        .await
}
pub async fn counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(pool.raw()).await.unwrap()
}

pub async fn run_with_source(
    pool: &DatabasePool,
    journal: &str,
    key: &str,
    input: JournalReviewInput,
) -> Result<(), Error> {
    let journal = journal.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let journal = journal.clone();
            let key = key.clone();
            let input = input.clone();
            Box::pin(async move {
                lock_journal_review_sources(c).await?;
                let mut command = command(&journal, &key);
                command.input = input.clone();
                let scope = inspect_journal_review_source(c, &review::actor(), &command).await?;
                review::invoke(
                    c,
                    &Mutex::new(review::Policy::default()),
                    &journal,
                    &key,
                    input,
                )
                .await
                .map_err(convert)?;
                scope.require_current(c).await
            })
        })
        .await
}
