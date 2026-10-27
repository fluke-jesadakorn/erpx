#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
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
    sqlx::raw_sql("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES('connection','company','Synthetic','entra_id','{}','active','fixture',0);UPDATE iam_members SET login_policy='directory',identity_id='identity',connection_id='connection' WHERE user_id='user'").execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at) VALUES('identity','company','connection','external','synthetic','Synthetic',true,$1)").bind(NOW-300000).execute(holding.raw()).await.unwrap();
    code(authorizer.execute("acme.example.test",proof(),input(),access(),|_,_|Box::pin(async{Ok(())})).await,"DIRECTORY_ACCESS_UNAVAILABLE");
    sqlx::query("UPDATE iam_identities SET observed_at=$1 WHERE id='identity'").bind(NOW).execute(holding.raw()).await.unwrap();
    authorizer.execute("acme.example.test",proof(),input(),access(),|_,context|Box::pin(async move{assert_eq!(context.expires_at(),NOW+300000);Ok(())})).await.unwrap();
    // Permission is published but unavailable at the concrete Branch workspace.
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',2,'[\"iam.roles.manage\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
    let mut branch=input();branch.branch_slug=Some("north".into());let mut scoped=access();scoped.permission="iam.roles.manage".into();scoped.module=None;
    code(authorizer.execute("acme.example.test",proof(),branch,scoped,|_,_|Box::pin(async{Ok(())})).await,"PERMISSION_REQUIRED");
 }).await;}).await;
}
