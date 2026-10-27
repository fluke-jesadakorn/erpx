//! Real migrated Holding PostgreSQL, with explicitly synthetic owner/IAM ports.
//! Actual Legal/Identity composition has its own integration suite.
#[path = "../../support/workflow/command-actor.rs"]
mod actor_fixture;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::*;
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
struct Time;
impl Clock for Time {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(100)
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn actor() -> CommandActor {
    actor_fixture::parse_frozen_actor(
        r#"{"userId":"user","assignmentId":"assignment","role":"admin","companyId":"company","branchId":null,"departmentId":null,"authority":{"sessionId":"session","revision":1,"assignmentVersion":1}}"#,
    )
}
struct Authority {
    deny: bool,
}
impl PgCommandAuthority for Authority {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowCommand,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowCommand,
    ) -> Result<CommandActor, Error> {
        if self.deny {
            return Err(BusinessError::invalid(
                "TEST_CURRENT_AUTHORITY_DENIED",
                "Synthetic current permission denied.",
            )
            .into());
        }
        Ok(actor())
    }
    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowCommand,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        Ok(())
    }
}
struct Effects {
    fail_retain: bool,
}
impl PgOwnerCommandEffects for Effects {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowCommand,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        command: &WorkflowCommand,
    ) -> Result<(), Error> {
        // The canonical receipt must already exist on this exact transaction.
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1)")
                .bind(&command.key)
                .fetch_one(&mut *c)
                .await?;
        assert!(exists);
        sqlx::query("INSERT INTO workflow_test_effects(command_id) VALUES($1)")
            .bind(&command.key)
            .execute(c)
            .await?;
        Ok(())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowCommand,
    ) -> Result<(), Error> {
        if self.fail_retain {
            return Err(BusinessError::invalid(
                "TEST_FINAL_SOURCE_CHANGED",
                "Synthetic final source changed.",
            )
            .into());
        }
        Ok(())
    }
}
fn command(input: &str) -> WorkflowCommand {
    WorkflowCommand {
        key: "workflow:test:command".into(),
        operation: "workflow.synthetic.update".into(),
        record_id: "record".into(),
        version: 2,
        input: CommandJson::parse(input).unwrap(),
        result: CommandJson::parse(r#"{"id":"record","state":"updated"}"#).unwrap(),
        event_scope: None,
    }
}
async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE workflow_test_effects(command_id text PRIMARY KEY)")
        .execute(pool.raw())
        .await
        .unwrap();
}
async fn execute(
    pool: &DatabasePool,
    input: &'static str,
    deny: bool,
    fail_retain: bool,
) -> Result<CommandOutcome, Error> {
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            Box::pin(async move {
                PgCommandLedger::new(
                    CompanyId::new("company").unwrap(),
                    c,
                    &mut Authority { deny },
                    &Time,
                    &mut Ids,
                )
                .execute(&command(input), &mut Effects { fail_retain })
                .await
            })
        })
        .await
}
async fn counts(pool: &DatabasePool) -> (i64, i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM workflow_test_effects),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)").fetch_one(pool.raw()).await.unwrap()
}
#[tokio::test]
async fn receipt_effect_audit_and_outbox_commit_once_and_replay_requires_current_authority() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let first = execute(&pool, r#"{"amount":1}"#, false, false)
            .await
            .unwrap();
        assert!(!first.replayed);
        let replay = execute(&pool, r#"{"amount":1}"#, false, false)
            .await
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(first.result, replay.result);
        assert!(matches!(
            execute(&pool, r#"{"amount":2}"#, false, false).await,
            Err(Error::Business(_))
        ));
        assert!(matches!(
            execute(&pool, r#"{"amount":1}"#, true, false).await,
            Err(Error::Business(_))
        ));
        assert_eq!(counts(&pool).await, (1, 1, 1, 1));
        let payload: String = sqlx::query_scalar("SELECT payload_json FROM outbox_events")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "updated");
    })
    .await;
}
#[tokio::test]
async fn failed_final_source_rolls_back_receipt_owner_effect_audit_and_outbox() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        assert!(matches!(
            execute(&pool, "{}", false, true).await,
            Err(Error::Business(_))
        ));
        assert_eq!(counts(&pool).await, (0, 0, 0, 0));
        assert!(!execute(&pool, "{}", false, false).await.unwrap().replayed);
        assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    })
    .await;
}
#[tokio::test]
async fn unowned_or_read_committed_transactions_are_refused_without_effects() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let no_transaction = PgCommandLedger::new(
            CompanyId::new("company").unwrap(),
            &mut c,
            &mut Authority { deny: false },
            &Time,
            &mut Ids,
        )
        .execute(&command("{}"), &mut Effects { fail_retain: false })
        .await;
        assert!(matches!(no_transaction, Err(Error::Persistence(_))));
        sqlx::raw_sql("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *c)
            .await
            .unwrap();
        let wrong_isolation = PgCommandLedger::new(
            CompanyId::new("company").unwrap(),
            &mut c,
            &mut Authority { deny: false },
            &Time,
            &mut Ids,
        )
        .execute(&command("{}"), &mut Effects { fail_retain: false })
        .await;
        assert!(matches!(wrong_isolation, Err(Error::Persistence(_))));
        sqlx::raw_sql("ROLLBACK").execute(&mut *c).await.unwrap();
        assert_eq!(counts(&pool).await, (0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn concurrent_old_snapshot_gets_real_40001_then_whole_unit_replays_without_duplicate_effects()
{
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut first = pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        let first_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *first).await.unwrap();
        assert!(!PgCommandLedger::new(CompanyId::new("company").unwrap(), &mut first, &mut Authority{deny:false}, &Time, &mut Ids).execute(&command("{}"), &mut Effects{fail_retain:false}).await.unwrap().replayed);
        let second_pool=pool.clone();
        let (started,waiting)=tokio::sync::oneshot::channel();
        let second=tokio::spawn(async move {
            let mut tx=second_pool.raw().begin().await.unwrap();
            sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            // Establish a genuinely older snapshot while the first receipt is uncommitted.
            let _:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts").fetch_one(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
            started.send(pid).unwrap();
            let result=PgCommandLedger::new(CompanyId::new("company").unwrap(), &mut tx, &mut Authority{deny:false}, &Time, &mut Ids).execute(&command("{}"), &mut Effects{fail_retain:false}).await;
            tx.rollback().await.unwrap();
            result
        });
        let second_pid=waiting.await.unwrap();
        let mut observed=false;
        for _ in 0..200 {
            let blocking:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(first_pid).bind(second_pid).fetch_one(pool.raw()).await.unwrap();
            if blocking { observed=true;break; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(observed,"actual concurrent receipt advisory lock must be observed");
        first.commit().await.unwrap();
        assert!(matches!(second.await.unwrap(),Err(Error::Persistence(ref e)) if e.sql_state()==Some("40001")));
        assert!(execute(&pool,"{}",false,false).await.unwrap().replayed);
        assert_eq!(counts(&pool).await,(1,1,1,1));
    }).await;
}

struct ReplayAuthority {
    actor: CommandActor,
    guard: Option<String>,
}
impl PgCommandAuthority for ReplayAuthority {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowCommand,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowCommand,
    ) -> Result<CommandActor, Error> {
        Ok(self.actor.clone())
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowCommand,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        assert_eq!(disposition, CommandDisposition::Replay);
        assert_eq!(
            current, &self.actor,
            "old saved receipt authority never replaces current authority"
        );
        match self.guard.as_deref() {
            Some("23514") => {
                return Err(BusinessError::invalid(
                    "WORK_CONTEXT_CHANGED",
                    "Synthetic current context changed.",
                )
                .into());
            }
            Some("23503") => {
                sqlx::raw_sql("DO $$BEGIN RAISE EXCEPTION 'synthetic test guard' USING ERRCODE='23503'; END$$").execute(c).await?;
            }
            None => {}
            Some(_) => panic!("unexpected frozen synthetic guard"),
        }
        Ok(())
    }
}
#[tokio::test]
async fn all_frozen_node_receipt_replays_preserve_results_conflicts_and_current_guard_failures() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let corpus:serde_json::Value=serde_json::from_str(include_str!("../../../fixtures/workflow/rust-commands/conformance.v1.json")).unwrap();
        let cases=corpus["replays"].as_array().unwrap();assert_eq!(cases.len(),24);
        for (index,case) in cases.iter().enumerate() {
            let key=format!("workflow:legacy:{index}");let receipt=&case["receipt"];
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES($1,$2,$3,$4,$5,$6,0,$7)")
                .bind(&key).bind(receipt["operation"].as_str().unwrap()).bind(receipt["actor_id"].as_str().unwrap()).bind(receipt["request_hash"].as_str().unwrap()).bind(receipt["record_id"].as_str().unwrap()).bind(receipt["result_json"].as_str().unwrap()).bind(receipt["authority_json"].as_str()).execute(pool.raw()).await.unwrap();
            let mut tx=pool.raw().begin().await.unwrap();
            sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let actor=actor_fixture::parse_frozen_actor(case["actorJson"].as_str().unwrap());
            let company=actor.company_id.clone();
            let mut policy=ReplayAuthority{actor,guard:case["guardState"].as_str().map(String::from)};
            let command=WorkflowCommand {key,operation:case["operation"].as_str().unwrap().into(),record_id:case["recordId"].as_str().unwrap().into(),version:1,input:CommandJson::parse(case["inputJson"].as_str().unwrap()).unwrap(),result:CommandJson::parse("{}").unwrap(),event_scope:None};
            let actual=PgCommandLedger::new(company,&mut tx,&mut policy,&Time,&mut Ids).execute(&command,&mut Effects{fail_retain:true}).await;
            if case["expectedError"].is_null() {
                let actual=actual.unwrap_or_else(|e|panic!("{}: {e:?}",case["id"]));
                assert!(actual.replayed);assert_eq!(actual.result.as_str(),case["expectedResultJson"].as_str().unwrap(),"{}",case["id"]);
            } else {
                let expected=case["expectedError"]["code"].as_str().unwrap_or("GUEST_ACCOUNT_REFERENCE_CONFLICT");
                let actual=match actual {Err(Error::Business(e))=>e.code.to_owned(),Err(Error::Persistence(e))=>e.sql_state().unwrap_or(e.code).to_owned(),Ok(_)=>panic!("{} unexpectedly replayed",case["id"])};
                assert_eq!(actual,expected,"{}",case["id"]);
            }
            tx.rollback().await.unwrap();
        }
        assert_eq!(counts(&pool).await,(24,0,0,0),"historical replay must not run owner/audit/outbox effects");
    }).await;
}
