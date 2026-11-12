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
use erpx_server_shared::public::{BusinessError, CommandKey, UserId};
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
fn dimensioned_draft(amount: &str, dimensions: serde_json::Value) -> JournalDraft {
    JournalDraft{chart_id:"chart".into(),period_id:"period".into(),entry_date:"2026-01-01".into(),currency:"THB".into(),fx_rate:None,is_opening:false,memo:"Reviewed correction of the draft amount".into(),source_type:"manual".into(),source_id:None,lines:serde_json::from_value(serde_json::json!([
        {"accountId":"cash","debit":amount,"credit":"0","dimensions":dimensions},
        {"accountId":"sales","debit":"0","credit":amount,"dimensions":dimensions}
    ])).unwrap()}
}
async fn edit(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
    expected: i64,
    fail_retain: bool,
) -> Result<(), Error> {
    edit_as(
        pool,
        key,
        expected,
        fail_retain,
        "reviewer",
        draft("Edited writer draft"),
        "Writer revision",
    )
    .await
}
#[expect(
    clippy::too_many_arguments,
    reason = "The retained edit fixture varies actor, draft and evidence."
)]
async fn edit_as(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
    expected: i64,
    fail_retain: bool,
    user_id: &str,
    draft: JournalDraft,
    reason: &str,
) -> Result<(), Error> {
    let key = key.to_owned();
    let user_id = user_id.to_owned();
    let reason = reason.to_owned();
    PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),move|c|{let key=key.clone();let user_id=user_id.clone();let reason=reason.clone();let draft=draft.clone();Box::pin(async move{
 let mut actor=source::review::actor();actor.user_id=UserId::new(user_id).unwrap();let id="journal-writer-root";let input=CommandJson::from_serializable(&serde_json::json!({"expectedVersion":expected,"reason":reason,"memo":draft.memo}))?;
 let request=WorkflowRequest{key:key.clone(),operation:"finance.manual-draft.edit".into(),record_id:id.into(),input};let mut auth=Authority{actor:actor.clone(),command:source::command(id,&key),policy:source::review::Policy::default()};let mut effects=Effects{actor:actor.clone(),draft,id:id.into(),expected,reason,captured:None,plan:None,applied:None,fail_retain};let mut ids=source::review::Ids;PgCommandLedger::new(actor.company_id,c,&mut auth,&source::review::FixedClock,&mut ids).execute(&request,&mut effects).await?;Ok(())
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

#[tokio::test]
async fn dimensioned_edit_retains_both_versions_and_posts_through_independent_native_review() {
    fixture::with_database(2,|pool|async move{
        source::review::setup(&pool,false).await;
        source::create(&pool,"writer-root",false,false).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,created_by,created_at)VALUES('dim-project','company','project','Project',1,'maker',0);\
             INSERT INTO finance_dimension_values(id,dimension_id,company_id,code,name,created_at)VALUES('dim-project-p1','dim-project','company','P1','Project one',0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        edit_as(
            &pool,
            "dimensioned-edit",
            1,
            false,
            "editor",
            dimensioned_draft("80.01", serde_json::json!({"project":"P1"})),
            "Corrected the draft amount and selected the required project before approval",
        )
        .await
        .unwrap();
        let editions:(Vec<i64>,Vec<serde_json::Value>)=sqlx::query_as("SELECT array_agg(journal_version ORDER BY journal_version),array_agg(snapshot_json::jsonb ORDER BY journal_version) FROM finance_manual_draft_editions WHERE journal_id='journal-writer-root'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(editions.0, vec![1, 2]);
        assert_eq!(editions.1[0]["lines"][0]["debit_minor"], 10_000);
        assert_eq!(editions.1[0]["lines"][0]["dimension_json"], "{}");
        assert_eq!(editions.1[1]["lines"][0]["debit_minor"], 8_001);
        assert_eq!(editions.1[1]["lines"][0]["dimension_json"], "{\"project\":\"P1\"}");
        source::run(&pool,std::sync::Arc::new(tokio::sync::Mutex::new(source::review::Policy::default())),"journal-writer-root","post-edited-native",JournalReviewInput::Post{expected_version:2}).await.unwrap();
        let posted:(String,i64)=sqlx::query_as("SELECT status,version FROM finance_journals WHERE id='journal-writer-root'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(posted,("posted".into(),3));
        let (debit,dimension):(i64,String)=sqlx::query_as("SELECT debit_minor,dimension_json FROM finance_journal_lines WHERE journal_id='journal-writer-root' AND ordinal=0").fetch_one(pool.raw()).await.unwrap();
        assert_eq!((debit,dimension),(8_001,"{\"project\":\"P1\"}".into()));
        let money:String=sqlx::query_scalar("SELECT amount_units::text FROM money_transaction_snapshots WHERE source_id='journal-writer-root'").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(money,"8001");
        source::denied(edit_as(&pool,"dimensioned-stale",3,false,"editor",dimensioned_draft("81.01", serde_json::json!({"project":"P1"})),"Stale correction after posting").await,"MANUAL_DRAFT_CHANGED");
        assert_eq!(source::counts(&pool,"dimensioned-stale").await,(0,0,0));
    }).await;
}
