#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_legal::public::{
    LegalProcessingActor, LegalProcessingCursor, LegalProcessingDocument, LegalProcessingJob,
    LegalProcessingKind, LegalProcessingLease, LegalProcessingQueueAuthority,
    LegalProcessingReceipt, LegalProcessingSourceRequest, PgLegalProcessingBridge,
    PgLegalProcessingRepository, load_current_legal_processing_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
const NOW: i64 = 100_000;
struct FixedClock;
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(NOW)
    }
}
#[derive(Default)]
struct Ids(u32);
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.0 += 1;
        id(&format!("lease-{}", self.0))
    }
}
fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}
fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}
fn actor() -> LegalProcessingActor {
    LegalProcessingActor {
        holding_id: HoldingId::new("holding-a").unwrap(),
        company_id: CompanyId::new("company-a").unwrap(),
        user_id: erpx_server_shared::public::UserId::new("requester").unwrap(),
        selection_revision: "selection-current".into(),
    }
}
fn denied() -> Error {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_PERMISSION_REQUIRED",
        "Current owner permission required.",
    )
    .into()
}

/// Explicit synthetic foreign-owner bridges. These prove Legal's transaction
/// composition; they do not establish production IAM or purchased capability.
#[derive(Default)]
struct Bridge {
    deny: bool,
    deny_completion: bool,
    fail_receipt: bool,
}
impl PgLegalProcessingBridge for Bridge {
    async fn authorize_queue(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &CommandKey,
        _: &LegalProcessingDocument,
        _: LegalProcessingKind,
    ) -> Result<LegalProcessingQueueAuthority, Error> {
        if self.deny {
            return Err(denied());
        }
        Ok(LegalProcessingQueueAuthority {
            revision: "authority-current".into(),
            actor_snapshot_json:
                r#"{"userId":"requester","companyId":"company-a","assignmentId":"current"}"#.into(),
        })
    }
    async fn replay(
        &mut self,
        connection: &mut PgConnection,
        _: &LegalProcessingActor,
        key: &CommandKey,
        document_id: &RecordId,
        kind: LegalProcessingKind,
    ) -> Result<Option<LegalProcessingReceipt>, Error> {
        let row: Option<(String, String, String)> =
            sqlx::query_as("SELECT document_id,kind,receipt FROM receipt_probe WHERE id=$1")
                .bind(key.as_str())
                .fetch_optional(connection)
                .await?;
        row.map(|(document, stored_kind, json)| {
            if document != document_id.as_str() || stored_kind != kind.as_str() {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "COMMAND_KEY_REUSED",
                    "Command key was reused.",
                )
                .into());
            }
            serde_json::from_str(&json).map_err(|_| denied())
        })
        .transpose()
    }
    async fn record_queued(
        &mut self,
        connection: &mut PgConnection,
        _: &LegalProcessingActor,
        key: &CommandKey,
        document: &LegalProcessingDocument,
        kind: LegalProcessingKind,
        receipt: &LegalProcessingReceipt,
    ) -> Result<(), Error> {
        sqlx::query("INSERT INTO receipt_probe(id,document_id,kind,receipt)VALUES($1,$2,$3,$4)")
            .bind(key.as_str())
            .bind(document.document_id.as_str())
            .bind(kind.as_str())
            .bind(serde_json::to_string(receipt).unwrap())
            .execute(&mut *connection)
            .await?;
        if self.fail_receipt {
            sqlx::query("INSERT INTO receipt_probe SELECT * FROM receipt_probe WHERE id=$1")
                .bind(key.as_str())
                .execute(connection)
                .await?;
        }
        Ok(())
    }
    async fn candidate_eligible(
        &mut self,
        _: &mut PgConnection,
        job: &LegalProcessingJob,
    ) -> Result<bool, Error> {
        Ok(!self.deny && !job.id.as_str().starts_with("blocked"))
    }
    async fn authorize_completion(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingLease,
    ) -> Result<(), Error> {
        if self.deny || self.deny_completion {
            Err(denied())
        } else {
            Ok(())
        }
    }
    async fn authorize_status(
        &mut self,
        _: &mut PgConnection,
        _: &LegalProcessingActor,
        _: &RecordId,
    ) -> Result<(), Error> {
        if self.deny { Err(denied()) } else { Ok(()) }
    }
}
fn repository<'c>(
    connection: &'c mut PgConnection,
    bridge: &'c mut Bridge,
    ids: &'c mut Ids,
) -> PgLegalProcessingRepository<'c, Bridge, FixedClock, Ids> {
    PgLegalProcessingRepository::new(
        HoldingId::new("holding-a").unwrap(),
        CompanyId::new("company-a").unwrap(),
        connection,
        bridge,
        &FixedClock,
        ids,
    )
}
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE receipt_probe(id text PRIMARY KEY,document_id text NOT NULL,kind text NOT NULL,receipt text NOT NULL);
INSERT INTO organization_nodes(id,kind,slug,display_name,company_id,status)VALUES('company-a','company','a','Synthetic A','company-a','active'),('company-b','company','b','Synthetic B','company-b','active');
INSERT INTO legal_settings(company_id,enabled)VALUES('company-a',true),('company-b',true);
INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('item-a','company-a','contract','Synthetic A','internal','TH','TH','en','THB','Asia/Bangkok','normal','requester','requester',0,0),('item-b','company-b','contract','Synthetic B','internal','TH','TH','en','THB','Asia/Bangkok','normal','requester','requester',0,0)").execute(pool.raw()).await.unwrap();
    for company in ["a", "b"] {
        sqlx::query("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at)VALUES($1,$2,1,'synthetic.pdf','application/pdf',$3,1,'x','requester',0)").bind(format!("document-{company}")).bind(format!("item-{company}")).bind("a".repeat(64)).execute(pool.raw()).await.unwrap();
    }
}
async fn seed_job(
    pool: &DatabasePool,
    name: &str,
    company: &str,
    state: &str,
    attempts: i32,
    until: Option<i64>,
) {
    sqlx::query("INSERT INTO legal_processing_jobs(id,company_id,item_id,document_id,checksum,kind,state,attempts,lease_token,lease_until,actor_json,created_by,created_at)VALUES($1,$2,$3,$4,$5,'verify',$6,$7,$8,$9,'{}','requester',0)").bind(name).bind(format!("company-{company}")).bind(format!("item-{company}")).bind(format!("document-{company}")).bind("a".repeat(64)).bind(state).bind(attempts).bind(until.map(|_|"previous-token")).bind(until).execute(pool.raw()).await.unwrap();
}
async fn claim(uow: &PgUnitOfWork) -> LegalProcessingLease {
    uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
        Box::pin(async move {
            let mut bridge = Bridge::default();
            let mut ids = Ids::default();
            Ok(repository(connection, &mut bridge, &mut ids)
                .claim(None)
                .await?
                .lease
                .unwrap())
        })
    })
    .await
    .unwrap()
}
async fn state(pool: &DatabasePool, job: &str) -> String {
    sqlx::query_scalar("SELECT state FROM legal_processing_jobs WHERE id=$1")
        .bind(job)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}
async fn inspection(
    connection: &mut PgConnection,
    lease: &LegalProcessingLease,
    name: &str,
    matching: bool,
) -> Result<(), Error> {
    let result = serde_json::json!({"status":"indeterminate","inspectionId":name,"processingJobId":if matching{lease.job.id.as_str()}else{"another-job"},"checksum":lease.job.document.checksum_sha256,"reasons":["VERIFIER_DEADLINE_EXCEEDED"]});
    sqlx::query("INSERT INTO legal_document_inspections(id,document_id,kind,result_json,created_by,created_at)VALUES($1,$2,'verify',$3,'requester',$4)").bind(name).bind(lease.job.document.document_id.as_str()).bind(result.to_string()).bind(NOW).execute(connection).await?;
    Ok(())
}

#[tokio::test]
async fn queue_and_workflow_receipt_share_one_transaction_and_current_authority() {
    fixture::with_database(2,|pool|async move{setup(&pool).await;let uow=PgUnitOfWork::new(pool.clone());
uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move{let mut bridge=Bridge::default();let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).queue(&actor(),&key("processing-first"),&id("document-a"),LegalProcessingKind::Verify).await})).await.unwrap();
let rollback=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move{let mut bridge=Bridge{fail_receipt:true,..Default::default()};let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).queue(&actor(),&key("processing-rollback"),&id("document-a"),LegalProcessingKind::Verify).await})).await;assert!(matches!(rollback,Err(PgTransactionError::Persistence(ref error))if error.sql_state()==Some("23505")));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_processing_jobs").fetch_one(pool.raw()).await.unwrap(),1);
let denied=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move{let mut bridge=Bridge{deny:true,..Default::default()};let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).queue(&actor(),&key("processing-first"),&id("document-a"),LegalProcessingKind::Verify).await})).await;assert!(matches!(denied,Err(PgTransactionError::Business(ref error))if error.code=="LEGAL_PERMISSION_REQUIRED"));
for index in 0..19{seed_job(&pool,&format!("recent-{index}"),"a","queued",0,None).await;}sqlx::query("UPDATE legal_processing_jobs SET created_at=$1 WHERE id LIKE 'recent-%'").bind(NOW).execute(pool.raw()).await.unwrap();
for (name,allowed) in [("processing-first",true),("processing-overflow",false)]{let name=name.to_owned();let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|{let name=name.clone();Box::pin(async move{let mut bridge=Bridge::default();let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).queue(&actor(),&key(&name),&id("document-a"),LegalProcessingKind::Verify).await})}).await;assert_eq!(result.is_ok(),allowed);}
let collision=uow.run::<_,BusinessError,_>(TransactionOptions::default(),|connection|Box::pin(async move{let mut bridge=Bridge::default();let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).queue(&actor(),&key("processing-first"),&id("document-a"),LegalProcessingKind::Ocr).await})).await;assert!(matches!(collision,Err(PgTransactionError::Business(ref error))if error.code=="COMMAND_KEY_REUSED"));
}).await;
}

#[tokio::test]
async fn recovery_is_company_scoped_and_old_tokens_cannot_overwrite_reclaimed_work() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_job(&pool, "retry", "a", "running", 1, Some(NOW - 1)).await;
        seed_job(&pool, "terminal", "a", "running", 3, Some(NOW - 1)).await;
        seed_job(&pool, "foreign", "b", "running", 1, Some(NOW - 1)).await;
        let uow = PgUnitOfWork::new(pool.clone());
        let recovered = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut bridge = Bridge::default();
                    let mut ids = Ids::default();
                    repository(connection, &mut bridge, &mut ids)
                        .recover_expired()
                        .await
                })
            })
            .await
            .unwrap();
        assert_eq!(recovered, 2);
        assert_eq!(state(&pool, "terminal").await, "failed");
        assert_eq!(state(&pool, "foreign").await, "running");
        let first = claim(&uow).await;
        assert_eq!(first.attempts, 2);
        sqlx::query("UPDATE legal_processing_jobs SET lease_until=$1 WHERE id='retry'")
            .bind(NOW - 1)
            .execute(pool.raw())
            .await
            .unwrap();
        let second = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut bridge = Bridge::default();
                    let mut ids = Ids(100);
                    let mut repository = repository(connection, &mut bridge, &mut ids);
                    repository.recover_expired().await?;
                    Ok(repository.claim(None).await?.lease.unwrap())
                })
            })
            .await
            .unwrap();
        assert_eq!(second.attempts, 3);
        assert_ne!(first.token, second.token);
        let stale = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let first = first.clone();
                Box::pin(async move {
                    let mut bridge = Bridge::default();
                    let mut ids = Ids::default();
                    repository(connection, &mut bridge, &mut ids)
                        .fail(&first, "DOCUMENT_PROCESSING_FAILED")
                        .await
                })
            })
            .await;
        assert!(stale.is_err());
        assert_eq!(state(&pool, "retry").await, "running");
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
            let second = second.clone();
            Box::pin(async move {
                let mut bridge = Bridge::default();
                let mut ids = Ids::default();
                repository(connection, &mut bridge, &mut ids)
                    .fail(&second, "DOCUMENT_PROCESSING_FAILED")
                    .await
            })
        })
        .await
        .unwrap();
        assert_eq!(state(&pool, "retry").await, "failed");
    })
    .await;
}

#[tokio::test]
async fn bounded_cursor_and_skip_locked_preserve_progress_past_ineligible_work() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        for index in 0..65 {
            seed_job(
                &pool,
                &format!("blocked-{index:03}"),
                "a",
                "queued",
                0,
                None,
            )
            .await;
        }
        seed_job(&pool, "eligible", "a", "queued", 0, None).await;
        let uow = PgUnitOfWork::new(pool.clone());
        let first = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut bridge = Bridge::default();
                    let mut ids = Ids::default();
                    repository(connection, &mut bridge, &mut ids)
                        .claim(None)
                        .await
                })
            })
            .await
            .unwrap();
        assert!(first.lease.is_none());
        let cursor: LegalProcessingCursor = first.next_cursor.unwrap();
        assert_eq!(cursor.id.as_str(), "blocked-063");
        let next = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let cursor = cursor.clone();
                Box::pin(async move {
                    let mut bridge = Bridge::default();
                    let mut ids = Ids::default();
                    repository(connection, &mut bridge, &mut ids)
                        .claim(Some(&cursor))
                        .await
                })
            })
            .await
            .unwrap();
        assert_eq!(next.lease.unwrap().job.id.as_str(), "eligible");
        sqlx::query("DELETE FROM legal_processing_jobs")
            .execute(pool.raw())
            .await
            .unwrap();
        for name in ["one", "two"] {
            seed_job(&pool, name, "a", "queued", 0, None).await;
        }
        let mut left = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *left)
            .await
            .unwrap();
        let mut right = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *right)
            .await
            .unwrap();
        // Retain only the competing job row. A real claim must commit before
        // expensive work; Holding cache-revision triggers serialize updates.
        sqlx::query("SELECT id FROM legal_processing_jobs WHERE id='one' FOR UPDATE")
            .fetch_one(&mut *left)
            .await
            .unwrap();
        let mut bridge = Bridge::default();
        let mut ids = Ids(100);
        let two = repository(&mut right, &mut bridge, &mut ids)
            .claim(None)
            .await
            .unwrap()
            .lease
            .unwrap();
        assert_eq!(two.job.id.as_str(), "two");
        left.rollback().await.unwrap();
        right.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn completion_requires_current_source_real_matching_inspection_and_outer_commit() {
    fixture::with_database(2,|pool|async move{setup(&pool).await;seed_job(&pool,"job","a","queued",0,None).await;let uow=PgUnitOfWork::new(pool.clone());let lease=claim(&uow).await;
for mode in ["outer-rollback","changed-authority","wrong-job","changed-source"]{let lease=lease.clone();let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|{let lease=lease.clone();Box::pin(async move{inspection(connection,&lease,"inspection",mode!="wrong-job").await?;if mode=="changed-source"{sqlx::query("UPDATE legal_documents SET checksum=$1 WHERE id='document-a'").bind("b".repeat(64)).execute(&mut *connection).await?;}let mut bridge=Bridge{deny_completion:mode=="changed-authority",..Default::default()};let mut ids=Ids::default();let result=repository(connection,&mut bridge,&mut ids).complete(&lease,&id("inspection")).await?;if mode=="outer-rollback"{sqlx::query("INSERT INTO legal_document_inspections SELECT * FROM legal_document_inspections WHERE id='inspection'").execute(connection).await?;}Ok(result)})}).await;assert!(result.is_err(),"{mode}");assert_eq!(state(&pool,"job").await,"running");assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM legal_document_inspections").fetch_one(pool.raw()).await.unwrap(),0);}
let retained=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection| {
    let lease=lease.clone();
    Box::pin(async move {
        let source=load_current_legal_processing_source(connection,source_request(&lease),NOW).await?;
        inspection(connection,source.lease(),"inspection",true).await?;
        let mut bridge=Bridge::default();
        let mut ids=Ids::default();
        let result=repository(connection,&mut bridge,&mut ids).complete_current_source(&source,&id("inspection")).await?;
        assert!(source.require_current(connection,NOW).await.is_err());
        source.require_current_or_completed(connection,NOW).await?;
        Ok(result)
    })
}).await.unwrap();assert_eq!(retained["status"],"indeterminate");assert_eq!(state(&pool,"job").await,"completed");
for deny in [false,true]{let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|connection|Box::pin(async move{let mut bridge=Bridge{deny,..Default::default()};let mut ids=Ids::default();repository(connection,&mut bridge,&mut ids).status(&actor(),&id("job")).await})).await;assert_eq!(result.is_err(),deny);if let Ok(result)=result{assert_eq!(result.result.unwrap()["inspectionId"],"inspection");}}
}).await;
}

fn source_request(lease: &LegalProcessingLease) -> LegalProcessingSourceRequest {
    LegalProcessingSourceRequest {
        holding_id: lease.job.holding_id.clone(),
        company_id: lease.job.company_id.clone(),
        job_id: lease.job.id.clone(),
        lease_token: lease.token.clone(),
    }
}

#[tokio::test]
async fn opaque_processing_source_binds_the_real_lease_and_exact_owner_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_job(&pool,"job","a","queued",0,None).await;
        let uow=PgUnitOfWork::new(pool.clone());
        let lease=claim(&uow).await;
        let mut connection=pool.raw().acquire().await.unwrap();
        assert!(load_current_legal_processing_source(&mut connection,source_request(&lease),NOW).await.is_err());
        drop(connection);
        let source=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
            let lease=lease.clone();
            Box::pin(async move {
                let source=load_current_legal_processing_source(connection,source_request(&lease),NOW).await?;
                assert_eq!(source.user_id().as_str(),"requester");
                assert_eq!(source.company_id().as_str(),"company-a");
                assert_eq!(source.deadline(),lease.until);
                // This fixture intentionally lacks Identity pins. The owner
                // proof grants no permission and must not infer missing pins.
                assert_eq!(source.actor_snapshot_json(),"{}");
                source.require_current(connection,NOW).await?;
                assert!(source.require_current(connection,lease.until+1).await.is_err());
                Ok(source)
            })
        }).await.unwrap();
        let source=std::sync::Arc::new(source);
        let current_source=source.clone();
        let replayed=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move |connection| {
            let source=source.clone();
            Box::pin(async move {source.require_current(connection,NOW).await})
        }).await;
        assert!(matches!(replayed,Err(PgTransactionError::Business(ref error)) if error.code=="LEGAL_PROCESSING_LEASE_CHANGED"));
        let unproved=uow.run::<(),BusinessError,_>(TransactionOptions::default(),move|connection|{
            let previous=current_source.clone();
            Box::pin(async move {
                let source=load_current_legal_processing_source(connection,source_request(previous.lease()),NOW).await?;
                sqlx::query("UPDATE legal_processing_jobs SET state='completed',lease_token=NULL,lease_until=NULL,result_json='{}' WHERE id='job'").execute(&mut *connection).await?;
                source.require_current_or_completed(connection,NOW).await
            })
        }).await;
        assert!(unproved.is_err(),"a completed flag cannot mint private owner completion proof");
        assert_eq!(state(&pool,"job").await,"running");
    }).await;
}

#[tokio::test]
async fn denial_committed_while_owner_lock_waits_is_seen_before_the_first_snapshot() {
    fixture::with_database(3,|pool|async move {
        setup(&pool).await;
        seed_job(&pool,"job","a","queued",0,None).await;
        let lease=claim(&PgUnitOfWork::new(pool.clone())).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("INSERT INTO legal_destructions(id,item_id,policy_id,policy_version,requested_by,state,reason,manifest,created_at)VALUES('destruction','item-a','synthetic-policy',1,'requester','executing','Synthetic denial','[]',0)")
            .execute(&mut *writer).await.unwrap();
        let (sent,received)=tokio::sync::oneshot::channel();
        let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {
            let mut connection=reader_pool.raw().acquire().await.unwrap();
            // Discover the owned PID outside the transaction under test.
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            let mut transaction=connection.begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *transaction).await.unwrap();
            sent.send(pid).unwrap();
            let result=load_current_legal_processing_source(&mut transaction,source_request(&lease),NOW).await.map(|_|());
            transaction.rollback().await.unwrap();
            result
        });
        let pid=received.await.unwrap();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='legal_destructions'::regclass AND mode='ShareLock' AND NOT granted)").bind(pid).fetch_one(pool.raw()).await.unwrap();
            if waiting {break;}
            assert!(std::time::Instant::now()<deadline,"owned reader never waited for the denial utility lock");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        writer.commit().await.unwrap();
        let result=reader.await.unwrap();
        assert!(matches!(result,Err(PgTransactionError::Business(ref error)) if error.code=="LEGAL_PROCESSING_LEASE_CHANGED"),"the new destruction must be seen after lock acquisition");
    }).await;
}

#[tokio::test]
async fn staged_queue_plan_rejects_same_backend_next_transaction_and_uses_the_canonical_application()
 {
    fixture::with_database(1,|pool|async move{
        setup(&pool).await;let mut bridge=Bridge::default();let mut ids=Ids::default();
        let mut first=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *first).await.unwrap();
        let plan=repository(&mut first,&mut bridge,&mut ids).stage_queue(&actor(),&key("staged-command"),&id("document-a"),LegalProcessingKind::Ocr).await.unwrap();
        first.commit().await.unwrap();
        let mut second=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *second).await.unwrap();
        assert!(matches!(repository(&mut second,&mut bridge,&mut ids).apply_staged_queue(&plan).await,Err(PgTransactionError::Business(_))));second.rollback().await.unwrap();
        let mut current=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *current).await.unwrap();
        let actual=repository(&mut current,&mut bridge,&mut ids).stage_queue(&actor(),&key("staged-command"),&id("document-a"),LegalProcessingKind::Ocr).await.unwrap();
        let receipt=repository(&mut current,&mut bridge,&mut ids).apply_staged_queue(&actual).await.unwrap();assert_eq!(receipt,actual.receipt().unwrap());actual.require_queued(&mut current).await.unwrap();current.rollback().await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT(SELECT count(*)FROM legal_processing_jobs)+(SELECT count(*)FROM receipt_probe)").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}
