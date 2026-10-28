//! Joined unmounted HTTP through the actual Account, Holding and owner unit.
#[allow(dead_code, reason = "Shared real Account and Field Service fixture")]
#[expect(
    unused_imports,
    reason = "Shared fixture reexports helpers used by other test targets"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
#[cfg(feature = "private-development-api")]
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_workflow::public::commands::CommandJson;
#[cfg(feature = "private-development-api")]
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api,
};
use erpx_service::{
    business_http::BusinessScopeSlugs, field_service_http::*, field_service_parts::*,
};
use facts::*;
use sqlx::PgConnection;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
struct CountingBody {
    reads: Arc<AtomicUsize>,
    data: Option<Vec<u8>>,
}
impl tokio::io::AsyncRead for CountingBody {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(bytes) = self.data.take() {
            self.reads.fetch_add(1, Ordering::SeqCst);
            buffer.put_slice(&bytes);
        }
        Poll::Ready(Ok(()))
    }
}
use tower::ServiceExt;

struct Scope;
impl BusinessScopeSlugs for Scope {
    fn slugs(&self, _: &HeaderMap) -> Result<(Option<String>, Option<String>), BusinessError> {
        Ok((None, None))
    }
}
struct Other;
impl OtherPost for Other {
    fn validate(&self, _: &OtherPostInput) -> Result<(), String> {
        Ok(())
    }
    fn admission_error(&self, _: OtherPostBranch, error: BusinessError) -> BusinessError {
        error
    }
    fn execute<'a>(
        &'a self,
        _: &'a mut PgConnection,
        _: &'a AuthorizedWorkContext,
        _: OtherPostInput,
    ) -> PgWorkFuture<'a, CommandJson, BusinessError> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "UNSUPPORTED_OTHER",
                "Other workflows are not implemented by this test.",
            )
            .into())
        })
    }
}
async fn send_raw(
    router: &axum::Router,
    cookie: &str,
    key: &str,
    body: &str,
) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/field-service")
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie)
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-revision", "8")
        .header("x-erpx-context-assignment", "field-fixture-a")
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
async fn send(
    router: &axum::Router,
    cookie: &str,
    key: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let (status, raw) = send_raw(router, cookie, key, body).await;
    (status, serde_json::from_str(&raw).unwrap())
}

async fn seed_order(p: &Parts, physical: &str, ticket: &str, event: &str, now: i64) {
    let mut tx = p.pool().raw().begin().await.unwrap();
    sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,source_kind,service_agreement_id,service_order_id,service_order_version) VALUES($3,'company','branch',NULL,'customer','Synthetic raw intake','Synthetic issue','fixture-a',to_char(to_timestamp(($2::bigint+60000)/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'),'fixture-a',$2,$2,'field_service','agreement',$1,1)").bind(physical).bind(now).bind(ticket).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,scheduled_at,status,created_by,version,created_at,updated_at) VALUES($1,'company','branch',$3,1,'agreement',2,'Synthetic issue','Synthetic site','fixture-a',$2+60000,'scheduled','fixture-a',1,$2,$2)").bind(physical).bind(now).bind(ticket).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES($3,$1,1,'create','fixture-a','Synthetic historical event','scheduled','fixture-a',$2+60000,'{}',$2)").bind(physical).bind(now).bind(event).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
}

// Fixed synthetic role only; fixture preparation, never an authorization port.
async fn publish_http_fixture_permissions(
    p: &Parts,
    previous: i32,
    next: i32,
    permissions: serde_json::Value,
    now: i64,
) {
    let mut tx = p.pool().raw().begin().await.unwrap();
    let inserted = sqlx::query(
        "INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) SELECT company_id,role_id,$1,$2::jsonb,grantable_json,scopes_json,'synthetic',$3 FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_field_parts' AND revision=$4",
    )
    .bind(next)
    .bind(permissions)
    .bind(now)
    .bind(previous)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(inserted.rows_affected(), 1);
    let updated = sqlx::query(
        "UPDATE iam_roles SET published_revision=$1 WHERE company_id='*' AND id='custom_field_parts' AND published_revision=$2 AND status='active' AND NOT built_in",
    )
    .bind(next)
    .bind(previous)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(updated.rows_affected(), 1);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn joined_real_reserve_issue_replay_and_late_key_source_order() {
    account::with_system(|system| async move {
        let p=setup(system).await;
        let leaf:serde_json::Value=serde_json::from_str(include_str!("../../../fixtures/field_service/rust-http/pg-text-transport.v1.json")).unwrap();
        let cases=leaf["cases"].as_array().unwrap();assert_eq!(cases.len(),8);
        for case in cases {
            let units=case["inputUtf16"].as_array().unwrap().iter().map(|n|n.as_u64().unwrap() as u16).collect();
            let target=erpx_server_field_service::public::parts::FieldServiceRawOrderTarget::from_utf16(units);
            let actual=sqlx::query_as::<_,(String,String,i32)>("SELECT $1::text,encode(convert_to($1::text,'UTF8'),'hex'),octet_length($1::text)").bind(target.storage_text()).fetch_one(p.pool().raw()).await;
            if case["ok"].as_bool().unwrap() {
                let (value,hex,bytes)=actual.unwrap();assert_eq!(value,case["result"]["value"].as_str().unwrap());assert_eq!(hex,case["bindHex"].as_str().unwrap());assert_eq!(i64::from(bytes),case["result"]["bytes"].as_i64().unwrap());
            } else {
                let error=actual.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("22021"));
            }
        }
        let central=account::login(&p.review.system,"a@example.test",None).await;
        let child=account::child(&p.review.system,&central,account::ACME).await;
        let now=p.review.system.scenario.clock.now().get();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)")
            .bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let config=FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),PackageEnforcementMode::Enforced).unwrap();
        let service=Arc::new(FieldServicePartsAdministration::new(p.review.authorizer.clone(),config,p.review.system.scenario.clock.clone(),p.review.system.scenario.clock.clone()));
        let router=field_service_http_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(Scope),vec![FieldServiceHttpTarget::new("acme.example.test".into(),"acme".into(),service,Arc::new(Other))]).unwrap();
        let reserve=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic genuine reservation"}}"#,now+3_600_000);
        let reads=Arc::new(AtomicUsize::new(0));
        let stream=tokio_util::codec::FramedRead::new(CountingBody { reads: reads.clone(), data: Some(b"{}".to_vec()) },tokio_util::codec::BytesCodec::new());
        let req=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service")
            .header("host","acme.example.test").header("idempotency-key","joined-auth-reader")
            .body(Body::from_stream(stream)).unwrap();
        let response=router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(),StatusCode::UNAUTHORIZED);assert_eq!(reads.load(Ordering::SeqCst),0);
        let unauthenticated=send(&router,"","joined-auth-denied","{").await;
        assert_eq!(unauthenticated.0,StatusCode::UNAUTHORIZED);
        for raw_id in ["".to_owned(), "x".repeat(220)] {
            let candidate=reserve.replace("reserve-order",&raw_id);
            let missing=send(&router,&cookie,"joined-missing-id",&candidate).await;
            assert_eq!(missing.0,StatusCode::NOT_FOUND);
        }
        let nul=reserve.replace("reserve-order",r"a\u0000b");
        let nul_result=send(&router,&cookie,"joined-nul-id",&nul).await;
        assert_eq!(nul_result.0,StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(nul_result.1["error"]["code"],"INTERNAL_ERROR");
        sqlx::raw_sql("CREATE SEQUENCE http_retry_attempt;CREATE FUNCTION http_retry_once() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF nextval('http_retry_attempt')=1 THEN RAISE EXCEPTION 'synthetic serialization retry' USING ERRCODE='40001'; END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER http_retry_once AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='joined-reserve-one') EXECUTE FUNCTION http_retry_once()")
            .execute(p.pool().raw()).await.unwrap();
        let reads=Arc::new(AtomicUsize::new(0));
        let stream=tokio_util::codec::FramedRead::new(CountingBody { reads: reads.clone(), data: Some(reserve.as_bytes().to_vec()) },tokio_util::codec::BytesCodec::new());
        let req=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service")
            .header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","joined-reserve-one")
            .header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3")
            .body(Body::from_stream(stream)).unwrap();
        let response=router.clone().oneshot(req).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();
        let first=(status,serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
        let retry_state=sqlx::query_as::<_,(bool,i64)>("SELECT is_called,last_value::bigint FROM http_retry_attempt").fetch_one(p.pool().raw()).await.unwrap();
        let effects=p.counts("joined-reserve-one").await;
        let owner=sqlx::query_as::<_,(i64,i64)>("SELECT version::bigint,(SELECT count(*) FROM field_service_part_reservations WHERE id='field-part-joined-reserve-one') FROM field_service_orders WHERE id='reserve-order'").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(first.0,StatusCode::OK,"first reserve response={:?}; body_reads={}; retry=(called,last)={:?}; effects=(receipt,audit,outbox,consumption,notice)={:?}; owner=(version,reservation_count)={:?}",first.1,reads.load(Ordering::SeqCst),retry_state,effects,owner);
        assert_eq!(first.1["data"]["version"],2);assert_eq!(reads.load(Ordering::SeqCst),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM http_retry_attempt").fetch_one(p.pool().raw()).await.unwrap(),2);
        let replay=send(&router,&cookie,"joined-reserve-one",&reserve).await; assert_eq!(replay,first); assert_eq!(p.counts("joined-reserve-one").await,(1,1,1,0,1));
        // Externally seeded history: bad! has never been admitted by native writes.
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'bad!',operation,actor_id,request_hash,record_id,result_json,created_at,authority_json FROM command_receipts WHERE command_id='joined-reserve-one'").execute(p.pool().raw()).await.unwrap();
        let historical=send(&router,&cookie,"bad!",&reserve).await;assert_eq!(historical,first);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM outbox_events WHERE command_id='bad!'").fetch_one(p.pool().raw()).await.unwrap(),0);
        let conflict=send(&router,&cookie,"joined-reserve-one",&reserve.replace("Synthetic genuine reservation","Changed reason")).await;
        assert_eq!(conflict.0,StatusCode::CONFLICT); assert_eq!(conflict.1["error"]["code"],"IDEMPOTENCY_CONFLICT");
        let issue=r#"{"id":"issue-order","action":"issue-part","expectedVersion":3,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic genuine Service issue"}"#;
        let invalid_key=send(&router,&cookie,"wrong!",issue).await;
        assert_eq!(invalid_key.0,StatusCode::UNPROCESSABLE_ENTITY);assert_eq!(invalid_key.1["error"]["code"],"COMMAND_KEY_REQUIRED");
        assert_eq!(p.counts("wrong!").await,(0,0,0,0,0));
        sqlx::raw_sql("CREATE SEQUENCE http_final_attempt;CREATE FUNCTION http_final_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('http_final_attempt');UPDATE commerce_stock SET quantity_milli=quantity_milli+1 WHERE warehouse_id='warehouse' AND product_id='product';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER http_final_fault AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='joined-rollback-one') EXECUTE FUNCTION http_final_fault()")
            .execute(p.pool().raw()).await.unwrap();
        let rejected=send(&router,&cookie,"joined-rollback-one",issue).await;
        assert_ne!(rejected.0,StatusCode::OK);
        assert!(sqlx::query_scalar::<_,bool>("SELECT is_called FROM http_final_attempt").fetch_one(p.pool().raw()).await.unwrap());
        assert_eq!(p.counts("joined-rollback-one").await,(0,0,0,0,0));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT consumed_milli FROM field_service_part_reservations WHERE id='reserved-part'").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version::bigint FROM field_service_orders WHERE id='issue-order'").fetch_one(p.pool().raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT service_order_version FROM support_tickets WHERE id='ticket-issue-order'").fetch_one(p.pool().raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM finance_journals").fetch_one(p.pool().raw()).await.unwrap(),0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM money_transaction_snapshots").fetch_one(p.pool().raw()).await.unwrap(),0);
        let issued=send(&router,&cookie,"joined-issue-one",issue).await; assert_eq!(issued.0,StatusCode::OK); assert_eq!(issued.1["data"]["version"],4);
        assert_eq!(p.counts("joined-issue-one").await,(1,1,1,1,1));
        let quote_failure=issue.replace("\"quantityMilli\":1000","\"quantityMilli\":100000");
        let quote=send(&router,&cookie,"invalid!",&quote_failure).await;
        assert_ne!(quote.0,StatusCode::OK);assert_ne!(quote.1["error"]["code"],"COMMAND_KEY_REQUIRED");
        assert_eq!(p.counts("invalid!").await,(0,0,0,0,0));
        publish_http_fixture_permissions(&p,1,2,serde_json::json!(["records.write","module.field_service.read"]),now).await;
        let invalid=reserve.replace("\"expectedVersion\":1","\"expectedVersion\":0");
        let schema=send(&router,&cookie,"joined-read-denied",&invalid).await;
        assert_eq!(schema.0,StatusCode::UNPROCESSABLE_ENTITY);assert_eq!(schema.1["error"]["code"],"VALIDATION_FAILED");
        let denied=send(&router,&cookie,"joined-read-denied",&reserve).await;
        assert_eq!(denied.0,StatusCode::FORBIDDEN);assert_eq!(denied.1["error"]["code"],"WORK_ROLE_UNAVAILABLE");
        publish_http_fixture_permissions(&p,2,3,serde_json::json!(["records.read","module.field_service.read"]),now).await;
        let denied=send(&router,&cookie,"joined-write-denied",&reserve).await;
        assert_eq!(denied.0,StatusCode::FORBIDDEN);assert_eq!(denied.1["error"]["code"],"WORK_ROLE_UNAVAILABLE");
        publish_http_fixture_permissions(&p,3,4,serde_json::json!(["records.read","records.write","module.field_service.read"]),now).await;
        let original_permissions:serde_json::Value=sqlx::query_scalar("SELECT permissions_json FROM iam_role_revisions WHERE company_id='*' AND role_id='custom_field_parts' AND revision=1").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(original_permissions,serde_json::json!(["records.read","records.write","module.field_service.read"]));
        // Two distinct original UTF-16 targets bind to the same actual PostgreSQL row.
        let physical="raw-\u{fffd}";
        seed_order(&p,physical,"ticket-raw","historical-raw-1",now).await;
        let identity_reserve=reserve.replace(r#""quantityMilli":1000"#,r#""quantityMilli":500"#);
        let high=identity_reserve.replace("reserve-order",r"raw-\ud800");
        let low=identity_reserve.replace("reserve-order",r"raw-\udc00");
        // Diagnostic only: distinguish shared wire classification and the exact
        // receipt-trigger JSONB expression from the HTTP response's hidden error.
        let classification=match classify_shared_post(high.as_bytes()) {
            Ok(SharedPostWire::Parts { .. }) => "Parts",
            Ok(SharedPostWire::Other { .. }) => "Other",
            Err(SharedPostWireFailure::InvalidJson) => "InvalidJson",
            Err(SharedPostWireFailure::Schema(_)) => "Schema",
        };
        let request=CommandJson::parse(&high).unwrap();
        let logical_id=request.property("id").unwrap().unwrap();
        let expected_version:i64=request.property("expectedVersion").unwrap().unwrap().as_str().parse().unwrap();
        let status:String=sqlx::query_scalar("SELECT status FROM field_service_orders WHERE id=$1").bind(physical).fetch_one(p.pool().raw()).await.unwrap();
        let logical_result=CommandJson::parse(&format!("{{\"id\":{},\"version\":{},\"status\":{}}}",logical_id.as_str(),expected_version+1,serde_json::to_string(&status).unwrap())).unwrap();
        let leaf=match sqlx::query_scalar::<_,bool>("SELECT jsonb_exists($1::jsonb,'_posInitialPhysicalStockCount')").bind(logical_result.as_str()).fetch_one(p.pool().raw()).await {
            Ok(value) => format!("Ok({value})"),
            Err(error) => { let error=PersistenceError::from(error); format!("Persistence(code={}, sql_state={:?})",error.code,error.sql_state()) },
        };
        let actual=send_raw(&router,&cookie,"joined-raw-rejected",&high).await;
        let effects=p.counts("joined-raw-rejected").await;
        let owner=sqlx::query_as::<_,(i64,i64)>("SELECT version::bigint,(SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1) FROM field_service_orders WHERE id=$1").bind(physical).fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(actual.0,StatusCode::INTERNAL_SERVER_ERROR,"classifier={classification}; logical_result={}; trigger_expression_leaf={leaf}; HTTP body={:?}; effects=(receipt,audit,outbox,consumption,notice)={effects:?}; owner=(version,reservation_count)={owner:?}",logical_result.as_str(),actual.1);
        let rejected:serde_json::Value=serde_json::from_str(&actual.1).unwrap();
        assert_eq!(rejected["error"]["code"],"INTERNAL_ERROR");
        assert_eq!(rejected["error"]["message"],"The request could not be completed.");
        assert_eq!(effects,(0,0,0,0,0));assert_eq!(owner,(1,0));
        let scalar=identity_reserve.replace("reserve-order",physical);
        let success=send(&router,&cookie,"joined-raw-identity",&scalar).await;
        let control_effects=p.counts("joined-raw-identity").await;
        let control_owner=sqlx::query_as::<_,(i64,i64)>("SELECT version::bigint,(SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1) FROM field_service_orders WHERE id=$1").bind(physical).fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(success.0,StatusCode::OK,"scalar control response={:?}; effects=(receipt,audit,outbox,consumption,notice)={control_effects:?}; owner=(version,reservation_count)={control_owner:?}",success.1);assert_eq!(success.1["data"]["id"],physical);
        assert_eq!(success.1["data"]["version"],2);assert_eq!(success.1["data"]["status"],"scheduled");
        let saved=sqlx::query_as::<_,(String,String)>("SELECT request_hash,result_json FROM command_receipts WHERE command_id='joined-raw-identity'").fetch_one(p.pool().raw()).await.unwrap();
        let saved_result:serde_json::Value=serde_json::from_str(&saved.1).unwrap();assert_eq!(saved_result["id"],physical);
        let success_effects=p.counts("joined-raw-identity").await;assert_eq!(success_effects,(1,1,1,0,1));
        let success_owner=sqlx::query_as::<_,(i64,i64)>("SELECT version::bigint,(SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1) FROM field_service_orders WHERE id=$1").bind(physical).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(success_owner,(2,1));
        for candidate in [&high,&low] {
            let conflict=send(&router,&cookie,"joined-raw-identity",candidate).await;
            assert_eq!(conflict.0,StatusCode::CONFLICT);assert_eq!(conflict.1["error"]["code"],"IDEMPOTENCY_CONFLICT");
            assert_eq!(p.counts("joined-raw-identity").await,success_effects);
            assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT request_hash,result_json FROM command_receipts WHERE command_id='joined-raw-identity'").fetch_one(p.pool().raw()).await.unwrap(),saved);
            assert_eq!(sqlx::query_as::<_,(i64,i64)>("SELECT version::bigint,(SELECT count(*) FROM field_service_part_reservations WHERE order_id=$1) FROM field_service_orders WHERE id=$1").bind(physical).fetch_one(p.pool().raw()).await.unwrap(),success_owner);
        }
        let long_id="x".repeat(220);seed_order(&p,&long_id,"ticket-long","historical-long-1",now).await;
        let long=identity_reserve.replace("reserve-order",&long_id);let long_result=send(&router,&cookie,"joined-long-identity",&long).await;
        assert_eq!(long_result.0,StatusCode::OK,"long220 response={long_result:?}");assert_eq!(long_result.1["data"]["id"],long_id,"long220 response={long_result:?}");
        let missing=reserve.replace("reserve-order","absent-order");
        let missing=send(&router,&cookie,"bad!",&missing).await; assert_eq!(missing.0,StatusCode::NOT_FOUND,"missing-order response={missing:?}"); assert_eq!(missing.1["error"]["code"],"FIELD_SERVICE_ORDER_NOT_FOUND","missing-order response={missing:?}");
        // History remains eligible after settlement is withdrawn; it cannot
        // supply settlement to a new command or mask a different-target collision.
        withdraw(&p,"service-parts-assets").await;
        let history=send(&router,&cookie,"bad!",&reserve).await;assert_eq!(history,first);
        let inactive=send(&router,&cookie,"bad!",issue).await;
        assert_eq!(inactive.1["error"]["code"],"PACKAGE_INACTIVE","inactive-settlement response={inactive:?}");
        let fresh=send(&router,&cookie,"new-inactive-key",issue).await;
        assert_eq!(fresh.1["error"]["code"],"PACKAGE_INACTIVE","inactive-new-command response={fresh:?}");
        let conflict=send(&router,&cookie,"bad!",&reserve.replace("Synthetic genuine reservation","Changed reason")).await;
        assert_eq!(conflict.0,StatusCode::CONFLICT,"changed-replay response={conflict:?}");assert_eq!(conflict.1["error"]["code"],"IDEMPOTENCY_CONFLICT","changed-replay response={conflict:?}");
    }).await;
}

#[tokio::test]
async fn existing_receipt_collision_does_not_mask_missing_purchase_history() {
    account::with_system(|system| async move {
        let p=setup_without_service_history(system).await;
        let now=p.review.system.scenario.clock.now().get();
        let central=account::login(&p.review.system,"a@example.test",None).await;
        let child=account::child(&p.review.system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)")
            .bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES('history-collision','field.order.reserve-part','fixture-a','different-hash','reserve-order','{}',$1,'{\"companyId\":\"company\"}')")
            .bind(now).execute(p.pool().raw()).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let config=FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),PackageEnforcementMode::Enforced).unwrap();
        let service=Arc::new(FieldServicePartsAdministration::new(p.review.authorizer.clone(),config,p.review.system.scenario.clock.clone(),p.review.system.scenario.clock.clone()));
        let router=field_service_http_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(Scope),vec![FieldServiceHttpTarget::new("acme.example.test".into(),"acme".into(),service,Arc::new(Other))]).unwrap();
        let body=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic source order"}}"#,now+3_600_000);
        let result=send(&router,&cookie,"history-collision",&body).await;
        assert_eq!(result.1["error"]["code"],"PACKAGE_REQUIRED");
        assert_ne!(result.1["error"]["code"],"IDEMPOTENCY_CONFLICT");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM outbox_events WHERE command_id='history-collision'").fetch_one(p.pool().raw()).await.unwrap(),0);
    }).await;
}

#[path = "../../support/runtime/private-session-config.rs"]
#[cfg(feature = "private-development-api")]
mod private_configuration;

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_reserve_readback_is_explicit_scoped_and_receipt_bound() {
    account::with_system(|system| async move {
        let p=setup(system).await;
        let system=&p.review.system;let now=system.scenario.clock.now().get();
        let central=account::login(system,"a@example.test",None).await;
        assert!(private_configuration::cookie(&central).contains("__Secure-erpx-account.session_token="));
        let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let mut config=private_configuration::document(&system.control,&system.holdings).await;
        config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        let body=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic actual reserve"}}"#,now+3_600_000);
        let request=|key:&str,value:&str,revision:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).body(Body::from(value.to_owned())).unwrap();
        let build=|doc:&serde_json::Value|DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let runtime=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("off-reserve",&body,"8")).await.unwrap().status(),StatusCode::NOT_FOUND);runtime.close().await;
        config["fieldServiceReservations"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("stale-context",&body,"9")).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(runtime.router().oneshot(request("unrelated-action",&body.replace("reserve-part","issue-part"),"8")).await.unwrap().status(),StatusCode::NOT_FOUND);
        let too_much=body.replace("1000","999999");assert_ne!(runtime.router().oneshot(request("not-enough",&too_much,"8")).await.unwrap().status(),StatusCode::OK);
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT fixture_field_audit CHECK(command_id<>'field-rollback')").execute(p.pool().raw()).await.unwrap();
        assert_eq!(runtime.router().oneshot(request("field-rollback",&body,"8")).await.unwrap().status(),StatusCode::INTERNAL_SERVER_ERROR);
        let rollback:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_part_reservations WHERE id='field-part-field-rollback'),(SELECT count(*) FROM command_receipts WHERE command_id='field-rollback'),(SELECT count(*) FROM audit_events WHERE command_id='field-rollback'),(SELECT count(*) FROM outbox_events WHERE command_id='field-rollback')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(rollback,(0,0,0,0));
        sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT fixture_field_audit").execute(p.pool().raw()).await.unwrap();
        let response=runtime.router().oneshot(request("reserve-native",&body,"8")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let receipt:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(receipt["data"]["id"],"reserve-order");assert_eq!(receipt["data"]["version"],2);
        assert_eq!(runtime.router().oneshot(request("reserve-native",&body,"8")).await.unwrap().status(),StatusCode::OK);
        assert_eq!(runtime.router().oneshot(request("stale-order",&body,"8")).await.unwrap().status(),StatusCode::CONFLICT);
        let get=Request::builder().uri("/api/v1/holdings/acme/field-service/reserve-order/reservations/field-part-reserve-native").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let read:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(read["reservation"]["quantityMilli"],1000);assert_eq!(read["event"]["reason"],"Synthetic actual reserve");assert_eq!(read["receipt"]["version"],2);assert_eq!(read["context"]["userId"],"fixture-a");assert_eq!(read["stock"]["quantityMilli"],5000);
        let foreign=Request::builder().uri("/api/v1/holdings/acme/field-service/reserve-order/reservations/field-part-reserve-native").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("x-erpx-context-company","foreign").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(foreign).await.unwrap().status(),StatusCode::OK);
        let missing=Request::builder().uri("/api/v1/holdings/acme/field-service/reserve-order/reservations/field-part-never-created").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::NOT_FOUND);
        let count:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_part_reservations WHERE id='field-part-reserve-native'),(SELECT count(*) FROM command_receipts WHERE command_id='reserve-native'),(SELECT count(*) FROM audit_events WHERE command_id='reserve-native'),(SELECT count(*) FROM outbox_events WHERE command_id='reserve-native')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,(1,1,1,1));
        let long_id="x".repeat(220);seed_order(&p,&long_id,"ticket-private-long","event-private-long",now).await;
        let long_body=body.replace("reserve-order",&long_id).replace("\"quantityMilli\":1000","\"quantityMilli\":500");
        let response=runtime.router().oneshot(request("reserve-long",&long_body,"8")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let get_long=||Request::builder().uri(format!("/api/v1/holdings/acme/field-service/{long_id}/reservations/field-part-reserve-long")).header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(get_long()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let read:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(read["receipt"]["id"],long_id);assert_eq!(read["reservation"]["quantityMilli"],500);
        let cjk_id="界".repeat(220);seed_order(&p,&cjk_id,"ticket-private-cjk","event-private-cjk",now).await;
        let cjk_body=body.replace("reserve-order",&cjk_id).replace("\"quantityMilli\":1000","\"quantityMilli\":200");
        let response=runtime.router().oneshot(request("reserve-cjk",&cjk_body,"8")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let encoded=cjk_id.as_bytes().iter().map(|b|format!("%{b:02X}")).collect::<String>();
        let cjk_get=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/{encoded}/reservations/field-part-reserve-cjk")).header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(cjk_get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let read:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(read["receipt"]["id"],cjk_id);assert_eq!(read["reservation"]["quantityMilli"],200);
        publish_http_fixture_permissions(&p,1,2,serde_json::json!(["records.read","module.field_service.read"]),now).await;
        assert_eq!(runtime.router().oneshot(get_long()).await.unwrap().status(),StatusCode::OK);
        assert_eq!(runtime.router().oneshot(request("reserve-long",&long_body,"8")).await.unwrap().status(),StatusCode::FORBIDDEN);
        publish_http_fixture_permissions(&p,2,3,serde_json::json!(["records.read","records.write","module.field_service.read"]),now).await;
        seed_order(&p,"race-order","ticket-race","event-race",now).await;
        let mut writer=p.pool().raw().begin().await.unwrap();sqlx::query("UPDATE commerce_stock SET quantity_milli=4700,version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();
        let race_request=request("stock-race",&body.replace("reserve-order","race-order"),"8");let race_router=runtime.router();let task=tokio::spawn(async move {race_router.oneshot(race_request).await.unwrap()});
        let waiting=tokio::time::timeout(std::time::Duration::from_secs(3),async {loop {let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='commerce_stock'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(p.pool().raw()).await.unwrap();if pending {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.is_ok();
        writer.commit().await.unwrap();let response=task.await.unwrap();assert!(waiting,"the real stock source fence must wait");assert_eq!(response.status(),StatusCode::CONFLICT);assert_eq!(p.counts("stock-race").await,(0,0,0,0,0));
        withdraw(&p,"service-parts-assets").await;
        assert_eq!(runtime.router().oneshot(request("reserve-native",&body,"8")).await.unwrap().status(),StatusCode::OK);
        assert_eq!(runtime.router().oneshot(get_long()).await.unwrap().status(),StatusCode::OK);
        assert_ne!(runtime.router().oneshot(request("new-after-withdrawal",&body,"8")).await.unwrap().status(),StatusCode::OK);
        runtime.close().await;
    }).await;
}

#[tokio::test]
async fn advancing_clock_retains_early_order_witness_across_real_retry() {
    use erpx_server_shared::public::TimestampMillis;
    use std::sync::atomic::AtomicI64;
    struct AdvancingClock(AtomicI64);
    impl Clock for AdvancingClock {
        fn now(&self) -> TimestampMillis {
            TimestampMillis::new(self.0.fetch_add(1, Ordering::SeqCst))
        }
    }
    account::with_system(|system| async move {
        let p=setup(system).await;let system=&p.review.system;let now=system.scenario.clock.now().get();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let clock=Arc::new(AdvancingClock(AtomicI64::new(now)));
        let config=FieldServicePartsConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),PackageEnforcementMode::Enforced).unwrap();
        let service=Arc::new(FieldServicePartsAdministration::new(p.review.authorizer.clone(),config,clock.clone(),clock.clone()));
        let router=field_service_http_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(Scope),vec![FieldServiceHttpTarget::new_private_reserve("acme.example.test".into(),"acme".into(),service)]).unwrap();
        sqlx::raw_sql("CREATE SEQUENCE advancing_http_attempt;CREATE FUNCTION advancing_retry_once() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF nextval('advancing_http_attempt')=1 THEN RAISE EXCEPTION 'synthetic serialization retry' USING ERRCODE='40001'; END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER advancing_retry_once AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='advancing-reserve') EXECUTE FUNCTION advancing_retry_once()").execute(p.pool().raw()).await.unwrap();
        let body=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Advancing clock reserve"}}"#,now+3_600_000);
        let reads=Arc::new(AtomicUsize::new(0));let stream=tokio_util::codec::FramedRead::new(CountingBody{reads:reads.clone(),data:Some(body.as_bytes().to_vec())},tokio_util::codec::BytesCodec::new());
        let req=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).header("idempotency-key","advancing-reserve").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").body(Body::from_stream(stream)).unwrap();
        let response=router.clone().oneshot(req).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(reads.load(Ordering::SeqCst),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT last_value FROM advancing_http_attempt").fetch_one(p.pool().raw()).await.unwrap(),2);
        let cookie=format!("__Host-erpx-product={}",child.token.expose());let replay=send(&router,&cookie,"advancing-reserve",&body).await;assert_eq!(replay.0,StatusCode::OK);assert_eq!(replay.1["data"]["version"],2);assert_eq!(p.counts("advancing-reserve").await,(1,1,1,0,1));
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM field_service_part_reservations WHERE id='field-part-advancing-reserve'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,1);
        sqlx::raw_sql("CREATE FUNCTION advancing_source_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='advancing-drift' THEN UPDATE field_service_agreements SET title='changed after source' WHERE id='agreement'; END IF; RETURN NEW; END $$; CREATE TRIGGER advancing_source_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION advancing_source_drift()").execute(p.pool().raw()).await.unwrap();
        let drift=send(&router,&cookie,"advancing-drift",&body.replace("\"expectedVersion\":1","\"expectedVersion\":2")).await;assert_eq!(drift.0,StatusCode::CONFLICT);assert_eq!(p.counts("advancing-drift").await,(0,0,0,0,0));
        let title:String=sqlx::query_scalar("SELECT title FROM field_service_agreements WHERE id='agreement'").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(title,"Synthetic retained agreement");
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_issue_readback_and_finance_source_are_receipt_bound() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        let system = &p.review.system;
        let now = system.scenario.clock.now().get();
        let central = account::login(system, "a@example.test", None).await;
        let child = account::child(system, &central, account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let mut config = private_configuration::document(&system.control, &system.holdings).await;
        config["financeJournalReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        config["fieldServiceReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        config["fieldServiceReservations"] = serde_json::json!(["h001"]);
        config["fieldServiceIssues"] = serde_json::json!(["h001"]);
        let runtime = assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let cookie = format!("__Host-erpx-product={}",child.token.expose());
        let reserve = format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Synthetic actual reserve"}}"#,now+3_600_000);
        let result = send(&runtime.router(),&cookie,"issue-reserve",&reserve).await;
        assert_eq!(result.0,StatusCode::OK,"{}",result.1);
        // Synthetic historical transition; command still consumes a genuine reservation.
        let mut historical = p.pool().raw().begin().await.unwrap();
        sqlx::query("UPDATE field_service_orders SET status='in_visit',version=3 WHERE id='reserve-order'").execute(&mut *historical).await.unwrap();
        sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('synthetic-start-issue','reserve-order',3,'start','fixture-a','Synthetic historical visit','in_visit','fixture-a',$1+60000,'{}',$1)").bind(now).execute(&mut *historical).await.unwrap();
        historical.commit().await.unwrap();
        let issue = r#"{"id":"reserve-order","action":"issue-part","expectedVersion":3,"reservationId":"field-part-issue-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Synthetic issue"}"#;
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT fixture_issue_audit CHECK(command_id<>'issue-late-rollback')").execute(p.pool().raw()).await.unwrap();
        assert_eq!(send(&runtime.router(),&cookie,"issue-late-rollback",issue).await.0,StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(p.counts("issue-late-rollback").await,(0,0,0,0,0));
        let rollback: (i64, i64, i64, i64) = sqlx::query_as("SELECT (SELECT consumed_milli FROM field_service_part_reservations WHERE id='field-part-issue-reserve'), (SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'), (SELECT version FROM field_service_orders WHERE id='reserve-order'), (SELECT count(*) FROM operational_inventory_consumptions WHERE id='operational-part-issue-late-rollback')").fetch_one(p.pool().raw()).await.unwrap();
        assert_eq!(rollback, (0, 5000, 3, 0));
        sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT fixture_issue_audit").execute(p.pool().raw()).await.unwrap();
        let result=send(&runtime.router(),&cookie,"issue-native-one",issue).await;
        assert_eq!(result.0,StatusCode::OK,"{}",result.1);
        assert_eq!(send(&runtime.router(),&cookie,"issue-native-one",issue).await.0,StatusCode::OK);
        assert_eq!(p.counts("issue-native-one").await,(1,1,1,1,1));
        let get = || Request::builder().uri("/api/v1/holdings/acme/field-service/reserve-order/issues/issue-native-one").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(get()).await.unwrap(); let status=response.status(); let bytes=to_bytes(response.into_body(),65536).await.unwrap();
        assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
        assert_eq!(data["event"]["consumptionId"],"operational-part-issue-native-one");
        assert_eq!(data["reservation"]["consumedMilli"],1000);
        assert_eq!(data["stock"]["quantityMilli"],4000);
        assert_eq!(data["receipt"]["version"],4);
        let journal=data["event"]["journalId"].as_str().unwrap();
        let finance_get=|suffix:&str,cookie:&str|Request::builder().uri(format!("/api/v1/holdings/acme/finance/journals/{journal}{suffix}")).header("host","acme.example.test").header("cookie",cookie).body(Body::empty()).unwrap();
        assert_ne!(runtime.router().oneshot(finance_get("",&cookie)).await.unwrap().status(),StatusCode::OK);
        let finance_cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());
        for suffix in ["", "/workspace"] {
            let response=runtime.router().oneshot(finance_get(suffix,&finance_cookie)).await.unwrap(); let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();
            assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["data"]["journal"]["id"],journal);
        }
        assert!(sqlx::query("UPDATE field_service_order_events SET details_json=jsonb_set(details_json,'{costMinor}','999'::jsonb) WHERE id='service-order-event-issue-native-one'").execute(p.pool().raw()).await.is_err());
        sqlx::query("UPDATE finance_journals SET source_id='unrelated' WHERE id=$1").bind(journal).execute(p.pool().raw()).await.unwrap();
        assert_ne!(runtime.router().oneshot(finance_get("",&finance_cookie)).await.unwrap().status(),StatusCode::OK);
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_field_cost_post_requires_qualified_source_and_real_flow_primary() {
    account::with_system(|system| async move {
        let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
        let p=setup(system).await;
        sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(p.pool().raw()).await.unwrap();
        let system=&p.review.system;
        let db=p.pool().raw();
        p.issue("post-field-source").await.unwrap();
        let journal:String=sqlx::query_scalar("SELECT id FROM finance_journals WHERE company_id='company' AND source_id='operational-part-post-field-source'").fetch_one(db).await.unwrap();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','field_post_reviewer','Field journal reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','field_post_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='field_post_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('field-post-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('field-post-engagement','field-post-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(db).await.unwrap();
        let mut config=private_configuration::document(&system.control,&system.holdings).await;
        config["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        config["financeJournalPosts"]=serde_json::json!(["h001"]);
        config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        config["fieldServiceReservations"]=serde_json::json!(["h001"]);
        config["fieldServiceIssues"]=serde_json::json!(["h001"]);
        let cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());
        let post=|key:&str, revision:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/post")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).header("idempotency-key",key).header("content-type","application/json").body(Body::from(r#"{"expectedVersion":1}"#)).unwrap();
        let build=|doc:&serde_json::Value|DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let closed=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_ne!(closed.router().oneshot(post("field-off","7")).await.unwrap().status(),StatusCode::OK);
        closed.close().await;
        config["financeFieldServiceJournalPosts"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("field-bad-context","999")).await.unwrap().status(),StatusCode::OK);
        sqlx::query("UPDATE hr_engagements SET status='ended' WHERE id='field-post-engagement'").execute(db).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("field-no-staff","7")).await.unwrap().status(),StatusCode::OK);
        sqlx::query("UPDATE hr_engagements SET status='active' WHERE id='field-post-engagement'").execute(db).await.unwrap();
        sqlx::query("UPDATE finance_journals SET source_id='unrelated' WHERE id=$1").bind(&journal).execute(db).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("field-source-drift","7")).await.unwrap().status(),StatusCode::OK);
        sqlx::query("UPDATE finance_journals SET source_id='operational-part-post-field-source' WHERE id=$1").bind(&journal).execute(db).await.unwrap();
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT field_post_reject CHECK(command_id<>'field-post-rollback')").execute(db).await.unwrap();
        let rollback_response=runtime.router().oneshot(post("field-post-rollback","7")).await.unwrap();let rollback_status=rollback_response.status();let rollback_body=to_bytes(rollback_response.into_body(),65536).await.unwrap();assert_eq!(rollback_status,StatusCode::INTERNAL_SERVER_ERROR,"{}",String::from_utf8_lossy(&rollback_body));
        let rollback:(String,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT status FROM finance_journals WHERE id=$1),(SELECT version FROM finance_journals WHERE id=$1),(SELECT count(*) FROM flow_events WHERE command_id='field-post-rollback'),(SELECT count(*) FROM flow_routes WHERE record_id=$1),(SELECT count(*) FROM command_receipts WHERE command_id='field-post-rollback'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id=$1),(SELECT count(*) FROM outbox_events WHERE command_id='field-post-rollback')").bind(&journal).fetch_one(db).await.unwrap();
        assert_eq!(rollback,("draft".into(),1,0,0,0,0,0));
        sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT field_post_reject").execute(db).await.unwrap();
        let response=runtime.router().oneshot(post("field-post-success","7")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
        assert_eq!(runtime.router().oneshot(post("field-post-success","7")).await.unwrap().status(),StatusCode::OK);
        let facts:(String,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT status FROM finance_journals WHERE id=$1),(SELECT version FROM finance_journals WHERE id=$1),(SELECT count(*) FROM flow_events WHERE command_id='field-post-success'),(SELECT count(*) FROM command_receipts WHERE command_id='field-post-success'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id=$1),(SELECT count(*) FROM audit_events WHERE command_id='field-post-success'),(SELECT count(*) FROM outbox_events WHERE command_id='field-post-success')").bind(&journal).fetch_one(db).await.unwrap();assert_eq!(facts,("posted".into(),2,1,1,1,1,1));
        let primary:(String,String)=sqlx::query_as("SELECT steps_json::jsonb->'draft'->>'primaryUserId',(SELECT actor_user_id FROM flow_events WHERE command_id='field-post-success') FROM flow_routes WHERE company_id='company' AND kind='journal' AND record_id=$1").bind(&journal).fetch_one(db).await.unwrap();
        assert_eq!(primary,("fixture-b".into(),"fixture-b".into()));
        let read=Request::builder().uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/workspace")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(read).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["data"]["journal"]["status"],"posted");assert_eq!(data["data"]["journal"]["version"],2);
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_request_create_is_atomic_and_replayable() {
    account::with_system(|system|async move{
  let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
  let p=setup(system).await;sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(p.pool().raw()).await.unwrap();let system=&p.review.system;let now=system.scenario.clock.now().get();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
  let mut doc=private_configuration::document(&system.control,&system.holdings).await;
  doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let body=r#"{"entity":"request","agreementId":"agreement","agreementVersion":2,"title":"New visit","issue":"Synthetic problem","site":"Synthetic site","technicianId":"fixture-a","scheduledAt":null}"#;
  let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send_raw(&closed.router(),&cookie,"request-off",body).await.0,StatusCode::NOT_FOUND);closed.close().await;
  doc["fieldServiceRequests"]=serde_json::json!(["h001"]);
  let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let result=send(&runtime.router(),&cookie,"native-request-one",body).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
  assert_eq!(result.1["data"]["id"],"service-order-native-request-one");assert_eq!(result.1["data"]["ticketId"],"ticket-native-request-one");
  assert_eq!(send(&runtime.router(),&cookie,"native-request-one",body).await.0,StatusCode::OK);
  let count:(i64,i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM support_tickets WHERE id='ticket-native-request-one'),(SELECT count(*) FROM support_ticket_events WHERE id='ticket-event-native-request-one-intake'),(SELECT count(*) FROM field_service_orders WHERE id='service-order-native-request-one'),(SELECT count(*) FROM field_service_order_events WHERE id='service-order-event-native-request-one'),(SELECT count(*) FROM staff_notifications WHERE id='service-order-notice-native-request-one'),(SELECT count(*) FROM command_receipts WHERE command_id='native-request-one'),(SELECT count(*) FROM audit_events WHERE command_id='native-request-one'),(SELECT count(*) FROM outbox_events WHERE command_id='native-request-one')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,(1,1,1,1,1,1,1,1));
  let read=||Request::builder().uri("/api/v1/holdings/acme/field-service/requests/native-request-one").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
  let response=runtime.router().oneshot(read()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["receipt"]["ticketId"],"ticket-native-request-one");assert_eq!(data["order"]["customerId"],"customer");assert_eq!(data["event"]["issue"],"Synthetic problem");assert_eq!(data["event"]["actorId"],"fixture-a");
  let unauth=Request::builder().uri("/api/v1/holdings/acme/field-service/requests/native-request-one").header("host","acme.example.test").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(unauth).await.unwrap().status(),StatusCode::OK);
  let foreign=Request::builder().uri("/api/v1/holdings/acme/field-service/requests/native-request-one").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","foreign").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(foreign).await.unwrap().status(),StatusCode::OK);
  let mut transition=p.pool().raw().begin().await.unwrap();
  sqlx::query("UPDATE field_service_orders SET status='scheduled',scheduled_at=$1,version=2,completion_evidence=repeat('x',1100000) WHERE id='service-order-native-request-one'").bind(now+60000).execute(&mut *transition).await.unwrap();
  sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('request-synthetic-schedule','service-order-native-request-one',2,'schedule','fixture-a','Synthetic later schedule','scheduled','fixture-a',$1,'{}',$2)").bind(now+60000).bind(now).execute(&mut *transition).await.unwrap();transition.commit().await.unwrap();
  let response=runtime.router().oneshot(read()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let later:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(later["order"]["version"],2);assert_eq!(later["order"]["status"],"scheduled");assert_eq!(later["receipt"]["status"],"open");assert_eq!(later["event"]["scheduledAt"],serde_json::Value::Null);
  let modified=body.replace("Synthetic problem","Different problem");assert_ne!(send(&runtime.router(),&cookie,"native-request-one",&modified).await.0,StatusCode::OK);
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT request_reject CHECK(command_id<>'request-rollback')").execute(p.pool().raw()).await.unwrap();
  assert_eq!(send(&runtime.router(),&cookie,"request-rollback",body).await.0,StatusCode::INTERNAL_SERVER_ERROR);
  let rollback:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM support_tickets WHERE id='ticket-request-rollback'),(SELECT count(*) FROM support_ticket_events WHERE id='ticket-event-request-rollback-intake'),(SELECT count(*) FROM field_service_orders WHERE id='service-order-request-rollback'),(SELECT count(*) FROM staff_notifications WHERE id='service-order-notice-request-rollback'),(SELECT count(*) FROM command_receipts WHERE command_id='request-rollback'),(SELECT count(*) FROM outbox_events WHERE command_id='request-rollback')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(rollback,(0,0,0,0,0,0));
  sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT request_reject").execute(p.pool().raw()).await.unwrap();
  let stale=body.replace("\"agreementVersion\":2","\"agreementVersion\":1");assert_ne!(send(&runtime.router(),&cookie,"request-stale-version",&stale).await.0,StatusCode::OK);
  sqlx::raw_sql("CREATE FUNCTION request_source_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='request-drift' THEN UPDATE field_service_agreements SET title='unintended' WHERE id='agreement'; END IF; RETURN NEW; END $$; CREATE TRIGGER request_source_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION request_source_drift()").execute(p.pool().raw()).await.unwrap();
  assert_ne!(send(&runtime.router(),&cookie,"request-drift",body).await.0,StatusCode::OK);
  let drift:(String,i64,i64)=sqlx::query_as("SELECT (SELECT title FROM field_service_agreements WHERE id='agreement'),(SELECT count(*) FROM field_service_orders WHERE id='service-order-request-drift'),(SELECT count(*) FROM command_receipts WHERE command_id='request-drift')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(drift,("Synthetic retained agreement".into(),0,0));
  sqlx::raw_sql("DROP TRIGGER request_source_drift ON command_receipts;DROP FUNCTION request_source_drift()").execute(p.pool().raw()).await.unwrap();
  publish_http_fixture_permissions(&p,1,2,serde_json::json!(["records.read","module.field_service.read"]),now).await;
  assert_eq!(runtime.router().oneshot(read()).await.unwrap().status(),StatusCode::OK);
  assert_ne!(send(&runtime.router(),&cookie,"request-forbidden",body).await.0,StatusCode::OK);
  publish_http_fixture_permissions(&p,2,3,serde_json::json!(["records.read","records.write","module.field_service.read"]),now).await;
  for (key,long_id) in [("request-long-ascii","x".repeat(210)),("request-long-cjk","界".repeat(90)),("request-large-physical","x".repeat(150_000))] {
   let mut source_tx=p.pool().raw().begin().await.unwrap();
   sqlx::query("INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||jsonb_build_object('id',$1::text))).* FROM field_service_agreements g WHERE id='agreement'").bind(&long_id).execute(&mut *source_tx).await.unwrap();
   sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES($1,$2,2,'activate','fixture-b','Independent synthetic approval','active',$3)").bind(format!("{key}-review")).bind(&long_id).bind(now-100).execute(&mut *source_tx).await.unwrap();source_tx.commit().await.unwrap();
   let long_body=body.replace("\"agreement\"",&serde_json::to_string(&long_id).unwrap());
   let response=send(&runtime.router(),&cookie,key,&long_body).await;assert_eq!(response.0,StatusCode::OK,"{}",response.1);
   assert_eq!(send(&runtime.router(),&cookie,key,&long_body).await.0,StatusCode::OK);
   let get=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/requests/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["order"]["agreementId"],long_id);
  }
  let mut draining=p.pool().raw().begin().await.unwrap();
  let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *draining).await.unwrap();
  sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *draining).await.unwrap();
  sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'request-service-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(now).execute(&mut *draining).await.unwrap();draining.commit().await.unwrap();
  assert_eq!(runtime.router().oneshot(read()).await.unwrap().status(),StatusCode::OK);
  assert_eq!(send(&runtime.router(),&cookie,"native-request-one",body).await.0,StatusCode::OK);
  assert_ne!(send(&runtime.router(),&cookie,"request-no-new-work",body).await.0,StatusCode::OK);
  runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_request_bounds_agreement_references_before_transfer() {
    account::with_system(|system| async move {
        let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
        let p=setup(system).await;let system=&p.review.system;let now=system.scenario.clock.now().get();
        sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(p.pool().raw()).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let customer="x".repeat(1000);let mut tx=p.pool().raw().begin().await.unwrap();
        sqlx::query("INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||jsonb_build_object('id',$1::text,'code','OVERSIZE'))).* FROM finance_parties p WHERE id='customer'").bind(&customer).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||jsonb_build_object('id','oversized-ref-agreement','customer_id',$1::text))).* FROM field_service_agreements g WHERE id='agreement'").bind(&customer).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES('oversized-ref-review','oversized-ref-agreement',2,'activate','fixture-b','Independent synthetic approval','active',$1)").bind(now-100).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        let mut doc=private_configuration::document(&system.control,&system.holdings).await;doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);doc["fieldServiceRequests"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());let body=r#"{"entity":"request","agreementId":"oversized-ref-agreement","agreementVersion":2,"title":"New visit","issue":"Synthetic problem","site":"Synthetic site","technicianId":"fixture-a","scheduledAt":null}"#;
        let response=send(&runtime.router(),&cookie,"request-oversized-ref",body).await;
        assert_eq!(response.0,StatusCode::SERVICE_UNAVAILABLE,"{}",response.1);
        assert_eq!(response.1["error"]["code"],"FIELD_SERVICE_REQUEST_SOURCE_LIMIT");
        let count:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-request-oversized-ref'),(SELECT count(*) FROM support_tickets WHERE id='ticket-request-oversized-ref'),(SELECT count(*) FROM command_receipts WHERE command_id='request-oversized-ref')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,(0,0,0));
        runtime.close().await;
    }).await;
}
