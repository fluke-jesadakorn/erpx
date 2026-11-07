#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../support/finance/journal-source-facts.rs"]
#[expect(
    dead_code,
    reason = "Shared source fixture also supports journal review cases."
)]
mod source;

use erpx_server_finance::public::{
    journal_review::*, manual_draft_edit::*, manual_draft_provenance::*, *,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, CommandKey};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;
struct Authority {
    actor: FinanceActor,
    command: JournalReviewCommand,
    policy: source::review::Policy,
}
impl PgCommandAuthority for Authority {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_journal_review_sources(c).await?;
        lock_manual_draft_provenance(c, true).await?;
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
        d: CommandDisposition,
    ) -> Result<(), Error> {
        self.policy
            .require_source_access(
                c,
                a,
                &self.command,
                if d == CommandDisposition::Replay {
                    FinanceJournalAccessPurpose::ExactReplayHistory
                } else {
                    FinanceJournalAccessPurpose::SettleExisting
                },
                0,
            )
            .await
    }
}
struct Effects {
    actor: FinanceActor,
    draft: JournalDraft,
    id: String,
    expected: i64,
    reason: String,
    captured: Option<CapturedManualDraftEdit>,
    plan: Option<JournalPreparation>,
    applied: Option<AppliedManualDraftEdit>,
    fail_retain: bool,
}
impl PgOwnerCommandEffects for Effects {
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
        r: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.captured = Some(
            capture_manual_draft_edit(
                c,
                &self.actor,
                &self.id,
                &self.draft,
                self.expected,
                &self.reason,
            )
            .await?,
        );
        let rules = embedded_currency_rules().unwrap();
        let collation = FinanceCollation::english().unwrap();
        let mut authority = source::preparation::JournalFacts::default();
        let mut facts = PgJournalPreparation::new(
            c,
            &self.actor,
            &mut authority,
            &source::review::FixedClock,
            &rules,
            &collation,
        )
        .map_err(source::convert)?;
        self.plan = Some(
            prepare_journal(
                &mut facts,
                &self.actor,
                &self.draft,
                &CommandKey::new(&r.key).unwrap(),
                JournalStatus::Draft,
            )
            .await
            .map_err(source::convert)?,
        );
        Ok(WorkflowOutput {
            version: self.expected + 1,
            result: CommandJson::from_serializable(&ManualJournalEditResult {
                id: self.id.clone(),
                status: JournalStatus::Draft,
                version: self.expected + 1,
            })?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.applied = Some(
            apply_manual_draft_edit(
                c,
                self.captured.take().unwrap(),
                self.plan.as_ref().unwrap(),
            )
            .await?,
        );
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
    ) -> Result<(), Error> {
        let proof = retain_manual_draft_edit(
            c,
            self.applied.as_ref().unwrap(),
            &CommandKey::new(&r.key).unwrap(),
        )
        .await?;
        proof.require_current(c).await?;
        if self.fail_retain {
            return Err(source::changed());
        }
        Ok(())
    }
}
fn draft(memo: &str) -> JournalDraft {
    JournalDraft{chart_id:"chart".into(),period_id:"period".into(),entry_date:"2026-01-01".into(),currency:"THB".into(),fx_rate:None,is_opening:false,memo:memo.into(),source_type:"manual".into(),source_id:None,lines:serde_json::from_value(serde_json::json!([{"accountId":"cash","debit":"100.00","credit":"0"},{"accountId":"sales","debit":"0","credit":"100.00"}])).unwrap()}
}
async fn edit(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
    expected: i64,
    fail_retain: bool,
) -> Result<(), Error> {
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),move|c|{let key=key.clone();Box::pin(async move{
 let actor=source::review::actor();let id="journal-writer-root";let input=CommandJson::from_serializable(&serde_json::json!({"expectedVersion":expected,"reason":"Writer revision","memo":"Edited writer draft"}))?;
 let request=WorkflowRequest{key:key.clone(),operation:"finance.manual-draft.edit".into(),record_id:id.into(),input};let mut auth=Authority{actor:actor.clone(),command:source::command(id,&key),policy:source::review::Policy::default()};let mut effects=Effects{actor:actor.clone(),draft:draft("Edited writer draft"),id:id.into(),expected,reason:"Writer revision".into(),captured:None,plan:None,applied:None,fail_retain};let mut ids=source::review::Ids;PgCommandLedger::new(actor.company_id,c,&mut auth,&source::review::FixedClock,&mut ids).execute(&request,&mut effects).await?;Ok(())
 })}).await
}
#[tokio::test]
async fn actual_edit_retains_original_edition_and_rolls_back_failed_final_retain() {
    fixture::with_database(2,|pool|async move{
 source::review::setup(&pool,false).await;source::create(&pool,"writer-root",false,false).await.unwrap();
 let before:(serde_json::Value,serde_json::Value,String)=sqlx::query_as("SELECT (SELECT to_jsonb(j) FROM finance_journals j WHERE id='journal-writer-root'),(SELECT jsonb_agg(to_jsonb(l) ORDER BY ordinal,id) FROM finance_journal_lines l WHERE journal_id='journal-writer-root'),(SELECT request_hash FROM command_receipts WHERE command_id='writer-root')").fetch_one(pool.raw()).await.unwrap();
 assert_ne!(before.2,"a".repeat(64));
 sqlx::raw_sql("CREATE FUNCTION fixture_change_create_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='writer-origin-drift' THEN UPDATE command_receipts SET request_hash=repeat('a',64) WHERE command_id='writer-root'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_change_create_receipt AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION fixture_change_create_receipt()").execute(pool.raw()).await.unwrap();
 source::denied(edit(&pool,"writer-origin-drift",1,false).await,"COMMAND_ORIGIN_CHANGED");
 let after:(serde_json::Value,serde_json::Value,String)=sqlx::query_as("SELECT (SELECT to_jsonb(j) FROM finance_journals j WHERE id='journal-writer-root'),(SELECT jsonb_agg(to_jsonb(l) ORDER BY ordinal,id) FROM finance_journal_lines l WHERE journal_id='journal-writer-root'),(SELECT request_hash FROM command_receipts WHERE command_id='writer-root')").fetch_one(pool.raw()).await.unwrap();assert_eq!(before,after);
 let retained:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_manual_draft_editions WHERE journal_id='journal-writer-root'),(SELECT count(*) FROM finance_manual_draft_receipt_links WHERE journal_id='journal-writer-root')").fetch_one(pool.raw()).await.unwrap();assert_eq!(retained,(0,0));assert_eq!(source::counts(&pool,"writer-origin-drift").await,(0,0,0));
 sqlx::raw_sql("DROP TRIGGER fixture_change_create_receipt ON audit_events;DROP FUNCTION fixture_change_create_receipt()").execute(pool.raw()).await.unwrap();
 source::denied(edit(&pool,"writer-failed",1,true).await,"FIXTURE_ROLLBACK");
 let state:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM finance_journals WHERE id='journal-writer-root'),(SELECT count(*) FROM finance_manual_draft_editions WHERE journal_id='journal-writer-root'),(SELECT count(*) FROM command_receipts WHERE command_id='writer-failed')").fetch_one(pool.raw()).await.unwrap();assert_eq!(state,(1,0,0));assert_eq!(source::counts(&pool,"writer-failed").await,(0,0,0));
 edit(&pool,"writer-edit",1,false).await.unwrap();let row:(i64,String,i64,i64)=sqlx::query_as("SELECT j.version,j.created_by,(SELECT count(*) FROM finance_manual_draft_editions e WHERE e.journal_id=j.id),(SELECT count(*) FROM finance_manual_draft_receipt_links l WHERE l.journal_id=j.id) FROM finance_journals j WHERE j.id='journal-writer-root'").fetch_one(pool.raw()).await.unwrap();assert_eq!(row,(2,"reviewer".into(),2,2));assert_eq!(source::counts(&pool,"writer-edit").await,(1,1,1));source::denied(edit(&pool,"writer-stale",1,false).await,"MANUAL_DRAFT_CHANGED");assert_eq!(source::counts(&pool,"writer-stale").await,(0,0,0));
 let image:(serde_json::Value,serde_json::Value)=sqlx::query_as("SELECT (SELECT snapshot_json::jsonb FROM finance_manual_draft_editions WHERE journal_id='journal-writer-root' AND journal_version=1),(SELECT snapshot_json::jsonb FROM finance_manual_draft_editions WHERE journal_id='journal-writer-root' AND journal_version=2)").fetch_one(pool.raw()).await.unwrap();assert_eq!(image.0["journal"]["created_by"],"preparer");assert_eq!(image.1["journal"]["created_by"],"reviewer");assert_eq!(image.1["journal"]["version"],2);assert_eq!(image.1["lines"].as_array().unwrap().len(),2);
 assert!(sqlx::query("UPDATE finance_manual_draft_receipt_links SET actor_id='tampered' WHERE journal_id='journal-writer-root'").execute(pool.raw()).await.is_err());
}).await;
}
