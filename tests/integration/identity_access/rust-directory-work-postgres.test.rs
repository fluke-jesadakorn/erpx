#[allow(dead_code, reason = "shared real session and loopback scenarios")]
#[path = "../../support/rust-directory-work.rs"]
mod fixture;
use erpx_server_shared::public::Clock;
use fixture::{account, ldap};
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;
#[tokio::test]
async fn stale_request_completes_real_observation_before_fresh_normal_work_authorization_without_minting_credentials()
 {
    account::with_system(|system| async move {
        let prepared = fixture::prepare(system, ldap::Scenario::Complete, false, true).await;
        let sessions = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session")
            .fetch_one(prepared.authority.system.control.raw())
            .await
            .unwrap();
        use std::sync::atomic::{AtomicUsize,Ordering};
        let parses=Arc::new(AtomicUsize::new(0));let effects=Arc::new(AtomicUsize::new(0));
        let input=prepared.authority.selection.clone();let count=parses.clone();let action_count=effects.clone();
        let denied=prepared.work.execute_with_sources_deferred("acme.example.test",erpx_server_identity_access::public::DeviceProof::default(),move||{count.fetch_add(1,Ordering::SeqCst);Ok(input)},prepared.authority.access.clone(),fixture::EffectSources,move|_,_|{let count=action_count.clone();Box::pin(async move{count.fetch_add(1,Ordering::SeqCst);Err::<String,_>(erpx_server_shared::public::BusinessError::new(erpx_server_shared::public::FailureKind::Forbidden,"UNEXPECTED_EFFECT","A denied request reached its effect.").into())})}).await;
        assert!(denied.is_err());assert_eq!(parses.load(Ordering::SeqCst),0);assert_eq!(effects.load(Ordering::SeqCst),0);assert_eq!(prepared.server.stats.lock().unwrap().connections,0);
        let input=prepared.authority.selection.clone();let count=parses.clone();
        let result=prepared.work.execute_with_sources_deferred("acme.example.test",prepared.authority.proof.clone(),move||{count.fetch_add(1,Ordering::SeqCst);Ok(input)},prepared.authority.access.clone(),fixture::EffectSources,|c,context|Box::pin(async move{sqlx::query("INSERT INTO synthetic_directory_work_effects(id) VALUES('actual-authorized-effect')").execute(c).await?;Ok(context.user_id().as_str().to_owned())})).await;
        assert_eq!(result.unwrap(),"fixture-a");assert_eq!(parses.load(Ordering::SeqCst),1);
        assert_eq!(fixture::effects(&prepared).await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session")
                .fetch_one(prepared.authority.system.control.raw())
                .await
                .unwrap(),
            sessions
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT observed_at FROM iam_identities WHERE id='directory-work-identity'"
            )
            .fetch_one(prepared.authority.system.holdings[0].raw())
            .await
            .unwrap(),
            prepared.authority.system.scenario.clock.now().get()
        );
        {
            let stats = prepared.server.stats.lock().unwrap();
            assert_eq!(stats.service_binds, 1);
            assert_eq!(stats.user_binds, 0);
        }
        prepared.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn current_observation_uses_current_normal_authority_without_provider_io() {
    account::with_system(|system| async move {
        let prepared = fixture::prepare(system, ldap::Scenario::Complete, false, false).await;
        assert_eq!(fixture::execute(&prepared).await.unwrap(), "fixture-a");
        assert_eq!(fixture::effects(&prepared).await, 1);
        assert_eq!(prepared.server.stats.lock().unwrap().connections, 0);
        prepared.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn revoked_session_precedes_malformed_optional_directory_configuration_and_never_contacts_provider()
 {
    account::with_system(|system|async move{
 let prepared=fixture::prepare(system,ldap::Scenario::Complete,false,true).await;let pool=&prepared.authority.system.holdings[0];
 sqlx::query("INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES($1,'company','fixture-a',$2)").bind(&prepared.authority.child_id).bind(prepared.authority.system.scenario.clock.now().get()).execute(pool.raw()).await.unwrap();
 sqlx::query("UPDATE iam_connections SET config='{\"mode\":\"ldaps\",\"secretProfile\":false}' WHERE id=$1").bind(ldap::CONNECTION).execute(pool.raw()).await.unwrap();
 assert!(matches!(fixture::execute(&prepared).await,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(error)) if error.code=="SESSION_REVOKED"));assert_eq!(fixture::effects(&prepared).await,0);assert_eq!(prepared.server.stats.lock().unwrap().connections,0);prepared.server.finish().await;
}).await;
}
#[tokio::test]
async fn actual_family_sign_out_can_commit_during_provider_io_and_the_original_request_cannot_authorize()
 {
    account::with_system(|system|async move{
 let prepared=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,true,true).await);let request={let prepared=prepared.clone();tokio::spawn(async move{fixture::execute(&prepared).await})};fixture::wait_provider(&prepared).await;
 // NOWAIT proves actual source leases ended; this is not a larger timeout or
 // an assumed idle pool. The real Account command must also fence both Holdings.
 for pool in [&prepared.authority.system.control,&prepared.authority.system.holdings[0]]{let mut tx=pool.raw().begin().await.unwrap();let statement=if std::ptr::eq(pool,&prepared.authority.system.control){"LOCK TABLE iam_connection_routes,holding_bindings IN ACCESS EXCLUSIVE MODE NOWAIT"}else{"LOCK TABLE iam_connections,iam_identities,iam_members,active_work_contexts IN ACCESS EXCLUSIVE MODE NOWAIT"};sqlx::query(statement).execute(&mut *tx).await.unwrap();tx.rollback().await.unwrap();}
 timeout(Duration::from_secs(3),prepared.authority.sign_out()).await.expect("no Control/Holding proof may span provider I/O").unwrap();prepared.server.resume.notify_one();assert!(request.await.unwrap().is_err());assert_eq!(fixture::effects(&prepared).await,0);assert_eq!(sqlx::query_scalar::<_,String>("SELECT health FROM iam_connections WHERE id=$1").bind(ldap::CONNECTION).fetch_one(prepared.authority.system.holdings[0].raw()).await.unwrap(),"untested");
 let prepared=Arc::try_unwrap(prepared).ok().unwrap();prepared.server.finish().await;
}).await;
}
#[tokio::test]
async fn original_selection_cas_is_rechecked_after_provider_io_without_adopting_a_different_current_selection()
 {
    account::with_system(|system|async move{
 let prepared=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,true,true).await);let request={let p=prepared.clone();tokio::spawn(async move{fixture::execute(&p).await})};fixture::wait_provider(&prepared).await;
 sqlx::query("UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=$1 AND company_id='company'").bind(&prepared.authority.child_id).execute(prepared.authority.system.holdings[0].raw()).await.unwrap();prepared.server.resume.notify_one();assert!(request.await.unwrap().is_err());assert_eq!(fixture::effects(&prepared).await,0);let p=Arc::try_unwrap(prepared).ok().unwrap();p.server.finish().await;
}).await;
}
#[tokio::test]
async fn role_permission_withdrawal_during_provider_io_is_read_fresh_instead_of_using_preflight_metadata()
 {
    account::with_system(|system|async move{
 let prepared=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,true,true).await);let request={let p=prepared.clone();tokio::spawn(async move{fixture::execute(&p).await})};fixture::wait_provider(&prepared).await;
 sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',2,'[]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',0); UPDATE iam_roles SET published_revision=2 WHERE company_id='*' AND id='custom_queued_legal'").execute(prepared.authority.system.holdings[0].raw()).await.unwrap();prepared.server.resume.notify_one();assert!(request.await.unwrap().is_err());assert_eq!(fixture::effects(&prepared).await,0);let p=Arc::try_unwrap(prepared).ok().unwrap();p.server.finish().await;
}).await;
}
#[tokio::test]
async fn connection_configuration_changed_during_provider_io_rejects_the_old_observation_without_relabeling_provider_health()
 {
    account::with_system(|system|async move{
 let prepared=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,true,true).await);let request={let p=prepared.clone();tokio::spawn(async move{fixture::execute(&p).await})};fixture::wait_provider(&prepared).await;
 sqlx::query("UPDATE iam_connections SET version=version+1,config=jsonb_set(config,'{authority}','\"new-reviewed-source\"') WHERE id=$1").bind(ldap::CONNECTION).execute(prepared.authority.system.holdings[0].raw()).await.unwrap();prepared.server.resume.notify_one();assert!(request.await.unwrap().is_err());assert_eq!(fixture::effects(&prepared).await,0);assert_eq!(sqlx::query_scalar::<_,String>("SELECT health FROM iam_connections WHERE id=$1").bind(ldap::CONNECTION).fetch_one(prepared.authority.system.holdings[0].raw()).await.unwrap(),"untested");let p=Arc::try_unwrap(prepared).ok().unwrap();p.server.finish().await;
}).await;
}
#[tokio::test]
async fn incomplete_provider_observation_cannot_commit_partial_rows_or_authorize_and_is_marked_as_a_real_source_failure()
 {
    account::with_system(|system| async move {
        let prepared = fixture::prepare(system, ldap::Scenario::Truncated, false, true).await;
        assert!(fixture::execute(&prepared).await.is_err());
        assert_eq!(fixture::effects(&prepared).await, 0);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT health FROM iam_connections WHERE id=$1")
                .bind(ldap::CONNECTION)
                .fetch_one(prepared.authority.system.holdings[0].raw())
                .await
                .unwrap(),
            "error"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM iam_identities WHERE id='directory-work-identity'"
            )
            .fetch_one(prepared.authority.system.holdings[0].raw())
            .await
            .unwrap(),
            "Before refresh"
        );
        prepared.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn queued_profile_reload_does_not_deadlock_reconciliation_and_generation_is_retained_through_the_work_commit()
 {
    account::with_system(|system|async move{
 let prepared=Arc::new(fixture::prepare(system,ldap::Scenario::Complete,true,true).await);let entered=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());
 let request={let p=prepared.clone();let entered=entered.clone();let release=release.clone();tokio::spawn(async move{
  p.work.execute("acme.example.test",p.authority.proof.clone(),p.authority.selection.clone(),p.authority.access.clone(),move|c,context|{let entered=entered.clone();let release=release.clone();Box::pin(async move{sqlx::query("INSERT INTO synthetic_directory_work_effects(id) VALUES('generation-bound-effect')").execute(c).await?;entered.notify_one();release.notified().await;Ok(context.user_id().as_str().to_owned())})}).await
 })};fixture::wait_provider(&prepared).await;
 ldap::write_profile(prepared.profile.directory.path(),Some(&ldap::material().ca),"ldap.fixture.test",true);let profiles=prepared.profile.profiles.clone();let reload=tokio::spawn(async move{profiles.reload().await});
 // Observe actual fair writer queue priority. A guessed sleep would not prove
 // the reload finished file loading and is waiting behind the live generation.
 timeout(Duration::from_secs(2),async{loop{if timeout(Duration::from_millis(10),prepared.profile.profiles.fingerprint("fixture",ldap::CONNECTION)).await.is_err(){break;}tokio::task::yield_now().await;}}).await.expect("observed actual queued generation writer");assert!(!reload.is_finished());
 prepared.server.resume.notify_one();timeout(Duration::from_secs(3),entered.notified()).await.expect("reconciliation/final authorization never reacquire the retained fair read lock");assert!(!reload.is_finished());release.notify_one();assert_eq!(timeout(Duration::from_secs(3),request).await.unwrap().unwrap().unwrap(),"fixture-a");timeout(Duration::from_secs(3),reload).await.unwrap().unwrap().unwrap();assert_eq!(fixture::effects(&prepared).await,1);
 let p=Arc::try_unwrap(prepared).ok().unwrap();p.server.finish().await;
}).await;
}
#[tokio::test]
async fn original_selected_grant_deadline_expiring_during_provider_io_cannot_be_extended_by_refresh()
 {
    account::with_system(|system| async move {
        let prepared =
            Arc::new(fixture::prepare(system, ldap::Scenario::Complete, true, true).await);
        let now = prepared.authority.system.scenario.clock.now().get();
        sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='queued-account-grant'")
            .bind(now + 500)
            .execute(prepared.authority.system.holdings[0].raw())
            .await
            .unwrap();
        let request = {
            let p = prepared.clone();
            tokio::spawn(async move { fixture::execute(&p).await })
        };
        fixture::wait_provider(&prepared).await;
        prepared.authority.system.scenario.clock.advance(501);
        prepared.server.resume.notify_one();
        assert!(request.await.unwrap().is_err());
        assert_eq!(fixture::effects(&prepared).await, 0);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT health FROM iam_connections WHERE id=$1")
                .bind(ldap::CONNECTION)
                .fetch_one(prepared.authority.system.holdings[0].raw())
                .await
                .unwrap(),
            "untested"
        );
        let p = Arc::try_unwrap(prepared).ok().unwrap();
        p.server.finish().await;
    })
    .await;
}
