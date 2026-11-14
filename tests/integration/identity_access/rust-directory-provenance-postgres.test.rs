#[allow(
    dead_code,
    reason = "shared actual Account, source policy and owned LDAPS scenarios"
)]
#[path = "../../support/rust-directory-work.rs"]
mod fixture;
use erpx_server_identity_access::public::{
    SystemAuthenticationSecrets, directory::*, effective_authority::HoldingWorkAuthorizer,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, HoldingId, RecordId};
use fixture::{account, ldap};
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::time::timeout;

fn rebuilt(p: &fixture::Prepared, profiles: Arc<DirectoryProfileStore>) -> Arc<PostgresDirectory> {
    let system = &p.authority.system;
    Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: system.control.clone(),
                holdings: BTreeMap::from([
                    (
                        HoldingId::new("h001").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::ACME.into(),
                            write_binding: "HOLDING_H001_DB".into(),
                            pool: system.holdings[0].clone(),
                        },
                    ),
                    (
                        HoldingId::new("h002").unwrap(),
                        DirectoryHoldingDatabase {
                            audience: account::NORTHSTAR.into(),
                            write_binding: "HOLDING_H002_DB".into(),
                            pool: system.holdings[1].clone(),
                        },
                    ),
                ]),
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            profiles,
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&system.holdings, system.scenario.clock.clone()),
        )
        .unwrap(),
    )
}
fn work(directory: Arc<PostgresDirectory>) -> HoldingWorkAuthorizer {
    DirectoryRefreshRuntime::new(directory)
        .holding(
            HoldingId::new("h001").unwrap(),
            account::fixture::account::policy(),
        )
        .unwrap()
}
async fn execute(
    work: &HoldingWorkAuthorizer,
    p: &fixture::Prepared,
    id: &'static str,
) -> Result<String, PgTransactionError<BusinessError>> {
    work.execute(
        "acme.example.test",
        p.authority.proof.clone(),
        p.authority.selection.clone(),
        p.authority.access.clone(),
        move |c, context| {
            Box::pin(async move {
                sqlx::query("INSERT INTO synthetic_directory_work_effects VALUES($1)")
                    .bind(id)
                    .execute(c)
                    .await?;
                Ok(context.user_id().as_str().into())
            })
        },
    )
    .await
}
fn denied<T>(value: Result<T, PgTransactionError<BusinessError>>, expected: &str) {
    assert!(matches!(value,Err(PgTransactionError::Business(error)) if error.code==expected));
}
async fn revision(pool: &DatabasePool) -> i64 {
    sqlx::query_scalar(
        "SELECT revision FROM iam_directory_source_selections WHERE connection_id=$1",
    )
    .bind(ldap::CONNECTION)
    .fetch_one(pool.raw())
    .await
    .unwrap()
}
async fn active(p: &fixture::Prepared) -> bool {
    sqlx::query_scalar("SELECT iam_principal_active('fixture-a','company',$1)")
        .bind(p.authority.system.scenario.clock.now().get())
        .fetch_one(p.authority.system.holdings[0].raw())
        .await
        .unwrap()
}
async fn source_audits(pool: &DatabasePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM iam_audit WHERE action='directory.source-selected'")
        .fetch_one(pool.raw())
        .await
        .unwrap()
}
async fn replacement_store(
    p: &fixture::Prepared,
    keys: Vec<(String, [u8; 32])>,
) -> Arc<DirectoryProfileStore> {
    Arc::new(
        DirectoryProfileStore::load(
            p.profile.directory.path().join("profiles.json"),
            Arc::new(provenance::ProvenanceKeyRing::from_keys("development".into(), keys).unwrap()),
        )
        .await
        .unwrap(),
    )
}
async fn wait_source_write(pool: &DatabasePool) {
    timeout(Duration::from_secs(3),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND relation='iam_directory_source_selections'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
            if waiting {return;}tokio::task::yield_now().await;
        }
    }).await.expect("actual source writer must wait on the retained source");
}
#[tokio::test]
async fn real_source_rotation_invalidates_an_already_fresh_observation_until_complete_native_refresh()
 {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;
        let pool=&p.authority.system.holdings[0];let now=p.authority.system.scenario.clock.now().get();
        assert!(active(&p).await);
        let sessions:i64=sqlx::query_scalar("SELECT count(*) FROM session").fetch_one(p.authority.system.control.raw()).await.unwrap();
        assert_eq!(fixture::source_change::select(&p.directory,ldap::CONNECTION,1,Some(1),"synthetic_provenance_v1").await.unwrap(),2);
        assert_eq!(p.authority.system.scenario.clock.now().get(),now);assert!(!active(&p).await);
        assert_eq!(fixture::execute(&p).await.unwrap(),"fixture-a");assert!(active(&p).await);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT source_revision FROM iam_directory_observation_sources WHERE connection_id=$1").bind(ldap::CONNECTION).fetch_one(pool.raw()).await.unwrap(),2);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session").fetch_one(p.authority.system.control.raw()).await.unwrap(),sessions);
        assert_eq!(fixture::effects(&p).await,1);assert!(p.server.stats.lock().unwrap().connections>0);p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn missing_observation_is_refreshable_but_an_unapproved_binding_never_contacts_provider() {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;let pool=&p.authority.system.holdings[0];
        sqlx::query("DELETE FROM iam_directory_observation_sources WHERE connection_id=$1").bind(ldap::CONNECTION).execute(pool.raw()).await.unwrap();
        assert!(!active(&p).await);execute(&p.work,&p,"completed-missing-observation").await.unwrap();assert!(active(&p).await);
        let contacts=p.server.stats.lock().unwrap().connections;
        const UNAPPROVED:&str="00000000-0000-4000-8000-000000000088";
        sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at) SELECT $1,company_id,name,provider,config,status,version,tested_version,created_by,created_at FROM iam_connections WHERE id=$2").bind(UNAPPROVED).bind(ldap::CONNECTION).execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id) VALUES($1,'h001','company')").bind(UNAPPROVED).execute(p.authority.system.control.raw()).await.unwrap();
        sqlx::query("UPDATE iam_identities SET connection_id=$1 WHERE id='directory-work-identity'").bind(UNAPPROVED).execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE iam_members SET connection_id=$1 WHERE company_id='company' AND user_id='fixture-a'").bind(UNAPPROVED).execute(pool.raw()).await.unwrap();
        denied(execute(&p.work,&p,"must-not-run").await,"DIRECTORY_SOURCE_CONFIGURATION");
        assert_eq!(p.server.stats.lock().unwrap().connections,contacts);assert_eq!(fixture::effects(&p).await,1);p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn independent_restart_and_unrelated_profile_format_changes_retain_the_same_approved_source()
{
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;
        let path=p.profile.directory.path().join("profiles.json");let bytes=tokio::fs::read(&path).await.unwrap();let mut document:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        document.as_object_mut().unwrap().insert("unrelated_profile".into(),serde_json::json!({"allowedHosts":["unrelated.fixture.invalid"],"connectionIds":["unrelated"],"bindDn":"cn=unrelated","bindPassword":"unrelated synthetic secret"}));
        tokio::fs::write(&path,serde_json::to_vec_pretty(&document).unwrap()).await.unwrap();
        let second=rebuilt(&p,replacement_store(&p,vec![("synthetic_provenance_v1".into(),[0x31;32])]).await);
        execute(&work(second),&p,"restarted-instance").await.unwrap();execute(&p.work,&p,"original-instance").await.unwrap();
        assert_eq!(revision(&p.authority.system.holdings[0]).await,1);assert_eq!(p.server.stats.lock().unwrap().connections,0);assert_eq!(fixture::effects(&p).await,2);
        // An actually changed selected credential fails before I/O and cannot
        // rewrite the authoritative source or invalidate the original instance.
        document["fixture"]["bindPassword"]=serde_json::Value::String("different synthetic service secret".into());
        tokio::fs::write(&path,serde_json::to_vec(&document).unwrap()).await.unwrap();
        let wrong=rebuilt(&p,replacement_store(&p,vec![("synthetic_provenance_v1".into(),[0x31;32])]).await);
        denied(execute(&work(wrong),&p,"wrong-instance").await,"DIRECTORY_SOURCE_CONFIGURATION");
        execute(&p.work,&p,"original-still-current").await.unwrap();assert!(active(&p).await);assert_eq!(p.server.stats.lock().unwrap().connections,0);assert_eq!(fixture::effects(&p).await,3);p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn rotation_during_real_provider_io_rejects_old_observation_without_partial_effects_or_false_health_failure()
 {
    account::with_system(|system| async move {
        let p = Arc::new(fixture::prepare(system, ldap::Scenario::Complete, true, true).await);
        let request = {
            let p = p.clone();
            tokio::spawn(async move { fixture::execute(&p).await })
        };
        fixture::wait_provider(&p).await;
        assert_eq!(
            fixture::source_change::select(
                &p.directory,
                ldap::CONNECTION,
                1,
                Some(1),
                "synthetic_provenance_v1"
            )
            .await
            .unwrap(),
            2
        );
        p.server.resume.notify_one();
        assert!(request.await.unwrap().is_err());
        assert_eq!(fixture::effects(&p).await, 0);
        let pool = &p.authority.system.holdings[0];
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM iam_directory_observation_sources")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT health FROM iam_connections WHERE id=$1")
                .bind(ldap::CONNECTION)
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            "untested"
        );
        let p = Arc::try_unwrap(p).ok().unwrap();
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn current_source_fences_block_actual_native_rotation_through_the_owner_commit() {
    account::with_system(|system|async move {
        let p=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,false,false).await);let entered=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
        let owner={let p=p.clone();let entered=entered.clone();let release=release.clone();tokio::spawn(async move{p.work.execute("acme.example.test",p.authority.proof.clone(),p.authority.selection.clone(),p.authority.access.clone(),move|c,_|{let entered=entered.clone();let release=release.clone();Box::pin(async move{entered.notify_one();release.notified().await;sqlx::query("INSERT INTO synthetic_directory_work_effects VALUES('retained-source-owner')").execute(c).await?;Ok(())})}).await})};
        timeout(Duration::from_secs(3),entered.notified()).await.unwrap();
        let writer={let p=p.clone();tokio::spawn(async move{fixture::source_change::select(&p.directory,ldap::CONNECTION,1,Some(1),"synthetic_provenance_v1").await})};
        wait_source_write(&p.authority.system.holdings[0]).await;assert!(!writer.is_finished());release.notify_one();owner.await.unwrap().unwrap();assert_eq!(writer.await.unwrap().unwrap(),2);
        assert_eq!(fixture::effects(&p).await,1);assert!(!active(&p).await);let p=Arc::try_unwrap(p).ok().unwrap();p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn positive_source_tuple_fence_preserves_real_40001_after_an_older_serializable_snapshot() {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;let pool=&p.authority.system.holdings[0];let mut old=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
        sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_directory_source_selections").fetch_one(&mut*old).await.unwrap();
        fixture::source_change::select(&p.directory,ldap::CONNECTION,1,Some(1),"synthetic_provenance_v1").await.unwrap();
        let read=provenance::read_current_directory_source(&mut old,ldap::CONNECTION,"company").await;
        assert!(matches!(read,Err(PgTransactionError::Persistence(error)) if error.sql_state()==Some("40001")));old.rollback().await.unwrap();assert_eq!(revision(pool).await,2);p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn concurrent_original_revision_cas_commits_once_and_cannot_reset_or_delete_source_history() {
    account::with_system(|system| async move {
        let p = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        let request = || {
            DirectorySourceChangeRequest::new(
                RecordId::new(ldap::CONNECTION).unwrap(),
                "synthetic_provenance_v1".into(),
                1,
                Some(provenance::SourceRevision::checked(1).unwrap()),
            )
            .unwrap()
        };
        let a = p.directory.prepare_source_change(request()).await.unwrap();
        let b = p.directory.prepare_source_change(request()).await.unwrap();
        let mut read=p.authority.system.holdings[0].raw().begin().await.unwrap();
        let approved=provenance::read_current_directory_source(&mut read,ldap::CONNECTION,"company").await.unwrap();
        assert_eq!(a.proposed_source().key_id,approved.key_id);
        assert!(a.proposed_source().profile_tag==approved.profile_tag);
        assert_eq!(a.proposed_source().config_fingerprint,approved.config_fingerprint);
        assert_eq!(a.proposed_source().connection_version,approved.connection_version);
        assert_eq!(a.proposed_source().revision.get(),2);
        assert!(a.proposed_source()==b.proposed_source());
        read.rollback().await.unwrap();

        let barrier=Arc::new(tokio::sync::Barrier::new(2));
        let attempts=Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let policy = || Arc::new(ConcurrentSourcePolicy{barrier:barrier.clone(),attempts:attempts.clone()}) as Arc<dyn PgDirectorySourceChangePolicy>;
        let (a, b) = tokio::join!(
            p.directory.change_source(a, policy()),
            p.directory.change_source(b, policy())
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst),3,"both first attempts retained source SHARE before their updates; the native bounded retry must perform exactly one fresh loser attempt");
        assert_eq!(revision(&p.authority.system.holdings[0]).await, 2);
        assert_eq!(source_audits(&p.authority.system.holdings[0]).await, 2);
        let pool = &p.authority.system.holdings[0];
        assert!(
            sqlx::query("DELETE FROM iam_directory_source_selections WHERE connection_id=$1")
                .bind(ldap::CONNECTION)
                .execute(pool.raw())
                .await
                .is_err()
        );
        assert!(
            sqlx::query(
                "UPDATE iam_directory_source_selections SET revision=1 WHERE connection_id=$1"
            )
            .bind(ldap::CONNECTION)
            .execute(pool.raw())
            .await
            .is_err()
        );
        assert_eq!(revision(pool).await, 2);
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn corrupted_source_afterimage_and_suppressed_complete_provenance_roll_back_atomically() {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,true).await;let pool=&p.authority.system.holdings[0];
        sqlx::raw_sql("CREATE FUNCTION synthetic_source_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.profile_tag:=decode(repeat('77',32),'hex');RETURN NEW;END $$;CREATE TRIGGER synthetic_source_tamper BEFORE UPDATE ON iam_directory_source_selections FOR EACH ROW EXECUTE FUNCTION synthetic_source_tamper()").execute(pool.raw()).await.unwrap();
        assert!(fixture::source_change::select(&p.directory,ldap::CONNECTION,1,Some(1),"synthetic_provenance_v1").await.is_err());assert_eq!(revision(pool).await,1);assert_eq!(source_audits(pool).await,1);
        sqlx::raw_sql("DROP TRIGGER synthetic_source_tamper ON iam_directory_source_selections;CREATE FUNCTION synthetic_source_suppress() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL;END $$;CREATE TRIGGER synthetic_source_suppress BEFORE INSERT ON iam_directory_observation_sources FOR EACH ROW EXECUTE FUNCTION synthetic_source_suppress()").execute(pool.raw()).await.unwrap();
        assert!(fixture::execute(&p).await.is_err());assert_eq!(fixture::effects(&p).await,0);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT name FROM iam_identities WHERE id='directory-work-identity'").fetch_one(pool.raw()).await.unwrap(),"Before refresh");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_directory_observation_sources").fetch_one(pool.raw()).await.unwrap(),0);p.server.finish().await;
    }).await;
}
#[tokio::test]
async fn explicit_key_rotation_admits_current_instances_and_missing_selected_key_never_invalidates_them()
 {
    account::with_system(|system| async move {
        let p = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        let new = rebuilt(
            &p,
            replacement_store(
                &p,
                vec![
                    ("synthetic_provenance_v1".into(), [0x31; 32]),
                    ("synthetic_provenance_v2".into(), [0x72; 32]),
                ],
            )
            .await,
        );
        fixture::source_change::select(
            &new,
            ldap::CONNECTION,
            1,
            Some(1),
            "synthetic_provenance_v2",
        )
        .await
        .unwrap();
        assert!(!active(&p).await);
        denied(
            execute(&p.work, &p, "old-key-must-not-run").await,
            "DIRECTORY_SOURCE_CONFIGURATION",
        );
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        let new_work = work(new);
        execute(&new_work, &p, "new-key-complete-refresh")
            .await
            .unwrap();
        assert!(active(&p).await);
        let contacts = p.server.stats.lock().unwrap().connections;
        denied(
            execute(&p.work, &p, "old-key-still-must-not-run").await,
            "DIRECTORY_SOURCE_CONFIGURATION",
        );
        assert!(active(&p).await);
        assert_eq!(p.server.stats.lock().unwrap().connections, contacts);
        assert_eq!(fixture::effects(&p).await, 1);
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn control_platform_route_fails_closed_without_trying_an_unversioned_or_holding_source() {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;let control=&p.authority.system.control;
        const PLATFORM:&str="00000000-0000-4000-8000-000000000099";
        sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id) VALUES($1,NULL,'platform')").bind(PLATFORM).execute(control.raw()).await.unwrap();
        assert_eq!(p.directory.current(PLATFORM).await.err().unwrap().code,"DIRECTORY_SOURCE_CONFIGURATION");
        assert!(sqlx::query_scalar::<_,Option<String>>("SELECT to_regclass('iam_directory_source_selections')::text").fetch_one(control.raw()).await.unwrap().is_none());
        assert_eq!(p.server.stats.lock().unwrap().connections,0);assert_eq!(revision(&p.authority.system.holdings[0]).await,1);p.server.finish().await;
    }).await;
}

#[tokio::test]
async fn configured_future_observation_source_denies_before_provider_or_owner_callback() {
    account::with_system(|system| async move {
        let p = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        // Establish that this exact native source/profile/real Account and
        // selected grant are initially admitted by the configured coordinator.
        let user = p
            .work
            .execute(
                "acme.example.test",
                p.authority.proof.clone(),
                p.authority.selection.clone(),
                p.authority.access.clone(),
                |_, context| Box::pin(async move { Ok(context.user_id().as_str().to_owned()) }),
            )
            .await
            .unwrap();
        assert_eq!(user, "fixture-a");
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        sqlx::query("UPDATE iam_identities SET observed_at=$1 WHERE id='directory-work-identity'")
            .bind(9_007_199_254_740_992_i64)
            .execute(p.authority.system.holdings[0].raw())
            .await
            .unwrap();
        let reached = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called = reached.clone();
        let outcome = p
            .work
            .execute(
                "acme.example.test",
                p.authority.proof.clone(),
                p.authority.selection.clone(),
                p.authority.access.clone(),
                move |_, _| {
                    called.store(true, std::sync::atomic::Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            )
            .await;
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));
        denied(outcome, "DIRECTORY_SOURCE_CONFIGURATION");
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        assert_eq!(fixture::effects(&p).await, 0);
        assert_eq!(revision(&p.authority.system.holdings[0]).await, 1);
        p.server.finish().await;
    })
    .await;
}

struct RejectPolicy {
    late: bool,
    advance: Option<Arc<dyn Fn() + Send + Sync>>,
}
struct RejectControl {
    inner: Arc<dyn PgDirectorySourceControlPolicy>,
    advance: Option<Arc<dyn Fn() + Send + Sync>>,
}
struct RejectHolding {
    inner: Box<dyn PgDirectorySourceHoldingPolicy>,
    advance: Option<Arc<dyn Fn() + Send + Sync>>,
}
fn policy_denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SYNTHETIC_SOURCE_POLICY_DENIED",
        "Explicit source policy denied this component operation.",
    )
}
impl PgDirectorySourceChangePolicy for RejectPolicy {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            let policy = fixture::source_change::SyntheticSourcePolicy {
                kind: DirectorySourceChangeKind::Rotation,
            };
            policy.prepare_control(c).await
        })
    }
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn PgDirectorySourceControlPolicy>, BusinessError> {
        Box::pin(async move {
            if !self.late {
                return Err(policy_denied().into());
            }
            let policy = fixture::source_change::SyntheticSourcePolicy {
                kind: DirectorySourceChangeKind::Rotation,
            };
            let inner = policy.retain_control(c, change, now).await?;
            Ok(Arc::new(RejectControl {
                inner,
                advance: self.advance.clone(),
            }) as Arc<dyn PgDirectorySourceControlPolicy>)
        })
    }
}
impl PgDirectorySourceControlPolicy for RejectControl {
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        self.inner.prepare_holding(c)
    }
    fn authorize_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn PgDirectorySourceHoldingPolicy>, BusinessError> {
        Box::pin(async move {
            let inner = self.inner.authorize_holding(c, change, now).await?;
            Ok(Box::new(RejectHolding {
                inner,
                advance: self.advance.clone(),
            }) as Box<dyn PgDirectorySourceHoldingPolicy>)
        })
    }
    fn deadline(&self) -> i64 {
        self.inner.deadline()
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        self.inner.require_current(c, now)
    }
}
impl PgDirectorySourceHoldingPolicy for RejectHolding {
    fn audit_actor(&self) -> &str {
        self.inner.audit_actor()
    }
    fn audit_assignment(&self) -> &str {
        self.inner.audit_assignment()
    }
    fn audit_reason(&self) -> &str {
        self.inner.audit_reason()
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.inner.require_current(c, now).await?;
            if let Some(advance) = &self.advance {
                advance();
                Ok(())
            } else {
                Err(policy_denied().into())
            }
        })
    }
}
#[tokio::test]
async fn mandatory_current_policy_and_final_retention_denial_leave_no_source_or_audit_effect() {
    account::with_system(|system| async move {
        let p = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        let pool = &p.authority.system.holdings[0];
        for late in [false, true] {
            let request = DirectorySourceChangeRequest::new(
                RecordId::new(ldap::CONNECTION).unwrap(),
                "synthetic_provenance_v1".into(),
                1,
                Some(provenance::SourceRevision::checked(1).unwrap()),
            )
            .unwrap();
            let prepared = p.directory.prepare_source_change(request).await.unwrap();
            assert_eq!(
                p.directory
                    .change_source(
                        prepared,
                        Arc::new(RejectPolicy {
                            late,
                            advance: None
                        })
                    )
                    .await
                    .err()
                    .unwrap()
                    .code,
                "SYNTHETIC_SOURCE_POLICY_DENIED"
            );
            assert_eq!(revision(pool).await, 1);
            assert_eq!(source_audits(pool).await, 1);
            assert!(active(&p).await);
        }
        assert_eq!(p.server.stats.lock().unwrap().connections, 0);
        p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn actual_deferred_source_corruption_cannot_replace_the_original_intended_afterimage() {
    account::with_system(|system|async move {
        let p=fixture::prepare(system,ldap::Scenario::Complete,false,false).await;let pool=&p.authority.system.holdings[0];
        sqlx::raw_sql("CREATE FUNCTION synthetic_source_deferred() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='directory.source-selected' THEN UPDATE iam_directory_source_selections SET revision=revision+1,profile_tag=decode(repeat('77',32),'hex') WHERE connection_id=NEW.target_id;END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER synthetic_source_deferred AFTER INSERT ON iam_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_source_deferred()").execute(pool.raw()).await.unwrap();
        assert!(fixture::source_change::select(&p.directory,ldap::CONNECTION,1,Some(1),"synthetic_provenance_v1").await.is_err());
        assert_eq!(revision(pool).await,1);assert_eq!(source_audits(pool).await,1);assert!(active(&p).await);assert_eq!(fixture::effects(&p).await,0);p.server.finish().await;
    }).await;
}

#[tokio::test]
async fn policy_wait_expiry_is_resampled_before_holding_source_commit() {
    account::with_system(|system| async move {
        let p = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        let clock = p.authority.system.scenario.clock.clone();
        let now = clock.now().get();
        let advance = Arc::new(move || {
            clock
                .0
                .store(now + 120000, std::sync::atomic::Ordering::SeqCst)
        }) as Arc<dyn Fn() + Send + Sync>;
        let request = DirectorySourceChangeRequest::new(
            RecordId::new(ldap::CONNECTION).unwrap(),
            "synthetic_provenance_v1".into(),
            1,
            Some(provenance::SourceRevision::checked(1).unwrap()),
        )
        .unwrap();
        let candidate = p.directory.prepare_source_change(request).await.unwrap();
        assert_eq!(
            p.directory
                .change_source(
                    candidate,
                    Arc::new(RejectPolicy {
                        late: true,
                        advance: Some(advance)
                    })
                )
                .await
                .err()
                .unwrap()
                .code,
            "DIRECTORY_SOURCE_CONFIGURATION"
        );
        assert_eq!(revision(&p.authority.system.holdings[0]).await, 1);
        assert_eq!(source_audits(&p.authority.system.holdings[0]).await, 1);
        p.authority
            .system
            .scenario
            .clock
            .0
            .store(now, std::sync::atomic::Ordering::SeqCst);
        p.server.finish().await;
    })
    .await;
}

// Observe native retries; never retry a callback in this test policy. Both
// original source readers meet only after their real same-row SHARE proof.
struct ConcurrentSourcePolicy {
    barrier: Arc<tokio::sync::Barrier>,
    attempts: Arc<std::sync::atomic::AtomicUsize>,
}
struct ConcurrentSourceControl {
    inner: Arc<dyn PgDirectorySourceControlPolicy>,
    barrier: Arc<tokio::sync::Barrier>,
    attempts: Arc<std::sync::atomic::AtomicUsize>,
    joined: std::sync::atomic::AtomicBool,
}
impl PgDirectorySourceChangePolicy for ConcurrentSourcePolicy {
    fn prepare_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            let policy = fixture::source_change::SyntheticSourcePolicy {
                kind: DirectorySourceChangeKind::Rotation,
            };
            policy.prepare_control(c).await
        })
    }
    fn retain_control<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Arc<dyn PgDirectorySourceControlPolicy>, BusinessError> {
        Box::pin(async move {
            let policy = fixture::source_change::SyntheticSourcePolicy {
                kind: DirectorySourceChangeKind::Rotation,
            };
            let inner = policy.retain_control(c, change, now).await?;
            Ok(Arc::new(ConcurrentSourceControl {
                inner,
                barrier: self.barrier.clone(),
                attempts: self.attempts.clone(),
                joined: std::sync::atomic::AtomicBool::new(false),
            }) as Arc<dyn PgDirectorySourceControlPolicy>)
        })
    }
}
impl PgDirectorySourceControlPolicy for ConcurrentSourceControl {
    fn prepare_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        self.attempts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.prepare_holding(c)
    }
    fn authorize_holding<'c>(
        &'c self,
        c: &'c mut PgConnection,
        change: &'c PreparedDirectorySourceChange,
        now: i64,
    ) -> PgWorkFuture<'c, Box<dyn PgDirectorySourceHoldingPolicy>, BusinessError> {
        Box::pin(async move {
            let retained = self.inner.authorize_holding(c, change, now).await?;
            if !self.joined.swap(true, std::sync::atomic::Ordering::SeqCst) {
                timeout(Duration::from_secs(3),self.barrier.wait()).await.expect("both approved source candidates must retain their actual first source read before competing writes");
            }
            Ok(retained)
        })
    }
    fn deadline(&self) -> i64 {
        self.inner.deadline()
    }
    fn require_current<'c>(
        &'c self,
        c: &'c mut PgConnection,
        now: i64,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        self.inner.require_current(c, now)
    }
}
