#[path = "../../support/rust-postgres.rs"]
mod database;
#[allow(
    dead_code,
    reason = "this suite uses signup fixture setup; policy denial cases run in the authority suite"
)]
#[path = "../../support/rust-account-invitation.rs"]
mod fixture;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::HoldingId;
use std::{collections::BTreeMap, sync::Arc};
use tower::ServiceExt;

fn invitations(
    control: &DatabasePool,
    holding: &DatabasePool,
    scenario: &fixture::credentials::Scenario,
) -> Arc<AccountInvitations> {
    Arc::new(AccountInvitations::new(
        Arc::new(PostgresAccountInvitations::new(
            control.clone(),
            BTreeMap::from([(HoldingId::new("h001").unwrap(), holding.clone())]),
            scenario.clock.clone(),
        )),
        scenario.credentials.clone(),
        Arc::new(Argon2Passwords::new(2).unwrap()),
        Arc::new(SystemAuthenticationSecrets),
        scenario.clock.clone(),
    ))
}
async fn grant_effects(holding: &DatabasePool) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM assignments WHERE user_id='usr_invite_invitation'),(SELECT count(*) FROM iam_audit WHERE action='identity.account.activate'),(SELECT count(*) FROM command_receipts WHERE operation='identity.account.activate')").fetch_one(holding.raw()).await.unwrap()
}
#[tokio::test]
async fn actual_acceptance_creates_one_grant_and_people_link_then_finalizes_control() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await; let invitations=invitations(&control,&holding,&scenario);
        let info=invitations.inspect(token.expose().into()).await.unwrap(); assert_eq!(info.email,"invited@example.test");assert!(info.credential.is_none());
        invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.unwrap();
        assert_eq!(grant_effects(&holding).await,(1,1,1));
        let person:(Option<String>,i64)=sqlx::query_as("SELECT user_id,version FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(); assert_eq!(person,(Some("usr_invite_invitation".into()),2));
        let assignment:(String,String,String,Option<String>,Option<String>)=sqlx::query_as("SELECT role,modules_json,scope_kind,scope_node_id,parent_id FROM assignments WHERE id='assignment-invitation'").fetch_one(holding.raw()).await.unwrap();
        assert_eq!(assignment.0,"finance_preparer"); assert!(!serde_json::from_str::<Vec<String>>(&assignment.1).unwrap().is_empty()); assert_eq!((assignment.2,assignment.3,assignment.4),("department".into(),Some("department".into()),Some("parent".into())));
        assert_eq!(sqlx::query_scalar::<_,Option<i64>>("SELECT accepted_at FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap(),Some(fixture::credentials::account::NOW));
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());assert_eq!(grant_effects(&holding).await,(1,1,1));
    }).await}).await;
}
#[tokio::test]
async fn failed_holding_audit_rolls_back_link_and_grant_and_requires_original_password_to_resume() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await; let invitations=invitations(&control,&holding,&scenario);
        sqlx::raw_sql("CREATE FUNCTION fail_invitation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='identity.account.activate' THEN RAISE EXCEPTION 'synthetic audit failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_invitation_audit BEFORE INSERT ON iam_audit FOR EACH ROW EXECUTE FUNCTION fail_invitation_audit()").execute(holding.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());
        assert_eq!(grant_effects(&holding).await,(0,0,0)); assert_eq!(sqlx::query_scalar::<_,Option<String>>("SELECT user_id FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(),None);
        assert_eq!(fixture::effects(&control).await,5); assert_eq!(sqlx::query_scalar::<_,Option<i64>>("SELECT accepted_at FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap(),None);
        sqlx::query("DROP TRIGGER fail_invitation_audit ON iam_audit").execute(holding.raw()).await.unwrap();
        let wrong=invitations.accept(token.expose().into(),"wrong-synthetic-password".into()).await; assert!(matches!(wrong,Err(error) if error.code=="ACTIVATION_RETRY_PASSWORD")); assert_eq!(grant_effects(&holding).await,(0,0,0));
        invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.unwrap(); assert_eq!(grant_effects(&holding).await,(1,1,1)); assert_eq!(fixture::effects(&control).await,5);
    }).await}).await;
}
#[tokio::test]
async fn failed_control_finalization_resumes_existing_holding_receipt_without_duplicate_effects() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await; let invitations=invitations(&control,&holding,&scenario);
        sqlx::raw_sql("CREATE FUNCTION fail_invitation_finalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.accepted_at IS NOT NULL THEN RAISE EXCEPTION 'synthetic Control finalization failure' USING ERRCODE='23514'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_invitation_finalize BEFORE UPDATE ON account_invitations FOR EACH ROW EXECUTE FUNCTION fail_invitation_finalize()").execute(control.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err()); assert_eq!(grant_effects(&holding).await,(1,1,1));
        assert_eq!(sqlx::query_scalar::<_,Option<i64>>("SELECT accepted_at FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap(),None);
        sqlx::query("DROP TRIGGER fail_invitation_finalize ON account_invitations").execute(control.raw()).await.unwrap();
        invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.unwrap(); assert_eq!(grant_effects(&holding).await,(1,1,1));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(),2);
    }).await}).await;
}
#[tokio::test]
async fn replay_never_recreates_a_revoked_durable_grant() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await; let invitations=invitations(&control,&holding,&scenario);
        sqlx::raw_sql("CREATE FUNCTION fail_invitation_finalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.accepted_at IS NOT NULL THEN RAISE EXCEPTION 'synthetic failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_invitation_finalize BEFORE UPDATE ON account_invitations FOR EACH ROW EXECUTE FUNCTION fail_invitation_finalize()").execute(control.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());
        sqlx::query("DROP TRIGGER fail_invitation_finalize ON account_invitations").execute(control.raw()).await.unwrap();
        sqlx::query("UPDATE hr_people SET user_id=NULL,version=version+1 WHERE id='person'").execute(holding.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());
        sqlx::query("UPDATE hr_people SET user_id='usr_invite_invitation',version=version+1 WHERE id='person'").execute(holding.raw()).await.unwrap();
        sqlx::query("UPDATE assignments SET scope_kind='holding',scope_node_id=NULL,branch_id=NULL,version=version+1 WHERE id='assignment-invitation'").execute(holding.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());
        sqlx::query("UPDATE assignments SET scope_kind='department',scope_node_id='department',branch_id='branch',version=version+1 WHERE id='assignment-invitation'").execute(holding.raw()).await.unwrap();
        sqlx::query("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='assignment-invitation'").execute(holding.raw()).await.unwrap();
        assert!(invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.is_err());assert_eq!(grant_effects(&holding).await,(1,1,1));
        assert_eq!(sqlx::query_scalar::<_,Option<i64>>("SELECT revoked_at FROM assignments WHERE id='assignment-invitation'").fetch_one(holding.raw()).await.unwrap(),Some(1));
    }).await}).await;
}

#[tokio::test]
async fn actual_http_requires_account_origin_and_emits_only_the_existing_public_envelopes() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await; let invitations=invitations(&control,&holding,&scenario);
        let router=account_invitations_router(scenario.auth.clone(),invitations).unwrap();
        let inspect=router.clone().oneshot(Request::builder().uri(format!("/api/v1/account-invitations?token={}",token.expose())).header("host","account.example.test").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(inspect.status(),StatusCode::OK);assert!(inspect.headers()["cache-control"].to_str().unwrap().contains("no-store"));assert_eq!(inspect.headers()["referrer-policy"],"no-referrer");
        let body=to_bytes(inspect.into_body(),8192).await.unwrap();let info:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(info,serde_json::json!({"data":{"name":"Synthetic Invitee","email":"invited@example.test","existingAccount":false}}));assert!(!String::from_utf8(body.to_vec()).unwrap().contains(token.expose()));
        for (host,origin,status) in [("unknown.example.test",Some("https://account.example.test"),StatusCode::NOT_FOUND),("account.example.test",Some("https://acme.example.test"),StatusCode::FORBIDDEN),("account.example.test",None,StatusCode::FORBIDDEN)] {
            let mut request=Request::builder().method("POST").uri("/api/v1/account-invitations").header("host",host).header("content-type","application/json");if let Some(origin)=origin{request=request.header("origin",origin);}
            let response=router.clone().oneshot(request.body(Body::from(serde_json::json!({"token":token.expose(),"password":fixture::credentials::PASSWORD}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),status);assert_eq!(grant_effects(&holding).await,(0,0,0));
        }
        let request=Request::builder().method("POST").uri("/api/v1/account-invitations").header("host","account.example.test").header("origin","https://account.example.test").header("content-type","application/json").body(Body::from(serde_json::json!({"token":token.expose(),"password":fixture::credentials::PASSWORD}).to_string())).unwrap();
        let response=router.oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert!(response.headers().get("set-cookie").is_none());let body=to_bytes(response.into_body(),8192).await.unwrap();assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap(),serde_json::json!({"data":{"activated":true}}));assert_eq!(grant_effects(&holding).await,(1,1,1));
    }).await}).await;
}

#[tokio::test]
async fn holding_serialization_retry_keeps_one_control_credential_and_one_durable_grant() {
    database::with_database(1,|control|async move {database::with_database(1,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await;let invitations=invitations(&control,&holding,&scenario);
        sqlx::raw_sql("CREATE SEQUENCE invitation_holding_attempt; CREATE FUNCTION retry_holding_invitation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='identity.account.activate' AND nextval('invitation_holding_attempt')=1 THEN RAISE EXCEPTION 'synthetic Holding retry' USING ERRCODE='40001'; END IF; RETURN NEW; END $$; CREATE TRIGGER retry_holding_invitation BEFORE INSERT ON iam_audit FOR EACH ROW EXECUTE FUNCTION retry_holding_invitation()").execute(holding.raw()).await.unwrap();
        invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await.unwrap();assert_eq!(grant_effects(&holding).await,(1,1,1));assert_eq!(fixture::effects(&control).await,5);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM invitation_holding_attempt").fetch_one(holding.raw()).await.unwrap(),2);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(),2);
    }).await}).await;
}

async fn waiting(pool: &DatabasePool, event: &str) -> bool {
    tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND wait_event=$1)").bind(event).fetch_one(pool.raw()).await.unwrap();if exists{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.is_ok()
}

#[tokio::test]
async fn retained_control_proof_blocks_revocation_and_expiry_rolls_back_the_whole_holding_unit() {
    database::with_database(3,|control|async move {database::with_database(3,|holding|async move {
        let (scenario,token)=fixture::setup(&control,&holding).await;let invitations=invitations(&control,&holding,&scenario);
        sqlx::raw_sql("CREATE FUNCTION pause_holding_invitation() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.action='identity.account.activate' THEN PERFORM pg_advisory_xact_lock(927341,23); END IF; RETURN NEW; END $$; CREATE TRIGGER pause_holding_invitation BEFORE INSERT ON iam_audit FOR EACH ROW EXECUTE FUNCTION pause_holding_invitation()").execute(holding.raw()).await.unwrap();
        let mut blocker=holding.raw().acquire().await.unwrap();blocker.close_on_drop();sqlx::query("SELECT pg_advisory_lock(927341,23)").execute(&mut *blocker).await.unwrap();
        let task=tokio::spawn(async move {invitations.accept(token.expose().into(),fixture::credentials::PASSWORD.into()).await});
        if !waiting(&holding,"advisory").await {task.abort();let _=task.await;sqlx::query("SELECT pg_advisory_unlock(927341,23)").execute(&mut *blocker).await.unwrap();panic!("activation did not reach actual Holding audit lock wait");}
        let writer_pool=control.clone();let writer=tokio::spawn(async move {sqlx::query("UPDATE principals SET status='suspended' WHERE user_id='usr_invite_invitation'").execute(writer_pool.raw()).await.unwrap()});
        let observed=waiting(&control,"relation").await;let pending=!writer.is_finished();scenario.clock.advance(3_600_000);
        sqlx::query("SELECT pg_advisory_unlock(927341,23)").execute(&mut *blocker).await.unwrap();assert!(task.await.unwrap().is_err());writer.await.unwrap();assert!(observed&&pending,"retained Control source proof must survive inner Holding writes");assert_eq!(grant_effects(&holding).await,(0,0,0));assert_eq!(sqlx::query_scalar::<_,Option<String>>("SELECT user_id FROM hr_people WHERE id='person'").fetch_one(holding.raw()).await.unwrap(),None);assert_eq!(sqlx::query_scalar::<_,Option<i64>>("SELECT accepted_at FROM account_invitations WHERE id='invitation'").fetch_one(control.raw()).await.unwrap(),None);
    }).await}).await;
}
