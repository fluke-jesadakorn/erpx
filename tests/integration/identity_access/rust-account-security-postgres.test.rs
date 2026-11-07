#[allow(
    dead_code,
    reason = "shared actual MFA/security fixtures expose operations used by the sibling target"
)]
#[path = "../../support/rust-account-security.rs"]
mod support;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use erpx_server_identity_access::public::{account_security_http::account_security_router, *};
use erpx_server_shared::public::Clock;
use serde_json::{Value, json};
use support::{fixture, *};
use tower::ServiceExt;

#[tokio::test]
async fn personal_sources_ignore_foreign_large_images_but_reject_own_row_and_aggregate_overflow() {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let _foreign=fixture::login(&system,"b@example.test",None).await;let s=services(&system);
    sqlx::query("UPDATE session SET \"userAgent\"=repeat('x',1000000) WHERE \"userId\"='fixture-b'").execute(system.control.raw()).await.unwrap();assert_eq!(s.service.read(&proof).await.unwrap().families.len(),1);
    sqlx::query("UPDATE session SET \"userAgent\"=repeat('x',100000) WHERE \"userId\"='fixture-a'").execute(system.control.raw()).await.unwrap();assert_eq!(s.service.read(&proof).await.err().unwrap().code,"ACCOUNT_SECURITY_SOURCE_LIMIT");
    sqlx::query("UPDATE session SET \"userAgent\"=NULL WHERE \"userId\"='fixture-a'").execute(system.control.raw()).await.unwrap();
    sqlx::query("INSERT INTO session(id,token,\"userId\",\"createdAt\",\"updatedAt\",\"expiresAt\") SELECT 'bounded-'||n,'bounded-token-'||n,'fixture-a',s.\"createdAt\",s.\"updatedAt\",s.\"expiresAt\" FROM session s CROSS JOIN generate_series(1,2050) n WHERE s.\"userId\"='fixture-a' LIMIT 2050").execute(system.control.raw()).await.unwrap();assert_eq!(s.service.read(&proof).await.err().unwrap().code,"ACCOUNT_SECURITY_SOURCE_LIMIT");
}).await;
}

#[tokio::test]
async fn other_family_revocation_fences_both_holdings_and_receipt_replay_requires_current_mfa() {
    fixture::with_system(|system| async move {
        let old = fixture::login(&system, "a@example.test", None).await;
        let children = fixture::children(&system, &old, "fixture-a").await;
        let proof = fixture::login(&system, "a@example.test", None).await;
        let live = fixture::children(&system, &proof, "fixture-a").await;
        let s = services(&system);
        verified(&system, &s, &proof).await;
        let seed = s.service.read(&proof).await.unwrap();
        assert_eq!(seed.families.len(), 2);
        let family = seed
            .families
            .iter()
            .find(|f| !f.current)
            .unwrap()
            .handle
            .clone();
        let command = input(&seed, Some(family));
        let (result, concurrent) = tokio::join!(
            s.service
                .revoke(&proof, command.clone(), "synthetic-family-key".into()),
            s.service
                .revoke(&proof, command.clone(), "synthetic-family-key".into())
        );
        let result = result.unwrap();
        assert_eq!(concurrent.unwrap().count, 3);
        assert_eq!(result.count, 3);
        assert!(!result.reauthenticate);
        fixture::assert_fenced(&system, &children, "fixture-a").await;
        for (pool, child) in system.holdings.iter().zip(&live) {
            assert!(!fixture::fenced(pool, child, "fixture-a").await);
        }
        let repeated = s
            .service
            .revoke(&proof, command.clone(), "synthetic-family-key".into())
            .await
            .unwrap();
        assert_eq!(repeated.count, 3);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM iam_audit WHERE action='account.sessions.revoked'"
            )
            .fetch_one(system.control.raw())
            .await
            .unwrap(),
            1
        );
        system.scenario.clock.advance(300_000);
        assert_eq!(
            s.service
                .revoke(&proof, command, "synthetic-family-key".into())
                .await
                .err()
                .unwrap()
                .code,
            "FRESH_MFA_REQUIRED"
        );
    })
    .await;
}
#[tokio::test]
async fn expiry_while_waiting_for_second_holding_keeps_monotonic_fences_but_denies_control_ack() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let children = fixture::children(&system, &proof, "fixture-a").await;
        let s = services(&system);
        verified(&system, &s, &proof).await;
        let seed = s.service.read(&proof).await.unwrap();
        let command = input(&seed, None);
        let mut blocked = system.holdings[1].raw().acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *blocked).await.unwrap();
        sqlx::query("LOCK TABLE active_work_contexts IN EXCLUSIVE MODE")
            .execute(&mut *blocked)
            .await
            .unwrap();
        let service = s.service.clone();
        let task = tokio::spawn(async move {
            service
                .revoke(&proof, command, "synthetic-expiry-key".into())
                .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !fixture::fenced(&system.holdings[0], &children[0], "fixture-a").await {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("first actual Holding commits before the second is released");
        system.scenario.clock.advance(300_001);
        sqlx::query("COMMIT").execute(&mut *blocked).await.unwrap();
        drop(blocked);
        assert_eq!(
            task.await.unwrap().err().unwrap().code,
            "FRESH_MFA_REQUIRED"
        );
        fixture::assert_fenced(&system, &children, "fixture-a").await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM iam_receipts")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM session WHERE \"userId\"='fixture-a'"
            )
            .fetch_one(system.control.raw())
            .await
            .unwrap(),
            3
        );
    })
    .await;
}
#[tokio::test]
async fn deferred_control_40001_never_replays_after_holding_commits_and_rolls_back_all_control_effects()
 {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let children=fixture::children(&system,&proof,"fixture-a").await;let s=services(&system);verified(&system,&s,&proof).await;let seed=s.service.read(&proof).await.unwrap();
    sqlx::raw_sql("CREATE SEQUENCE synthetic_security_attempts; CREATE FUNCTION synthetic_security_serialization() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM nextval('synthetic_security_attempts'); RAISE EXCEPTION 'synthetic late serialization' USING ERRCODE='40001'; END$$; CREATE CONSTRAINT TRIGGER synthetic_security_serialization AFTER INSERT ON iam_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_security_serialization()").execute(system.control.raw()).await.unwrap();
    assert_eq!(s.service.revoke(&proof,input(&seed,None),"synthetic-serialization-key".into()).await.err().unwrap().code,"ACCOUNT_PERSISTENCE_UNAVAILABLE");fixture::assert_fenced(&system,&children,"fixture-a").await;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM synthetic_security_attempts").fetch_one(system.control.raw()).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_receipts").fetch_one(system.control.raw()).await.unwrap(),0);assert!(system.scenario.auth.session(&proof,RequestAudience::Account).await.unwrap().is_some());
}).await;
}
#[tokio::test]
async fn all_devices_includes_own_legacy_sessions_but_preserves_foreign_users_and_contexts() {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let children=fixture::children(&system,&proof,"fixture-a").await;let sibling=fixture::login(&system,"a@example.test",None).await;let sibling_children=fixture::children(&system,&sibling,"fixture-a").await;
    let foreign=fixture::login(&system,"b@example.test",None).await;let foreign_children=fixture::children(&system,&foreign,"fixture-b").await;
    sqlx::query("INSERT INTO session(id,token,\"userId\",\"createdAt\",\"updatedAt\",\"expiresAt\") SELECT 'synthetic-legacy','synthetic-legacy-token','fixture-a',\"createdAt\",\"updatedAt\",\"expiresAt\" FROM session WHERE \"userId\"='fixture-a' LIMIT 1").execute(system.control.raw()).await.unwrap();
    let s=services(&system);verified(&system,&s,&proof).await;let seed=s.service.read(&proof).await.unwrap();assert_eq!(seed.legacy_sessions,1);
    let result=s.service.revoke(&proof,input(&seed,None),"synthetic-all-key".into()).await.unwrap();assert!(result.reauthenticate);assert_eq!(result.count,7);
    fixture::assert_fenced(&system,&children,"fixture-a").await;fixture::assert_fenced(&system,&sibling_children,"fixture-a").await;
    for(pool,child)in system.holdings.iter().zip(&foreign_children){assert!(!fixture::fenced(pool,child,"fixture-b").await);}
    assert!(system.scenario.auth.session(&foreign,RequestAudience::Account).await.unwrap().is_some());assert!(s.service.read(&proof).await.is_err());
}).await;
}
#[tokio::test]
async fn partial_holding_failure_has_no_ack_receipt_or_control_deletion_then_exact_retry_recovers()
{
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let children = fixture::children(&system, &proof, "fixture-a").await;
        let s = services(&system);
        verified(&system, &s, &proof).await;
        let seed = s.service.read(&proof).await.unwrap();
        let command = input(&seed, None);
        sqlx::query("ALTER TABLE active_work_contexts RENAME TO synthetic_unavailable")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        assert_eq!(
            s.service
                .revoke(&proof, command.clone(), "synthetic-partial-key".into())
                .await
                .err()
                .unwrap()
                .code,
            "DEVICE_ACCOUNT_REENTRY_REQUIRED"
        );
        assert!(fixture::fenced(&system.holdings[0], &children[0], "fixture-a").await);
        assert!(
            system
                .scenario
                .auth
                .session(&proof, RequestAudience::Account)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM iam_receipts")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
        sqlx::query("ALTER TABLE synthetic_unavailable RENAME TO active_work_contexts")
            .execute(system.holdings[1].raw())
            .await
            .unwrap();
        s.service
            .revoke(&proof, command, "synthetic-partial-key".into())
            .await
            .unwrap();
        fixture::assert_fenced(&system, &children, "fixture-a").await;
    })
    .await;
}
#[tokio::test]
async fn final_source_effect_denial_rolls_back_receipt_audit_deletion_without_undoing_tombstones() {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let children=fixture::children(&system,&proof,"fixture-a").await;let s=services(&system);verified(&system,&s,&proof).await;let seed=s.service.read(&proof).await.unwrap();
    sqlx::raw_sql("CREATE FUNCTION synthetic_security_late_effect() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE \"user\" SET name='unauthorized late effect' WHERE id='fixture-a'; RETURN NEW; END$$; CREATE CONSTRAINT TRIGGER synthetic_security_late_effect AFTER INSERT ON iam_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_security_late_effect()").execute(system.control.raw()).await.unwrap();
    assert_eq!(s.service.revoke(&proof,input(&seed,None),"synthetic-late-key".into()).await.err().unwrap().code,"ACCOUNT_SECURITY_CHANGED");fixture::assert_fenced(&system,&children,"fixture-a").await;
    assert!(system.scenario.auth.session(&proof,RequestAudience::Account).await.unwrap().is_some());assert_eq!(sqlx::query_scalar::<_,String>("SELECT name FROM \"user\" WHERE id='fixture-a'").fetch_one(system.control.raw()).await.unwrap(),"fixture-a");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_audit").fetch_one(system.control.raw()).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM iam_receipts").fetch_one(system.control.raw()).await.unwrap(),0);
}).await;
}
#[tokio::test]
async fn capture_revision_and_account_handle_reject_stale_or_foreign_commands_without_effects() {
    fixture::with_system(|system| async move {
        let proof = fixture::login(&system, "a@example.test", None).await;
        let s = services(&system);
        verified(&system, &s, &proof).await;
        let seed = s.service.read(&proof).await.unwrap();
        let _new = fixture::login(&system, "a@example.test", None).await;
        assert_eq!(
            s.service
                .revoke(&proof, input(&seed, None), "synthetic-stale-key".into())
                .await
                .err()
                .unwrap()
                .code,
            "ACCOUNT_SECURITY_CHANGED"
        );
        let current = s.service.read(&proof).await.unwrap();
        let mut wrong = input(&current, None);
        wrong.account_key = "a".repeat(64);
        assert_eq!(
            s.service
                .revoke(&proof, wrong, "synthetic-wrong-key".into())
                .await
                .err()
                .unwrap()
                .code,
            "ACCOUNT_SECURITY_CHANGED"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM iam_receipts")
                .fetch_one(system.control.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}
#[tokio::test]
async fn actual_axum_security_contract_rejects_foreign_origin_scope_cookie_replay_and_never_returns_bearers()
 {
    fixture::with_system(|system|async move{
    let proof=fixture::login(&system,"a@example.test",None).await;let s=services(&system);let router=account_security_router(s.service.clone());let mut jar=jar(&proof);
    let(status,seed,headers)=http::send(&router,&mut jar,"GET","/api/v1/account/security",None,None).await;assert_eq!(status,StatusCode::OK);assert!(headers[header::CACHE_CONTROL].to_str().unwrap().contains("no-store"));assert!(!headers.contains_key(header::SET_COOKIE));http::no_credentials(&seed,&jar);
    let(status,_,_)=http::send(&router,&mut jar,"GET","/api/v1/account/security?user=foreign",None,None).await;assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY);
    let(status,_,_)=http::send(&router,&mut jar,"POST","/api/v1/account/iam/mfa/enroll",Some("https://foreign.example.test"),Some(json!({}))).await;assert_eq!(status,StatusCode::FORBIDDEN);
    let(status,enrollment,headers)=http::send(&router,&mut jar,"POST","/api/v1/account/iam/mfa/enroll",Some(http::ACCOUNT),Some(json!({}))).await;assert_eq!(status,StatusCode::OK);assert_eq!(enrollment["data"]["secret"].as_str().unwrap().len(),32);assert!(!headers.contains_key(header::SET_COOKIE));
    let c=code(&system,&s.keys,"fixture-a",0).await;let(status,verified,_)=http::send(&router,&mut jar,"POST","/api/v1/account/iam/mfa/verify",Some(http::ACCOUNT),Some(json!({"code":c}))).await;assert_eq!(status,StatusCode::OK);assert!(verified["data"]["verifiedUntil"].as_i64().unwrap()>system.scenario.clock.now().get());
    let(status,seed,_)=http::send(&router,&mut jar,"GET","/api/v1/account/security",None,None).await;assert_eq!(status,StatusCode::OK);
    let command=json!({"accountKey":seed["data"]["accountKey"],"expectedRevision":seed["data"]["revision"],"scope":"all","reason":"Synthetic authenticated browser request"});
        let mut explicit_null=command.clone();explicit_null["family"]=Value::Null;
        let(status,_,_)=http::send(&router,&mut jar,"POST","/api/v1/account/security",Some(http::ACCOUNT),Some(explicit_null)).await;assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY);
    let request=Request::builder().method("POST").uri("/api/v1/account/security").header("host","account.example.test").header("origin",http::ACCOUNT).header("cookie",jar.header()).header("content-type","application/json").header("idempotency-key","synthetic-http-key").body(Body::from(serde_json::to_vec(&command).unwrap())).unwrap();
    let response=router.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let body:Value=serde_json::from_slice(&to_bytes(response.into_body(),8192).await.unwrap()).unwrap();assert_eq!(body["data"]["reauthenticate"],true);http::no_credentials(&body,&jar);
    let(status,_,_)=http::send(&router,&mut jar,"GET","/api/v1/account/security",None,None).await;assert_eq!(status,StatusCode::UNAUTHORIZED);
}).await;
}

#[tokio::test]
async fn foreign_child_reference_and_oversized_receipt_or_mfa_material_fail_without_ack() {
    fixture::with_system(|system|async move{
    use erpx_server_identity_access::public::account_security::AccountSecurityActor;
    use sha2::{Digest,Sha256};
    let proof=fixture::login(&system,"a@example.test",None).await;let child=fixture::child(&system,&proof,fixture::ACME).await;let foreign=fixture::login(&system,"b@example.test",None).await;let foreign_child=fixture::child(&system,&foreign,fixture::ACME).await;let s=services(&system);verified(&system,&s,&proof).await;
    let parent=system.scenario.auth.session(&proof,RequestAudience::Account).await.unwrap().unwrap();
    sqlx::query("UPDATE sso_session_bindings SET parent_session_id=$1 WHERE session_id=$2").bind(&parent.id).bind(&foreign_child.result.session_id).execute(system.control.raw()).await.unwrap();assert_eq!(s.service.read(&proof).await.err().unwrap().code,"ACCOUNT_SECURITY_CHANGED");
    sqlx::query("DELETE FROM sso_session_bindings WHERE session_id=$1").bind(&foreign_child.result.session_id).execute(system.control.raw()).await.unwrap();
    let seed=s.service.read(&proof).await.unwrap();let command=input(&seed,None);let actor=AccountSecurityActor{user_id:"fixture-a".into(),session_id:parent.id};let hash=hex::encode(Sha256::digest(serde_json::to_vec(&("account.sessions.revoke",actor,&command)).unwrap()));
    sqlx::query("INSERT INTO iam_receipts(id,company_id,actor_id,request_hash,result,created_at) VALUES('synthetic-oversized-receipt','account','fixture-a',$1,jsonb_build_object('padding',repeat('x',1000000)),0)").bind(hash).execute(system.control.raw()).await.unwrap();
    assert_eq!(s.service.revoke(&proof,command,"synthetic-oversized-receipt".into()).await.err().unwrap().code,"ACCOUNT_PERSISTENCE_UNAVAILABLE");
    sqlx::query("UPDATE iam_mfa SET secret=repeat('x',1000000) WHERE user_id='fixture-a'").execute(system.control.raw()).await.unwrap();assert_eq!(s.service.read(&proof).await.err().unwrap().code,"ACCOUNT_SECURITY_SOURCE_LIMIT");
    assert!(system.scenario.auth.session(&proof,RequestAudience::Account).await.unwrap().is_some());assert!(!fixture::fenced(&system.holdings[0],&child,"fixture-a").await);
}).await;
}
