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
use sqlx::{Connection, PgConnection};
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
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
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
        _: &WorkflowRequest,
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
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        Ok(WorkflowOutput {
            version: 2,
            result: CommandJson::parse(r#"{"id":"record","state":"updated"}"#).unwrap(),
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        command: &WorkflowRequest,
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
        _: &WorkflowRequest,
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
fn command(input: &str) -> WorkflowRequest {
    WorkflowRequest {
        key: "workflow:test:command".into(),
        operation: "workflow.synthetic.update".into(),
        record_id: "record".into(),
        input: CommandJson::parse(input).unwrap(),
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

/// Models a record whose current state no longer permits preparing the command
/// anew. A valid committed receipt must still replay after current authorization.
struct Unpreparable;
impl PgOwnerCommandEffects for Unpreparable {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        Err(BusinessError::invalid(
            "TEST_OWNER_PREPARATION_DENIED",
            "Synthetic current record cannot prepare again.",
        )
        .into())
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        panic!("unprepared effects must never execute")
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        panic!("unprepared effects must never retain")
    }
}
#[tokio::test]
async fn exact_replay_and_missing_receipt_inspection_never_prepare_a_new_owner_command() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let missing=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(), |c| Box::pin(async move {
            PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut Authority{deny:false},&Time,&mut Ids).replay(&command("{}"),&mut Unpreparable).await
        })).await.unwrap();
        assert!(missing.is_none());assert_eq!(counts(&pool).await,(0,0,0,0));
        let rejected=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(), |c| Box::pin(async move {
            PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut Authority{deny:false},&Time,&mut Ids).execute(&command("{}"),&mut Unpreparable).await
        })).await;
        assert!(matches!(rejected,Err(Error::Business(ref e)) if e.code=="TEST_OWNER_PREPARATION_DENIED"));
        assert_eq!(counts(&pool).await,(0,0,0,0));
        execute(&pool,"{}",false,false).await.unwrap();
        for explicit_inspection in [true,false] {
            let result=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(), move |c| Box::pin(async move {
                let mut authority=Authority{deny:false};let mut ids=Ids;
                let mut ledger=PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut authority,&Time,&mut ids);
                if explicit_inspection { Ok(ledger.replay(&command("{}"),&mut Unpreparable).await?.unwrap()) }
                else { ledger.execute(&command("{}"),&mut Unpreparable).await }
            })).await.unwrap();
            assert!(result.replayed);
        }
        assert_eq!(counts(&pool).await,(1,1,1,1));
    }).await;
}

#[tokio::test]
async fn oversized_persisted_receipt_data_is_rejected_before_result_or_authority_transfer() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let large=serde_json::json!({"synthetic":"x".repeat(2*1024*1024)}).to_string();
        for field in ["result","authority"] {
            let mut request=command("{}");request.key=format!("workflow:oversized:{field}");
            let digest=command_fingerprint(&actor(),&request.operation,&request.record_id,&request.input).unwrap();
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)VALUES($1,$2,'user',$3,'record',$4,0,$5)")
                .bind(&request.key).bind(&request.operation).bind(digest).bind(if field=="result"{large.as_str()}else{"{}"}).bind(if field=="authority"{Some(large.as_str())}else{None}).execute(pool.raw()).await.unwrap();
            let result=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),move |c| {
                let request=WorkflowRequest{key:request.key.clone(),operation:request.operation.clone(),record_id:request.record_id.clone(),input:request.input.clone()};
                Box::pin(async move {PgCommandLedger::new(CompanyId::new("company").unwrap(),c,&mut Authority{deny:false},&Time,&mut Ids).execute(&request,&mut Unpreparable).await})
            }).await;
            assert!(matches!(result,Err(Error::Business(ref e)) if e.code=="COMMAND_RECEIPT_TOO_LARGE"));
        }
        assert_eq!(counts(&pool).await,(2,0,0,0));
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
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.actor.clone())
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
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
            let command=WorkflowRequest {key,operation:case["operation"].as_str().unwrap().into(),record_id:case["recordId"].as_str().unwrap().into(),input:CommandJson::parse(case["inputJson"].as_str().unwrap()).unwrap()};
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

use erpx_server_workflow::public::record_origin::*;
fn origin_request() -> RecordOriginRequest {
    RecordOriginRequest {
        command_key: "workflow:test:command".into(),
        operation: "workflow.synthetic.update".into(),
        record_id: "record".into(),
        creator_user_id: erpx_server_shared::public::UserId::new("user").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
    }
}
async fn origin_transaction(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    lock_record_origin_sources(&mut tx).await.unwrap();
    tx
}

#[tokio::test]
async fn positive_origin_matches_actual_ledger_bytes_and_all_closed_identity_fields() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        execute(&pool, "{}", false, false).await.unwrap();
        let mut tx = origin_transaction(&pool).await;
        let proof = read_record_origin(&mut tx, origin_request())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            proof.result().as_str(),
            r#"{"id":"record","state":"updated"}"#
        );
        assert!(proof.retained_text_bytes() > proof.result().as_str().len() * 2);
        assert!(proof.retained_text_bytes() < 4096);

        verify_record_origin(&mut tx, &proof).await.unwrap();
        for field in ["key", "operation", "record", "creator", "company"] {
            let mut request = origin_request();
            match field {
                "key" => request.command_key = "workflow:missing:command".into(),
                "operation" => request.operation = "foreign.synthetic.update".into(),
                "record" => request.record_id = "foreign-record".into(),
                "creator" => {
                    request.creator_user_id =
                        erpx_server_shared::public::UserId::new("foreign-user").unwrap()
                }
                "company" => request.company_id = CompanyId::new("foreign-company").unwrap(),
                _ => unreachable!(),
            }
            assert!(
                read_record_origin(&mut tx, request)
                    .await
                    .unwrap()
                    .is_none(),
                "{field}"
            );
        }
        tx.commit().await.unwrap();
        assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn record_origin_requires_a_real_serializable_unit_and_bounded_request() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut c=pool.raw().acquire().await.unwrap();
        assert!(matches!(lock_record_origin_sources(&mut c).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_OWNER_TRANSACTION_REQUIRED"));
        assert!(matches!(read_record_origin(&mut c,origin_request()).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_OWNER_TRANSACTION_REQUIRED"));
        let mut tx=pool.raw().begin().await.unwrap();
        assert!(matches!(read_record_origin(&mut tx,origin_request()).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_SERIALIZABLE_REQUIRED"));
        tx.rollback().await.unwrap();
        let mut tx=origin_transaction(&pool).await;
        let mut request=origin_request();request.command_key="unbounded".repeat(1000);
        assert!(matches!(read_record_origin(&mut tx,request).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_REQUEST_INVALID"));
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn corrupt_or_legacy_receipts_do_not_establish_positive_provenance() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        execute(&pool,"{}",false,false).await.unwrap();
        let mut tx=origin_transaction(&pool).await;
        let saved_hash:String=sqlx::query_scalar("SELECT request_hash FROM command_receipts").fetch_one(&mut *tx).await.unwrap();
        let saved_authority:String=sqlx::query_scalar("SELECT authority_json FROM command_receipts").fetch_one(&mut *tx).await.unwrap();
        for hash in ["", "123", "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] {
            sqlx::query("UPDATE command_receipts SET request_hash=$1").bind(hash).execute(&mut *tx).await.unwrap();
            assert!(matches!(read_record_origin(&mut tx,origin_request()).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_INVALID"));
        }
        sqlx::query("UPDATE command_receipts SET request_hash=$1").bind(saved_hash).execute(&mut *tx).await.unwrap();
        for authority in [None,Some(r#"{"unsupported":true}"#),Some("null"),Some("{}"),Some(r#"{"companyId":null}"#),Some(r#"{"companyId":"foreign"}"#)] {
            sqlx::query("UPDATE command_receipts SET authority_json=$1").bind(authority).execute(&mut *tx).await.unwrap();
            match read_record_origin(&mut tx,origin_request()).await {
                Ok(value)=>assert!(value.is_none()),
                Err(Error::Business(e))=>assert_eq!(e.code,"COMMAND_ORIGIN_INVALID"),
                _=>panic!("unexpected provenance error"),
            }
        }
        sqlx::query("UPDATE command_receipts SET authority_json=$1,result_json='null'")
            .bind(saved_authority).execute(&mut *tx).await.unwrap();
        // Generic provenance does not invent the owner's result shape.
        let proof=read_record_origin(&mut tx,origin_request()).await.unwrap().unwrap();
        assert_eq!(proof.result().as_str(),"null");
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn every_persisted_origin_text_is_bounded_before_transfer() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;execute(&pool,"{}",false,false).await.unwrap();
        for field in ["operation","actor_id","record_id","request_hash","authority_json","result_json"] {
            let mut tx=origin_transaction(&pool).await;
            let query=match field {
                "operation"=>"UPDATE command_receipts SET operation=repeat('x',201)",
                "actor_id"=>"UPDATE command_receipts SET actor_id=repeat('x',201)",
                "record_id"=>"UPDATE command_receipts SET record_id=repeat('x',201)",
                "request_hash"=>"UPDATE command_receipts SET request_hash=repeat('a',65)",
                "authority_json"=>"UPDATE command_receipts SET authority_json=json_build_object('synthetic',repeat('x',2097153))::text",
                "result_json"=>"UPDATE command_receipts SET result_json=json_build_object('synthetic',repeat('x',2097153))::text",
                _=>unreachable!(),
            };
            sqlx::query(query).execute(&mut *tx).await.unwrap();
            assert!(matches!(read_record_origin(&mut tx,origin_request()).await,Err(Error::Business(ref e)) if e.code=="COMMAND_RECEIPT_TOO_LARGE"),"{field}");
            tx.rollback().await.unwrap();
        }
    }).await;
}

#[tokio::test]
async fn positive_origin_respects_the_callers_remaining_budget_before_transfer() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        execute(&pool, "{}", false, false).await.unwrap();
        let mut tx = origin_transaction(&pool).await;
        sqlx::query("UPDATE command_receipts SET authority_json=(authority_json::jsonb || jsonb_build_object('padding',repeat('x',5000)))::text").execute(&mut *tx).await.unwrap();
        assert!(read_record_origin(&mut tx, origin_request()).await.unwrap().is_some());
        assert!(matches!(read_record_origin_with_limit(&mut tx, origin_request(), 4096).await, Err(Error::Business(ref e)) if e.code == "COMMAND_RECEIPT_TOO_LARGE"));
        assert!(matches!(read_record_origin_with_limit(&mut tx, origin_request(), 0).await, Err(Error::Business(ref e)) if e.code == "COMMAND_RECEIPT_TOO_LARGE"));
        let proof = read_record_origin_with_limit(&mut tx, origin_request(), 8192).await.unwrap().unwrap();
        verify_record_origin(&mut tx, &proof).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn raw_result_is_bounded_and_scoped_to_the_same_full_receipt() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        execute(&pool,"{}",false,false).await.unwrap();
        let mut tx=origin_transaction(&pool).await;
        let long="x".repeat(220);
        let result=serde_json::json!({"id":long,"status":"scheduled","version":2}).to_string();
        sqlx::query("UPDATE command_receipts SET record_id=$1,result_json=$2").bind(&long).bind(&result).execute(&mut *tx).await.unwrap();
        let request=RawRecordOriginRequest{command_key:"workflow:test:command".into(),operation:"workflow.synthetic.update".into(),record_id:long.clone(),creator_user_id:"user".into(),company_id:CompanyId::new("company").unwrap()};
        let (proof,actual)=read_raw_record_origin_result_with_limit(&mut tx,request.clone(),65536).await.unwrap().unwrap();assert_eq!(actual.property("id").unwrap().unwrap().string_utf8_storage().unwrap().unwrap(),long);verify_raw_record_origin(&mut tx,&proof).await.unwrap();
        assert!(matches!(read_raw_record_origin_result_with_limit(&mut tx,request.clone(),100).await,Err(Error::Business(ref e)) if e.code=="COMMAND_RECEIPT_TOO_LARGE"));
        for field in ["actor","operation","company"] {let mut other=request.clone();match field {"actor"=>other.creator_user_id="foreign".into(),"operation"=>other.operation="foreign.operation".into(),_=>other.company_id=CompanyId::new("foreign").unwrap()};assert!(read_raw_record_origin_result_with_limit(&mut tx,other,65536).await.unwrap().is_none());}
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn a_positive_origin_cannot_be_reused_in_another_transaction_or_backend() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;execute(&pool,"{}",false,false).await.unwrap();
        let mut original=origin_transaction(&pool).await;
        let proof=read_record_origin(&mut original,origin_request()).await.unwrap().unwrap();
        let mut other=origin_transaction(&pool).await;
        assert!(matches!(verify_record_origin(&mut other,&proof).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_UNIT_CHANGED"));
        other.rollback().await.unwrap();
        original.commit().await.unwrap();
        // Begin again on the very same retained connection: backend equality is
        // insufficient without the real transaction identity.
        let mut c=pool.raw().acquire().await.unwrap();
        let mut first=c.begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        lock_record_origin_sources(&mut first).await.unwrap();
        let second=read_record_origin(&mut first,origin_request()).await.unwrap().unwrap();
        first.commit().await.unwrap();
        let mut next=c.begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *next).await.unwrap();
        lock_record_origin_sources(&mut next).await.unwrap();
        assert!(matches!(verify_record_origin(&mut next,&second).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_UNIT_CHANGED"));
        next.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn same_unit_origin_mutation_is_rejected_and_all_owner_effects_can_roll_back() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;execute(&pool,"{}",false,false).await.unwrap();
        let mut tx=origin_transaction(&pool).await;
        let proof=read_record_origin(&mut tx,origin_request()).await.unwrap().unwrap();
        sqlx::query("UPDATE command_receipts SET created_at=created_at+1").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO workflow_test_effects VALUES('later-owner-effect')").execute(&mut *tx).await.unwrap();
        assert!(matches!(verify_record_origin(&mut tx,&proof).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_CHANGED"));
        tx.rollback().await.unwrap();
        assert_eq!(counts(&pool).await,(1,1,1,1));
    }).await;
}

#[tokio::test]
async fn provenance_wait_after_old_snapshot_yields_real_40001_then_fresh_source() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;execute(&pool,"{}",false,false).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE command_receipts SET created_at=created_at+1").execute(&mut *writer).await.unwrap();
        let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        let other_pool=pool.clone();let (started,waiter)=tokio::sync::oneshot::channel();
        let other=tokio::spawn(async move {
            let mut tx=origin_transaction(&other_pool).await;
            let _:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts").fetch_one(&mut *tx).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();
            started.send(pid).unwrap();
            let result=read_record_origin(&mut tx,origin_request()).await;
            tx.rollback().await.unwrap();result
        });
        let pid=waiter.await.unwrap();let mut blocked=false;
        for _ in 0..200 {
            blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();
            if blocked { break; }tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked,"observe a real conflicting source update before releasing it");
        writer.commit().await.unwrap();
        assert!(matches!(other.await.unwrap(),Err(Error::Persistence(ref e)) if e.sql_state()==Some("40001")));
        let mut tx=origin_transaction(&pool).await;
        let proof=read_record_origin(&mut tx,origin_request()).await.unwrap().unwrap();
        verify_record_origin(&mut tx,&proof).await.unwrap();tx.commit().await.unwrap();
    }).await;
}

#[tokio::test]
async fn a_retained_positive_origin_holds_its_actual_row_until_the_owner_unit_ends() {
    fixture::with_database(4, |pool| async move {
        setup(&pool).await;
        execute(&pool, "{}", false, false).await.unwrap();
        let mut tx = origin_transaction(&pool).await;
        let proof = read_record_origin(&mut tx, origin_request())
            .await
            .unwrap()
            .unwrap();
        let owner_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let writer_pool = pool.clone();
        let (started, waiter) = tokio::sync::oneshot::channel();
        let writer = tokio::spawn(async move {
            let mut c = writer_pool.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *c)
                .await
                .unwrap();
            started.send(pid).unwrap();
            sqlx::query("UPDATE command_receipts SET created_at=created_at+1")
                .execute(&mut *c)
                .await
                .unwrap();
        });
        let writer_pid = waiter.await.unwrap();
        let mut blocked = false;
        for _ in 0..200 {
            blocked = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                .bind(owner_pid)
                .bind(writer_pid)
                .fetch_one(pool.raw())
                .await
                .unwrap();
            if blocked {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            blocked,
            "positive provenance must retain its real tuple against a concurrent writer"
        );
        verify_record_origin(&mut tx, &proof).await.unwrap();
        tx.commit().await.unwrap();
        writer.await.unwrap();
        assert_eq!(counts(&pool).await, (1, 1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn raw_factual_origin_preserves_exact_historical_identity_and_detects_full_row_changes() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await; execute(&pool,"{}",false,false).await.unwrap();
        let record=format!("historical-{}\n","r".repeat(220));let creator=format!("creator-{}\t","c".repeat(220));
        sqlx::query("UPDATE command_receipts SET record_id=$1,actor_id=$2 WHERE command_id='workflow:test:command'").bind(&record).bind(&creator).execute(pool.raw()).await.unwrap();
        let request=RawRecordOriginRequest{command_key:"workflow:test:command".into(),operation:"workflow.synthetic.update".into(),record_id:record,creator_user_id:creator,company_id:CompanyId::new("company").unwrap()};
        let mut tx=origin_transaction(&pool).await;
        let proof=read_raw_record_origin(&mut tx,request.clone()).await.unwrap().unwrap();
        verify_raw_record_origin(&mut tx,&proof).await.unwrap();
        for field in ["key","operation","record","creator","company"] {
            let mut wrong=request.clone();match field {"key"=>wrong.command_key="missing".into(),"operation"=>wrong.operation="wrong".into(),"record"=>wrong.record_id="wrong".into(),"creator"=>wrong.creator_user_id="wrong".into(),"company"=>wrong.company_id=CompanyId::new("foreign").unwrap(),_=>unreachable!()};
            assert!(read_raw_record_origin(&mut tx,wrong).await.unwrap().is_none(),"{field}");
        }
        let mut other=origin_transaction(&pool).await;
        assert!(matches!(verify_raw_record_origin(&mut other,&proof).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_UNIT_CHANGED"));
        other.rollback().await.unwrap();
        sqlx::query("UPDATE command_receipts SET result_json='{}' WHERE command_id=$1").bind(&request.command_key).execute(&mut *tx).await.unwrap();
        assert!(matches!(verify_raw_record_origin(&mut tx,&proof).await,Err(Error::Business(ref e)) if e.code=="COMMAND_ORIGIN_CHANGED"));
        tx.rollback().await.unwrap();
        let mut malformed=origin_transaction(&pool).await;
        let error=sqlx::query("UPDATE command_receipts SET authority_json='{' WHERE command_id=$1").bind(&request.command_key).execute(&mut *malformed).await.unwrap_err();
        assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));
        malformed.rollback().await.unwrap();
        let mut fresh=origin_transaction(&pool).await;
        let proof=read_raw_record_origin(&mut fresh,request.clone()).await.unwrap().unwrap();
        verify_raw_record_origin(&mut fresh,&proof).await.unwrap();fresh.rollback().await.unwrap();
        for authority in ["{}","[]","null"] {
            let mut missing=origin_transaction(&pool).await;
            sqlx::query("UPDATE command_receipts SET authority_json=$1 WHERE command_id=$2").bind(authority).bind(&request.command_key).execute(&mut *missing).await.unwrap();
            assert!(read_raw_record_origin(&mut missing,request.clone()).await.unwrap().is_none());
            missing.rollback().await.unwrap();
        }
    }).await;
}

/// Workflow-only uncommitted receipt fixture: no Commerce source is fabricated
/// or committed, and this test grants no business authorization.
struct SetupMarkerEffects;
impl PgOwnerCommandEffects for SetupMarkerEffects {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let result = format!(
            r#"{{"id":"company","status":"configured","version":1,"_servicePartsPostingSetup":{{"kind":"service-parts-posting-setup","companyId":"company","commandId":"{}","mode":"initialize","mapping":{{"company_id":"company","chart_id":"{}"}}}}}}"#,
            command.key,
            if command.key == "z-service-test" {
                "later"
            } else {
                "earlier"
            }
        );
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::parse(&result)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
}
#[tokio::test]
async fn service_posting_receipt_discovery_preserves_first_marker_and_absence() {
    use erpx_server_workflow::public::service_parts_posting::*;
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_service_posting_receipts(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let missing=read_service_posting_receipt(&mut tx,&company).await.unwrap();
        assert!(missing.facts().is_none());
        let missing_key=read_service_prerequisite_receipt(&mut tx,"a-service-test").await.unwrap();assert!(missing_key.facts().is_none());
        let make=|key:&str|WorkflowRequest {key:key.into(),operation:"commerce.service-parts-posting-setup.initialize".into(),record_id:"company".into(),input:CommandJson::parse("{}").unwrap()};
        for key in ["z-service-test","a-service-test"] {
            PgCommandLedger::new(company.clone(),&mut tx,&mut Authority{deny:false},&Time,&mut Ids).execute(&make(key),&mut SetupMarkerEffects).await.unwrap();
        }
        assert!(matches!(missing.require_current(&mut tx).await,Err(Error::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
        assert!(matches!(missing_key.require_current(&mut tx).await,Err(Error::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
        let keyed=read_service_prerequisite_receipt(&mut tx,"a-service-test").await.unwrap();let facts=keyed.facts().unwrap();assert_eq!(facts.record_id,"company");assert_eq!(facts.operation,"commerce.service-parts-posting-setup.initialize");assert_eq!(facts.company_id.as_deref(),Some("company"));keyed.require_current(&mut tx).await.unwrap();
        let selected=read_service_posting_receipt(&mut tx,&company).await.unwrap();
        assert_eq!(selected.facts().unwrap().command_id,"a-service-test");
        assert!(selected.facts().unwrap().proof_json.contains("a-service-test"));
        selected.require_current(&mut tx).await.unwrap();
        let mapping=r#"{"company_id":"company","chart_id":"later"}"#;
        let stock=read_service_stock_posting_receipt(&mut tx,&company,mapping).await.unwrap();
        let oracle:String=sqlx::query_scalar("SELECT command_id FROM command_receipts WHERE record_id=$1 AND result_json::jsonb->'_servicePartsPostingSetup'->'mapping'=$2::jsonb ORDER BY command_id LIMIT 1").bind(company.as_str()).bind(mapping).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(oracle,"z-service-test");assert_eq!(stock.facts().unwrap().command_id,oracle);stock.require_current(&mut tx).await.unwrap();
        let foreign=read_service_posting_receipt(&mut tx,&CompanyId::new("foreign").unwrap()).await.unwrap();
        assert!(foreign.facts().is_none());
        // The intentionally incomplete owner effect is never committed.
        tx.rollback().await.unwrap();
        assert_eq!(counts(&pool).await,(0,0,0,0));
    }).await;
}

#[tokio::test]
async fn service_posting_receipt_lock_waits_before_snapshot() {
    use erpx_server_workflow::public::service_parts_posting::*;
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *writer).await.unwrap();
        PgCommandLedger::new(CompanyId::new("company").unwrap(),&mut writer,&mut Authority{deny:false},&Time,&mut Ids).execute(&command("{}"),&mut Effects{fail_retain:false}).await.unwrap();
        let reader=pool.clone();let task=tokio::spawn(async move{
            PgUnitOfWork::new(reader).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
                lock_service_posting_receipts(c).await?;
                let source=read_service_posting_receipt(c,&CompanyId::new("company").unwrap()).await?;
                source.require_current(c).await?;
                Ok(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id='workflow:test:command')").fetch_one(c).await?)
            })).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(10),async{loop{
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='command_receipts'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
            if waiting{break}tokio::task::yield_now().await;
        }}).await.unwrap();
        writer.commit().await.unwrap();assert!(task.await.unwrap().unwrap());
    }).await;
}

#[tokio::test]
async fn count_origin_marker_collection_is_global_exact_and_retained() {
    use erpx_server_workflow::public::count_origin_markers::*;
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let count=RecordId::new("real-count").unwrap();
        let mut no_lock=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *no_lock).await.unwrap();
        assert!(inspect_count_origin_markers(&mut no_lock,&count).await.is_err());
        no_lock.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_count_origin_marker_sources(&mut tx).await.unwrap();
        let empty=inspect_count_origin_markers(&mut tx,&count).await.unwrap();
        assert_eq!(empty.markers(),CountOriginMarkers{initial:false,service:false,pos:false,manufacturing:false});
        // These synthetic legacy receipts deliberately do not derive their
        // command IDs from the Count or share its Company label.
        for (id,operation,record,result,authority) in [
            ("marker-service","inventory.count.create","real-count",Some(r#"{"_serviceInitialPhysicalStockCount":null}"#),Some(r#"{"companyId":"foreign"}"#)),
            ("marker-array","inventory.count.create","real-count",Some(r#"["_posInitialPhysicalStockCount"]"#),None),
            ("marker-manufacturing","inventory.count.create","real-count",Some(r#"{"_manufacturingInitialPhysicalStockCount":false}"#),None),
            ("other-record","inventory.count.create","other-count",Some(r#"{"_initialPhysicalStockCount":true}"#),None),
            ("other-operation","other","real-count",Some(r#"{"ordinary":true}"#),None),
            // SQL NULL is rejected by the current Service setup birth trigger;
            // JSON null is accepted and still exercises a null-valued document.
            ("json-null-result","inventory.count.create","real-count",Some("null"),None),
        ] {
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,$2,'fixture','synthetic',$3,$4,0,$5)").bind(id).bind(operation).bind(record).bind(result).bind(authority).execute(&mut *tx).await.unwrap();
        }
        assert!(empty.require_current(&mut tx).await.is_err());
        let present=inspect_count_origin_markers(&mut tx,&count).await.unwrap();
        assert_eq!(present.count_id(),&count);
        assert_eq!(present.markers(),CountOriginMarkers{initial:false,service:true,pos:true,manufacturing:true});
        present.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        lock_count_origin_marker_sources(&mut other).await.unwrap();
        assert!(present.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
        let mut invalid=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *invalid).await.unwrap();
        lock_count_origin_marker_sources(&mut invalid).await.unwrap();
        // Current marker birth triggers parse result_json on INSERT; malformed
        // historic rows cannot be fabricated in this migrated fixture.
        let malformed=sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('bad-marker','inventory.count.create','fixture','synthetic','real-count','{invalid',0)").execute(&mut *invalid).await.unwrap_err();
        assert_eq!(malformed.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));
        invalid.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();
        sqlx::query("SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();
        lock_count_origin_marker_sources(&mut saved).await.unwrap();
        let retained=inspect_count_origin_markers(&mut saved,&count).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();
        assert!(retained.require_current(&mut saved).await.is_err());
        saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn record_origin_require_held_checks_existing_lock_without_upgrade() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let mut plain=pool.raw().acquire().await.unwrap();
        assert!(matches!(require_record_origin_sources_held(&mut plain).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_OWNER_TRANSACTION_REQUIRED"));
        drop(plain);
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        assert!(matches!(require_record_origin_sources_held(&mut tx).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_ORIGIN_LOCK_REQUIRED"));
        sqlx::query("LOCK TABLE command_receipts IN ACCESS SHARE MODE").execute(&mut *tx).await.unwrap();
        assert!(matches!(require_record_origin_sources_held(&mut tx).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_ORIGIN_LOCK_REQUIRED"));
        sqlx::query("SAVEPOINT before_record_origin_lock").execute(&mut *tx).await.unwrap();
        lock_record_origin_sources(&mut tx).await.unwrap();
        require_record_origin_sources_held(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_record_origin_lock").execute(&mut *tx).await.unwrap();
        assert!(matches!(require_record_origin_sources_held(&mut tx).await,Err(Error::Persistence(ref e)) if e.code=="WORKFLOW_ORIGIN_LOCK_REQUIRED"));
        tx.rollback().await.unwrap();
        let mut stronger=pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE command_receipts IN ROW EXCLUSIVE MODE").execute(&mut *stronger).await.unwrap();
        require_record_origin_sources_held(&mut stronger).await.unwrap();stronger.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn field_history_receipt_join_retains_exact_candidates_without_temp_tables() {
    use erpx_server_workflow::public::record_origin::*;
    use sqlx::Acquire;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut conn=pool.raw().acquire().await.unwrap();
        sqlx::query("CREATE TEMP TABLE command_receipts(command_id text)").execute(&mut *conn).await.unwrap();
        let mut unlocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_field_history_receipt_join(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_field_history_receipt_join_sources(&mut tx).await.unwrap();let source=inspect_field_history_receipt_join(&mut tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        let initial_temp_count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();
        // With an empty TEMP shadow, the fixed source still sees the canonical
        // public table. No receipt contents are returned as a Rust array.
        let count_sql=format!("WITH {} SELECT count(*) FROM field_history_receipts",source.cte_sql());
        let n:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str())).fetch_one(&mut *tx).await.unwrap();assert_eq!(n,0);
        let after_temp_count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();assert_eq!(after_temp_count,initial_temp_count);
        sqlx::query("DROP TABLE pg_temp.command_receipts").execute(&mut *tx).await.unwrap();
        // Real database rows, deliberately uncommitted and with all native
        // guards enabled, verify this Workflow join only—not Finance authority.
        sqlx::query("INSERT INTO public.finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES('payment','company','receipt','PAY','2026-01-01','THB',100,100,'maker',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('good-a','finance.payment.create','maker','synthetic','payment','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-a\"}','{\"companyId\":\"company\"}',0),('wrong-record','finance.payment.create','maker','synthetic','other','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-b\"}','{\"companyId\":\"company\"}',0),('wrong-actor','finance.payment.create','other','synthetic','payment','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-c\"}','{\"companyId\":\"company\"}',0),('wrong-company','finance.payment.create','maker','synthetic','payment','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-d\"}','{\"companyId\":\"foreign\"}',0),('null-result','finance.payment.create','maker','synthetic','payment','null','{\"companyId\":\"company\"}',0),('different-op','unrelated','maker','synthetic','payment','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-e\"}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();
        // Existing migrated marker guard rejects SQL NULL at receipt birth.
        // Do not disable it or claim a persisted SQL-NULL receipt fixture.
        sqlx::query("SAVEPOINT null_receipt").execute(&mut *tx).await.unwrap();
        let sql_null=sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('sql-null','finance.payment.create','maker','synthetic','payment',NULL,'{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap_err();
        assert_eq!(sql_null.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));sqlx::query("ROLLBACK TO SAVEPOINT null_receipt").execute(&mut *tx).await.unwrap();
        let joins="SELECT count(*) FROM public.finance_payments p JOIN field_history_receipts r ON r.record_id=p.id AND r.operation='finance.payment.create' AND r.actor_id=p.created_by AND r.authority_json::jsonb->>'companyId'=p.company_id AND r.result_json::jsonb->>'id'=p.id WHERE p.id=$1 AND p.company_id=$2 AND r.result_json::jsonb->>'settlementInvoiceId' IS NOT NULL";
        let sql=format!("WITH {} {joins}",source.cte_sql());
        let observed:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind("payment").bind("company").fetch_one(&mut *tx).await.unwrap();assert_eq!(observed,1);
        let direct:i64=sqlx::query_scalar("SELECT count(*) FROM public.finance_payments p JOIN public.command_receipts r ON r.record_id=p.id AND r.operation='finance.payment.create' AND r.actor_id=p.created_by AND r.authority_json::jsonb->>'companyId'=p.company_id AND r.result_json::jsonb->>'id'=p.id WHERE p.id=$1 AND p.company_id=$2 AND r.result_json::jsonb->>'settlementInvoiceId' IS NOT NULL").bind("payment").bind("company").fetch_one(&mut *tx).await.unwrap();assert_eq!(observed,direct);
        assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("payment").bind("foreign").fetch_one(&mut *tx).await.unwrap(),0);
        sqlx::query("SAVEPOINT second_receipt").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('good-b','finance.payment.create','maker','synthetic','payment','{\"id\":\"payment\",\"settlementInvoiceId\":\"invoice-b\"}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();
        let changed_count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str())).bind("payment").bind("company").fetch_one(&mut *tx).await.unwrap();assert_eq!(changed_count,2);source.require_current(&mut tx).await.unwrap();
        // Source fencing is intentionally not content retention: consumer must
        // compare its complete joined decision after its own late effects.
        assert_ne!(observed,changed_count);sqlx::query("ROLLBACK TO SAVEPOINT second_receipt").execute(&mut *tx).await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("payment").bind("company").fetch_one(&mut *tx).await.unwrap(),observed);
        sqlx::query("SAVEPOINT bad_receipt").execute(&mut *tx).await.unwrap();let malformed=sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('bad-json','finance.payment.create','maker','synthetic','payment','{invalid','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await;
        match malformed {Err(error)=>assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02")),Ok(_)=>{let error=sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(sql.as_str())).bind("payment").bind("company").fetch_one(&mut *tx).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));}}
        sqlx::query("ROLLBACK TO SAVEPOINT bad_receipt").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        sqlx::query("DROP TABLE pg_temp.command_receipts").execute(&mut *conn).await.unwrap();let mut fresh=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_field_history_receipt_join_sources(&mut fresh).await.unwrap();assert!(source.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut saved=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT receipt_lock").execute(&mut *saved).await.unwrap();lock_field_history_receipt_join_sources(&mut saved).await.unwrap();let saved_source=inspect_field_history_receipt_join(&mut saved).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT receipt_lock").execute(&mut *saved).await.unwrap();assert!(saved_source.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.command_receipts IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let conflict=lock_field_history_receipt_join_sources(&mut blocked).await;assert!(matches!(conflict,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_count_history_receipts_match_actual_legacy_marker_sources() {
    use erpx_server_workflow::public::record_origin::*;
    use sqlx::Acquire;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut conn=pool.raw().acquire().await.unwrap();sqlx::query("CREATE TEMP TABLE command_receipts(command_id text)").execute(&mut *conn).await.unwrap();
        let mut unlocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_initial_count_history_receipt_join(&mut unlocked).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();let source=inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();
        let temp_before:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();let rows_sql=format!("WITH {} SELECT count(*) FROM initial_count_history_receipts",source.cte_sql());assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(rows_sql.as_str())).fetch_one(&mut *tx).await.unwrap(),0);let temp_after:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class WHERE relnamespace=pg_my_temp_schema()").fetch_one(&mut *tx).await.unwrap();assert_eq!(temp_after,temp_before);
        // Allow actual legacy unqualified SQL to see public; rollback restores
        // this TEMP shadow. No production table/trigger is altered.
        sqlx::query("DROP TABLE pg_temp.command_receipts").execute(&mut *tx).await.unwrap();
        let long_id=format!("count-{}","x".repeat(240));
        let names=["_initialPhysicalStockCount","_serviceInitialPhysicalStockCount","_posInitialPhysicalStockCount","_manufacturingInitialPhysicalStockCount"];
        for (n,marker) in names.iter().enumerate(){
            // A JSON-null marker is still present under jsonb_exists. These
            // synthetic receipts stay uncommitted with deferred guards enabled.
            let result=serde_json::json!({(*marker):null}).to_string();
            sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,'inventory.count.create','fixture','synthetic',$2,$3,'{\"companyId\":\"foreign\"}',0)").bind(format!("marker-{n}")).bind(&long_id).bind(result).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('count-plain','inventory.count.create','fixture','synthetic',$1,'null','{\"companyId\":\"company\"}',0)").bind(&long_id).execute(&mut *tx).await.unwrap();
        let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../server-inventory/src/adapters");
        let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const keys=['initialPhysicalStockMarkedRows','serviceInitialPhysicalStockMarkedRows','posInitialPhysicalStockMarkedRows','manufacturingInitialPhysicalStockMarkedRows'];const out=[];for(let i=0;i<keys.length;i++){const m=await import(process.argv[i+1]);out.push(m[keys[i]])}process.stdout.write(JSON.stringify(out));"#;
        let output=std::process::Command::new(node).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg(script).arg(dir.join("initial-physical-stock-count-source-sql.ts")).arg(dir.join("service-initial-physical-stock-count-source-sql.ts")).arg(dir.join("pos-initial-physical-stock-count-source-sql.ts")).arg(dir.join("manufacturing-initial-physical-stock-count-source-sql.ts")).output().expect("pinned Node marker source");
        assert!(output.status.success(),"actual TS marker sources: {}",String::from_utf8_lossy(&output.stderr));let actual:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        let projected=format!("WITH {} SELECT count(*) FROM initial_count_history_receipts r WHERE r.record_id=$1 AND jsonb_exists(r.result_json::jsonb,$2)",source.cte_sql());
        for (n,marker) in names.iter().enumerate(){let legacy=format!("SELECT count(*) FROM ({}) marker WHERE marker.id=$1",actual[n].as_str().unwrap());let expected:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy.as_str())).bind(&long_id).fetch_one(&mut *tx).await.unwrap();let got:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind(&long_id).bind(marker).fetch_one(&mut *tx).await.unwrap();assert_eq!((got,expected),(1,1),"marker {marker}");}
        let foreign_is_not_a_filter:i64=sqlx::query_scalar("SELECT count(*) FROM public.command_receipts WHERE operation='inventory.count.create' AND record_id=$1 AND authority_json::jsonb->>'companyId'='foreign' AND jsonb_exists(result_json::jsonb,'_serviceInitialPhysicalStockCount')").bind(&long_id).fetch_one(&mut *tx).await.unwrap();assert_eq!(foreign_is_not_a_filter,1);
        source.require_current(&mut tx).await.unwrap();sqlx::query("SAVEPOINT extra_count_marker").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('marker-extra','inventory.count.create','fixture','synthetic',$1,'{\"_serviceInitialPhysicalStockCount\":null}','{\"companyId\":\"another\"}',0)").bind(&long_id).execute(&mut *tx).await.unwrap();let count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(projected.as_str())).bind(&long_id).bind("_serviceInitialPhysicalStockCount").fetch_one(&mut *tx).await.unwrap();assert_eq!(count,2);source.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT extra_count_marker").execute(&mut *tx).await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(projected.as_str())).bind(&long_id).bind("_serviceInitialPhysicalStockCount").fetch_one(&mut *tx).await.unwrap(),1);
        tx.rollback().await.unwrap();sqlx::query("DROP TABLE pg_temp.command_receipts").execute(&mut *conn).await.unwrap();let mut other=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_initial_count_history_receipt_sources(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_count_history_support_receipts_match_actual_setup_sql() {
    use erpx_server_workflow::public::record_origin::*;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_initial_count_history_receipt_sources(&mut tx).await.unwrap();
        let source=inspect_initial_count_history_receipt_join(&mut tx).await.unwrap();
        let operations=["commerce.service-parts-posting-setup.initialize","commerce.service-parts-posting-setup.confirm-existing","inventory.count.post","inventory.count.reject","inventory.count.cancel","attachment.record.bind","attachment.compatibility.bind"];
        let long_id=format!("company-{}","x".repeat(240));
        for (n,operation) in operations.iter().enumerate(){
            let command=format!("support-{n}");let status=if n==0{"configured"}else{"confirmed-existing"};
            let result=if n<2 {serde_json::json!({"id":long_id,"version":1,"status":status,"_servicePartsPostingSetup":{"kind":"service-parts-posting-setup","companyId":long_id,"commandId":command,"mapping":{"company_id":long_id}}}).to_string()} else {"null".to_owned()};
            let authority=serde_json::json!({"companyId":long_id}).to_string();
            sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,'fixture','synthetic',$3,$4,$5,0)").bind(&command).bind(operation).bind(&long_id).bind(result).bind(authority).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('support-unrelated','other.operation','fixture','synthetic',$1,'null','{}',0)").bind(&long_id).execute(&mut *tx).await.unwrap();
        let all=format!("WITH {} SELECT count(*) FROM initial_count_history_support_receipts WHERE record_id=$1",source.cte_sql());assert_eq!(sqlx::query_scalar::<_,i64>(sqlx::AssertSqlSafe(all.as_str())).bind(&long_id).fetch_one(&mut *tx).await.unwrap(),7);
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../server-commerce/src/adapters/service-parts-posting-proof.ts");
        let script=r#"if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);process.stdout.write(JSON.stringify(m.servicePartsPostingSourceRows));"#;
        let output=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into())).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg(script).arg(dir).output().unwrap();assert!(output.status.success(),"actual TS source: {}",String::from_utf8_lossy(&output.stderr));let actual:String=serde_json::from_slice(&output.stdout).unwrap();
        let replaced=actual.replace("FROM command_receipts receipt","FROM initial_count_history_support_receipts receipt");assert_ne!(actual,replaced);
        let legacy=format!("SELECT count(*) FROM ({actual}) source WHERE source.company_id=$1");let projected=format!("WITH {} SELECT count(*) FROM ({replaced}) source WHERE source.company_id=$1",source.cte_sql());
        async fn compare(c:&mut sqlx::PgConnection,id:&str,a:&str,b:&str)->i64{let expected:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(a)).bind(id).fetch_one(&mut *c).await.unwrap();let got=sqlx::query_scalar(sqlx::AssertSqlSafe(b)).bind(id).fetch_one(&mut *c).await.unwrap();assert_eq!(expected,got);got}
        assert_eq!(compare(&mut tx,&long_id,&legacy,&projected).await,2);assert_eq!(compare(&mut tx,"foreign",&legacy,&projected).await,0);
        source.require_current(&mut tx).await.unwrap();sqlx::query("SAVEPOINT changed_setup").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.command_receipts SET result_json=jsonb_set(result_json::jsonb,'{status}',to_jsonb('wrong'::text))::text WHERE command_id='support-1'").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&legacy,&projected).await,1);source.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT changed_setup").execute(&mut *tx).await.unwrap();assert_eq!(compare(&mut tx,&long_id,&legacy,&projected).await,2);
        // Synthetic receipts never commit and deferred guards remain enabled.
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn asset_purchase_receipt_purpose_matches_actual_ts_without_hash_or_target() {
    use erpx_server_workflow::public::asset_purchase_receipt::*;
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_asset_purchase_receipt_purpose(&mut unlocked,"asset-key","asset.create","user",&CompanyId::new("company").unwrap()).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_asset_purchase_receipt_sources(&mut tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        for (key,op,actor,authority) in [("asset-key","asset.create","user",Some("{\"companyId\":\"company\"}")),("asset-op","asset.place","user",Some("{\"companyId\":\"company\"}")),("asset-actor","asset.create","other",Some("{\"companyId\":\"company\"}")),("asset-company","asset.create","user",Some("{\"companyId\":\"other\"}")),("asset-null","asset.create","user",None),("asset-json-null","asset.create","user",Some("null"))] {sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES($1,$2,$3,'noncanonical-hash','different-target','{}',$4,0)").bind(key).bind(op).bind(actor).bind(authority).execute(&mut *tx).await.unwrap();}
        // Extract the exact repo-owned purpose lookup; values remain SQL binds.
        let script=r#"import fs from 'node:fs';const s=fs.readFileSync(process.argv[1],'utf8');const prefix='database.prepare(\"SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>\'companyId\'=?\")';const start=s.indexOf(prefix);if(start<0)throw Error('ASSET_RECEIPT_SOURCE_MISSING');let n=0;const q=prefix.slice('database.prepare(\"'.length,-2).replace(/\?/g,()=>'$'+(++n));process.stdout.write(JSON.stringify(q));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../server-assets/src/adapters/purchases.ts");let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").unwrap_or_else(|_|"bun".into())).arg("-e").arg(script).arg(path).output().unwrap();assert!(output.status.success(),"actual TS receipt lookup: {}",String::from_utf8_lossy(&output.stderr));let legacy:String=serde_json::from_slice(&output.stdout).unwrap();
        for (key,expected) in [("asset-key",true),("asset-op",false),("asset-actor",false),("asset-company",false),("asset-null",false),("asset-json-null",false),("asset-missing",false)] {let oracle:Option<i32>=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy.as_str())).bind(key).bind("asset.create").bind("user").bind("company").fetch_optional(&mut *tx).await.unwrap();let observed=inspect_asset_purchase_receipt_purpose(&mut tx,key,"asset.create","user",&company).await.unwrap();assert_eq!((observed.selects_history(),oracle.is_some()),(expected,expected),"{key}");observed.require_unchanged_before_effects(&mut tx).await.unwrap();}
        let fresh=inspect_asset_purchase_receipt_purpose(&mut tx,"fresh-key","asset.create","user",&company).await.unwrap();assert!(!fresh.selects_history());sqlx::query("SAVEPOINT own_receipt").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('fresh-key','asset.create','user','different-hash','new-target','{}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();assert!(fresh.require_unchanged_before_effects(&mut tx).await.is_err());assert!(inspect_asset_purchase_receipt_purpose(&mut tx,"fresh-key","asset.create","user",&company).await.unwrap().selects_history());sqlx::query("ROLLBACK TO SAVEPOINT own_receipt").execute(&mut *tx).await.unwrap();fresh.require_unchanged_before_effects(&mut tx).await.unwrap();
        // HoldingWrite migrations include an expression index casting authority
        // JSON. This is a real migrated schema, not a hand-built receipt table:
        // malformed authority is rejected on INSERT before a lookup can see it.
        let authority_index:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid='public.command_receipts'::regclass AND pg_get_indexdef(i.indexrelid) LIKE '%authority_json%' AND pg_get_indexdef(i.indexrelid) LIKE '%jsonb%')").fetch_one(&mut *tx).await.unwrap();assert!(authority_index,"the migrated authority JSON expression index must remain present");
        sqlx::query("SAVEPOINT malformed_authority").execute(&mut *tx).await.unwrap();let insertion=sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('bad-authority','asset.create','user','hash','target','{}','{bad',0)").execute(&mut *tx).await.unwrap_err();assert_eq!(insertion.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));sqlx::query("ROLLBACK TO SAVEPOINT malformed_authority").execute(&mut *tx).await.unwrap();
        tx.rollback().await.unwrap();let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_asset_purchase_receipt_sources(&mut other).await.unwrap();assert!(fresh.require_unchanged_before_effects(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_receipt_lock").execute(&mut *saved).await.unwrap();lock_asset_purchase_receipt_sources(&mut saved).await.unwrap();let retained=inspect_asset_purchase_receipt_purpose(&mut saved,"fresh-key","asset.create","user",&company).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_receipt_lock").execute(&mut *saved).await.unwrap();assert!(retained.require_unchanged_before_effects(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.command_receipts IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let e=lock_asset_purchase_receipt_sources(&mut blocked).await;assert!(matches!(e,Err(PgTransactionError::Persistence(ref p)) if p.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && p.sql_state()==Some("55P03") && p.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn quality_return_receipts_match_actual_typescript_join_and_retain_fence() {
    use erpx_server_shared::public::{CompanyId, RecordId, UserId};
    use erpx_server_workflow::public::quality_return_receipts::{
        QualityReturnReceiptScope, inspect_quality_return_receipts, lock_quality_return_receipts,
    };
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        let script=r#"
if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');
const {commerceQualityOriginRows}=await import('./packages/server-commerce/src/adapters/quality-purchase-sources.ts');
const matches=[...commerceQualityOriginRows.matchAll(/JOIN command_receipts receipt ON ([^\n]+)/g)].map(x=>x[1]);
if(matches.length!==2||matches[0]!==matches[1])throw Error('QUALITY_RETURN_TS_JOIN_CHANGED');
process.stdout.write(matches[0]);
"#;
        let out=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).arg("--conditions=react-server").arg("-e").arg(script).output().unwrap();assert!(out.status.success(),"actual Commerce Quality TS join unavailable");let predicate=String::from_utf8(out.stdout).unwrap();
        let company=CompanyId::new("company").unwrap();
        let return_id=RecordId::new("return").unwrap();
        let reviewer=UserId::new("reviewer").unwrap();
        let scope=QualityReturnReceiptScope{company:&company,return_id:&return_id,actor:&reviewer};
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_quality_return_receipts(&mut unlocked,scope).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_quality_return_receipts(&mut tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('return-a','commerce.return.approve','reviewer','synthetic','return','{}','{\"companyId\":\"company\"}',0),('return-b','commerce.return.approve','reviewer','synthetic','return','{}','{\"companyId\":\"company\"}',0),('return-wrong-actor','commerce.return.approve','other','synthetic','return','{}','{\"companyId\":\"company\"}',0),('return-wrong-company','commerce.return.approve','reviewer','synthetic','return','{}','{\"companyId\":\"foreign\"}',0),('return-wrong-record','commerce.return.approve','reviewer','synthetic','other','{}','{\"companyId\":\"company\"}',0),('return-other-op','other.operation','reviewer','synthetic','return','{}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();
        let source=inspect_quality_return_receipts(&mut tx,scope).await.unwrap();
        let prefix="(SELECT 'return'::text id,'reviewer'::text approved_by,'company'::text company_id) r";
        let original=format!("SELECT receipt.command_id FROM {prefix} JOIN command_receipts receipt ON {predicate} ORDER BY receipt.command_id");
        let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(original)).fetch_all(&mut *tx).await.unwrap();
        let actual:Vec<String>=source.command_ids().map(str::to_owned).collect();
        assert_eq!(expected,vec!["return-a","return-b"]);assert_eq!(actual,expected);source.require_current(&mut tx).await.unwrap();
        // Unrelated history in the same partitioned receipt table must not consume
        // the scoped row/byte budget or change the selected source.
        sqlx::query("SAVEPOINT receipt_unrelated").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) SELECT 'quality-unrelated-'||n,'commerce.return.approve','reviewer','synthetic','unrelated','{}','{\"companyId\":\"company\"}',0 FROM generate_series(1,10001) n").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT receipt_unrelated").execute(&mut *tx).await.unwrap();
        // A new matching receipt changes the retained source and must conflict.
        sqlx::query("SAVEPOINT receipt_drift").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('return-c','commerce.return.approve','reviewer','synthetic','return','{}','{\"companyId\":\"company\"}',0)").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT receipt_drift").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        // A matching oversized authority row fails the scoped byte budget.
        sqlx::query("SAVEPOINT receipt_limit").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('return-huge','commerce.return.approve','reviewer','synthetic','return','{}',jsonb_build_object('companyId','company','blob',repeat('x',2097153))::text,0)").execute(&mut *tx).await.unwrap();
        assert!(matches!(source.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e)) if e.code=="QUALITY_RETURN_RECEIPT_LIMIT"));
        sqlx::query("ROLLBACK TO SAVEPOINT receipt_limit").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT receipt_shadow").execute(&mut *tx).await.unwrap();sqlx::query("CREATE TEMP TABLE command_receipts(command_id text)").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT receipt_shadow").execute(&mut *tx).await.unwrap();source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;
}
