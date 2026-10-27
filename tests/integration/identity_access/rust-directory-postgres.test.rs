#[allow(dead_code, reason = "shared real Account/SSO scenarios")]
#[path = "../../support/rust-account-revocation.rs"]
mod account;
#[allow(dead_code, reason = "shared owned loopback protocol scenarios")]
#[path = "../../support/rust-directory-ldaps.rs"]
mod ldap;
#[path = "../../support/rust-directory-source-change.rs"]
mod source_change;
use erpx_server_identity_access::public::{directory::*, *};
use erpx_server_shared::public::{Clock, HoldingId};
use sqlx::Row;
use std::{collections::BTreeMap, sync::Arc};

struct Setup {
    fixture: ldap::Fixture,
    directory: Arc<PostgresDirectory>,
}
async fn setup(system: &account::System, port: u16) -> Setup {
    let fixture =
        ldap::Fixture::new(port, false, Some(&ldap::material().ca), "ldap.fixture.test").await;
    let holding = &system.holdings[0];
    let now = system.scenario.clock.now().get();
    sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id) VALUES($1,'h001','c001')").bind(ldap::CONNECTION).execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at) VALUES($1,'c001','Synthetic Directory','univention',$2,'active',1,1,'fixture',0)").bind(ldap::CONNECTION).bind(serde_json::to_value(&fixture.descriptor.config).unwrap()).execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,email,enabled,groups_json,observed_at,user_id) VALUES('identity-a','c001',$1,'alice-id','alice','Synthetic Alice','a@example.test',true,'[]',$2,'fixture-a')").bind(ldap::CONNECTION).bind(now).execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,connection_id,identity_id) VALUES('c001','fixture-a','p-a','active','directory',$1,'identity-a'),('c001','fixture-b','p-b','active','local',NULL,NULL)").bind(ldap::CONNECTION).execute(holding.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('c001','company','directory','Synthetic Company',NULL,'c001',NULL,'active'); INSERT INTO iam_roles(company_id,id,name,published_revision) VALUES('c001','directory_delegate','Reviewed delegator',1),('c001','directory_member','Reviewed member',1); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('c001','directory_delegate',1,'[]','[\"iam.self\"]','[\"holding\"]','fixture',0),('c001','directory_member',1,'[\"iam.self\"]','[]','[\"holding\"]','fixture',0); INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('directory-parent','fixture-b','directory_delegate','[]','c001',0,4000000000000,'holding')").execute(holding.raw()).await.unwrap();
    let holdings = BTreeMap::from([
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
    ]);
    let directory = Arc::new(
        PostgresDirectory::new(
            DirectoryDatabaseBindings {
                control: system.control.clone(),
                holdings,
                registry: account::fixture::account::holding_registry(),
                environment: "development".into(),
            },
            fixture.profiles.clone(),
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
            account::invalidator(&system.holdings, system.scenario.clock.clone()),
        )
        .unwrap(),
    );
    source_change::select(
        &directory,
        ldap::CONNECTION,
        1,
        None,
        "synthetic_provenance_v1",
    )
    .await
    .unwrap();
    Setup { fixture, directory }
}
fn snapshot(now: i64, enabled: bool, member: bool) -> CompleteDirectorySnapshot {
    CompleteDirectorySnapshot::checked(
        vec![ObservedDirectoryUser {
            external_id: "alice-id".into(),
            username: "alice".into(),
            name: "Synthetic Alice current".into(),
            email: Some("a@example.test".into()),
            enabled,
            group_ids: if member {
                vec!["group-id".into()]
            } else {
                vec![]
            },
        }],
        vec![ObservedDirectoryGroup {
            external_id: "group-id".into(),
            name: "Synthetic group".into(),
        }],
        DirectoryHierarchy {
            containers: vec![],
            placements: vec![],
            group_edges: vec![],
        },
        now,
    )
    .unwrap()
}
async fn mapping(system: &account::System) {
    sqlx::query("INSERT INTO iam_mappings(id,company_id,connection_id,group_id,role_id,scope_kind,scope_node_id,valid_until,parent_id) VALUES('mapping','c001',$1,'group-id','directory_member','holding',NULL,3900000000000,'directory-parent')").bind(ldap::CONNECTION).execute(system.holdings[0].raw()).await.unwrap();
}
#[tokio::test]
async fn complete_snapshot_preserves_reviewed_links_reconciles_mapping_and_never_recreates_revoked_grant()
 {
    account::with_system(|system|async move{
 let prepared=setup(&system,49155).await;mapping(&system).await;let d=prepared.directory.current(ldap::CONNECTION).await.unwrap();let now=system.scenario.clock.now().get();
 prepared.directory.apply(&d,snapshot(now,true,true)).await.unwrap();
 let row=sqlx::query("SELECT i.id,i.user_id,i.username,(SELECT count(*) FROM assignments WHERE source='directory') grants,(SELECT count(*) FROM iam_audit WHERE action='directory.synchronized') audit FROM iam_identities i WHERE i.id='identity-a'").fetch_one(system.holdings[0].raw()).await.unwrap();assert_eq!(row.get::<String,_>("user_id"),"fixture-a");assert_eq!(row.get::<i64,_>("grants"),1);assert_eq!(row.get::<i64,_>("audit"),1);
 system.scenario.clock.advance(1);
 sqlx::raw_sql("CREATE FUNCTION synthetic_directory_suppress_revocation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source='directory' THEN NEW.revoked_at=NULL; END IF; RETURN NEW; END $$; CREATE TRIGGER synthetic_directory_suppress_revocation BEFORE UPDATE OF revoked_at ON assignments FOR EACH ROW EXECUTE FUNCTION synthetic_directory_suppress_revocation()").execute(system.holdings[0].raw()).await.unwrap();
 assert!(prepared.directory.apply(&d,snapshot(now+1,true,false)).await.is_err(),"a trigger cannot suppress the intended Directory revocation");
 assert!(sqlx::query_scalar::<_,bool>("SELECT groups_json @> '[\"group-id\"]'::jsonb FROM iam_identities WHERE id='identity-a'").fetch_one(system.holdings[0].raw()).await.unwrap());
 sqlx::raw_sql("DROP TRIGGER synthetic_directory_suppress_revocation ON assignments").execute(system.holdings[0].raw()).await.unwrap();
 prepared.directory.apply(&d,snapshot(now+1,true,false)).await.unwrap();let revoked:bool=sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap();assert!(revoked);
 system.scenario.clock.advance(1);prepared.directory.apply(&d,snapshot(now+2,true,true)).await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap(),1);
 let _=&prepared.fixture;
}).await;
}
#[tokio::test]
async fn older_snapshot_and_failure_cannot_override_newer_complete_success() {
    account::with_system(|system|async move{
 let prepared=setup(&system,49155).await;let d=prepared.directory.current(ldap::CONNECTION).await.unwrap();let now=system.scenario.clock.now().get();prepared.directory.apply(&d,snapshot(now,true,true)).await.unwrap();
 let outcome=prepared.directory.apply(&d,snapshot(now-1,false,false)).await.unwrap();assert!(matches!(outcome,SynchronizationOutcome::NewerCompleteObservationRetained));prepared.directory.failed(&d,now-1,"DIRECTORY_SYNC_FAILED").await.unwrap();
 let row=sqlx::query("SELECT c.health,i.enabled,i.observed_at FROM iam_connections c JOIN iam_identities i ON i.connection_id=c.id WHERE i.id='identity-a'").fetch_one(system.holdings[0].raw()).await.unwrap();assert!(row.get::<bool,_>("enabled"));assert_eq!(row.get::<String,_>("health"),"healthy");assert_eq!(row.get::<i64,_>("observed_at"),now);
 prepared.directory.failed(&d,now,"DIRECTORY_SYNC_FAILED").await.unwrap();assert_eq!(sqlx::query_scalar::<_,i64>("SELECT observed_at FROM iam_identities WHERE id='identity-a'").fetch_one(system.holdings[0].raw()).await.unwrap(),now-300000);
}).await;
}
#[tokio::test]
async fn deferred_audit_denial_rolls_back_snapshot_and_mapping_effects() {
    account::with_system(|system|async move{
 let prepared=setup(&system,49155).await;mapping(&system).await;let d=prepared.directory.current(ldap::CONNECTION).await.unwrap();
 sqlx::raw_sql("CREATE FUNCTION synthetic_directory_deny() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic final audit denial' USING ERRCODE='23514'; END $$; CREATE CONSTRAINT TRIGGER synthetic_directory_final AFTER INSERT ON iam_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_directory_deny()").execute(system.holdings[0].raw()).await.unwrap();
 assert!(prepared.directory.apply(&d,snapshot(system.scenario.clock.now().get(),true,true)).await.is_err());
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_groups").fetch_one(system.holdings[0].raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,String>("SELECT name FROM iam_identities WHERE id='identity-a'").fetch_one(system.holdings[0].raw()).await.unwrap(),"Synthetic Alice");
}).await;
}
#[tokio::test]
async fn real_ldaps_credentials_issue_opaque_session_and_retire_actual_children_in_both_holdings() {
    account::with_system(|system|async move{
 let server=ldap::Server::start(ldap::Scenario::Complete,false,None).await;let prepared=setup(&system,server.port).await;let central=account::login(&system,"a@example.test",None).await;let a=account::child(&system,&central,account::ACME).await;let b=account::child(&system,&central,account::NORTHSTAR).await;account::context(&system.holdings[0],&a,"fixture-a").await;account::context(&system.holdings[1],&b,"fixture-a").await;
 let auth=DirectoryAuthentication::new(prepared.directory.clone(),Arc::new(LdapDirectoryProvider::new(prepared.fixture.profiles.clone(),system.scenario.clock.clone())),prepared.directory.clone(),prepared.directory.clone(),prepared.directory.clone(),system.scenario.clock.clone());let revision=system.scenario.auth.devices(&central).await.unwrap().revision;
 let signed=auth.sign_in(ldap::CONNECTION,"alice",ldap::USER_PASSWORD.into(),&central,Some(revision)).await.unwrap();assert_eq!(signed.result.session.user.id,"fixture-a");assert_eq!(signed.result.devices.revision,revision+1);assert!(signed.browser_secret.is_none());
 for(pool,child)in system.holdings.iter().zip([&a,&b]){assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1 AND user_id='fixture-a') AND NOT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1)").bind(&child.result.session_id).fetch_one(pool.raw()).await.unwrap());}
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM account WHERE \"userId\"='fixture-a'").fetch_one(system.control.raw()).await.unwrap(),1);server.finish().await;
}).await;
}
#[tokio::test]
async fn invalid_credentials_remain_enumeration_safe_and_attempts_commit_through_rejected_ninth() {
    account::with_system(|system| async move {
        let server = ldap::Server::start(ldap::Scenario::Complete, false, None).await;
        let prepared = setup(&system, server.port).await;
        let auth = DirectoryAuthentication::new(
            prepared.directory.clone(),
            Arc::new(LdapDirectoryProvider::new(
                prepared.fixture.profiles.clone(),
                system.scenario.clock.clone(),
            )),
            prepared.directory.clone(),
            prepared.directory.clone(),
            prepared.directory.clone(),
            system.scenario.clock.clone(),
        );
        for attempt in 1..=10 {
            let result = auth
                .sign_in(
                    ldap::CONNECTION,
                    "alice",
                    "wrong-synthetic-password".into(),
                    &account::fixture::account::empty(),
                    None,
                )
                .await;
            let Err(error) = result else {
                panic!("invalid provider credentials granted a session")
            };
            assert_eq!(
                error.code,
                if attempt <= 8 {
                    "DIRECTORY_LOGIN_FAILED"
                } else {
                    "IAM_RATE_LIMIT"
                }
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i32>("SELECT attempts FROM iam_throttles")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            10
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
        server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn credential_expiry_after_actual_policy_lock_wait_denies_without_ack_or_new_browser() {
    account::with_system(|system|async move{
 let server=ldap::Server::start(ldap::Scenario::Complete,false,None).await;let prepared=setup(&system,server.port).await;let d=prepared.directory.current(ldap::CONNECTION).await.unwrap();let provider=LdapDirectoryProvider::new(prepared.fixture.profiles.clone(),system.scenario.clock.clone());let subject=provider.verify(&d,"alice",DirectoryPassword::new(ldap::USER_PASSWORD.into()).unwrap()).await.unwrap();
 let mut writer=system.holdings[0].raw().begin().await.unwrap();sqlx::query("UPDATE iam_connections SET health='healthy' WHERE id=$1").bind(ldap::CONNECTION).execute(&mut *writer).await.unwrap();
 let directory=prepared.directory.clone();let task=tokio::spawn(async move{directory.issue(subject,d,&account::fixture::account::empty(),None).await});
 tokio::time::timeout(std::time::Duration::from_secs(3),async{loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'LOCK TABLE iam_connections,iam_members,iam_identities,iam_mappings%')").fetch_one(system.holdings[0].raw()).await.unwrap();if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.expect("observed actual second backend waiting for the current Directory policy source");
 system.scenario.clock.advance(120000);writer.rollback().await.unwrap();assert!(matches!(task.await.unwrap(),Err(DirectoryAdmissionError::IdentityDenied)));
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM session").fetch_one(system.control.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM account_device_browsers").fetch_one(system.control.raw()).await.unwrap(),0);server.finish().await;
}).await;
}
#[tokio::test]
async fn second_holding_failure_keeps_first_tombstone_without_control_ack_and_normal_retry_recovers()
 {
    account::with_system(|system|async move{
 let server=ldap::Server::start(ldap::Scenario::Complete,false,None).await;let prepared=setup(&system,server.port).await;let central=account::login(&system,"a@example.test",None).await;let a=account::child(&system,&central,account::ACME).await;let b=account::child(&system,&central,account::NORTHSTAR).await;account::context(&system.holdings[0],&a,"fixture-a").await;account::context(&system.holdings[1],&b,"fixture-a").await;
 sqlx::raw_sql("CREATE FUNCTION synthetic_directory_retirement_deny() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic second Holding denial' USING ERRCODE='23514'; END $$; CREATE TRIGGER synthetic_directory_retirement BEFORE DELETE ON active_work_contexts FOR EACH ROW EXECUTE FUNCTION synthetic_directory_retirement_deny()").execute(system.holdings[1].raw()).await.unwrap();
 let auth=DirectoryAuthentication::new(prepared.directory.clone(),Arc::new(LdapDirectoryProvider::new(prepared.fixture.profiles.clone(),system.scenario.clock.clone())),prepared.directory.clone(),prepared.directory.clone(),prepared.directory.clone(),system.scenario.clock.clone());let revision=system.scenario.auth.devices(&central).await.unwrap().revision;
 let Err(error)=auth.sign_in(ldap::CONNECTION,"alice",ldap::USER_PASSWORD.into(),&central,Some(revision)).await else{panic!("partial Holding failure returned a session")};assert_eq!(error.code,"DEVICE_ACCOUNT_REENTRY_REQUIRED");
 assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1) AND NOT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1)").bind(&a.result.session_id).fetch_one(system.holdings[0].raw()).await.unwrap());
 assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id=$1) AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1)").bind(&b.result.session_id).fetch_one(system.holdings[1].raw()).await.unwrap());assert_eq!(system.scenario.auth.devices(&central).await.unwrap().revision,revision);assert!(system.scenario.auth.session(&central,RequestAudience::Account).await.unwrap().is_some());
 sqlx::raw_sql("DROP TRIGGER synthetic_directory_retirement ON active_work_contexts").execute(system.holdings[1].raw()).await.unwrap();let signed=auth.sign_in(ldap::CONNECTION,"alice",ldap::USER_PASSWORD.into(),&central,Some(revision)).await.unwrap();assert_eq!(signed.result.devices.revision,revision+1);assert!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=$1)").bind(&b.result.session_id).fetch_one(system.holdings[1].raw()).await.unwrap());server.finish().await;
}).await;
}
#[tokio::test]
async fn final_deferred_parent_revision_change_rolls_back_observations_and_all_new_grants() {
    account::with_system(|system|async move{
 let prepared=setup(&system,49155).await;mapping(&system).await;let d=prepared.directory.current(ldap::CONNECTION).await.unwrap();
 sqlx::raw_sql("CREATE FUNCTION synthetic_directory_parent_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='directory-parent'; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER synthetic_directory_parent AFTER INSERT ON iam_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_directory_parent_change()").execute(system.holdings[0].raw()).await.unwrap();
 assert!(prepared.directory.apply(&d,snapshot(system.scenario.clock.now().get(),true,true)).await.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM assignments WHERE id='directory-parent'").fetch_one(system.holdings[0].raw()).await.unwrap(),1);
}).await;
}
#[tokio::test]
async fn actual_axum_directory_ingress_keeps_exact_account_origin_closed_body_and_cookie_only_session()
 {
    account::with_system(|system|async move{
 use axum::{body::{Body,to_bytes},http::{Request,StatusCode,header}};use tower::ServiceExt;
 let server=ldap::Server::start(ldap::Scenario::Complete,false,None).await;let prepared=setup(&system,server.port).await;let auth=Arc::new(DirectoryAuthentication::new(prepared.directory.clone(),Arc::new(LdapDirectoryProvider::new(prepared.fixture.profiles.clone(),system.scenario.clock.clone())),prepared.directory.clone(),prepared.directory.clone(),prepared.directory.clone(),system.scenario.clock.clone()));let router=directory_account_router(system.scenario.auth.clone(),auth);
 let body=serde_json::json!({"connectionId":ldap::CONNECTION,"username":"alice","password":ldap::USER_PASSWORD});
 for(host,origin,body,status)in [("account.example.test",Some("https://acme.example.test"),body.to_string(),StatusCode::FORBIDDEN),("acme.example.test",Some("https://acme.example.test"),body.to_string(),StatusCode::NOT_FOUND),("account.example.test",None,body.to_string(),StatusCode::FORBIDDEN),("account.example.test",Some("https://account.example.test"),serde_json::json!({"connectionId":ldap::CONNECTION,"username":"alice","password":null}).to_string(),StatusCode::BAD_REQUEST)]{
  let mut request=Request::builder().method("POST").uri("/api/auth/directory/sign-in").header("host",host).header("content-type","application/json");if let Some(origin)=origin{request=request.header("origin",origin);}let response=router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();assert_eq!(response.status(),status);assert!(!response.headers().contains_key(header::SET_COOKIE));
 }
 assert_eq!(server.stats.lock().unwrap().connections,0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_throttles").fetch_one(system.control.raw()).await.unwrap(),0);
 let response=router.oneshot(Request::builder().method("POST").uri("/api/auth/directory/sign-in").header("host","account.example.test").header("origin","https://account.example.test").header("content-type","application/json").body(Body::from(format!(r#"{{"connectionId":"{}","username":"wrong-first-value","username":"alice","password":"{}","theme":"dark","actor":{{"userId":"untrusted-other-user","permissions":["*"]}},"grant":{{"role":"holding_owner"}},"sessionId":"untrusted-session"}}"#,ldap::CONNECTION,ldap::USER_PASSWORD))).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers().get("cache-control").unwrap(),"no-store");
 let cookies=response.headers().get_all(header::SET_COOKIE).iter().map(|v|v.to_str().unwrap().to_owned()).collect::<Vec<_>>();assert_eq!(cookies.len(),3);for cookie in &cookies{assert!(cookie.contains("; Secure; HttpOnly; SameSite=Lax"));assert!(!cookie.contains("Domain="));}
 let payload=to_bytes(response.into_body(),8192).await.unwrap();assert_eq!(&payload[..],b"{\"success\":true}");let cookie_header=cookies.iter().map(|v|v.split(';').next().unwrap()).collect::<Vec<_>>().join("; ");let response=account_auth_router(system.scenario.auth.clone()).oneshot(Request::builder().uri("/api/auth/get-session").header("host","account.example.test").header(header::COOKIE,cookie_header).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let session:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),8192).await.unwrap()).unwrap();assert_eq!(session["user"]["id"],"fixture-a");server.finish().await;
}).await;
}
#[tokio::test]
async fn axum_throttled_directory_response_keeps_real_429_and_has_no_cookie_ack() {
    account::with_system(|system|async move{
 use axum::{body::Body,http::{Request,StatusCode,header}};use tower::ServiceExt;
 let prepared=setup(&system,49155).await;for _ in 0..8{prepared.directory.attempt(ldap::CONNECTION,"alice").await.unwrap();}let auth=Arc::new(DirectoryAuthentication::new(prepared.directory.clone(),Arc::new(LdapDirectoryProvider::new(prepared.fixture.profiles.clone(),system.scenario.clock.clone())),prepared.directory.clone(),prepared.directory.clone(),prepared.directory.clone(),system.scenario.clock.clone()));let router=directory_account_router(system.scenario.auth.clone(),auth);
 let response=router.oneshot(Request::builder().method("POST").uri("/api/auth/directory/sign-in").header("host","account.example.test").header("origin","https://account.example.test").header("content-type","application/json").body(Body::from(serde_json::json!({"connectionId":ldap::CONNECTION,"username":"alice","password":"not-checked-after-throttle"}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::TOO_MANY_REQUESTS);assert!(!response.headers().contains_key(header::SET_COOKIE));assert_eq!(sqlx::query_scalar::<_,i32>("SELECT attempts FROM iam_throttles").fetch_one(system.control.raw()).await.unwrap(),9);
}).await;
}
#[tokio::test]
async fn immediate_grant_parent_revision_change_cannot_replace_the_original_policy_witness() {
    account::with_system(|system|async move{
        let prepared=setup(&system,49155).await;
        mapping(&system).await;
        let descriptor=prepared.directory.current(ldap::CONNECTION).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION synthetic_directory_immediate_parent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source='directory' THEN UPDATE assignments SET version=version+1 WHERE id=NEW.parent_id; END IF; RETURN NEW; END $$; CREATE TRIGGER synthetic_directory_immediate_parent AFTER INSERT ON assignments FOR EACH ROW EXECUTE FUNCTION synthetic_directory_immediate_parent()")
            .execute(system.holdings[0].raw()).await.unwrap();
        assert!(prepared.directory.apply(&descriptor,snapshot(system.scenario.clock.now().get(),true,true)).await.is_err(),"grant insertion must not replace the original parent evidence with its own immediate trigger mutation");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM assignments WHERE id='directory-parent'").fetch_one(system.holdings[0].raw()).await.unwrap(),1);
        sqlx::raw_sql("CREATE OR REPLACE FUNCTION synthetic_directory_immediate_parent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source='directory' THEN UPDATE assignments SET user_id='fixture-b' WHERE id=NEW.id; END IF; RETURN NEW; END $$").execute(system.holdings[0].raw()).await.unwrap();
        assert!(prepared.directory.apply(&descriptor,snapshot(system.scenario.clock.now().get(),true,true)).await.is_err(),"an immediate trigger cannot replace the intended grant recipient");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM assignments WHERE source='directory'").fetch_one(system.holdings[0].raw()).await.unwrap(),0);
    }).await;
}
