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
    authorizer.execute("acme.example.test",proof(),input(),hr.clone(),|_,context|Box::pin(async move{assert_eq!(context.hr_appointment_version(),Some(2));assert!(context.same_person_user(&UserId::new("suspended-alias").unwrap()));assert_eq!(context.person_id().unwrap().as_str(),"person");Ok(())})).await.unwrap();
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
