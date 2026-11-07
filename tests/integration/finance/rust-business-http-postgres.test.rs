#[allow(
    dead_code,
    reason = "real Account, Finance and Workflow fixture also serves owner composition tests"
)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_legal::public::record_policy::LegalCommercialMode;
use erpx_server_platform_administration::public::legal_release::current_legal_release_catalogue;
use erpx_server_pms::public::scheduled_legal::ScheduledReleaseMode;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, HoldingId};
use erpx_service::{business_http::*, legal_owner_policy::*, legal_queue_administration::*};
use facts::{account, *};
use std::sync::Arc;
use tower::ServiceExt;
struct Scope;
impl BusinessScopeSlugs for Scope {
    fn slugs(&self, _: &HeaderMap) -> Result<(Option<String>, Option<String>), BusinessError> {
        Ok((None, None))
    }
}
fn router(p: &Prepared, legal: bool) -> Router {
    let config = LegalOwnerConfiguration {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        release_mode: ScheduledReleaseMode::Development,
        commercial_mode: LegalCommercialMode::Enforced,
        artifact_version: "unbuilt".into(),
        release_catalogue: Arc::new(current_legal_release_catalogue().unwrap()),
        purchase_catalogue: Arc::new(current_package_catalogue().unwrap()),
        engineering: None,
    };
    business_router(
        Arc::new(account::fixture::account::policy()),
        account::fixture::account::holding_registry(),
        Arc::new(Scope),
        vec![BusinessHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            finance: Some(Arc::new(p.administration())),
            legal: legal.then(|| {
                Arc::new(LegalQueueAdministration::new(
                    p.authorizer.clone(),
                    config,
                    p.system.scenario.clock.clone(),
                ))
            }),
        }],
    )
    .unwrap()
}
fn request(p: &Prepared, path: &str, body: &str, key: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/v1/holdings/acme/{path}"))
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header(
            "cookie",
            format!("__Host-erpx-product={}", p.reviewer_token.expose()),
        )
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-assignment", "finance-fixture-b")
        .header("x-erpx-context-revision", "7")
        .header("x-erpx-context-assignment-version", "3")
        .header("idempotency-key", key)
        .body(Body::from(body.to_owned()))
        .unwrap()
}
async fn invoke(router: &Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let r = router.clone().oneshot(request).await.unwrap();
    assert_eq!(r.headers()["cache-control"], "private, no-store");
    assert!(r.headers().get("access-control-allow-origin").is_none());
    let status = r.status();
    let bytes = to_bytes(r.into_body(), 2 * 1024 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8(bytes.to_vec()).unwrap())
        }),
    )
}
async fn totals(p: &Prepared, key: &str) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(p.system.holdings[0].raw()).await.unwrap()
}
#[tokio::test]
async fn actual_finance_http_normalizes_wire_before_hash_and_preserves_post_reverse_and_exact_replay()
 {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("http-wire", false).await.unwrap();
        let app = router(&p, false);
        let mut r = request(
            &p,
            "finance/journals/journal-http-wire/post",
            r#"{"expectedVersion":1.0,"ignored":{"bad":1e400,"utf16":"\ud800"}}"#,
            "http-post",
        );
        r.headers_mut().remove("origin");
        let (status, result) = invoke(&app, r).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["data"]["status"], "posted");
        assert_eq!(result["data"]["version"], 2);
        let (status, result) = invoke(
            &app,
            request(
                &p,
                "finance/journals/journal-http-wire/post",
                r#"{"expectedVersion":1e0}"#,
                "http-post",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(totals(&p, "http-post").await, (1, 1, 1));
        assert_eq!(
            invoke(
                &app,
                request(
                    &p,
                    "finance/journals/journal-http-wire/post",
                    r#"{"expectedVersion":2}"#,
                    "http-post"
                )
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let (status, result) = invoke(
            &app,
            request(
                &p,
                "finance/journals/journal-http-wire/reverse",
                r#"{"periodId":"period","entryDate":"2026-01-02","memo":"","ignored":true}"#,
                "http-reverse",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{result}");
        assert_eq!(result["data"]["reversedJournalId"], "journal-http-wire");
        assert!(result["data"]["reversalJournalId"].as_str().is_some());
        assert_eq!(totals(&p, "http-reverse").await, (1, 1, 1));
        p.create("http-utf16", false).await.unwrap();
        assert_eq!(
            invoke(
                &app,
                request(
                    &p,
                    "finance/journals/journal-http-utf16/post",
                    r#"{"expectedVersion":1}"#,
                    "http-utf16-post"
                )
            )
            .await
            .0,
            StatusCode::OK
        );
        let (status, result) = invoke(
            &app,
            request(
                &p,
                "finance/journals/journal-http-utf16/reverse",
                r#"{"periodId":"period","entryDate":"2026-01-03","memo":"\ud800"}"#,
                "http-utf16-reverse",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{result}");
        let stored: String = sqlx::query_scalar("SELECT memo FROM finance_journals WHERE id=$1")
            .bind(result["data"]["reversalJournalId"].as_str().unwrap())
            .fetch_one(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_eq!(stored, "\u{fffd}");
        assert_eq!(
            invoke(
                &app,
                request(
                    &p,
                    "finance/journals/journal-http-utf16/reverse",
                    r#"{"periodId":"period","entryDate":"2026-01-03","memo":"\ufffd"}"#,
                    "http-utf16-reverse"
                )
            )
            .await
            .0,
            StatusCode::CONFLICT,
            "different original UTF16 must never collide despite identical UTF8 SQL storage"
        );
        assert_eq!(totals(&p, "http-utf16-reverse").await, (1, 1, 1));
    })
    .await;
}
#[tokio::test]
async fn actual_auth_origin_host_cookie_cas_and_validation_order_fail_before_any_financial_effect()
{
    account::with_system(|system| async move {
        let p = setup(system).await;
        let app = router(&p, false);
        let path = "finance/journals/missing/post";
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().remove("cookie");
        r.headers_mut().remove("x-erpx-context-assignment-version");
        let (status, result) = invoke(&app, r).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{result}");
        assert_eq!(result["error"]["code"], "UNAUTHENTICATED");
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().insert(
            "origin",
            HeaderValue::from_static("https://foreign.example.test"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::FORBIDDEN);
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut()
            .insert("host", HeaderValue::from_static("retired.example.test"));
        assert_eq!(invoke(&app, r).await.0, StatusCode::NOT_FOUND);
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().append(
            "cookie",
            HeaderValue::from_static("unrelated=x; unrelated=y"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::BAD_REQUEST);
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().remove("x-erpx-context-assignment-version");
        assert_eq!(invoke(&app, r).await.0, StatusCode::CONFLICT);
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().append(
            "x-erpx-context-assignment-version",
            HeaderValue::from_static("3"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::CONFLICT);
        let mut r = request(&p, path, "not-json", "key");
        r.headers_mut().insert(
            "x-erpx-context-assignment-version",
            HeaderValue::from_static("2"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::CONFLICT);
        let (status, result) = invoke(&app, request(&p, path, "not-json", "key")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{result}");
        assert_eq!(result["error"]["code"], "INVALID_JSON");
        assert_eq!(totals(&p, "key").await, (0, 0, 0));
    })
    .await;
}
#[tokio::test]
async fn actual_finance_http_retains_audit_rollback_and_live_binding_denial_without_leaking_database_errors()
 {
    account::with_system(|system|async move {
  let p=setup(system).await;p.create("http-rollback",false).await.unwrap();let app=router(&p,false);
  sqlx::raw_sql("CREATE FUNCTION deny_http_audit()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='finance.journal.post'THEN RAISE EXCEPTION 'private-database-detail' USING ERRCODE='23514';END IF;RETURN NEW;END$$;CREATE TRIGGER deny_http_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION deny_http_audit()") .execute(p.system.holdings[0].raw()).await.unwrap();
  let (status,result)=invoke(&app,request(&p,"finance/journals/journal-http-rollback/post",r#"{"expectedVersion":1}"#,"http-rollback-post")).await;assert_eq!(status,StatusCode::INTERNAL_SERVER_ERROR,"{result}");assert!(!result.to_string().contains("private-database-detail"));assert_eq!(totals(&p,"http-rollback-post").await,(0,0,0));
  let status:String=sqlx::query_scalar("SELECT status FROM finance_journals WHERE id='journal-http-rollback'").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(status,"draft");
  sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(p.system.control.raw()).await.unwrap();assert_eq!(invoke(&app,request(&p,"finance/journals/journal-http-rollback/post",r#"{"expectedVersion":1}"#,"http-denied")).await.0,StatusCode::NOT_FOUND);assert_eq!(totals(&p,"http-denied").await,(0,0,0));
 }).await;
}
async fn legal_seed(p: &Prepared) {
    let pool = &p.system.holdings[0];
    let now = p.system.scenario.clock.now().get();
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_finance_review',2,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"legal.read\",\"module.legal.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_finance_review';INSERT INTO legal_settings(company_id,enabled)VALUES('company',true);INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)VALUES('http-item','company','contract','Synthetic HTTP','confidential','TH','TH','en','THB','Asia/Bangkok','normal','fixture-b','fixture-b',0,0);INSERT INTO legal_contracts VALUES('http-item','{}');INSERT INTO legal_members VALUES('http-item','fixture-b','read');INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,body,created_by,created_at)VALUES('http-document','http-item',1,'Authored text','text/plain',repeat('a',64),5,'draft','fixture-b',0)").execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at)VALUES('company','legal',1,'active',0,$1,1,'Synthetic reviewed Legal','synthetic evidence',0)").bind(now+3_600_000).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'http-legal-purchase','company','legal','fixture-b','synthetic-request',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),0 FROM company_package_grants WHERE company_id='company'AND package_id='legal'").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn actual_legal_queue_http_ignores_all_bodies_requires_exact_origin_and_preserves_current_record_access()
 {
    account::with_system(|system| async move {
        let p = setup(system).await;
        legal_seed(&p).await;
        let app = router(&p, true);
        for kind in ["ocr", "verify"] {
            for content in [None, Some("application/json"), Some("text/plain")] {
                for body in ["", "not-json", "null"] {
                    let mut r = request(
                        &p,
                        &format!("legal/documents/http-document/{kind}"),
                        body,
                        &format!("http-legal-{kind}"),
                    );
                    if let Some(content) = content {
                        r.headers_mut()
                            .insert("content-type", HeaderValue::from_str(content).unwrap());
                    }
                    let (status, result) = invoke(&app, r).await;
                    assert_eq!(status, StatusCode::OK, "{kind}/{content:?}/{body}:{result}");
                    assert_eq!(result["data"]["id"], "http-item");
                    assert_eq!(result["data"]["state"], "queued");
                }
            }
        }
        assert_eq!(totals(&p, "http-legal-ocr").await, (1, 1, 1));
        assert_eq!(totals(&p, "http-legal-verify").await, (1, 1, 1));
        let mut r = request(&p, "legal/documents/http-document/ocr", "bad", "fresh-key");
        r.headers_mut().remove("origin");
        assert_eq!(invoke(&app, r).await.0, StatusCode::FORBIDDEN);
        let mut r = request(&p, "legal/documents/http-document/ocr", "bad", "fresh-key");
        r.headers_mut().insert(
            "origin",
            HeaderValue::from_static("https://foreign.example.test"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::FORBIDDEN);
        sqlx::query("DELETE FROM legal_members WHERE item_id='http-item'")
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        let (status, result) = invoke(
            &app,
            request(
                &p,
                "legal/documents/http-document/ocr",
                "bad",
                "http-legal-ocr",
            ),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{result}");
        assert_eq!(totals(&p, "http-legal-ocr").await, (1, 1, 1));
    })
    .await;
}
