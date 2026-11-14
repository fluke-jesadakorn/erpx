#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
#[path = "../../support/rust-directory-provenance-facts.rs"]
mod source_facts;
use auth::*;
use erpx_server_identity_access::public::effective_authority::{
    PgQueuedWorkAuthorizer, PgQueuedWorkSourcePort, QueuedWorkBinding,
};
use erpx_server_runtime::public::postgres::PgWorkFuture;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, UserId};
use erpx_server_shared::public::{FailureKind, HoldingId};
use sqlx::{PgConnection, Row};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

#[tokio::test]
async fn quality_same_person_retains_exact_company_mapping_and_unit() {
    use erpx_server_identity_access::public::quality_same_person::{inspect_quality_same_person, lock_quality_same_person_sources};
    use erpx_server_shared::public::CompanyId;
    use sqlx::Connection;
    fixture::with_database(2, |control| async move { fixture::with_database(3, move |holding| async move {
        setup(&control, &holding).await;
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES ('company','quality-a','local:quality-a','shared-person','active'),('company','quality-b','local:quality-b','shared-person','suspended'),('company','quality-null','local:quality-null',NULL,'active'),('foreign','quality-a','foreign:a','different-person','active'),('foreign','quality-b','foreign:b','other-person','active')").execute(holding.raw()).await.unwrap();
        let company = CompanyId::new("company").unwrap();
        let mut tx = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        let locks = lock_quality_same_person_sources(&mut tx).await.unwrap().bind().await.unwrap();
        let same = inspect_quality_same_person(&mut tx, &locks, &company, "quality-a", "quality-b").await.unwrap();
        assert!(same.same_person(), "iam_same_person ignores member status");
        let self_without_member = inspect_quality_same_person(&mut tx, &locks, &company, "missing-user", "missing-user").await.unwrap();
        assert!(self_without_member.same_person());
        let missing = inspect_quality_same_person(&mut tx, &locks, &company, "quality-a", "missing-user").await.unwrap();
        assert!(!missing.same_person());
        let null_person = inspect_quality_same_person(&mut tx, &locks, &company, "quality-a", "quality-null").await.unwrap();
        assert!(!null_person.same_person());
        let foreign = inspect_quality_same_person(&mut tx, &locks, &CompanyId::new("foreign").unwrap(), "quality-a", "quality-b").await.unwrap();
        assert!(!foreign.same_person());
        sqlx::query("SAVEPOINT changed_members").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE iam_members SET person_id='new-shared-person' WHERE company_id='company' AND user_id IN ('quality-a','quality-b')").execute(&mut *tx).await.unwrap();
        // Even if the boolean remains true, the retained source mapping changed.
        assert!(same.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT changed_members").execute(&mut *tx).await.unwrap();
        same.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
        let no_lock = inspect_quality_same_person(&mut other, &locks, &company, "quality-a", "quality-b").await;
        assert!(matches!(no_lock, Err(PgTransactionError::Persistence(e)) if e.code == "IAM_SAME_PERSON_LOCK_REQUIRED"));
        other.rollback().await.unwrap();
        let mut third = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *third).await.unwrap();
        let _other_locks = lock_quality_same_person_sources(&mut third).await.unwrap().bind().await.unwrap();
        assert!(same.require_current(&mut third).await.is_err());
        third.rollback().await.unwrap();
    }).await; }).await;
}

#[tokio::test]
async fn quality_same_person_writer_contention_is_scoped_and_fresh() {
    use erpx_server_identity_access::public::quality_same_person::{inspect_quality_same_person, lock_quality_same_person_sources};
    use erpx_server_shared::public::CompanyId;
    use sqlx::Connection;
    fixture::with_database(2, |control| async move { fixture::with_database(3, move |holding| async move {
        setup(&control, &holding).await;
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status) VALUES ('company','quality-a','local:quality-a','shared-person','active'),('company','quality-b','local:quality-b','shared-person','active')").execute(holding.raw()).await.unwrap();
        let company = CompanyId::new("company").unwrap();
        let mut writer = holding.raw().begin().await.unwrap();
        sqlx::query("UPDATE iam_members SET person_id='new-person' WHERE company_id='company' AND user_id='quality-b'").execute(&mut *writer).await.unwrap();
        let mut blocked = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        let error = lock_quality_same_person_sources(&mut blocked).await;
        assert!(matches!(error, Err(PgTransactionError::Persistence(e)) if e.code == "PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state() == Some("55P03") && e.retryable()));
        blocked.rollback().await.unwrap();
        writer.commit().await.unwrap();
        let mut reader = holding.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let locks = lock_quality_same_person_sources(&mut reader).await.unwrap().bind().await.unwrap();
        let witness = inspect_quality_same_person(&mut reader, &locks, &company, "quality-a", "quality-b").await.unwrap();
        assert!(!witness.same_person());
        let reader_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let cloned = holding.clone();
        let (send, receive) = tokio::sync::oneshot::channel();
        let pending = tokio::spawn(async move {
            let mut connection = cloned.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *connection).await.unwrap();
            send.send(pid).unwrap();
            sqlx::query("UPDATE iam_members SET person_id='shared-person' WHERE company_id='company' AND user_id='quality-b'").execute(&mut *connection).await.unwrap();
        });
        let writer_pid = receive.await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let blocked: bool = sqlx::query_scalar("SELECT $2=ANY(pg_blocking_pids($1))").bind(writer_pid).bind(reader_pid).fetch_one(holding.raw()).await.unwrap();
                if blocked { break; }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("Identity writer must wait for the retained reader");
        witness.require_current(&mut reader).await.unwrap();
        reader.rollback().await.unwrap();
        pending.await.unwrap();
    }).await; }).await;
}

#[tokio::test]
async fn permission_catalog_utility_marks_only_its_nowait_conflict_and_retains_lock() {
    use erpx_server_identity_access::public::permission_catalog_lock::lock_permission_catalog_sources;
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        setup(&control,&holding).await;
        let mut writer=holding.raw().begin().await.unwrap();
        sqlx::query("UPDATE iam_permissions SET definition=definition WHERE id='quality.inspect'").execute(&mut *writer).await.unwrap();
        let mut blocked=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        let error=lock_permission_catalog_sources(&mut blocked).await.unwrap_err();
        assert!(matches!(error,PgTransactionError::Persistence(ref e) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        blocked.rollback().await.unwrap();writer.commit().await.unwrap();
        let mut guard=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *guard).await.unwrap();
        lock_permission_catalog_sources(&mut guard).await.unwrap();
        let guard_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *guard).await.unwrap();
        let pool=holding.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {let mut tx=pool.raw().begin().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();send.send(pid).unwrap();sqlx::query("UPDATE iam_permissions SET definition=definition WHERE id='quality.inspect'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();});
        let writer_pid=receive.await.unwrap();tokio::time::timeout(Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(guard_pid).bind(writer_pid).fetch_one(holding.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await.expect("catalog writer did not wait for permission source lock");
        guard.rollback().await.unwrap();writer.await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn selected_quality_recipients_require_real_presnapshot_catalog_lock() {
    use erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation;
    use erpx_server_identity_access::public::quality_recipients::{QualityRecipientRequest,QualityRecipientStatus,inspect_quality_recipients_for_selected,lock_quality_recipient_sources};
    use erpx_server_shared::public::{CompanyId,RecordId};
    #[derive(Clone)] struct QualitySources;
    impl HoldingWorkSourcePreparation for QualitySources {
        fn prepare<'a>(&'a self,c:&'a mut PgConnection)->PgWorkFuture<'a,(),BusinessError> {
            Box::pin(async move {let guard=lock_quality_recipient_sources(c).await?;drop(guard);Ok(())})
        }
    }
    #[derive(Clone)] struct StrongQualitySources;
    impl HoldingWorkSourcePreparation for StrongQualitySources {
        fn prepare<'a>(&'a self,c:&'a mut PgConnection)->PgWorkFuture<'a,(),BusinessError> {
            Box::pin(async move {sqlx::query("LOCK TABLE iam_permissions IN SHARE ROW EXCLUSIVE MODE NOWAIT").execute(c).await?;Ok(())})
        }
    }
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,_)=setup(&control,&holding).await;
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"finance.read\",\"finance.write\",\"module.finance.read\",\"quality.read\",\"quality.inspect\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'; INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('selected-dept','department','selected','Selected','branch','company','branch','active')").execute(holding.raw()).await.unwrap();
        let request=|| QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
        authorizer.execute("acme.example.test",proof(),input(),access(),|c,current|Box::pin(async move {
            let request=QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
            let error=inspect_quality_recipients_for_selected(c,current,request,NOW).await.err().unwrap();
            assert!(matches!(error,PgTransactionError::Business(e) if e.code=="QUALITY_RECIPIENT_LOCK_REQUIRED"));Ok(())
        })).await.unwrap();
        let other_pool=holding.clone();
        authorizer.execute_with_sources("acme.example.test",proof(),input(),access(),QualitySources,move|c,current|{let other_pool=other_pool.clone();Box::pin(async move {
            let request=QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
            let witness=inspect_quality_recipients_for_selected(c,current,request.clone(),NOW).await?;
            assert_eq!(witness.recipients(), &["user".to_string()]);witness.require_current(c,NOW).await?;
            let foreign=QualityRecipientRequest {company_id:CompanyId::new("foreign").unwrap(),..request.clone()};
            assert!(inspect_quality_recipients_for_selected(c,current,foreign,NOW).await.is_err());
            let mut other=other_pool.raw().begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(witness.require_current(&mut other,NOW).await.is_err());other.rollback().await?;Ok(())
        })}).await.unwrap();
        authorizer.execute_with_sources("acme.example.test",proof(),input(),access(),StrongQualitySources,|c,current|Box::pin(async move {
            let request=QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
            let source=inspect_quality_recipients_for_selected(c,current,request,NOW).await?;
            assert_eq!(source.recipients(), &["user".to_string()]);source.require_current(c,NOW).await?;Ok(())
        })).await.unwrap();
        let mut branch_input=input();branch_input.branch_slug=Some("north".into());
        authorizer.execute_with_sources("acme.example.test",proof(),branch_input,access(),QualitySources,|c,current|Box::pin(async move {
            let request=QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
            assert!(inspect_quality_recipients_for_selected(c,current,request.clone(),NOW).await.is_ok());
            assert!(inspect_quality_recipients_for_selected(c,current,QualityRecipientRequest {branch_id:RecordId::new("other").unwrap(),..request},NOW).await.is_err());Ok(())
        })).await.unwrap();
        let mut department_input=input();department_input.branch_slug=Some("north".into());department_input.department_slug=Some("selected".into());
        authorizer.execute_with_sources("acme.example.test",proof(),department_input,access(),QualitySources,move|c,current|Box::pin(async move {
            assert!(inspect_quality_recipients_for_selected(c,current,request(),NOW).await.is_err());Ok(())
        })).await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn quality_recipient_permissions_first_writer_forces_scoped_retry() {
    use erpx_server_identity_access::public::quality_recipients::{QualityRecipientRequest,QualityRecipientStatus,inspect_quality_recipients,lock_quality_recipient_sources};
    use erpx_server_runtime::public::postgres::{PgUnitOfWork,TransactionOptions};
    use erpx_server_shared::public::{CompanyId,RecordId};
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        setup(&control,&holding).await;
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"quality.inspect\"]','[]','[\"holding\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'").execute(holding.raw()).await.unwrap();
        let mut writer=holding.raw().begin().await.unwrap();
        sqlx::query("UPDATE iam_permissions SET definition=jsonb_set(definition,'{availability}','\"planned\"'::jsonb) WHERE id='quality.inspect'").execute(&mut *writer).await.unwrap();
        let pool=holding.clone();let attempts=Arc::new(AtomicUsize::new(0));let owner_attempts=attempts.clone();let (send,mut receive)=tokio::sync::mpsc::unbounded_channel();
        let owner=tokio::spawn(async move {PgUnitOfWork::new(pool).run::<Vec<String>,BusinessError,_>(TransactionOptions::default(),move|c| {
            let attempts=owner_attempts.clone();let send=send.clone();Box::pin(async move {
                attempts.fetch_add(1,Ordering::SeqCst);
                let prep=match lock_quality_recipient_sources(c).await { Ok(prep)=>prep,Err(e)=>{if matches!(&e,PgTransactionError::Persistence(p) if p.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && p.sql_state()==Some("55P03")){send.send(()).unwrap();}return Err(e)} };
                let locks=prep.bind().await?;
                let current=inspect_quality_recipients(c,&locks,QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held}).await?;
                current.require_current(c).await?;Ok(current.recipients().to_vec())
            })
        }).await});
        tokio::time::timeout(Duration::from_secs(5),receive.recv()).await.unwrap().unwrap();
        // This role write would form a cycle if the reader waited on the
        // permission table while retaining its already-acquired role lock.
        sqlx::query("UPDATE iam_roles SET name=name WHERE id='custom_native_authority'").execute(&mut *writer).await.unwrap();
        writer.commit().await.unwrap();
        assert!(owner.await.unwrap().unwrap().is_empty());
        assert!(attempts.load(Ordering::SeqCst)>=2);
    }).await;}).await;
}

#[tokio::test]
async fn quality_recipient_roles_first_writer_commits_before_first_snapshot() {
    use erpx_server_identity_access::public::quality_recipients::{QualityRecipientRequest,QualityRecipientStatus,inspect_quality_recipients,lock_quality_recipient_sources};
    use erpx_server_shared::public::{CompanyId,RecordId};
    use sqlx::Connection;
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        setup(&control,&holding).await;
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"quality.inspect\"]','[]','[\"holding\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'").execute(holding.raw()).await.unwrap();
        let mut writer=holding.raw().begin().await.unwrap();let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("UPDATE iam_roles SET status='archived' WHERE id='custom_native_authority'").execute(&mut *writer).await.unwrap();
        let pool=holding.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let reader=tokio::spawn(async move {let mut c=pool.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();let mut tx=c.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();send.send(pid).unwrap();
            let locks=lock_quality_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
            let source=inspect_quality_recipients(&mut tx,&locks,QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held}).await.unwrap();
            assert!(source.recipients().is_empty());source.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();});
        let reader_pid=receive.await.unwrap();tokio::time::timeout(Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(reader_pid).fetch_one(holding.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await.expect("selected reader did not wait for role writer");
        sqlx::query("UPDATE iam_permissions SET definition=jsonb_set(definition,'{availability}','\"planned\"'::jsonb) WHERE id='quality.inspect'").execute(&mut *writer).await.unwrap();writer.commit().await.unwrap();reader.await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn quality_recipient_missing_permission_is_stable_until_unit_finishes() {
    use erpx_server_identity_access::public::quality_recipients::{QualityRecipientRequest,QualityRecipientStatus,inspect_quality_recipients,lock_quality_recipient_sources};
    use erpx_server_shared::public::{CompanyId,RecordId};
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        setup(&control,&holding).await;
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"quality.inspect\"]','[]','[\"holding\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'").execute(holding.raw()).await.unwrap();
        let definition:serde_json::Value=sqlx::query_scalar("DELETE FROM iam_permissions WHERE id='quality.inspect' RETURNING definition").fetch_one(holding.raw()).await.unwrap();
        let mut reader=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let locks=lock_quality_recipient_sources(&mut reader).await.unwrap().bind().await.unwrap();
        let request=QualityRecipientRequest {company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:None,status:QualityRecipientStatus::Held};
        let witness=inspect_quality_recipients(&mut reader,&locks,request.clone()).await.unwrap();assert!(witness.recipients().is_empty());
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader).await.unwrap();
        let pool=holding.clone();let (send,receive)=tokio::sync::oneshot::channel();
        let writer=tokio::spawn(async move {let mut tx=pool.raw().begin().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *tx).await.unwrap();send.send(pid).unwrap();sqlx::query("INSERT INTO iam_permissions(id,definition) VALUES('quality.inspect',$1)").bind(definition).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();});
        let writer_pid=receive.await.unwrap();tokio::time::timeout(Duration::from_secs(5),async {loop {
            let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(reader_pid).bind(writer_pid).fetch_one(holding.raw()).await.unwrap();
            if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;
        }}).await.expect("permission insertion did not wait for protected absence");
        witness.require_current(&mut reader).await.unwrap();reader.commit().await.unwrap();writer.await.unwrap();
        let mut next=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *next).await.unwrap();
        let locks=lock_quality_recipient_sources(&mut next).await.unwrap().bind().await.unwrap();
        let current=inspect_quality_recipients(&mut next,&locks,request).await.unwrap();assert_eq!(current.recipients(), &["user".to_string()]);
        next.rollback().await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn quality_recipient_set_matches_scoped_status_and_alias_rules() {
    use erpx_server_identity_access::public::quality_recipients::{inspect_quality_recipients,lock_quality_recipient_sources,QualityRecipientRequest,QualityRecipientStatus};
    use erpx_server_shared::public::{CompanyId,RecordId};
    fixture::with_database(2, |control| async move {
        fixture::with_database(3, move |holding| async move {
            setup(&control,&holding).await;
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"quality.read\",\"quality.inspect\",\"quality.release\"]','[]','[\"holding\",\"branch\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'; INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','creator','local:creator','same-person','active','local'),('company','alias','local:alias','same-person','active','local'),('company','manager','local:manager','manager-person','active','local'),('company','other','local:other','other-person','active','local'); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('quality-creator','creator','custom_native_authority','[]','company',0,9007199254740991,1,'holding',NULL),('quality-alias','alias','custom_native_authority','[]','company',0,9007199254740991,1,'holding',NULL),('quality-manager','manager','custom_native_authority','[]','company',0,9007199254740991,1,'branch','branch'),('quality-other-branch','other','custom_native_authority','[]','company',0,9007199254740991,1,'branch','other')").execute(holding.raw()).await.unwrap();
            let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let locks=lock_quality_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
            let request=QualityRecipientRequest { company_id:CompanyId::new("company").unwrap(),branch_id:RecordId::new("branch").unwrap(),creator:"creator".into(),cancellation_requester:Some("user".into()),status:QualityRecipientStatus::CancellationPending };
            let witness=inspect_quality_recipients(&mut tx,&locks,request.clone()).await.unwrap();
            assert_eq!(witness.recipients(), &["manager"]);witness.require_current(&mut tx).await.unwrap();
            let held=inspect_quality_recipients(&mut tx,&locks,QualityRecipientRequest { status:QualityRecipientStatus::Held,..request.clone() }).await.unwrap();
            assert_eq!(held.recipients(), &["manager","user"]);
            let cancelled=inspect_quality_recipients(&mut tx,&locks,QualityRecipientRequest { status:QualityRecipientStatus::Cancelled,..request.clone() }).await.unwrap();
            assert_eq!(cancelled.recipients(), &["creator"]);
            let missing_requester=inspect_quality_recipients(&mut tx,&locks,QualityRecipientRequest { cancellation_requester:None,..request.clone() }).await.unwrap();
            assert!(missing_requester.recipients().is_empty());
            let foreign=inspect_quality_recipients(&mut tx,&locks,QualityRecipientRequest { company_id:CompanyId::new("foreign").unwrap(),..request.clone() }).await.unwrap();
            assert!(foreign.recipients().is_empty());
            let mut other=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
            assert!(witness.require_current(&mut other).await.is_err());
            assert!(inspect_quality_recipients(&mut other,&locks,request.clone()).await.is_err());other.rollback().await.unwrap();
            sqlx::query("UPDATE assignments SET valid_until=1 WHERE id='quality-manager'").execute(&mut *tx).await.unwrap();
            assert!(witness.require_current(&mut tx).await.is_err());
            sqlx::query("UPDATE assignments SET valid_until=9007199254740991 WHERE id='quality-manager'").execute(&mut *tx).await.unwrap();
            witness.require_current(&mut tx).await.unwrap();
            sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='quality-manager'").execute(&mut *tx).await.unwrap();
            assert!(witness.require_current(&mut tx).await.is_err());
            tx.rollback().await.unwrap();
            let mut guard=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *guard).await.unwrap();
            let locks=lock_quality_recipient_sources(&mut guard).await.unwrap().bind().await.unwrap();
            let witness=inspect_quality_recipients(&mut guard,&locks,request).await.unwrap();
            let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *guard).await.unwrap();
            let writer_pool=holding.clone();
            let writer=tokio::spawn(async move { sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='quality-manager'").execute(writer_pool.raw()).await.unwrap(); });
            tokio::time::timeout(Duration::from_secs(5),async { loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))").bind(pid).fetch_one(holding.raw()).await.unwrap();
                if blocked { break; } tokio::time::sleep(Duration::from_millis(10)).await;
            }}).await.expect("recipient withdrawal did not wait for the source guard");
            witness.require_current(&mut guard).await.unwrap();guard.commit().await.unwrap();writer.await.unwrap();
        }).await;
    }).await;
}

#[tokio::test]
async fn sales_recipient_scoped_current_assignment_is_transaction_bound() {
    use erpx_server_identity_access::public::sales_recipient::{SalesRecipientRequest, inspect_sales_recipient, lock_sales_recipient_sources, inspect_sales_assignment_join};
    use erpx_server_shared::public::CompanyId;
    fixture::with_database(2, |control| async move {
        fixture::with_database(3, move |holding| async move {
            setup(&control, &holding).await;
            sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"commerce.read\",\"commerce.sales\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'; UPDATE assignments SET valid_from=0,valid_until=9007199254740991 WHERE id='grant'")
                .execute(holding.raw()).await.unwrap();
            sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('recipient-dept','department','recipient','Recipient','branch','company','branch','active')").execute(holding.raw()).await.unwrap();
            let mut tx = holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let locks = lock_sales_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
            let join=inspect_sales_assignment_join(&mut tx,&locks).await.unwrap();
            join.require_current(&mut tx).await.unwrap();
            let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
                .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
                .args(["--conditions=react-server","-e", "import {commercialOwnerProjectionSources as s} from './packages/server-identity_access/src/adapters/commercial-owner-projection-sources.ts'; process.stdout.write(s.assignments)"])
                .output().unwrap();
            assert!(oracle.status.success());
            let legacy=String::from_utf8(oracle.stdout).unwrap();
            let comparison=format!("WITH {}, legacy AS ({legacy}) SELECT NOT EXISTS((SELECT * FROM sales_recipient_assignments EXCEPT ALL SELECT * FROM legacy) UNION ALL (SELECT * FROM legacy EXCEPT ALL SELECT * FROM sales_recipient_assignments))",join.cte_sql());
            assert!(sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(comparison.as_str())).fetch_one(&mut *tx).await.unwrap());
            let request = || SalesRecipientRequest { company_id: CompanyId::new("company").unwrap(), user_id: "user".into(), branch_id: "branch".into(), department_id: None };
            let witness = inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap();
            assert!(witness.is_eligible());
            witness.require_current(&mut tx).await.unwrap();
            sqlx::query("UPDATE assignments SET scope_kind='branch',scope_node_id='branch' WHERE id='grant'").execute(&mut *tx).await.unwrap();
            assert!(inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible());
            let wrong = SalesRecipientRequest { branch_id: "other".into(), ..request() };
            assert!(!inspect_sales_recipient(&mut tx, &locks, wrong).await.unwrap().is_eligible());
            // Identity seam does not promise row stability: a consumer must rerun its full join.
            join.require_current(&mut tx).await.unwrap();
            assert!(sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(comparison.as_str())).fetch_one(&mut *tx).await.unwrap());
            let foreign = SalesRecipientRequest { company_id: CompanyId::new("foreign").unwrap(), ..request() };
            assert!(!inspect_sales_recipient(&mut tx, &locks, foreign).await.unwrap().is_eligible());
            sqlx::query("UPDATE assignments SET scope_kind='department',scope_node_id='recipient-dept' WHERE id='grant'").execute(&mut *tx).await.unwrap();
            let department = SalesRecipientRequest { department_id: Some("recipient-dept".into()), ..request() };
            assert!(inspect_sales_recipient(&mut tx, &locks, department).await.unwrap().is_eligible());
            assert!(!inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible());
            sqlx::query("UPDATE assignments SET scope_kind='holding',scope_node_id=NULL,valid_until=1 WHERE id='grant'").execute(&mut *tx).await.unwrap();
            assert!(!inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible());
            sqlx::query("UPDATE assignments SET valid_until=9007199254740991 WHERE id='grant'").execute(&mut *tx).await.unwrap();
            sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','sales_only_recipient','Sales only',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','sales_only_recipient',1,'[\"commerce.sales\"]','[]','[\"holding\"]','fixture',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('sales-only','user','sales_only_recipient','[]','company',0,9007199254740991,1,'holding'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',3,'[\"commerce.read\"]','[]','[\"holding\",\"branch\"]','fixture',0); UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'")
                .execute(&mut *tx).await.unwrap();
            assert!(!inspect_sales_recipient(&mut tx, &locks, request()).await.unwrap().is_eligible(), "different assignments cannot combine permissions");
            sqlx::query("UPDATE iam_roles SET status='archived' WHERE id='custom_native_authority'").execute(&mut *tx).await.unwrap();
            assert!(witness.require_current(&mut tx).await.is_err());
            tx.rollback().await.unwrap();
            let mut other = holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
            assert!(witness.require_current(&mut other).await.is_err());
            assert!(join.require_current(&mut other).await.is_err());
            assert!(inspect_sales_recipient(&mut other, &locks, request()).await.is_err());
            other.rollback().await.unwrap();
            let mut guard = holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *guard).await.unwrap();
            let guarded_locks = lock_sales_recipient_sources(&mut guard).await.unwrap().bind().await.unwrap();
            let guarded = inspect_sales_recipient(&mut guard, &guarded_locks, request()).await.unwrap();
            assert!(guarded.is_eligible());
            let guard_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *guard).await.unwrap();
            let writer_pool = holding.clone();
            let writer = tokio::spawn(async move {
                sqlx::query("UPDATE iam_roles SET status='archived' WHERE id='custom_native_authority'").execute(writer_pool.raw()).await.unwrap();
            });
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                        .bind(guard_pid).fetch_one(holding.raw()).await.unwrap();
                    if blocked { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("role withdrawal did not wait for the source guard");
            guarded.require_current(&mut guard).await.unwrap();
            guard.commit().await.unwrap();
            writer.await.unwrap();
            let role: String = sqlx::query_scalar("SELECT status FROM iam_roles WHERE id='custom_native_authority'").fetch_one(holding.raw()).await.unwrap();
            assert_eq!(role, "archived");
        }).await;
    }).await;
}

#[tokio::test]
async fn sales_recipient_preparation_waits_for_crm_writer_before_snapshot() {
    use erpx_server_identity_access::public::sales_recipient::lock_sales_recipient_sources;
    fixture::with_database(2, |control| async move {
        fixture::with_database(3, move |holding| async move {
            setup(&control, &holding).await;
            sqlx::query("INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,version,created_by,created_at,updated_at) VALUES('recipient-lead','company','branch','Lead','Customer','Contact','Staff','Brief','user','2026-12-01','open',1,'user',0,0)").execute(holding.raw()).await.unwrap();
            let mut writer = holding.raw().begin().await.unwrap();
            let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
            sqlx::query("UPDATE crm_leads SET status='disqualified',version=version+1 WHERE id='recipient-lead'").execute(&mut *writer).await.unwrap();
            let source_pool = holding.clone();
            let reader = tokio::spawn(async move {
                let mut tx = source_pool.raw().begin().await.unwrap();
                sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
                let mut prep = lock_sales_recipient_sources(&mut tx).await.unwrap();
                sqlx::query("LOCK TABLE crm_leads IN SHARE MODE").execute(prep.connection()).await.unwrap();
                let _bound = prep.bind().await.unwrap();
                let status: String = sqlx::query_scalar("SELECT status FROM crm_leads WHERE id='recipient-lead'").fetch_one(&mut *tx).await.unwrap();
                tx.rollback().await.unwrap();
                status
            });
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))")
                        .bind(writer_pid).fetch_one(holding.raw()).await.unwrap();
                    if blocked { break; }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("source preparation did not wait for CRM writer");
            writer.commit().await.unwrap();
            assert_eq!(reader.await.unwrap(), "disqualified");
        }).await;
    }).await;
}

// An explicit synthetic foreign-owner port. Production composition must use an
// actual owner's opaque source; Identity never queries a foreign business table.
#[derive(Clone)]
struct FixtureQueuedSource;
struct FixtureOwnedSource {
    binding: QueuedWorkBinding,
    payload: String,
    deadline: i64,
    transaction: (String, i32, String),
}
fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUEUED_OWNER_SOURCE_CHANGED",
        "The original owner source changed.",
    )
}
async fn transaction(
    connection: &mut PgConnection,
) -> Result<(String, i32, String), PgTransactionError<BusinessError>> {
    Ok(sqlx::query_as(
        "SELECT current_database()::text,pg_backend_pid(),pg_current_xact_id()::text",
    )
    .fetch_one(connection)
    .await?)
}
impl PgQueuedWorkSourcePort for FixtureQueuedSource {
    type Source = FixtureOwnedSource;
    fn load_source<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, Self::Source, BusinessError> {
        Box::pin(async move {
            let row=sqlx::query("SELECT company_id,user_id,payload,deadline FROM native_queued_owner_jobs WHERE id='job' AND token='lease' AND deadline>$1 FOR UPDATE").bind(now).fetch_optional(&mut *connection).await?.ok_or_else(source_changed)?;
            let payload: String = row.try_get("payload")?;
            let binding = QueuedWorkBinding::parse(&payload)?;
            if binding.company_id.as_str() != row.try_get::<String, _>("company_id")?
                || binding.user_id.as_str() != row.try_get::<String, _>("user_id")?
            {
                return Err(source_changed().into());
            }
            Ok(FixtureOwnedSource {
                binding,
                payload,
                deadline: row.try_get("deadline")?,
                transaction: transaction(connection).await?,
            })
        })
    }
    fn binding(&self, source: &Self::Source) -> Result<QueuedWorkBinding, BusinessError> {
        Ok(source.binding.clone())
    }
    fn require_current<'c>(
        &'c self,
        connection: &'c mut PgConnection,
        source: &'c Self::Source,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if source.deadline <= now || source.transaction != transaction(&mut *connection).await?
            {
                return Err(source_changed().into());
            }
            let payload=sqlx::query_scalar::<_,String>("SELECT payload FROM native_queued_owner_jobs WHERE id='job' AND token='lease' AND deadline>$1").bind(now).fetch_optional(connection).await?;
            if payload.as_deref() != Some(source.payload.as_str()) {
                return Err(source_changed().into());
            }
            Ok(())
        })
    }
}
async fn queue_from_current(
    authorizer: &erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer,
) {
    authorizer.execute("acme.example.test",proof(),input(),access(),|connection,current|Box::pin(async move{
        sqlx::query("CREATE TABLE native_queued_owner_jobs(id text PRIMARY KEY,company_id text,user_id text,payload text,token text,deadline bigint)").execute(&mut *connection).await?;
        let snapshot=QueuedWorkBinding::from_current(current).to_json()?;
        sqlx::query("INSERT INTO native_queued_owner_jobs VALUES('job','company','user',$1,'lease',$2)").bind(snapshot).bind(NOW+10000).execute(connection).await?;Ok(())
    })).await.unwrap();
}
#[tokio::test]
async fn queued_authority_uses_actual_owner_source_and_current_native_witnesses() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            queue_from_current(authorizer.as_ref()).await;
            let queued = PgQueuedWorkAuthorizer::new(
                control.clone(),
                holding.clone(),
                HoldingId::new("h001").unwrap(),
                clock,
                holding_registry(),
            );
            queued
                .execute(FixtureQueuedSource, access(), |connection, current, _| {
                    Box::pin(async move {
                        current.require_permission("finance.write")?;
                        sqlx::query("INSERT INTO native_owner_effects VALUES('queued')")
                            .execute(connection)
                            .await?;
                        Ok(())
                    })
                })
                .await
                .unwrap();
            let original: String =
                sqlx::query_scalar("SELECT payload FROM native_queued_owner_jobs")
                    .fetch_one(holding.raw())
                    .await
                    .unwrap();
            for (field, value, expected) in [
                (
                    "role",
                    serde_json::json!("holding_owner"),
                    "QUEUED_AUTHORITY_CHANGED",
                ),
                (
                    "identityVersion",
                    serde_json::json!(9),
                    "QUEUED_AUTHORITY_CHANGED",
                ),
                (
                    "roleRevision",
                    serde_json::json!(9),
                    "QUEUED_AUTHORITY_CHANGED",
                ),
                (
                    "userId",
                    serde_json::json!("foreign-user"),
                    "QUEUED_OWNER_SOURCE_CHANGED",
                ),
                (
                    "companyId",
                    serde_json::json!("foreign-company"),
                    "QUEUED_OWNER_SOURCE_CHANGED",
                ),
            ] {
                let mut forged: serde_json::Value = serde_json::from_str(&original).unwrap();
                forged[field] = value;
                sqlx::query("UPDATE native_queued_owner_jobs SET payload=$1")
                    .bind(forged.to_string())
                    .execute(holding.raw())
                    .await
                    .unwrap();
                code(
                    queued
                        .execute(FixtureQueuedSource, access(), |_, _, _| {
                            Box::pin(async {
                                panic!("historical actor bypassed current owner/IAM facts");
                                #[allow(unreachable_code)]
                                Ok(())
                            })
                        })
                        .await,
                    expected,
                );
            }
            sqlx::query("UPDATE native_queued_owner_jobs SET payload=$1")
                .bind(original)
                .execute(holding.raw())
                .await
                .unwrap();
            sqlx::query(
                "UPDATE iam_roles SET status='archived' WHERE id='custom_native_authority'",
            )
            .execute(holding.raw())
            .await
            .unwrap();
            assert!(
                queued
                    .execute(FixtureQueuedSource, access(), |_, _, _| Box::pin(async {
                        Ok(())
                    }))
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_owner_effects")
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                1
            );
        })
        .await;
    })
    .await;
}
#[tokio::test]
async fn queued_owner_expiry_after_action_rolls_back_the_complete_holding_attempt() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, clock) = setup(&control, &holding).await;
            queue_from_current(authorizer.as_ref()).await;
            let queued = PgQueuedWorkAuthorizer::new(
                control.clone(),
                holding.clone(),
                HoldingId::new("h001").unwrap(),
                clock.clone(),
                holding_registry(),
            );
            let advancing = clock.clone();
            let result = queued
                .execute(FixtureQueuedSource, access(), move |connection, _, _| {
                    let clock = advancing.clone();
                    Box::pin(async move {
                        sqlx::query("INSERT INTO native_owner_effects VALUES('expired-job')")
                            .execute(connection)
                            .await?;
                        clock.0.store(NOW + 10001, Ordering::SeqCst);
                        Ok(())
                    })
                })
                .await;
            code(result, "QUEUED_OWNER_SOURCE_CHANGED");
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_owner_effects")
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                0
            );
        })
        .await;
    })
    .await;
}
fn code<T: std::fmt::Debug>(result: Result<T, PgTransactionError<BusinessError>>, expected: &str) {
    assert!(
        matches!(result,Err(PgTransactionError::Business(ref e))if e.code==expected),
        "expected {expected}, got {result:?}"
    );
}
async fn wait_for_lock(pool: &DatabasePool) {
    tokio::time::timeout(Duration::from_secs(3),async{loop{if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap(){break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.expect("owned backend never reached a lock wait");
}
#[tokio::test]
async fn effective_context_is_scoped_and_owner_effects_roll_back_when_time_or_authority_changes() {
    fixture::with_database(3,|control|async move{fixture::with_database(3,move|holding|async move{
    let(authorizer,clock)=setup(&control,&holding).await;
    let result=authorizer.execute("acme.example.test",proof(),input(),access(),|connection,context|Box::pin(async move{
        assert_eq!(context.holding_id().as_str(),"h001");assert_eq!(context.selection().workspace().company_id.as_str(),"company");assert!(context.has_permission("finance.write"));assert!(!context.has_permission("finance.pay"));assert!(context.same_person_user(&UserId::new("user").unwrap()));
        sqlx::query("INSERT INTO native_owner_effects VALUES('committed')").execute(connection).await?;Ok(context.user_id().as_str().to_owned())
    })).await.unwrap();assert_eq!(result,"user");
    let advancing=clock.clone();let expired=authorizer.execute("acme.example.test",proof(),input(),access(),move|connection,_|{let advancing=advancing.clone();Box::pin(async move{sqlx::query("INSERT INTO native_owner_effects VALUES('expired')").execute(connection).await?;advancing.0.store(NOW+3600000,Ordering::SeqCst);Ok(())})}).await;
    code(expired,"WORK_AUTHORITY_EXPIRED");clock.0.store(NOW,Ordering::SeqCst);
    let changed=authorizer.execute("acme.example.test",proof(),input(),access(),|connection,_|Box::pin(async move{sqlx::query("UPDATE active_work_contexts SET revision=8 WHERE session_id='session'").execute(&mut *connection).await?;sqlx::query("INSERT INTO native_owner_effects VALUES('changed')").execute(connection).await?;Ok(())})).await;
    code(changed,"WORK_CONTEXT_CHANGED");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM active_work_contexts WHERE session_id='session'").fetch_one(holding.raw()).await.unwrap(),7);
 }).await;}).await;
}
#[tokio::test]
async fn published_permissions_module_and_beta_admission_never_fall_back_to_role_names() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
    let(authorizer,_)=setup(&control,&holding).await;let calls=Arc::new(AtomicUsize::new(0));
    for permission in ["finance.pay","unknown.permission"]{let mut request=access();request.permission=permission.into();let calls=calls.clone();code(authorizer.execute("acme.example.test",proof(),input(),request,move|_,_|{calls.fetch_add(1,Ordering::SeqCst);Box::pin(async{Ok(())})}).await,"PERMISSION_REQUIRED");}
    let mut request=access();request.module=Some("legal".into());code(authorizer.execute("acme.example.test",proof(),input(),request,|_,_|Box::pin(async{Ok(())})).await,"MODULE_UNAVAILABLE");
    sqlx::raw_sql("INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES('company',10,2,100,1000,0)").execute(holding.raw()).await.unwrap();
    code(authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"BETA_CAPABILITY_UNAVAILABLE");
    let mut allowed=access();allowed.destination="requests".into();allowed.module=None;
    authorizer.execute("acme.example.test",proof(),input(),allowed.clone(),|_,context|Box::pin(async move{assert!(context.modules().is_empty());Ok(())})).await.unwrap();
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"finance.read\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
    code(authorizer.execute("acme.example.test",proof(),input(),allowed,|_,_|Box::pin(async{Ok(())})).await,"PERMISSION_REQUIRED");assert_eq!(calls.load(Ordering::SeqCst),0);
 }).await;}).await;
}
#[tokio::test]
async fn inserted_denials_committed_before_policy_lock_are_visible_to_the_first_snapshot() {
    fixture::with_database(3,|control|async move{fixture::with_database(3,move|holding|async move{
    let(authorizer,_)=setup(&control,&holding).await;
    let mut writer=holding.raw().begin().await.unwrap();sqlx::query("INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES('session','company','user',$1)").bind(NOW).execute(&mut *writer).await.unwrap();
    let read={let authorizer=authorizer.clone();tokio::spawn(async move{authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{panic!("revoked actor reached owner callback");#[allow(unreachable_code)]Ok(())})).await})};
    wait_for_lock(&holding).await;writer.commit().await.unwrap();code(read.await.unwrap(),"SESSION_REVOKED");
 }).await;}).await;
}
#[tokio::test]
async fn control_and_holding_source_leases_block_changes_through_owner_commit() {
    fixture::with_database(3,|control|async move{fixture::with_database(3,move|holding|async move{
    let(authorizer,_)=setup(&control,&holding).await;let entered=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
    let owner={let authorizer=authorizer.clone();let entered=entered.clone();let release=release.clone();tokio::spawn(async move{authorizer.execute("acme.example.test",proof(),input(),access(),move|connection,_|{let entered=entered.clone();let release=release.clone();Box::pin(async move{entered.notify_one();release.notified().await;sqlx::query("INSERT INTO native_owner_effects VALUES('owner')").execute(connection).await?;Ok(())})}).await})};
    tokio::time::timeout(Duration::from_secs(3),entered.notified()).await.unwrap();
    let parent_change={let pool=control.clone();tokio::spawn(async move{sqlx::query("UPDATE account_device_browsers SET active_session_id=NULL WHERE id='browser'").execute(pool.raw()).await.unwrap();})};
    let revoke={let pool=holding.clone();tokio::spawn(async move{sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='grant'").bind(NOW).execute(pool.raw()).await.unwrap();})};
    wait_for_lock(&control).await;wait_for_lock(&holding).await;assert!(!parent_change.is_finished());assert!(!revoke.is_finished());release.notify_one();owner.await.unwrap().unwrap();parent_change.await.unwrap();revoke.await.unwrap();
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),1);
    code(authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"UNAUTHENTICATED");
 }).await;}).await;
}
#[tokio::test]
async fn hr_appointment_requires_live_people_facts_and_same_person_and_keeps_aliases_for_separation()
 {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
    let(authorizer,_)=setup(&control,&holding).await;
    let mut hr=access();hr.permission="people.manage".into();hr.module=Some("people_payroll".into());hr.destination="people/directory".into();
    code(authorizer.execute("acme.example.test",proof(),input(),hr.clone(),|_,_|Box::pin(async{Ok(())})).await,"MODULE_UNAVAILABLE");
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person',NULL,'company','Synthetic','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('engagement','person','company','employee','salaried','active','2026-01-01','Synthetic','THB','fixture',0,0);INSERT INTO authority_seats(company_id,kind,scope_id,version,person_id,effective_at) VALUES('company','hr','company',2,'person',0) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=excluded.version,person_id=excluded.person_id,effective_at=excluded.effective_at").execute(holding.raw()).await.unwrap();
    // HR eligibility without an account is not authority for an unrelated login.
    code(authorizer.execute("acme.example.test",proof(),input(),hr.clone(),|_,_|Box::pin(async{Ok(())})).await,"MODULE_UNAVAILABLE");
    sqlx::raw_sql("UPDATE hr_people SET user_id='delegate' WHERE id='person';INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy) VALUES('company','user','local:user','person','active','local'),('company','delegate','local:delegate','person','active','local'),('company','suspended-alias','local:alias','person','suspended','local')").execute(holding.raw()).await.unwrap();
    authorizer.execute("acme.example.test",proof(),input(),hr.clone(),|_,context|Box::pin(async move{assert_eq!(context.hr_appointment_version(),Some(2));assert!(context.same_person_user(&UserId::new("suspended-alias").unwrap()));assert!(context.same_person_user_raw("user"));assert!(context.same_person_user_raw("suspended-alias"));assert!(!context.same_person_user_raw(" Suspended-alias"));assert!(!context.same_person_user_raw(&"x".repeat(500)));assert_eq!(context.person_id().unwrap().as_str(),"person");Ok(())})).await.unwrap();
    sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='engagement'").execute(holding.raw()).await.unwrap();
    code(authorizer.execute("acme.example.test",proof(),input(),hr,|_,_|Box::pin(async{Ok(())})).await,"MODULE_UNAVAILABLE");
 }).await;}).await;
}
#[tokio::test]
async fn directory_freshness_emergency_expiry_scope_and_exact_host_are_mandatory() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
    let(authorizer,clock)=setup(&control,&holding).await;
    code(authorizer.execute("acme.example.test.attacker.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"WORKSPACE_NOT_FOUND");
    sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES('company','user','local:user','active','emergency')").execute(holding.raw()).await.unwrap();
    code(authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"EMERGENCY_ACCESS_REQUIRED");
    sqlx::query("INSERT INTO iam_emergency_sessions(session_id,company_id,user_id,expires_at,reason) VALUES('session','company','user',$1,'Synthetic reviewed elevation')").bind(NOW+1000).execute(holding.raw()).await.unwrap();
    let expired_clock=clock.clone();code(authorizer.execute("acme.example.test",proof(),input(),access(),move|_,context|{assert_eq!(context.emergency_until(),Some(NOW+1000));let clock=expired_clock.clone();Box::pin(async move{clock.0.store(NOW+1000,Ordering::SeqCst);Ok(())})}).await,"WORK_AUTHORITY_EXPIRED");clock.0.store(NOW,Ordering::SeqCst);
    sqlx::raw_sql("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES('connection','company','Synthetic','univention','{\"mode\":\"ldaps\",\"secretProfile\":\"synthetic\",\"authority\":\"synthetic approved source facts\"}','active','fixture',0);UPDATE iam_members SET login_policy='directory',identity_id='identity',connection_id='connection' WHERE user_id='user'").execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id) VALUES('identity','company','connection','external','synthetic','Synthetic',true,$1,'user')").bind(NOW-300000).execute(holding.raw()).await.unwrap();
    source_facts::approve_synthetic_source(&holding,"connection","company",NOW).await;
    source_facts::stamp_synthetic_observation(&holding,"connection","company",NOW-300000).await;
    code(authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"DIRECTORY_ACCESS_UNAVAILABLE");
    sqlx::query("UPDATE iam_identities SET observed_at=$1 WHERE id='identity'").bind(NOW).execute(holding.raw()).await.unwrap();
    source_facts::stamp_synthetic_observation(&holding,"connection","company",NOW).await;
    authorizer.execute("acme.example.test",proof(),input(),access(),|_,context|Box::pin(async move{assert_eq!(context.expires_at(),NOW+300000);Ok(())})).await.unwrap();
    // Permission is published but unavailable at the concrete Branch workspace.
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"iam.roles.manage\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
    let mut branch=input();branch.branch_slug=Some("north".into());let mut scoped=access();scoped.permission="iam.roles.manage".into();scoped.module=None;
    code(authorizer.execute("acme.example.test",proof(),branch,scoped,|_,_|Box::pin(async{Ok(())})).await,"PERMISSION_REQUIRED");
 }).await;}).await;
}

async fn seed_operational_worker_reference(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('worker','user','company','PRIVATE_WORKER_NAME','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('worker-engagement','worker','company','employee','salaried','active','2026-01-01','PRIVATE_JOB_TITLE','THB','fixture',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES('company','user','local:user','worker','active','local');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',2,'[\"finance.read\",\"finance.write\",\"module.finance.read\",\"records.write\",\"records.review\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority';INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('worker-dept','department','workshop','Workshop','branch','company','branch','active')").execute(pool.raw()).await.unwrap();
}
fn operational_worker_request(
    permission:erpx_server_identity_access::public::operational_worker::OperationalWorkerPermission,
    branch: &str,
    department: Option<&str>,
) -> erpx_server_identity_access::public::operational_worker::OperationalWorkerRequest {
    erpx_server_identity_access::public::operational_worker::OperationalWorkerRequest {
        company_id: erpx_server_shared::public::CompanyId::new("company").unwrap(),
        user_id: UserId::new("user").unwrap(),
        branch_id: erpx_server_shared::public::RecordId::new(branch).unwrap(),
        department_id: department.map(|s| erpx_server_shared::public::RecordId::new(s).unwrap()),
        permission,
    }
}
#[tokio::test]
async fn operational_worker_reference_requires_actual_people_current_permission_and_exact_record_scope()
 {
    use erpx_server_identity_access::public::operational_worker::*;
    use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
        setup(&control,&holding).await;seed_operational_worker_reference(&holding).await;
        PgUnitOfWork::new(holding.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
            lock_operational_worker_sources(c).await?;let now=TimestampMillis::new(NOW);
            let write=inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await?;assert!(write.is_eligible());write.require_current(c,now).await?;
            assert!(inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Review,"branch",None),now).await?.is_eligible());
            sqlx::query("UPDATE assignments SET scope_kind='branch',scope_node_id='branch',version=version+1 WHERE id='grant'").execute(&mut *c).await?;
            assert!(inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await?.is_eligible());
            assert!(!inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"other-branch",None),now).await?.is_eligible());
            sqlx::query("UPDATE assignments SET scope_kind='department',scope_node_id='worker-dept',version=version+1 WHERE id='grant'").execute(&mut *c).await?;
            assert!(inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",Some("worker-dept")),now).await?.is_eligible());
            assert!(!inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await?.is_eligible());
            sqlx::query("UPDATE assignments SET scope_kind='holding',scope_node_id='branch',version=version+1 WHERE id='grant'").execute(&mut *c).await?;
            // Explicit ADR-055 difference: no ignored node can participate in
            // a broader native Holding grant/reference.
            assert!(!inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await?.is_eligible());
            sqlx::query("UPDATE assignments SET scope_node_id=NULL,version=version+1 WHERE id='grant'").execute(&mut *c).await?;
            sqlx::query("UPDATE hr_engagements SET start_date='2030-01-01',version=version+1 WHERE id='worker-engagement'").execute(&mut *c).await?;
            assert!(!inspect_operational_worker(c,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await?.is_eligible());assert!(write.require_current(c,now).await.is_err());
            Ok(())
        })).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
    }).await;}).await;
}
#[tokio::test]
async fn operational_worker_reference_retains_current_role_and_refuses_a_different_transaction() {
    use erpx_server_identity_access::public::operational_worker::*;
    use erpx_server_shared::public::TimestampMillis;
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
        setup(&control,&holding).await;seed_operational_worker_reference(&holding).await;
        let now=TimestampMillis::new(NOW);let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_operational_worker_sources(&mut tx).await.unwrap();
        let source=inspect_operational_worker(&mut tx,operational_worker_request(OperationalWorkerPermission::Review,"branch",None),now).await.unwrap();assert!(source.is_eligible());
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"records.write\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(&mut *tx).await.unwrap();assert!(source.require_current(&mut tx,now).await.is_err());tx.rollback().await.unwrap();
        let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_operational_worker_sources(&mut tx).await.unwrap();let source=inspect_operational_worker(&mut tx,operational_worker_request(OperationalWorkerPermission::Write,"branch",None),now).await.unwrap();assert!(source.is_eligible());tx.commit().await.unwrap();
        let mut other=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();assert!(source.require_current(&mut other,now).await.is_err());other.rollback().await.unwrap();
    }).await;}).await;
}

#[derive(Clone)]
struct SelectedFixtureLocks;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SelectedFixtureLocks
{
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn selected_ingress_wire_precedes_permission_but_current_identity_precedes_wire() {
    fixture::with_database(3,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,_) = setup(&control,&holding).await;let calls=Arc::new(AtomicUsize::new(0));let mut denied=access();denied.permission="finance.pay".into();
        let action_calls=calls.clone();let local=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(denied.clone(),SelectedFixtureLocks),|selected| {assert_eq!(selected.company_id().as_str(),"company");Err::<(),_>("wire")},move |_,_,_| {action_calls.fetch_add(1,Ordering::SeqCst);Box::pin(async {Ok(())})}).await.unwrap();assert_eq!(local,Err("wire"));assert_eq!(calls.load(Ordering::SeqCst),0);
        let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(denied,SelectedFixtureLocks),|_|Ok::<_,&'static str>(()),|_,_,_|Box::pin(async {Ok(())})).await;code(result,"PERMISSION_REQUIRED");
        let seen=Arc::new(AtomicUsize::new(0));let observer=seen.clone();let result=authorizer.execute_with_selected_ingress_deferred("wrong.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),move |_| {observer.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>(())},|_,_,_|Box::pin(async{Ok(())})).await;code(result,"WORKSPACE_NOT_FOUND");assert_eq!(seen.load(Ordering::SeqCst),0);
        let observer=seen.clone();let mut stale=input();stale.expected.as_mut().unwrap().revision=6;let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),move||Ok(stale),(access(),SelectedFixtureLocks),move |_| {observer.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>(())},|_,_,_|Box::pin(async{Ok(())})).await;code(result,"WORK_CONTEXT_CHANGED");assert_eq!(seen.load(Ordering::SeqCst),0);
        let observer=seen.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",erpx_server_identity_access::public::DeviceProof::default(),||Ok(input()),(access(),SelectedFixtureLocks),move |_| {observer.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>(())},|_,_,_|Box::pin(async{Ok(())})).await;code(result,"UNAUTHENTICATED");
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES('company','user','local:user','active','emergency')").execute(holding.raw()).await.unwrap();
        let observer=seen.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),move |_| {observer.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>(())},|_,_,_|Box::pin(async{Ok(())})).await;code(result,"EMERGENCY_ACCESS_REQUIRED");assert_eq!(seen.load(Ordering::SeqCst),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
    }).await;}).await;
}

#[tokio::test]
async fn selected_ingress_local_error_yields_to_expiry_and_success_retains_selection() {
    fixture::with_database(3,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,clock)=setup(&control,&holding).await;let advance=clock.clone();
        let expired=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),move |_| {advance.0.store(NOW+3600000,Ordering::SeqCst);Err::<(),_>("wire")},|_,_,_|Box::pin(async{Ok(())})).await;code(expired,"WORK_AUTHORITY_EXPIRED");clock.0.store(NOW,Ordering::SeqCst);
        let changed=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),|_|Ok::<_,&'static str>(()),|c,_,_|Box::pin(async move {sqlx::query("UPDATE active_work_contexts SET revision=8 WHERE session_id='session'").execute(&mut *c).await?;sqlx::query("INSERT INTO native_owner_effects VALUES('selected-changed')").execute(c).await?;Ok(())})).await;code(changed,"WORK_CONTEXT_CHANGED");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM active_work_contexts WHERE session_id='session'").fetch_one(holding.raw()).await.unwrap(),7);
    }).await;}).await;
}

#[tokio::test]
async fn selected_ingress_repeats_immutable_normalization_after_synthetic_serialization_retry() {
    fixture::with_database(3,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,_)=setup(&control,&holding).await;
        sqlx::raw_sql("CREATE SEQUENCE selected_retry_seen;CREATE FUNCTION selected_retry_fault() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF nextval('selected_retry_seen')=1 THEN RAISE EXCEPTION 'synthetic retry' USING ERRCODE='40001'; END IF;RETURN NEW;END$$;CREATE TRIGGER selected_retry_fault BEFORE INSERT ON native_owner_effects FOR EACH ROW EXECUTE FUNCTION selected_retry_fault()").execute(holding.raw()).await.unwrap();
        let calls=Arc::new(AtomicUsize::new(0));let seen=calls.clone();
        let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),move |_| {seen.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>("immutable-body".to_owned())},|c,_,value|Box::pin(async move {assert_eq!(value,"immutable-body");sqlx::query("INSERT INTO native_owner_effects VALUES('retry')").execute(c).await?;Ok(())})).await.unwrap();assert_eq!(result,Ok(()));assert_eq!(calls.load(Ordering::SeqCst),2);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),1);
    }).await;}).await;
}

#[tokio::test]
async fn selected_ingress_never_invokes_schema_for_revoked_assignment_or_inactive_principal() {
    for scenario in ["assignment", "principal", "revoked-session"] {
        fixture::with_database(3,move |control|async move {fixture::with_database(3,move |holding|async move {
            let(authorizer,_)=setup(&control,&holding).await;
            match scenario {
                "assignment"=>{sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='grant'").bind(NOW).execute(holding.raw()).await.unwrap();},
                "principal"=>{sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES('company','user','local:user','suspended','local')").execute(holding.raw()).await.unwrap();},
                _=>{sqlx::raw_sql("DELETE FROM session WHERE id='session'").execute(control.raw()).await.unwrap();},
            }
            let seen=Arc::new(AtomicUsize::new(0));let observer=seen.clone();
            let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),move |_|{observer.fetch_add(1,Ordering::SeqCst);Ok::<_,&'static str>(())},|_,_,_|Box::pin(async{Ok(())})).await;
            code(result,if scenario=="revoked-session" {"UNAUTHENTICATED"}else{"WORK_ROLE_UNAVAILABLE"});assert_eq!(seen.load(Ordering::SeqCst),0);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
        }).await;}).await;
    }
}

#[tokio::test]
async fn selected_ingress_holds_real_control_and_assignment_locks_until_owner_commit() {
    fixture::with_database(3,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,_)=setup(&control,&holding).await;let entered=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
        let owner={let authorizer=authorizer.clone();let entered=entered.clone();let release=release.clone();tokio::spawn(async move {authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),|_|Ok::<_,&'static str>(()),move |connection,_,_| {let entered=entered.clone();let release=release.clone();Box::pin(async move {entered.notify_one();release.notified().await;sqlx::query("INSERT INTO native_owner_effects VALUES('selected-lock')").execute(connection).await?;Ok(())})}).await})};
        tokio::time::timeout(Duration::from_secs(3),entered.notified()).await.unwrap();
        let parent_change={let pool=control.clone();tokio::spawn(async move {sqlx::query("UPDATE account_device_browsers SET active_session_id=NULL WHERE id='browser'").execute(pool.raw()).await.unwrap();})};
        let revoke={let pool=holding.clone();tokio::spawn(async move {sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='grant'").bind(NOW).execute(pool.raw()).await.unwrap();})};
        wait_for_lock(&control).await;wait_for_lock(&holding).await;assert!(!parent_change.is_finished());assert!(!revoke.is_finished());release.notify_one();assert_eq!(owner.await.unwrap().unwrap(),Ok(()));parent_change.await.unwrap();revoke.await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),1);
    }).await;}).await;
}

#[tokio::test]
async fn selected_ingress_checks_time_again_after_fresh_final_identity_inspection() {
    use erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer;
    use erpx_server_shared::public::{Clock, TimestampMillis};
    use std::sync::atomic::AtomicBool;
    struct SteppedClock {
        armed: AtomicBool,
        reads: AtomicUsize,
    }
    impl Clock for SteppedClock {
        fn now(&self) -> TimestampMillis {
            if self.armed.load(Ordering::SeqCst) && self.reads.fetch_add(1, Ordering::SeqCst) >= 3 {
                TimestampMillis::new(NOW + 3600000)
            } else {
                TimestampMillis::new(NOW)
            }
        }
    }
    fixture::with_database(3, |control| async move {
        fixture::with_database(3, move |holding| async move {
            let _ = setup(&control, &holding).await;
            let clock = Arc::new(SteppedClock {
                armed: AtomicBool::new(false),
                reads: AtomicUsize::new(0),
            });
            let authorizer = HoldingWorkAuthorizer::new(
                control.clone(),
                holding.clone(),
                HoldingId::new("h001").unwrap(),
                policy(),
                clock.clone(),
                holding_registry(),
            )
            .unwrap();
            let armed = clock.clone();
            let result = authorizer
                .execute_with_selected_ingress_deferred(
                    "acme.example.test",
                    proof(),
                    || Ok(input()),
                    (access(), SelectedFixtureLocks),
                    move |_| {
                        armed.armed.store(true, Ordering::SeqCst);
                        Err::<(), _>("wire")
                    },
                    |_, _, _| Box::pin(async { Ok(()) }),
                )
                .await;
            code(result, "WORK_AUTHORITY_EXPIRED");
            assert_eq!(clock.reads.load(Ordering::SeqCst), 4);
            clock.armed.store(false, Ordering::SeqCst);
            clock.reads.store(0, Ordering::SeqCst);
            let armed = clock.clone();
            let async_result = authorizer
                .execute_with_selected_transport_ingress_deferred(
                    "acme.example.test",
                    proof(),
                    || Ok(input()),
                    (access(), SelectedFixtureLocks),
                    erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, (), &'static str, _>::new::<()>(  move |_| {
                        let armed = armed.clone();
                        Box::pin(async move {
                            tokio::task::yield_now().await;
                            armed.armed.store(true, Ordering::SeqCst);
                            Err::<(), _>("wire")
                        })
                    },
                    |_, error| Err(error),
                    |_, _, _| {
                        Box::pin(async {
                            Ok::<_, PgTransactionError<BusinessError>>(Ok::<_, &'static str>(()))
                        })
                    }, ),
                )
                .await;
            code(async_result, "WORK_AUTHORITY_EXPIRED");
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM native_owner_effects")
                    .fetch_one(holding.raw())
                    .await
                    .unwrap(),
                0
            );
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn transport_initial_read_tag_is_distinct_from_final_permission_and_local_action_failure() {
    fixture::with_database(3,|control|async move {fixture::with_database(3,move|holding|async move {
        let (authorizer,_) = setup(&control,&holding).await;
        let calls=Arc::new(AtomicUsize::new(0));let action_calls=calls.clone();
        let mut denied=access();denied.permission="finance.pay".into();
        let result=authorizer.execute_with_selected_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),(denied,SelectedFixtureLocks),
            erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, (), &'static str, _>::new::<()>(  |_|Box::pin(async {Ok::<_,&'static str>(())}),|_,error|{assert_eq!(error.code,"PERMISSION_REQUIRED");Ok("initial-read")},
            move |_,_,_| { action_calls.fetch_add(1,Ordering::SeqCst);Box::pin(async {Ok::<_,PgTransactionError<BusinessError>>(Ok::<_,&'static str>(()))}) }
        )).await.unwrap();
        assert_eq!(result,Err("initial-read"));assert_eq!(calls.load(Ordering::SeqCst),0);
        let local=authorizer.execute_with_selected_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),
            erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, (), &'static str, _>::new::<()>(  |_|Box::pin(async {Ok::<_,&'static str>(())}),|_,_|Ok("initial-read"),
            |_,_,_|Box::pin(async {Ok::<_,PgTransactionError<BusinessError>>(Err::<(),_>("initial-write"))})
        )).await.unwrap();assert_eq!(local,Err("initial-write"));
        let final_denial=authorizer.execute_with_selected_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),
            erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, (), &'static str, _>::new::<()>(  |_|Box::pin(async {Ok::<_,&'static str>(())}),|_,_|Ok("initial-read"),
            |c,current,_|Box::pin(async move {
                assert!(current.has_permission("finance.write"));
                assert_eq!(current.selection().role_revision(),1);
                sqlx::query("INSERT INTO native_owner_effects(id) VALUES('transport-final-permission')").execute(&mut *c).await?;
                let inserted=sqlx::query(r#"INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) SELECT company_id,role_id,2,'["finance.read","module.finance.read"]'::jsonb,grantable_json,scopes_json,'fixture',$1 FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_native_authority' AND revision=1"#).bind(NOW).execute(&mut *c).await?;
                assert_eq!(inserted.rows_affected(),1);
                let updated=sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_native_authority' AND published_revision=1 AND status='active' AND NOT built_in").execute(&mut *c).await?;
                assert_eq!(updated.rows_affected(),1);
                Ok::<_,PgTransactionError<BusinessError>>(Err::<(),_>("initial-write"))
            })
        )).await;
        code(final_denial,"PERMISSION_REQUIRED");
        let head:i64=sqlx::query_scalar("SELECT published_revision::bigint FROM iam_roles WHERE company_id='*' AND id='custom_native_authority'").fetch_one(holding.raw()).await.unwrap();assert_eq!(head,1);
        let revision2:i64=sqlx::query_scalar("SELECT count(*) FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_native_authority' AND revision=2").fetch_one(holding.raw()).await.unwrap();assert_eq!(revision2,0);
        let permissions:serde_json::Value=sqlx::query_scalar("SELECT permissions_json FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_native_authority' AND revision=1").fetch_one(holding.raw()).await.unwrap();
        assert_eq!(permissions,serde_json::json!(["finance.read","finance.write","module.finance.read"]));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
        let final_revoked=authorizer.execute_with_selected_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),(access(),SelectedFixtureLocks),
            erpx_server_identity_access::public::effective_authority::SelectedTransportCallbacks::<_, (), &'static str, _>::new::<()>(  |_|Box::pin(async {Ok::<_,&'static str>(())}),|_,_|Ok("initial-read"),
            |c,_,_|Box::pin(async move {
                sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='grant'").bind(NOW).execute(c).await?;
                Ok::<_,PgTransactionError<BusinessError>>(Err::<(),_>("initial-write"))
            })
        )).await;
        code(final_revoked,"WORK_ROLE_UNAVAILABLE");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM native_owner_effects").fetch_one(holding.raw()).await.unwrap(),0);
    }).await;}).await;
}

#[tokio::test]
async fn permission_catalog_require_checks_actual_mode_and_savepoint_release() {
    use erpx_server_identity_access::public::permission_catalog_lock::{lock_permission_catalog_sources,require_permission_catalog_lock};
    fixture::with_database(2, |control| async move {fixture::with_database(2, move |holding| async move {
        setup(&control,&holding).await;
        let mut outside=holding.raw().acquire().await.unwrap();assert!(require_permission_catalog_lock(&mut outside).await.is_err());drop(outside);
        let mut weak=holding.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE iam_permissions IN ROW SHARE MODE").execute(&mut *weak).await.unwrap();
        assert!(matches!(require_permission_catalog_lock(&mut weak).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="IAM_PERMISSION_CATALOG_LOCK_REQUIRED"));weak.rollback().await.unwrap();
        let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_catalog_lock").execute(&mut *tx).await.unwrap();
        lock_permission_catalog_sources(&mut tx).await.unwrap();require_permission_catalog_lock(&mut tx).await.unwrap();
        let actual:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='iam_permissions'::regclass AND mode='ShareLock' AND granted)").fetch_one(&mut *tx).await.unwrap();assert!(actual);
        sqlx::query("ROLLBACK TO SAVEPOINT before_catalog_lock").execute(&mut *tx).await.unwrap();
        assert!(matches!(require_permission_catalog_lock(&mut tx).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="IAM_PERMISSION_CATALOG_LOCK_REQUIRED"));
        sqlx::query("LOCK TABLE iam_permissions IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_permission_catalog_lock(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn effective_authority_requires_all_owner_fences_in_same_unit() {
    use erpx_server_identity_access::public::effective_authority::require_effective_authority_sources_held;
    use erpx_server_identity_access::public::field_service_workspace::lock_service_workspace_authority;
    use erpx_server_manage_erpx::public::lock_beta_policy_sources;
    use erpx_server_organization::public::lock_organization_authority_sources;
    use erpx_server_people_payroll::public::identity_facts::lock_people_authority_sources;
    const IDENTITY_LOCKS:&str="LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,active_work_contexts,iam_access_subjects,iam_access_invitations,um_attributes,um_values,um_groups,um_memberships,um_policies,um_attachments,um_candidates,iam_directory_source_selections,iam_directory_observation_sources IN SHARE MODE";
    fixture::with_database(2, |control| async move {fixture::with_database(2, move |holding| async move {
        setup(&control,&holding).await;
        let mut outside=holding.raw().acquire().await.unwrap();assert!(require_effective_authority_sources_held(&mut outside).await.is_err());drop(outside);
        for missing in ["identity","organization","people","beta"] {
            let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            if missing!="identity"{sqlx::query(IDENTITY_LOCKS).execute(&mut *tx).await.unwrap();}
            if missing!="organization"{lock_organization_authority_sources(&mut tx).await.unwrap();}
            if missing!="people"{lock_people_authority_sources(&mut tx).await.unwrap();}
            if missing!="beta"{lock_beta_policy_sources(&mut tx).await.unwrap();}
            assert!(require_effective_authority_sources_held(&mut tx).await.is_err(),"missing {missing} must fail");tx.rollback().await.unwrap();
        }
        let mut partial=holding.raw().begin().await.unwrap();sqlx::query("LOCK TABLE assignments,iam_roles IN SHARE MODE").execute(&mut *partial).await.unwrap();assert!(require_effective_authority_sources_held(&mut partial).await.is_err());partial.rollback().await.unwrap();
        let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_authority_sources").execute(&mut *tx).await.unwrap();
        lock_service_workspace_authority(&mut tx).await.unwrap();require_effective_authority_sources_held(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_authority_sources").execute(&mut *tx).await.unwrap();assert!(require_effective_authority_sources_held(&mut tx).await.is_err());
        // A stronger lock on every source is equally sufficient.
        sqlx::query("LOCK TABLE assignments,iam_roles,iam_role_revisions,iam_members,iam_identities,iam_connections,iam_revoked_sessions,iam_emergency_sessions,active_work_contexts,iam_access_subjects,iam_access_invitations,um_attributes,um_values,um_groups,um_memberships,um_policies,um_attachments,um_candidates,iam_directory_source_selections,iam_directory_observation_sources,organization_nodes,authority_seats,organization_positions,hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants,beta_policy IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();
        require_effective_authority_sources_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;}).await;
}

#[tokio::test]
async fn field_workspace_authority_fence_requires_catalog_and_marks_conflicts_retryable() {
    use erpx_server_identity_access::public::field_service_workspace::{lock_service_workspace_authority,lock_service_workspace_authority_fenced,require_service_workspace_authority_fenced_held};
    fixture::with_database(2, |control| async move {fixture::with_database(2, move |holding| async move {
        setup(&control,&holding).await;
        let mut absent=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *absent).await.unwrap();lock_service_workspace_authority(&mut absent).await.unwrap();
        assert!(matches!(require_service_workspace_authority_fenced_held(&mut absent).await,Err(PgTransactionError::Persistence(ref e)) if e.code=="IAM_PERMISSION_CATALOG_LOCK_REQUIRED"));
        sqlx::query("LOCK TABLE iam_permissions IN ROW SHARE MODE").execute(&mut *absent).await.unwrap();assert!(require_service_workspace_authority_fenced_held(&mut absent).await.is_err());absent.rollback().await.unwrap();
        let mut writer=holding.raw().begin().await.unwrap();sqlx::query("LOCK TABLE iam_permissions IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let mut blocked=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();
        let failure=lock_service_workspace_authority_fenced(&mut blocked).await;
        assert!(matches!(failure,Err(PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
        let mut tx=holding.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();sqlx::query("SAVEPOINT before_field_authority_fence").execute(&mut *tx).await.unwrap();
        lock_service_workspace_authority_fenced(&mut tx).await.unwrap();require_service_workspace_authority_fenced_held(&mut tx).await.unwrap();
        let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='iam_permissions'::regclass AND mode='ShareLock' AND granted)").fetch_one(&mut *tx).await.unwrap();assert!(held);
        sqlx::query("ROLLBACK TO SAVEPOINT before_field_authority_fence").execute(&mut *tx).await.unwrap();assert!(require_service_workspace_authority_fenced_held(&mut tx).await.is_err());
        lock_service_workspace_authority(&mut tx).await.unwrap();sqlx::query("LOCK TABLE iam_permissions IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_service_workspace_authority_fenced_held(&mut tx).await.unwrap();tx.rollback().await.unwrap();
    }).await;}).await;
}

#[derive(Clone)]
struct TypedCatalogPreparation { attempts:Arc<AtomicUsize>, drops:Arc<AtomicUsize>, conflicts:tokio::sync::mpsc::UnboundedSender<()>, conflict_release:Option<Arc<tokio::sync::Notify>> }
struct TypedCatalogToken { unit:(String,i32,String,String), drops:Arc<AtomicUsize> }
impl Drop for TypedCatalogToken {fn drop(&mut self){self.drops.fetch_add(1,Ordering::SeqCst);}}
impl erpx_server_identity_access::public::effective_authority::HoldingPreparedSource for TypedCatalogToken {
    fn require_current<'c>(&'c self,c:&'c mut PgConnection)->PgWorkFuture<'c,(),BusinessError>{
        Box::pin(async move {
            erpx_server_identity_access::public::permission_catalog_lock::require_permission_catalog_lock(&mut *c).await?;
            let unit:(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
            if unit!=self.unit{return Err(BusinessError::new(FailureKind::Conflict,"TEST_TOKEN_UNIT_CHANGED","Token changed unit.").into());}Ok(())
        })
    }
}
impl erpx_server_identity_access::public::effective_authority::TypedHoldingWorkSourcePreparation for TypedCatalogPreparation {
    type Token=TypedCatalogToken;
    fn prepare_typed<'c>(&'c self,c:&'c mut PgConnection)->PgWorkFuture<'c,Self::Token,BusinessError>{
        Box::pin(async move {
            self.attempts.fetch_add(1,Ordering::SeqCst);
            if let Err(error)=erpx_server_identity_access::public::permission_catalog_lock::lock_permission_catalog_sources(&mut *c).await {
                if matches!(&error,PgTransactionError::Persistence(e) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED") {let _=self.conflicts.send(()); if let Some(release)=&self.conflict_release { release.notified().await; }}
                return Err(error);
            }
            let unit=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
            Ok(TypedCatalogToken{unit,drops:self.drops.clone()})
        })
    }
}

#[tokio::test]
async fn typed_preparation_is_attempt_local_and_retries_real_catalog_conflict() {
    fixture::with_database(2,|control|async move {fixture::with_database(3,move|holding|async move {
        let(authorizer,_clock)=setup(&control,&holding).await;
        let mut writer=holding.raw().begin().await.unwrap();sqlx::query("LOCK TABLE iam_permissions IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let attempts=Arc::new(AtomicUsize::new(0));let drops=Arc::new(AtomicUsize::new(0));let (send,mut receive)=tokio::sync::mpsc::unbounded_channel();
        let release=Arc::new(tokio::sync::Notify::new());
        let prep=TypedCatalogPreparation{attempts:attempts.clone(),drops:drops.clone(),conflicts:send,conflict_release:Some(release.clone())};
        let selected=tokio::spawn(async move {
            authorizer.execute_with_typed_sources_deferred("acme.example.test",proof(),||Ok(input()),access(),prep,|c,_actor,token|Box::pin(async move {
                let actual:(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;assert_eq!(actual,token.unit);Ok(actual.2)
            })).await
        });
        tokio::time::timeout(Duration::from_secs(3),receive.recv()).await.unwrap().unwrap();writer.rollback().await.unwrap();release.notify_one();
        let first=tokio::time::timeout(Duration::from_secs(10),selected).await.unwrap().unwrap().unwrap();assert!(!first.is_empty());assert!(attempts.load(Ordering::SeqCst)>=2);assert_eq!(drops.load(Ordering::SeqCst),1);
    }).await;}).await;
}

#[tokio::test]
async fn selected_transport_typed_token_is_dropped_on_success_and_action_failure() {
    use erpx_server_identity_access::public::effective_authority::SelectedIngressFuture;
    fixture::with_database(2,|control|async move {fixture::with_database(2,move|holding|async move {
        let(authorizer,_clock)=setup(&control,&holding).await;let attempts=Arc::new(AtomicUsize::new(0));let drops=Arc::new(AtomicUsize::new(0));let (send,_receive)=tokio::sync::mpsc::unbounded_channel();
        let prep=TypedCatalogPreparation{attempts:attempts.clone(),drops:drops.clone(),conflicts:send,conflict_release:None};
        let selected=authorizer.execute_with_selected_typed_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),access(),prep.clone(),
            |_selected|->SelectedIngressFuture<i64,BusinessError>{Box::pin(async{Ok(7)})},
            |_,error|Err(error),
            |c,_actor,value,token|Box::pin(async move {let actual:(String,i32,String,String)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;assert_eq!(actual,token.unit);Ok(Ok(value))}),
        ).await.unwrap().unwrap();assert_eq!(selected,7);assert_eq!(drops.load(Ordering::SeqCst),1);
        let failed=authorizer.execute_with_typed_sources_deferred("acme.example.test",proof(),||Ok(input()),access(),prep.clone(),|_c,_actor,_token|Box::pin(async move{Err::<(),_>(BusinessError::new(FailureKind::Conflict,"TEST_ABORT","Abort this attempt.").into())})).await;
        assert!(matches!(failed,Err(PgTransactionError::Business(ref e)) if e.code=="TEST_ABORT"));assert_eq!(attempts.load(Ordering::SeqCst),2);assert_eq!(drops.load(Ordering::SeqCst),2);
        let mut denied=access(); denied.permission="finance.pay".into();
        let denied_result:Result<(), &'static str>=authorizer.execute_with_selected_typed_transport_ingress_deferred(
            "acme.example.test",proof(),||Ok(input()),denied,prep,
            |_selected|->SelectedIngressFuture<(), &'static str>{Box::pin(async {Ok(())})},
            |_,error| {assert_eq!(error.code,"PERMISSION_REQUIRED"); Ok("denied")},
            |_c,_actor,_value,_token|Box::pin(async {panic!("denied action must not run")}),
        ).await.unwrap();
        assert_eq!(denied_result,Err("denied")); assert_eq!(attempts.load(Ordering::SeqCst),3); assert_eq!(drops.load(Ordering::SeqCst),3);
    }).await;}).await;
}

#[tokio::test]
async fn typed_preparation_mints_a_fresh_token_after_serialization_retry() {
    fixture::with_database(2, |control| async move { fixture::with_database(3, move |holding| async move {
        let (authorizer, _) = setup(&control, &holding).await;
        sqlx::query("INSERT INTO native_owner_effects VALUES ('typed-before')").execute(holding.raw()).await.unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let (conflicts, _conflict_rx) = tokio::sync::mpsc::unbounded_channel();
        let prep = TypedCatalogPreparation { attempts: attempts.clone(), drops: drops.clone(), conflicts, conflict_release: None };
        let (entered, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let action_release = release.clone(); let action_seen = seen.clone();
        let task = tokio::spawn(async move {
            authorizer.execute_with_typed_sources_deferred("acme.example.test", proof(), || Ok(input()), access(), prep, move |c, _, token| {
                let entered = entered.clone(); let release = action_release.clone(); let seen = action_seen.clone();
                Box::pin(async move {
                    let old: String = sqlx::query_scalar("SELECT id FROM native_owner_effects WHERE id IN ('typed-before','typed-writer')").fetch_one(&mut *c).await?;
                    let first = { let mut values = seen.lock().unwrap(); values.push(token.unit.2.clone()); values.len() == 1 };
                    if first { entered.send(()).unwrap(); release.notified().await; }
                    sqlx::query("UPDATE native_owner_effects SET id='typed-final' WHERE id=$1").bind(old).execute(&mut *c).await?;
                    Ok(())
                })
            }).await
        });
        tokio::time::timeout(Duration::from_secs(3), entered_rx.recv()).await.unwrap().unwrap();
        sqlx::query("UPDATE native_owner_effects SET id='typed-writer' WHERE id='typed-before'").execute(holding.raw()).await.unwrap();
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(10), task).await.unwrap().unwrap().unwrap();
        { let values = seen.lock().unwrap(); assert_eq!(values.len(), 2); assert_ne!(values[0], values[1]); }
        assert_eq!(attempts.load(Ordering::SeqCst), 2); assert_eq!(drops.load(Ordering::SeqCst), 2);
        let id: String = sqlx::query_scalar("SELECT id FROM native_owner_effects WHERE id='typed-final'").fetch_one(holding.raw()).await.unwrap(); assert_eq!(id, "typed-final");
    }).await; }).await;
}

#[derive(Clone)]
struct DeferredFactPreparation(TypedCatalogPreparation);
struct DeferredFactToken(TypedCatalogToken);
impl erpx_server_identity_access::public::effective_authority::HoldingPreparedSource for DeferredFactToken {
    fn require_current<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            use erpx_server_identity_access::public::effective_authority::HoldingPreparedSource;
            self.0.require_current(&mut *c).await?;
            let changed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM native_owner_effects WHERE id='deferred-fact')").fetch_one(c).await?;
            if changed { return Err(BusinessError::new(FailureKind::Conflict,"TEST_DEFERRED_FACT_CHANGED","Retained fact changed.").into()); }
            Ok(())
        })
    }
}
impl erpx_server_identity_access::public::effective_authority::TypedHoldingWorkSourcePreparation for DeferredFactPreparation {
    type Token = DeferredFactToken;
    fn prepare_typed<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, Self::Token, BusinessError> {
        Box::pin(async move {
            use erpx_server_identity_access::public::effective_authority::{HoldingPreparedSource,TypedHoldingWorkSourcePreparation};
            let token = DeferredFactToken(self.0.prepare_typed(&mut *c).await?);
            token.require_current(c).await?;
            Ok(token)
        })
    }
}

#[tokio::test]
async fn typed_final_guard_rejects_deferred_trigger_fact_drift_and_rolls_back() {
    use erpx_server_identity_access::public::effective_authority::SelectedIngressFuture;
    fixture::with_database(2, |control| async move { fixture::with_database(2, move |holding| async move {
        let (authorizer, _) = setup(&control, &holding).await;
        sqlx::raw_sql("CREATE FUNCTION typed_deferred_fact() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO native_owner_effects VALUES ('deferred-fact'); RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER typed_deferred_fact AFTER INSERT ON native_owner_effects DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.id = 'deferred-action') EXECUTE FUNCTION typed_deferred_fact()").execute(holding.raw()).await.unwrap();
        let (conflicts, _rx) = tokio::sync::mpsc::unbounded_channel();
        let prep = DeferredFactPreparation(TypedCatalogPreparation { attempts: Arc::new(AtomicUsize::new(0)), drops: Arc::new(AtomicUsize::new(0)), conflicts, conflict_release: None });
        let normal = authorizer.execute_with_typed_sources_deferred("acme.example.test", proof(), || Ok(input()), access(), prep.clone(), |c, _, _| Box::pin(async move {
            sqlx::query("INSERT INTO native_owner_effects VALUES ('deferred-action')").execute(c).await?; Ok(())
        })).await;
        assert!(matches!(normal, Err(PgTransactionError::Business(ref e)) if e.code == "TEST_DEFERRED_FACT_CHANGED"));
        let selected = authorizer.execute_with_selected_typed_transport_ingress_deferred("acme.example.test", proof(), || Ok(input()), access(), prep,
            |_| -> SelectedIngressFuture<(), &'static str> { Box::pin(async { Ok(()) }) }, |_, _| Ok("denied"),
            |c, _, _, _| Box::pin(async move { sqlx::query("INSERT INTO native_owner_effects VALUES ('deferred-action')").execute(c).await?; Ok(Ok::<(), &'static str>(())) })
        ).await;
        assert!(matches!(selected, Err(PgTransactionError::Business(ref e)) if e.code == "TEST_DEFERRED_FACT_CHANGED"));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM native_owner_effects WHERE id IN ('deferred-action','deferred-fact')").fetch_one(holding.raw()).await.unwrap();
        assert_eq!(count, 0);
    }).await; }).await;
}

#[tokio::test]
async fn sales_iam_boundary_requires_canonical_sources_and_same_unit() {
    use erpx_server_identity_access::public::{permission_catalog_lock::lock_permission_catalog_sources, sales_recipient::{lock_sales_recipient_sources,inspect_sales_iam_boundary}};
    fixture::with_database(2,|control|async move {
        fixture::with_database(2,move|holding|async move {
            setup(&control,&holding).await;
            let mut tx=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
            sqlx::query("SAVEPOINT sales_iam_locks").execute(&mut *tx).await.unwrap();
            lock_permission_catalog_sources(&mut tx).await.unwrap();
            let locks=lock_sales_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
            let released=inspect_sales_iam_boundary(&mut tx,&locks).await.unwrap();
            released.require_current(&mut tx).await.unwrap();
            sqlx::query("ROLLBACK TO SAVEPOINT sales_iam_locks").execute(&mut *tx).await.unwrap();
            assert!(released.require_current(&mut tx).await.is_err());
            assert!(inspect_sales_iam_boundary(&mut tx,&locks).await.is_err());
            tx.rollback().await.unwrap();
            let mut tx=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::query("CREATE TEMP TABLE assignments(id text)").execute(&mut *tx).await.unwrap();
            sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
            lock_permission_catalog_sources(&mut tx).await.unwrap();
            let locks=lock_sales_recipient_sources(&mut tx).await.unwrap().bind().await.unwrap();
            let source=inspect_sales_iam_boundary(&mut tx,&locks).await.unwrap();
            source.require_current(&mut tx).await.unwrap();
            sqlx::query("SET LOCAL search_path TO pg_catalog, pg_temp, public").execute(&mut *tx).await.unwrap();
            assert!(source.require_current(&mut tx).await.is_err());
            sqlx::query("SET LOCAL search_path TO pg_catalog, public, pg_temp").execute(&mut *tx).await.unwrap();
            source.require_current(&mut tx).await.unwrap();
            let mut other=holding.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();
            assert!(source.require_current(&mut other).await.is_err());
            other.rollback().await.unwrap();tx.rollback().await.unwrap();
        }).await;
    }).await;
}
