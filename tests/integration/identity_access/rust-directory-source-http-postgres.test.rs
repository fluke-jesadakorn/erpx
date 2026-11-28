#[allow(
    dead_code,
    reason = "shared actual login/SSO/MFA/private source fixture"
)]
#[path = "../../support/rust-directory-source-http.rs"]
mod fixture;
use axum::http::{HeaderMap, StatusCode, header};
use fixture::*;
use serde_json::{Value, json};
fn code(body: &Value, expected: &str) {
    assert_eq!(body["error"]["code"], expected);
}
fn private(headers: &HeaderMap) {
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
}
async fn preview(p: &mut HttpFixture, key: &str) -> Value {
    let (status, body, h) = send(
        &p.router,
        admin::HOST,
        &mut p.holding_a,
        "POST",
        &preview_path(),
        Some("https://acme.example.test"),
        serde_json::to_vec(&preview_body()).unwrap(),
        headers(false, key),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    private(&h);
    assert_eq!(body["data"]["status"], "pending");
    body["data"].clone()
}
#[tokio::test]
async fn actual_http_login_sso_mfa_preview_and_independent_apply_bind_reviewer_reason_to_review_receipt_and_audit()
 {
    account::with_system(|system|async move{
  let mut p=prepare(system,true).await;let proposal=preview(&mut p,"http-source-preview-one").await;let path=approve_path(proposal["id"].as_str().unwrap());
  let request=json!({"expectedVersion":1,"reason":"\u{feff}  Independently verified selected key and current profile.  "});
  let(status,body,h)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&path,Some("https://acme.example.test"),serde_json::to_vec(&request).unwrap(),headers(true,"http-source-apply-one")).await;assert_eq!(status,StatusCode::OK);private(&h);assert_eq!(body["data"]["decisionReason"],"Independently verified selected key and current profile.");assert_eq!(body["data"]["sourceRevision"],1);
  let decision:String=sqlx::query_scalar("SELECT reason FROM iam_reviews").fetch_one(p.p.system.holdings[0].raw()).await.unwrap();assert_eq!(decision,"Reviewed exact source proposal\nReview: Independently verified selected key and current profile.");
  let receipts:Value=sqlx::query_scalar("SELECT result FROM iam_receipts WHERE id='http-source-apply-one'").fetch_one(p.p.system.holdings[0].raw()).await.unwrap();assert_eq!(receipts,body["data"]);
  let evidence:Value=sqlx::query_scalar("SELECT evidence FROM iam_audit WHERE action='directory.source-review.apply'").fetch_one(p.p.system.holdings[0].raw()).await.unwrap();assert_eq!(evidence["decisionReason"],body["data"]["decisionReason"]);
  let(status,replay,_)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&path,Some("https://acme.example.test"),serde_json::to_vec(&request).unwrap(),headers(true,"http-source-apply-one")).await;assert_eq!(status,StatusCode::OK);assert_eq!(replay,body);assert_eq!(admin::counts(&p.p).await,(1,1,2,3));
  let(status,changed,_)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&path,Some("https://acme.example.test"),serde_json::to_vec(&json!({"expectedVersion":1,"reason":"Different independent review justification"})).unwrap(),headers(true,"http-source-apply-one")).await;assert_eq!(status,StatusCode::CONFLICT);code(&changed,"IDEMPOTENCY_CONFLICT");assert_eq!(admin::counts(&p.p).await,(1,1,2,3));assert_eq!(p.p.server.stats.lock().unwrap().connections,0);p.p.server.finish().await;
 }).await;
}
#[tokio::test]
async fn live_authentication_precedes_deferred_cas_then_mfa_and_body_failures() {
    account::with_system(|system| async move {
        let mut p = prepare(system, false).await;
        let mut anonymous = Jar::default();
        let (status, body, h) = send(
            &p.router,
            admin::HOST,
            &mut anonymous,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            b"not json".to_vec(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        code(&body, "UNAUTHENTICATED");
        private(&h);
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            b"not json".to_vec(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        code(&body, "WORK_CONTEXT_CHANGED");
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            b"not json".to_vec(),
            headers(false, "http-body-before-mfa"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        code(&body, "FRESH_MFA_REQUIRED");
        mfa(&p.p, &p.router, &mut p.holding_a, "fixture-a").await;
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            b"not json".to_vec(),
            headers(false, "http-invalid-json-proof"),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        code(&body, "DIRECTORY_SOURCE_BODY_INVALID");
        assert_eq!(admin::counts(&p.p).await, (0, 0, 0, 0));
        p.p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn the_new_private_surface_requires_exact_origin_and_registered_host_slug_without_forwarded_authority()
 {
    account::with_system(|system| async move {
        let mut p = prepare(system, true).await;
        for origin in [None, Some("https://foreign.example.test")] {
            let (status, body, h) = send(
                &p.router,
                admin::HOST,
                &mut p.holding_a,
                "POST",
                &preview_path(),
                origin,
                serde_json::to_vec(&preview_body()).unwrap(),
                headers(false, "http-origin-rejected"),
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            code(&body, "ORIGIN_FORBIDDEN");
            private(&h);
        }
        let mut forwarded = headers(false, "http-forwarded-rejected");
        forwarded.insert("x-forwarded-host", "acme.example.test".parse().unwrap());
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            vec![],
            forwarded,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        code(&body, "UNTRUSTED_REQUEST_AUTHORITY");
        let (status, _, h) = send(
            &p.router,
            "account.example.test",
            &mut p.account_a,
            "POST",
            &preview_path(),
            Some("https://account.example.test"),
            vec![],
            headers(false, "http-central-not-holding"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        private(&h);
        let (status, _, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path().replace("/holdings/acme/", "/holdings/northstar/"),
            Some("https://acme.example.test"),
            vec![],
            headers(false, "http-slug-not-registered"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(admin::counts(&p.p).await, (0, 0, 0, 0));
        p.p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn structural_safe_numbers_and_last_duplicate_property_are_supported_but_unknown_actor_inputs_never_authorize()
 {
    account::with_system(|system|async move{
  let mut p=prepare(system,true).await;
  for body in [json!({"keyId":"synthetic_provenance_v1","expectedConnectionVersion":9007199254740992_u64,"reason":"Unsafe version"}),json!({"keyId":"synthetic_provenance_v1","expectedConnectionVersion":1,"reason":"Copied input must not authorize","actor":{"userId":"fixture-b","permissions":["iam.review"]}})]{
   let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_a,"POST",&preview_path(),Some("https://acme.example.test"),serde_json::to_vec(&body).unwrap(),headers(false,"http-invalid-closed-body")).await;assert_eq!(status,StatusCode::UNPROCESSABLE_ENTITY);code(&body,"DIRECTORY_SOURCE_BODY_INVALID");
  }
  assert_eq!(admin::counts(&p.p).await,(0,0,0,0));
  let bytes=format!(r#"{{"keyId":"synthetic_provenance_v1","expectedConnectionVersion":0,"expectedConnectionVersion":1e0,"expectedSourceRevision":null,"reason":"\ufeff{} "}}"#,"🙃".repeat(600)).into_bytes();
  let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_a,"POST",&preview_path(),Some("https://acme.example.test"),bytes,headers(false,"http-numeric-duplicate")).await;assert_eq!(status,StatusCode::OK);assert_eq!(body["data"]["sourceRevision"],1);assert_eq!(admin::counts(&p.p).await,(0,1,1,1));p.p.server.finish().await;
 }).await;
}
#[tokio::test]
async fn bounded_body_and_duplicate_cas_are_reported_only_after_the_original_session_is_verified() {
    account::with_system(|system| async move {
        let mut p = prepare(system, true).await;
        let large = vec![b'x'; 16 * 1024 + 1];
        let mut anonymous = Jar::default();
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut anonymous,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            large.clone(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        code(&body, "UNAUTHENTICATED");
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            large,
            headers(false, "http-bounded-body"),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        code(&body, "BODY_TOO_LARGE");
        let mut duplicate = headers(false, "http-duplicate-cas");
        duplicate.append("x-erpx-context-revision", "7".parse().unwrap());
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            serde_json::to_vec(&preview_body()).unwrap(),
            duplicate,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        code(&body, "WORK_CONTEXT_CHANGED");
        assert_eq!(admin::counts(&p.p).await, (0, 0, 0, 0));
        p.p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn an_actual_parent_mfa_verification_does_not_unlock_the_holding_child_source_endpoint() {
    account::with_system(|system| async move {
        let mut p = prepare(system, false).await;
        let (status, _, _) = send(
            &p.router,
            "account.example.test",
            &mut p.account_a,
            "POST",
            "/api/v1/account/iam/mfa/enroll",
            Some("https://account.example.test"),
            b"{}".to_vec(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let code_value = admin::security::code(&p.p.system, &p.p.keys, "fixture-a", 0).await;
        let (status, _, _) = send(
            &p.router,
            "account.example.test",
            &mut p.account_a,
            "POST",
            "/api/v1/account/iam/mfa/verify",
            Some("https://account.example.test"),
            serde_json::to_vec(&json!({"code":code_value})).unwrap(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, body, _) = send(
            &p.router,
            admin::HOST,
            &mut p.holding_a,
            "POST",
            &preview_path(),
            Some("https://acme.example.test"),
            serde_json::to_vec(&preview_body()).unwrap(),
            headers(false, "http-parent-mfa-not-child"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        code(&body, "FRESH_MFA_REQUIRED");
        assert_eq!(admin::counts(&p.p).await, (0, 0, 0, 0));
        p.p.server.finish().await;
    })
    .await;
}
#[tokio::test]
async fn a_proposer_cannot_approve_and_deferred_reviewer_reason_tampering_rolls_back_the_complete_source_command()
 {
    account::with_system(|system|async move{
  let mut p=prepare(system,true).await;let proposal=preview(&mut p,"http-source-reason-tamper-preview").await;let path=approve_path(proposal["id"].as_str().unwrap());let bytes=serde_json::to_vec(&json!({"expectedVersion":1,"reason":"Actual independent reviewer reason"})).unwrap();
  let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_a,"POST",&path,Some("https://acme.example.test"),bytes.clone(),headers(false,"http-source-self-approve")).await;assert_eq!(status,StatusCode::CONFLICT);code(&body,"INDEPENDENT_REVIEW_REQUIRED");
  sqlx::raw_sql("CREATE FUNCTION synthetic_http_reason_tamper()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.result->>'status'='approved' THEN UPDATE iam_reviews SET reason='Reviewed exact source proposal'||E'\\nReview: Replaced reviewer justification' WHERE id=NEW.result->>'id';END IF;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_http_reason_tamper AFTER INSERT ON iam_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_http_reason_tamper()").execute(p.p.system.holdings[0].raw()).await.unwrap();
  let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&path,Some("https://acme.example.test"),bytes,headers(true,"http-source-reason-tamper-apply")).await;assert_eq!(status,StatusCode::CONFLICT);code(&body,"DIRECTORY_SOURCE_REVIEW_CHANGED");assert_eq!(admin::counts(&p.p).await,(0,1,1,1));p.p.server.finish().await;
 }).await;
}

#[tokio::test]
async fn duplicate_cookie_proofs_fail_closed_and_http_receipt_replay_cannot_substitute_another_connection()
 {
    account::with_system(|system|async move{
  let mut p=prepare(system,true).await;let mut duplicate=headers(false,"http-duplicate-cookie");duplicate.append(header::COOKIE,p.holding_a.header().parse().unwrap());
  let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_a,"POST",&preview_path(),Some("https://acme.example.test"),serde_json::to_vec(&preview_body()).unwrap(),duplicate).await;assert_eq!(status,StatusCode::BAD_REQUEST);code(&body,"AMBIGUOUS_COOKIE_HEADER");assert_eq!(admin::counts(&p.p).await,(0,0,0,0));
  let proposal=preview(&mut p,"http-source-target-preview").await;let original=approve_path(proposal["id"].as_str().unwrap());let bytes=serde_json::to_vec(&json!({"expectedVersion":1,"reason":"Independent reviewed target and source"})).unwrap();
  let(status,_,_)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&original,Some("https://acme.example.test"),bytes.clone(),headers(true,"http-source-target-apply")).await;assert_eq!(status,StatusCode::OK);
  sqlx::query("INSERT INTO iam_connection_routes(connection_id,holding_id,company_id)VALUES('second-http-connection','h001','company')").execute(p.p.system.control.raw()).await.unwrap();
  sqlx::query("INSERT INTO iam_connections(id,company_id,name,provider,config,status,version,tested_version,created_by,created_at)VALUES('second-http-connection','company','Another current HTTP target','univention',$1,'active',1,1,'fixture-a',0)").bind(serde_json::to_value(&p.p.profile.descriptor.config).unwrap()).execute(p.p.system.holdings[0].raw()).await.unwrap();
  let replaced=original.replace(ldap::CONNECTION,"second-http-connection");let(status,body,_)=send(&p.router,admin::HOST,&mut p.holding_b,"POST",&replaced,Some("https://acme.example.test"),bytes,headers(true,"http-source-target-apply")).await;assert_eq!(status,StatusCode::CONFLICT);code(&body,"DIRECTORY_SOURCE_ADMIN_SCOPE_REQUIRED");assert_eq!(admin::counts(&p.p).await,(1,1,2,3));p.p.server.finish().await;
 }).await;
}
