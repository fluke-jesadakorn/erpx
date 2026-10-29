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
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
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

async fn send_as(
    router: &axum::Router,
    cookie: &str,
    assignment: &str,
    key: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/holdings/acme/field-service")
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie)
        .header("idempotency-key", key)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-revision", "8")
        .header("x-erpx-context-assignment", assignment)
        .header("x-erpx-context-assignment-version", "3")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = router.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
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
async fn private_issue_readback_accepts_two_issues_against_historical_reservation() {
    account::with_system(|system| async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=setup_workspace(system).await;
        p.issue("historical-issue-one").await.unwrap();
        let body=r#"{"action":"issue-part","expectedVersion":4,"reservationId":"reserved-part","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Second genuine issue"}"#;
        p.execute("issue-order","historical-issue-two",body).await.unwrap();
        p.execute("issue-order","historical-issue-two",body).await.unwrap();
        assert_eq!(p.counts("historical-issue-two").await,(1,1,1,1,1));
        let system=&p.review.system;
        let central=account::login(system,"a@example.test",None).await;
        let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
        let mut cfg=private_configuration::document(&system.control,&system.holdings).await;
        cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        cfg["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        cfg["fieldServiceReservations"]=serde_json::json!(["h001"]);
        cfg["fieldServiceIssues"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        for (key,version) in [("historical-issue-one",4),("historical-issue-two",5)] {
            let request=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/issue-order/issues/{key}")).header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();
            let response=runtime.router().oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();
            assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
            let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();
            assert_eq!(data["receipt"]["version"],version);assert_eq!(data["event"]["consumptionId"],format!("operational-part-{key}"));assert_eq!(data["reservation"]["consumedMilli"],2000);assert_eq!(data["order"]["version"],5);
        }
        runtime.close().await;
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
        config["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);config["fieldServiceOrderCancellations"]=serde_json::json!(["h001"]);
        let cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());
        let post=|key:&str, revision:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/post")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision",revision).header("idempotency-key",key).header("content-type","application/json").body(Body::from(r#"{"expectedVersion":1}"#)).unwrap();
        let build=|doc:&serde_json::Value|DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let closed=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_ne!(closed.router().oneshot(post("field-off","7")).await.unwrap().status(),StatusCode::OK);
        closed.close().await;
        config["financeFieldServiceJournalPosts"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(build(&config),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let field=account::child(system,&central,account::ACME).await;let now=system.scenario.clock.now().get();sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&field.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let field_cookie=format!("__Host-erpx-product={}",field.token.expose());let exception=r#"{"id":"issue-order","action":"exception","expectedVersion":4,"reason":"Unable to continue"}"#;let result=send(&runtime.router(),&field_cookie,"cancel-posted-exception",exception).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);let cancel=r#"{"id":"issue-order","action":"cancel","expectedVersion":5,"reason":"Cancel visit"}"#;assert_ne!(send(&runtime.router(),&field_cookie,"cancel-draft-denied",cancel).await.0,StatusCode::OK);assert_eq!(p.counts("cancel-draft-denied").await,(0,0,0,0,0));
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
        let stock_before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();let cancelled=send(&runtime.router(),&field_cookie,"cancel-posted-cost",cancel).await;assert_eq!(cancelled.0,StatusCode::OK,"{}",cancelled.1);assert_eq!(send(&runtime.router(),&field_cookie,"cancel-posted-cost",cancel).await.0,StatusCode::OK);let release:(i64,String,i64,Option<String>)=sqlx::query_as("SELECT r.released_milli,r.release_reason,s.version,o.exception_reason FROM field_service_part_reservations r JOIN commerce_stock s ON s.warehouse_id=r.warehouse_id AND s.product_id=r.product_id JOIN field_service_orders o ON o.id=r.order_id WHERE r.id='reserved-part'").fetch_one(db).await.unwrap();assert_eq!(release,(2000,"Service order cancelled".into(),stock_before+1,Some("Unable to continue".into())));
        let primary:(String,String)=sqlx::query_as("SELECT steps_json::jsonb->'draft'->>'primaryUserId',(SELECT actor_user_id FROM flow_events WHERE command_id='field-post-success') FROM flow_routes WHERE company_id='company' AND kind='journal' AND record_id=$1").bind(&journal).fetch_one(db).await.unwrap();
        assert_eq!(primary,("fixture-b".into(),"fixture-b".into()));
        let read=Request::builder().uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/workspace")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(read).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["data"]["journal"]["status"],"posted");assert_eq!(data["data"]["journal"]["version"],2);
        seed_order(&p,"cancel-reviewer-target","cancel-reviewer-ticket","cancel-reviewer-created",now).await;sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','cancel_reviewer','Cancel reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','cancel_reviewer',1,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\"]','[]','[\"holding\"]','fixture',0)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('cancel-reviewer-b','fixture-b','cancel_reviewer','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let bc=account::login(system,"b@example.test",None).await;let bs=account::child(system,&bc,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','cancel-reviewer-b',3,8,$2,$3)").bind(&bs.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();sqlx::query("UPDATE hr_engagements SET status='ended' WHERE id='field-engagement'").execute(db).await.unwrap();let reviewer_cookie=format!("__Host-erpx-product={}",bs.token.expose());let reviewer_cancel=r#"{"id":"cancel-reviewer-target","action":"cancel","expectedVersion":1,"reason":"Reviewer cancels"}"#;let result=send_as(&runtime.router(),&reviewer_cookie,"cancel-reviewer-b","cancel-eligible-reviewer",reviewer_cancel).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
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
async fn private_request_accepts_bounded_physical_customer_reference() {
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
        assert_eq!(response.0,StatusCode::OK,"{}",response.1);
        let count:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-request-oversized-ref'),(SELECT count(*) FROM support_tickets WHERE id='ticket-request-oversized-ref'),(SELECT count(*) FROM command_receipts WHERE command_id='request-oversized-ref')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,(1,1,1));
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_existing_ticket_create_orders_and_replays() {
    account::with_system(|system|async move{
  let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
  let p=setup(system).await;let system=&p.review.system;let now=system.scenario.clock.now().get();sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(p.pool().raw()).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('source-sale','company','branch','sales','SOURCE-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,version)VALUES('existing-ticket','company','branch','source-sale','customer','Synthetic source','Synthetic original problem','fixture-a','2026-12-31','fixture-a',$1,$1,3)").bind(now).execute(p.pool().raw()).await.unwrap();
  let mut doc=private_configuration::document(&system.control,&system.holdings).await;doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);doc["fieldServiceOrderCreates"]=serde_json::json!(["h001"]);
  let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let body=r#"{"ticketId":"existing-ticket","ticketVersion":3,"agreementId":"agreement","agreementVersion":2,"assetId":null,"issue":"Synthetic order issue","site":"Synthetic site","technicianId":"fixture-a","scheduledAt":null}"#;
  assert_eq!(send_raw(&runtime.router(),&cookie,"request-closed",&body.replace("{", "{\"entity\":\"request\", ")).await.0,StatusCode::NOT_FOUND);
  let reserve=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":1,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Reserve test"}}"#,now+3_600_000);assert_eq!(send_raw(&runtime.router(),&cookie,"reserve-closed",&reserve).await.0,StatusCode::NOT_FOUND);
  let result=send(&runtime.router(),&cookie,"existing-order-one",body).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(result.1["data"]["id"],"service-order-existing-order-one");assert!(result.1["data"].get("ticketId").is_none());assert_eq!(send(&runtime.router(),&cookie,"existing-order-one",body).await.0,StatusCode::OK);
  let count:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-existing-order-one' AND ticket_id='existing-ticket' AND ticket_version=3),(SELECT count(*) FROM field_service_order_events WHERE id='service-order-event-existing-order-one'),(SELECT count(*) FROM staff_notifications WHERE id='service-order-notice-existing-order-one'),(SELECT count(*) FROM command_receipts WHERE command_id='existing-order-one'),(SELECT count(*) FROM audit_events WHERE command_id='existing-order-one'),(SELECT count(*) FROM outbox_events WHERE command_id='existing-order-one')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(count,(1,1,1,1,1,1));let get=||Request::builder().uri("/api/v1/holdings/acme/field-service/orders/existing-order-one").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["order"]["ticketVersion"],3);assert_eq!(value["ticket"]["id"],"existing-ticket");assert_eq!(value["order"]["customerId"],"customer");assert_eq!(value["event"]["actorId"],"fixture-a");assert_eq!(value["event"]["issue"],"Synthetic order issue");
  let stale=body.replace("\"ticketVersion\":3","\"ticketVersion\":2");assert_ne!(send(&runtime.router(),&cookie,"existing-stale",&stale).await.0,StatusCode::OK);
  let altered=body.replace("Synthetic order issue","Changed input");assert_ne!(send(&runtime.router(),&cookie,"existing-order-one",&altered).await.0,StatusCode::OK);
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT existing_order_audit CHECK(command_id<>'existing-rollback')").execute(p.pool().raw()).await.unwrap();assert_eq!(send(&runtime.router(),&cookie,"existing-rollback",body).await.0,StatusCode::INTERNAL_SERVER_ERROR);let rollback:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id='service-order-existing-rollback'),(SELECT count(*) FROM staff_notifications WHERE id='service-order-notice-existing-rollback'),(SELECT count(*) FROM command_receipts WHERE command_id='existing-rollback'),(SELECT count(*) FROM outbox_events WHERE command_id='existing-rollback')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(rollback,(0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT existing_order_audit").execute(p.pool().raw()).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION existing_order_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='existing-drift' THEN UPDATE support_tickets SET title='unintended' WHERE id='existing-ticket'; END IF;RETURN NEW;END $$;CREATE TRIGGER existing_order_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION existing_order_drift()").execute(p.pool().raw()).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"existing-drift",body).await.0,StatusCode::OK);let drift:(String,i64)=sqlx::query_as("SELECT (SELECT title FROM support_tickets WHERE id='existing-ticket'),(SELECT count(*) FROM command_receipts WHERE command_id='existing-drift')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(drift,("Synthetic source".into(),0));sqlx::raw_sql("DROP TRIGGER existing_order_drift ON command_receipts;DROP FUNCTION existing_order_drift()").execute(p.pool().raw()).await.unwrap();
  let router=runtime.router();let (one,two)=tokio::join!(send(&router,&cookie,"existing-concurrent-one",body),send(&router,&cookie,"existing-concurrent-two",body));assert_eq!(one.0,StatusCode::OK,"{}",one.1);assert_eq!(two.0,StatusCode::OK,"{}",two.1);let facts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_orders WHERE id IN ('service-order-existing-concurrent-one','service-order-existing-concurrent-two') AND ticket_id='existing-ticket' AND ticket_version=3),(SELECT version FROM support_tickets WHERE id='existing-ticket'),(SELECT count(*) FROM command_receipts WHERE command_id IN ('existing-concurrent-one','existing-concurrent-two'))").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(facts,(2,3,2));
  let linked=body.replace("existing-ticket","ticket-issue-order").replace("\"ticketVersion\":3","\"ticketVersion\":1");assert_ne!(send(&runtime.router(),&cookie,"existing-linked",&linked).await.0,StatusCode::OK);
  for (key,long_id) in [("existing-long-ascii","x".repeat(210)),("existing-long-cjk","界".repeat(90))] {sqlx::query("INSERT INTO support_tickets SELECT (jsonb_populate_record(NULL::support_tickets,to_jsonb(t)||jsonb_build_object('id',$1::text))).* FROM support_tickets t WHERE id='existing-ticket'").bind(&long_id).execute(p.pool().raw()).await.unwrap();let long_body=body.replace("\"existing-ticket\"",&serde_json::to_string(&long_id).unwrap());let response=send(&runtime.router(),&cookie,key,&long_body).await;assert_eq!(response.0,StatusCode::OK,"{}",response.1);assert_eq!(send(&runtime.router(),&cookie,key,&long_body).await.0,StatusCode::OK);let read=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/orders/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(read).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["order"]["ticketId"],long_id);}
  sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('service-asset','company','branch','SYNTHETIC','Synthetic asset','synthetic','THB','fixture-a',$1,$1)").bind(now).execute(p.pool().raw()).await.unwrap();
  let with_asset=body.replace("\"assetId\":null","\"assetId\":\"service-asset\"");let response=send(&runtime.router(),&cookie,"existing-with-asset",&with_asset).await;assert_eq!(response.0,StatusCode::OK,"{}",response.1);
  let read_asset=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/existing-with-asset").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(read_asset).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["order"]["assetId"],"service-asset");
  for (key,asset_id) in [("existing-asset-long-ascii","a".repeat(210)),("existing-asset-long-cjk","界".repeat(90))] {sqlx::query("INSERT INTO operational_assets SELECT (jsonb_populate_record(NULL::operational_assets,to_jsonb(a)||jsonb_build_object('id',$1::text,'code',$2::text))).* FROM operational_assets a WHERE id='service-asset'").bind(&asset_id).bind(key).execute(p.pool().raw()).await.unwrap();let long_body=body.replace("\"assetId\":null",&format!("\"assetId\":{}",serde_json::to_string(&asset_id).unwrap()));let response=send(&runtime.router(),&cookie,key,&long_body).await;assert_eq!(response.0,StatusCode::OK,"{}",response.1);assert_eq!(send(&runtime.router(),&cookie,key,&long_body).await.0,StatusCode::OK);let read=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/orders/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(read).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["order"]["assetId"],asset_id);}
  publish_http_fixture_permissions(&p,1,2,serde_json::json!(["records.read","module.field_service.read"]),now).await;assert_eq!(runtime.router().oneshot(get()).await.unwrap().status(),StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"existing-forbidden",body).await.0,StatusCode::OK);publish_http_fixture_permissions(&p,2,3,serde_json::json!(["records.read","records.write","module.field_service.read"]),now).await;
  withdraw(&p,"service-parts-assets").await;assert_ne!(send(&runtime.router(),&cookie,"existing-asset-no-purchase",&with_asset).await.0,StatusCode::OK);assert_eq!(send(&runtime.router(),&cookie,"existing-no-asset",body).await.0,StatusCode::OK);
  runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn long_physical_references_cross_create_parts_and_finance() {
    account::with_system(|system| async move {
        let wall=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(wall,std::sync::atomic::Ordering::SeqCst);
        let p=setup(system).await; let system=&p.review.system; let db=p.pool().raw(); let now=system.scenario.clock.now().get();
        sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
        let ticket="t".repeat(210);let agreement="界".repeat(90);let asset="a".repeat(210);
        sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('long-sale','company','branch','sales','LONG-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,version)VALUES($1,'company','branch','long-sale','customer','Long reference','Synthetic issue','fixture-a','2026-12-31','fixture-a',$2,$2,3)").bind(&ticket).bind(now).execute(db).await.unwrap();
        let mut source_tx=db.begin().await.unwrap();sqlx::query("INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||jsonb_build_object('id',$1::text))).* FROM field_service_agreements g WHERE id='agreement'").bind(&agreement).execute(&mut *source_tx).await.unwrap();sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES('long-review',$1,2,'activate','fixture-b','Independent synthetic approval','active',$2)").bind(&agreement).bind(now-100).execute(&mut *source_tx).await.unwrap();source_tx.commit().await.unwrap();
        sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES($1,'company','branch','LONG-ASSET','Synthetic asset','synthetic','THB','fixture-a',$2,$2)").bind(&asset).bind(now).execute(db).await.unwrap();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','long_post_reviewer','Field journal reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','long_post_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='long_post_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('long-post-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('long-post-engagement','long-post-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(db).await.unwrap();
        let mut config=private_configuration::document(&system.control,&system.holdings).await;
        config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["fieldServiceOrderCreates"]=serde_json::json!(["h001"]);config["fieldServiceVisits"]=serde_json::json!(["h001"]);config["fieldServiceVisitCompletions"]=serde_json::json!(["h001"]);config["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);config["fieldServiceReservations"]=serde_json::json!(["h001"]);config["fieldServiceIssues"]=serde_json::json!(["h001"]);config["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["financeJournalPosts"]=serde_json::json!(["h001"]);config["financeFieldServiceJournalPosts"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let create=serde_json::json!({"ticketId":ticket,"ticketVersion":3,"agreementId":agreement,"agreementVersion":2,"assetId":asset,"issue":"Long source issue","site":"Synthetic site","technicianId":"fixture-a","scheduledAt":null}).to_string();
        let result=send(&runtime.router(),&cookie,"long-flow-order",&create).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
        let schedule=serde_json::json!({"id":"service-order-long-flow-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule visit"}).to_string();let result=send(&runtime.router(),&cookie,"long-flow-schedule",&schedule).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"long-flow-schedule",&schedule).await.0,StatusCode::OK);
        let reserve=format!(r#"{{"id":"service-order-long-flow-order","action":"reserve-part","expectedVersion":2,"warehouseId":"warehouse","productId":"product","quantityMilli":2000,"expiresAt":{},"reason":"Long reference reserve"}}"#,now+3_600_000);
        assert_ne!(send(&runtime.router(),"","long-flow-unauthorized",&reserve).await.0,StatusCode::OK);
        assert_ne!(send(&runtime.router(),&cookie,"long-flow-target-mismatch",&reserve.replace("service-order-long-flow-order","missing-order")).await.0,StatusCode::OK);
        sqlx::query("UPDATE field_service_orders SET technician_id=repeat('x',2097153) WHERE id='service-order-long-flow-order'").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"long-flow-oversized",&reserve).await.0,StatusCode::OK);assert_eq!(p.counts("long-flow-oversized").await,(0,0,0,0,0));sqlx::query("UPDATE field_service_orders SET technician_id='fixture-a' WHERE id='service-order-long-flow-order'").execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION long_flow_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='long-flow-drift' THEN UPDATE field_service_agreements SET title='changed' WHERE id=(SELECT agreement_id FROM field_service_orders WHERE id='service-order-long-flow-order'); END IF; RETURN NEW; END $$; CREATE TRIGGER long_flow_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION long_flow_drift()").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"long-flow-drift",&reserve).await.0,StatusCode::OK);assert_eq!(p.counts("long-flow-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER long_flow_drift ON command_receipts;DROP FUNCTION long_flow_drift()").execute(db).await.unwrap();
        let result=send(&runtime.router(),&cookie,"long-flow-reserve",&reserve).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"long-flow-reserve",&reserve).await.0,StatusCode::OK);
        let dispatch=r#"{"id":"service-order-long-flow-order","action":"dispatch","expectedVersion":3,"reason":"Dispatch visit"}"#;let result=send(&runtime.router(),&cookie,"long-flow-dispatch",dispatch).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"long-flow-dispatch",dispatch).await.0,StatusCode::OK);
        let start=r#"{"id":"service-order-long-flow-order","action":"start","expectedVersion":4,"reason":"Start visit"}"#;let result=send(&runtime.router(),&cookie,"long-flow-start",start).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"long-flow-start",start).await.0,StatusCode::OK);
        for key in ["long-flow-schedule","long-flow-dispatch","long-flow-start"] {let read=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/visits/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let r=runtime.router().oneshot(read).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));}
        let issue=r#"{"id":"service-order-long-flow-order","action":"issue-part","expectedVersion":5,"reservationId":"field-part-long-flow-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Long reference issue"}"#;
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT long_flow_rollback CHECK(command_id<>'long-flow-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"long-flow-rollback",issue).await.0,StatusCode::OK);assert_eq!(p.counts("long-flow-rollback").await,(0,0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT long_flow_rollback").execute(db).await.unwrap();
        let result=send(&runtime.router(),&cookie,"long-flow-issue",issue).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"long-flow-issue",issue).await.0,StatusCode::OK);
        let get=Request::builder().uri("/api/v1/holdings/acme/field-service/service-order-long-flow-order/issues/long-flow-issue").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let journal=data["event"]["journalId"].as_str().unwrap();assert_eq!(data["reservation"]["consumedMilli"],1000);
        let complete=r#"{"id":"service-order-long-flow-order","action":"complete","expectedVersion":6,"laborMinutes":0,"evidence":"  Actual visit done  ","customerConfirmation":"  Confirmed by customer  ","reason":" Complete visit "}"#;
        let denied=send(&runtime.router(),&cookie,"complete-draft",complete).await;assert_ne!(denied.0,StatusCode::OK);assert_eq!(p.counts("complete-draft").await,(0,0,0,0,0));
        let finance_cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());let finance_get=Request::builder().uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/workspace")).header("host","acme.example.test").header("cookie",&finance_cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(finance_get).await.unwrap().status(),StatusCode::OK);
        let post=||Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/post")).header("host","acme.example.test").header("cookie",&finance_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").header("idempotency-key","long-flow-post").header("content-type","application/json").body(Body::from(r#"{"expectedVersion":1}"#)).unwrap();let response=runtime.router().oneshot(post()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(runtime.router().oneshot(post()).await.unwrap().status(),StatusCode::OK);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM money_transaction_snapshots WHERE source_id=$1").bind(journal).fetch_one(db).await.unwrap(),1);
        let before_recovery:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT to_jsonb(r) FROM field_service_part_reservations r WHERE id='field-part-long-flow-reserve'),(SELECT to_jsonb(s) FROM commerce_stock s WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT to_jsonb(i) FROM operational_inventory_consumptions i WHERE id='operational-part-long-flow-issue'),(SELECT to_jsonb(j) FROM finance_journals j WHERE id=$1),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.amount_kind) FROM money_transaction_snapshots m WHERE m.source_id=$1))").bind(journal).fetch_one(db).await.unwrap();
        let recovery_exception=r#"{"id":"service-order-long-flow-order","action":"exception","expectedVersion":6,"reason":"Access blocked"}"#;let v=send(&runtime.router(),&cookie,"posted-recovery-exception",recovery_exception).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);let recovery_resume=r#"{"id":"service-order-long-flow-order","action":"resume","expectedVersion":7,"reason":"Access restored"}"#;let v=send(&runtime.router(),&cookie,"posted-recovery-resume",recovery_resume).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
        let after_recovery:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT to_jsonb(r) FROM field_service_part_reservations r WHERE id='field-part-long-flow-reserve'),(SELECT to_jsonb(s) FROM commerce_stock s WHERE warehouse_id='warehouse' AND product_id='product'),(SELECT to_jsonb(i) FROM operational_inventory_consumptions i WHERE id='operational-part-long-flow-issue'),(SELECT to_jsonb(j) FROM finance_journals j WHERE id=$1),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.amount_kind) FROM money_transaction_snapshots m WHERE m.source_id=$1))").bind(journal).fetch_one(db).await.unwrap();assert_eq!(before_recovery,after_recovery);
        let complete_after=complete.replace("\"expectedVersion\":6","\"expectedVersion\":8");
        sqlx::raw_sql("CREATE FUNCTION complete_stock_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='complete-drift' THEN UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'; END IF; RETURN NEW; END $$;CREATE TRIGGER complete_stock_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION complete_stock_drift()").execute(db).await.unwrap();let prior:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"complete-drift",&complete_after).await.0,StatusCode::OK);assert_eq!(p.counts("complete-drift").await,(0,0,0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap(),prior);sqlx::raw_sql("DROP TRIGGER complete_stock_drift ON command_receipts;DROP FUNCTION complete_stock_drift()").execute(db).await.unwrap();
        let stock_before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT completion_rollback CHECK(command_id<>'complete-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"complete-rollback",&complete_after).await.0,StatusCode::OK);assert_eq!(p.counts("complete-rollback").await,(0,0,0,0,0));let unchanged:(i64,i64)=sqlx::query_as("SELECT (SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-long-flow-reserve'),(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product')").fetch_one(db).await.unwrap();assert_eq!(unchanged,(0,stock_before));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT completion_rollback").execute(db).await.unwrap();
        let done=send(&runtime.router(),&cookie,"complete-posted",&complete_after).await;assert_eq!(done.0,StatusCode::OK,"{}",done.1);assert_eq!(send(&runtime.router(),&cookie,"complete-posted",&complete_after).await.0,StatusCode::OK);let released:(i64,String,i64)=sqlx::query_as("SELECT released_milli,release_reason,(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product') FROM field_service_part_reservations WHERE id='field-part-long-flow-reserve'").fetch_one(db).await.unwrap();assert_eq!(released,(1000,"Visit completed".into(),stock_before+1));
        let get=Request::builder().uri("/api/v1/holdings/acme/field-service/visits/complete-posted").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let evidence:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(evidence["event"]["laborMinutes"],0);assert_eq!(evidence["event"]["evidence"],"Actual visit done");assert_eq!(evidence["event"]["customerConfirmation"],"Confirmed by customer");
        let reviewer_order=send(&runtime.router(),&cookie,"long-review-order",&create).await;assert_eq!(reviewer_order.0,StatusCode::OK,"{}",reviewer_order.1);
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','visit_reviewer','Visit reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','visit_reviewer',1,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\"]','[]','[\"holding\"]','fixture',0)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('visit-reviewer-b','fixture-b','visit_reviewer','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let bc=account::login(system,"b@example.test",None).await;let bs=account::child(system,&bc,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','visit-reviewer-b',3,8,$2,$3)").bind(&bs.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();sqlx::query("UPDATE hr_engagements SET status='ended' WHERE id='field-engagement'").execute(db).await.unwrap();let reviewer_cookie=format!("__Host-erpx-product={}",bs.token.expose());let reassign=serde_json::json!({"id":"service-order-long-review-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-b","scheduledAt":now+60000,"reason":"Reassign inactive technician"}).to_string();let result=send_as(&runtime.router(),&reviewer_cookie,"visit-reviewer-b","visit-review-reassign",&reassign).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_visit_authority_expiry_support_and_rollback() {
    account::with_system(|system|async move {
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,std::sync::atomic::Ordering::SeqCst);let p=setup(system).await;let system=&p.review.system;let db=p.pool().raw();sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut config=private_configuration::document(&system.control,&system.holdings).await;config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["fieldServiceReservations"]=serde_json::json!(["h001"]);
  let schedule=serde_json::json!({"id":"reserve-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule visit"}).to_string();
  let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&closed.router(),&cookie,"visit-disabled",&schedule).await.0,StatusCode::NOT_FOUND);let read=Request::builder().uri("/api/v1/holdings/acme/field-service/visits/visit-disabled").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(read).await.unwrap().status(),StatusCode::NOT_FOUND);closed.close().await;
  config["fieldServiceVisits"]=serde_json::json!(["h001"]);let only_visits=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let completion_off=r#"{"id":"issue-order","action":"complete","expectedVersion":3,"laborMinutes":0,"evidence":"Valid evidence","customerConfirmation":"Confirmed","reason":"Finish"}"#;assert_eq!(send(&only_visits.router(),&cookie,"completion-off",completion_off).await.0,StatusCode::NOT_FOUND);only_visits.close().await;config["fieldServiceVisitCompletions"]=serde_json::json!(["h001"]);config["fieldServiceOrderCancellations"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company','fixture-b','local:fixture-b','field-person','active','local',1)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('field-alias-b','fixture-b','custom_field_parts','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let bc=account::login(system,"b@example.test",None).await;let bs=account::child(system,&bc,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','field-alias-b',3,8,$2,$3)").bind(&bs.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let alias_cookie=format!("__Host-erpx-product={}",bs.token.expose());assert!(sqlx::query_scalar::<_,bool>("SELECT iam_same_person('fixture-a','fixture-b','company')").fetch_one(db).await.unwrap());let alias=send_as(&runtime.router(),&alias_cookie,"field-alias-b","visit-person-alias",&schedule).await;assert_eq!(alias.0,StatusCode::FORBIDDEN,"{}",alias.1);assert_eq!(alias.1["error"]["code"],"FIELD_SERVICE_OWNER_REQUIRED");
  assert_ne!(send(&runtime.router(),"","visit-unauthenticated",&schedule).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"visit-stale",&schedule.replace("\"expectedVersion\":1","\"expectedVersion\":2")).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"visit-ineligible",&schedule.replace("fixture-a","fixture-b")).await.0,StatusCode::OK);
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT visit_rollback CHECK(command_id<>'visit-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"visit-rollback",&schedule).await.0,StatusCode::OK);assert_eq!(p.counts("visit-rollback").await,(0,0,0,0,0));let unchanged:(i64,i64)=sqlx::query_as("SELECT o.version,t.version FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(unchanged,(1,1));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT visit_rollback").execute(db).await.unwrap();
  for (key,size,ok) in [("visit-large-readable",150_000usize,true)] {let physical="z".repeat(size);seed_order(&p,&physical,&format!("{key}-ticket"),&format!("{key}-event"),now).await;let body=serde_json::json!({"id":physical,"action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":1,"reason":"Bounded visit"}).to_string();let response=send(&runtime.router(),&cookie,key,&body).await;if ok {assert_eq!(response.0,StatusCode::OK,"{}",response.1);let get=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/visits/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(get).await.unwrap().status(),StatusCode::OK);}else{assert_ne!(response.0,StatusCode::OK);assert_eq!(p.counts(key).await,(0,0,0,0,0));}}
  let result=send(&runtime.router(),&cookie,"visit-schedule",&schedule).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"visit-schedule",&schedule).await.0,StatusCode::OK);let projection:(i64,i64)=sqlx::query_as("SELECT o.version,t.service_order_version FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(projection,(2,2));
  let reserve=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":2,"warehouseId":"warehouse","productId":"product","quantityMilli":1000,"expiresAt":{},"reason":"Reserve visit"}}"#,now+60_000);let r=send(&runtime.router(),&cookie,"visit-reserve",&reserve).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);
  let second_reserve=format!(r#"{{"id":"reserve-order","action":"reserve-part","expectedVersion":3,"warehouseId":"warehouse","productId":"product","quantityMilli":500,"expiresAt":{},"reason":"Second reservation"}}"#,now+120_000);let r=send(&runtime.router(),&cookie,"visit-reserve-two",&second_reserve).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);
  let dispatch=r#"{"id":"reserve-order","action":"dispatch","expectedVersion":4,"reason":"Dispatch visit"}"#;system.scenario.clock.0.store(now+60_001,std::sync::atomic::Ordering::SeqCst);assert_ne!(send(&runtime.router(),&cookie,"visit-expired",dispatch).await.0,StatusCode::OK);assert_eq!(p.counts("visit-expired").await,(0,0,0,0,0));system.scenario.clock.0.store(now,std::sync::atomic::Ordering::SeqCst);
  let result=send(&runtime.router(),&cookie,"visit-dispatch",dispatch).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);let start=r#"{"id":"reserve-order","action":"start","expectedVersion":5,"reason":"Start visit"}"#;let result=send(&runtime.router(),&cookie,"visit-start",start).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);let projection:(i64,i64,i64)=sqlx::query_as("SELECT o.version,t.service_order_version,(SELECT count(*) FROM support_ticket_events WHERE ticket_id=t.id AND action IN ('service-schedule','service-dispatch','service-start')) FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(projection,(6,6,3));system.scenario.clock.0.store(now+60_001,std::sync::atomic::Ordering::SeqCst);let before_stock:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();let complete=r#"{"id":"reserve-order","action":"complete","expectedVersion":6,"laborMinutes":15,"evidence":"Synthetic \ud800","customerConfirmation":"Customer confirmed","reason":"Finish visit"}"#;let denied=send_as(&runtime.router(),&alias_cookie,"field-alias-b","completion-alias",complete).await;assert_eq!(denied.0,StatusCode::FORBIDDEN,"{}",denied.1);assert_eq!(denied.1["error"]["code"],"FIELD_SERVICE_OWNER_REQUIRED");assert_ne!(send(&runtime.router(),&cookie,"completion-stale",&complete.replace("\"expectedVersion\":6","\"expectedVersion\":5")).await.0,StatusCode::OK);let result=send(&runtime.router(),&cookie,"visit-no-parts-complete",complete).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"visit-no-parts-complete",complete).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"visit-no-parts-complete",&complete.replace("Finish visit","Changed reason")).await.0,StatusCode::OK);let get=Request::builder().uri("/api/v1/holdings/acme/field-service/visits/visit-no-parts-complete").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["event"]["evidence"],"Synthetic �");let released:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-visit-reserve'),(SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-visit-reserve-two'),(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product')").fetch_one(db).await.unwrap();assert_eq!(released,(1000,500,before_stock+2));let cancel_completed=r#"{"id":"reserve-order","action":"cancel","expectedVersion":7,"reason":"Too late"}"#;assert_ne!(send(&runtime.router(),&cookie,"cancel-completed",cancel_completed).await.0,StatusCode::OK);runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_visit_exception_resume_is_atomic_and_isolated() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,std::sync::atomic::Ordering::SeqCst);let p=setup(system).await;let system=&p.review.system;let db=p.pool().raw();sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut doc=private_configuration::document(&system.control,&system.holdings).await;doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);doc["fieldServiceVisits"]=serde_json::json!(["h001"]);
  let exception=r#"{"id":"reserve-order","action":"exception","expectedVersion":2,"reason":"  Site access blocked  "}"#;let dispatch=r#"{"id":"reserve-order","action":"dispatch","expectedVersion":1,"reason":"Dispatch visit"}"#;
  let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&closed.router(),&cookie,"recovery-off",exception).await.0,StatusCode::NOT_FOUND);closed.close().await;
  doc["fieldServiceVisits"]=serde_json::json!([]);doc["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);let recovery_only=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&recovery_only.router(),&cookie,"recovery-no-dispatch",dispatch).await.0,StatusCode::NOT_FOUND);let completion=r#"{"id":"issue-order","action":"complete","expectedVersion":3,"laborMinutes":0,"evidence":"Valid evidence","customerConfirmation":"Confirmed","reason":"Finish"}"#;assert_eq!(send(&recovery_only.router(),&cookie,"recovery-no-complete",completion).await.0,StatusCode::NOT_FOUND);recovery_only.close().await;
  doc["fieldServiceVisits"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  assert_ne!(send(&runtime.router(),"","recovery-unauthenticated",exception).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"recovery-wrong-state",exception).await.0,StatusCode::OK);
  let result=send(&runtime.router(),&cookie,"recovery-dispatch",dispatch).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);
  sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-b','local:fixture-b','field-person','active','local',1)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('recovery-alias-b','fixture-b','custom_field_parts','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let bc=account::login(system,"b@example.test",None).await;let bs=account::child(system,&bc,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','recovery-alias-b',3,8,$2,$3)").bind(&bs.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let alias_cookie=format!("__Host-erpx-product={}",bs.token.expose());let denied=send_as(&runtime.router(),&alias_cookie,"recovery-alias-b","recovery-alias",exception).await;assert_eq!(denied.0,StatusCode::FORBIDDEN,"{}",denied.1);assert_eq!(denied.1["error"]["code"],"FIELD_SERVICE_OWNER_REQUIRED");assert_ne!(send(&runtime.router(),&cookie,"recovery-stale",&exception.replace("\"expectedVersion\":2","\"expectedVersion\":1")).await.0,StatusCode::OK);
  let snapshot:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM field_service_part_reservations r),(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.warehouse_id,s.product_id) FROM commerce_stock s),(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM operational_inventory_consumptions i),(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM finance_journals j),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_id) FROM money_transaction_snapshots m))").fetch_one(db).await.unwrap();
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT recovery_rollback CHECK(command_id<>'recovery-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"recovery-rollback",exception).await.0,StatusCode::OK);assert_eq!(p.counts("recovery-rollback").await,(0,0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT recovery_rollback").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION recovery_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='recovery-drift' THEN UPDATE field_service_agreements SET title='drift' WHERE id='agreement'; END IF; RETURN NEW; END $$;CREATE TRIGGER recovery_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION recovery_drift()").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"recovery-drift",exception).await.0,StatusCode::OK);assert_eq!(p.counts("recovery-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER recovery_drift ON command_receipts;DROP FUNCTION recovery_drift()").execute(db).await.unwrap();
  let result=send(&runtime.router(),&cookie,"recovery-exception",exception).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(result.1["data"]["status"],"exception");assert_eq!(send(&runtime.router(),&cookie,"recovery-exception",exception).await.0,StatusCode::OK);let value:(String,String,i64)=sqlx::query_as("SELECT status,exception_reason,version FROM field_service_orders WHERE id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(value,("exception".into(),"Site access blocked".into(),3));
  let resume=r#"{"id":"reserve-order","action":"resume","expectedVersion":3,"reason":"Access restored"}"#;let result=send(&runtime.router(),&cookie,"recovery-resume",resume).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(send(&runtime.router(),&cookie,"recovery-resume",resume).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"recovery-resume",&resume.replace("Access restored","Changed reason")).await.0,StatusCode::OK);let value:(String,Option<String>,i64)=sqlx::query_as("SELECT status,exception_reason,version FROM field_service_orders WHERE id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(value,("in_visit".into(),None,4));
  let exception_again=r#"{"id":"reserve-order","action":"exception","expectedVersion":4,"reason":"Safety issue"}"#;assert_eq!(send(&runtime.router(),&cookie,"recovery-exception-again",exception_again).await.0,StatusCode::OK);let resume_again=r#"{"id":"reserve-order","action":"resume","expectedVersion":5,"reason":"Safety restored"}"#;assert_eq!(send(&runtime.router(),&cookie,"recovery-resume-again",resume_again).await.0,StatusCode::OK);assert_eq!(send(&runtime.router(),&cookie,"recovery-exception",exception).await.0,StatusCode::OK);
  for (key,action,status,reason) in [("recovery-exception","exception","exception","Site access blocked"),("recovery-resume","resume","in_visit","Access restored")] {let get=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/visits/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get).await.unwrap();let st=r.status();let bytes=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(st,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["event"]["action"],action);assert_eq!(data["event"]["status"],status);assert_eq!(data["event"]["reason"],reason);assert_eq!(data["order"]["version"],6);}
  let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM field_service_part_reservations r),(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.warehouse_id,s.product_id) FROM commerce_stock s),(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM operational_inventory_consumptions i),(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM finance_journals j),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_id) FROM money_transaction_snapshots m))").fetch_one(db).await.unwrap();assert_eq!(snapshot,after);let projection:(i64,i64,i64)=sqlx::query_as("SELECT o.version,t.service_order_version,(SELECT count(*) FROM support_ticket_events WHERE ticket_id=t.id AND action IN ('service-exception','service-resume')) FROM field_service_orders o JOIN support_tickets t ON t.id=o.ticket_id WHERE o.id='reserve-order'").fetch_one(db).await.unwrap();assert_eq!(projection,(6,6,4));runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_cancel_states_reservations_authority_and_rollback() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,std::sync::atomic::Ordering::SeqCst);let p=setup(system).await;let system=&p.review.system;let db=p.pool().raw();sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut doc=private_configuration::document(&system.control,&system.holdings).await;doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);doc["fieldServiceVisits"]=serde_json::json!(["h001"]);doc["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);doc["fieldServiceReservations"]=serde_json::json!(["h001"]);
  let cancel=r#"{"id":"reserve-order","action":"cancel","expectedVersion":3,"reason":"  Customer cancelled  "}"#;let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&closed.router(),&cookie,"cancel-off",cancel).await.0,StatusCode::NOT_FOUND);closed.close().await;
  doc["fieldServiceVisits"]=serde_json::json!([]);doc["fieldServiceVisitRecoveries"]=serde_json::json!([]);doc["fieldServiceOrderCancellations"]=serde_json::json!(["h001"]);let only=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&only.router(),&cookie,"cancel-no-visit",r#"{"id":"reserve-order","action":"dispatch","expectedVersion":1,"reason":"Dispatch"}"#).await.0,StatusCode::NOT_FOUND);assert_eq!(send(&only.router(),&cookie,"cancel-no-recovery",r#"{"id":"issue-order","action":"exception","expectedVersion":3,"reason":"Blocked"}"#).await.0,StatusCode::NOT_FOUND);assert_eq!(send(&only.router(),&cookie,"cancel-no-completion",r#"{"id":"issue-order","action":"complete","expectedVersion":3,"laborMinutes":0,"evidence":"Evidence","customerConfirmation":"Confirmed","reason":"Finish"}"#).await.0,StatusCode::NOT_FOUND);only.close().await;
  doc["fieldServiceVisits"]=serde_json::json!(["h001"]);doc["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let reserve=|key:&str,version:i64,expiry:i64,qty:i64|{let body=serde_json::json!({"id":"reserve-order","action":"reserve-part","expectedVersion":version,"warehouseId":"warehouse","productId":"product","quantityMilli":qty,"expiresAt":expiry,"reason":"Cancel reservation"}).to_string();(key.to_owned(),body)};for (key,body) in [reserve("cancel-reserve-one",1,now+60_000,1000),reserve("cancel-reserve-two",2,now+120_000,500)] {let r=send(&runtime.router(),&cookie,&key,&body).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);}
  assert_ne!(send(&runtime.router(),"","cancel-unauthenticated",cancel).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"cancel-stale",&cancel.replace("\"expectedVersion\":3","\"expectedVersion\":2")).await.0,StatusCode::OK);let in_visit=r#"{"id":"issue-order","action":"cancel","expectedVersion":3,"reason":"Not allowed"}"#;assert_ne!(send(&runtime.router(),&cookie,"cancel-in-visit",in_visit).await.0,StatusCode::OK);
  sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-b','local:fixture-b','field-person','active','local',1)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('cancel-alias-b','fixture-b','custom_field_parts','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let bc=account::login(system,"b@example.test",None).await;let bs=account::child(system,&bc,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','cancel-alias-b',3,8,$2,$3)").bind(&bs.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let alias_cookie=format!("__Host-erpx-product={}",bs.token.expose());let denied=send_as(&runtime.router(),&alias_cookie,"cancel-alias-b","cancel-alias",cancel).await;assert_eq!(denied.0,StatusCode::FORBIDDEN,"{}",denied.1);assert_eq!(denied.1["error"]["code"],"FIELD_SERVICE_OWNER_REQUIRED");
  system.scenario.clock.0.store(now+60_001,std::sync::atomic::Ordering::SeqCst);let stock_before:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap();sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT cancel_rollback CHECK(command_id<>'cancel-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"cancel-rollback",cancel).await.0,StatusCode::OK);assert_eq!(p.counts("cancel-rollback").await,(0,0,0,0,0));let unchanged:(i64,i64)=sqlx::query_as("SELECT (SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-cancel-reserve-one'),(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product')").fetch_one(db).await.unwrap();assert_eq!(unchanged,(0,stock_before));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT cancel_rollback").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION cancel_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='cancel-drift' THEN UPDATE commerce_stock SET version=version+1 WHERE warehouse_id='warehouse' AND product_id='product'; END IF; RETURN NEW; END $$;CREATE TRIGGER cancel_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION cancel_drift()").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"cancel-drift",cancel).await.0,StatusCode::OK);assert_eq!(p.counts("cancel-drift").await,(0,0,0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product'").fetch_one(db).await.unwrap(),stock_before);sqlx::raw_sql("DROP TRIGGER cancel_drift ON command_receipts;DROP FUNCTION cancel_drift()").execute(db).await.unwrap();
  let done=send(&runtime.router(),&cookie,"cancel-scheduled",cancel).await;assert_eq!(done.0,StatusCode::OK,"{}",done.1);assert_eq!(done.1["data"]["status"],"cancelled");assert_eq!(send(&runtime.router(),&cookie,"cancel-scheduled",cancel).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"cancel-scheduled",&cancel.replace("Customer cancelled","Changed reason")).await.0,StatusCode::OK);let released:(i64,i64,String,String,i64)=sqlx::query_as("SELECT (SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-cancel-reserve-one'),(SELECT released_milli FROM field_service_part_reservations WHERE id='field-part-cancel-reserve-two'),(SELECT release_reason FROM field_service_part_reservations WHERE id='field-part-cancel-reserve-one'),(SELECT status FROM support_tickets WHERE id='ticket-reserve-order'),(SELECT version FROM commerce_stock WHERE warehouse_id='warehouse' AND product_id='product')").fetch_one(db).await.unwrap();assert_eq!(released,(1000,500,"Service order cancelled".into(),"cancelled".into(),stock_before+2));let get=Request::builder().uri("/api/v1/holdings/acme/field-service/visits/cancel-scheduled").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get).await.unwrap();let st=r.status();let bytes=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(st,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["event"]["action"],"cancel");assert_eq!(data["event"]["reason"],"Customer cancelled");
  for suffix in ["open","dispatch","exception"]{seed_order(&p,&format!("cancel-{suffix}"),&format!("cancel-ticket-{suffix}"),&format!("cancel-create-{suffix}"),now).await;}
  let mut tx=db.begin().await.unwrap();sqlx::query("UPDATE field_service_orders SET status='open',version=2 WHERE id='cancel-open'").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,scheduled_at,details_json,created_at) VALUES('cancel-open-history','cancel-open',2,'schedule','fixture-a','Synthetic history','open','fixture-a',$1+60000,'{}',$1)").bind(now).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
  let open=r#"{"id":"cancel-open","action":"cancel","expectedVersion":2,"reason":"Cancel open"}"#;assert_eq!(send(&runtime.router(),&cookie,"cancel-open-action",open).await.0,StatusCode::OK);
  for id in ["cancel-dispatch","cancel-exception"]{let body=serde_json::json!({"id":id,"action":"dispatch","expectedVersion":1,"reason":"Dispatch"}).to_string();assert_eq!(send(&runtime.router(),&cookie,&format!("{id}-dispatch"),&body).await.0,StatusCode::OK);}
  let dispatched=r#"{"id":"cancel-dispatch","action":"cancel","expectedVersion":2,"reason":"Cancel dispatch"}"#;assert_eq!(send(&runtime.router(),&cookie,"cancel-dispatch-action",dispatched).await.0,StatusCode::OK);
  let exception=r#"{"id":"cancel-exception","action":"exception","expectedVersion":2,"reason":"Site blocked"}"#;assert_eq!(send(&runtime.router(),&cookie,"cancel-exception-action",exception).await.0,StatusCode::OK);let cancel_exception=r#"{"id":"cancel-exception","action":"cancel","expectedVersion":3,"reason":"Cancel blocked"}"#;let r=send(&runtime.router(),&cookie,"cancel-exception-final",cancel_exception).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);let reason:Option<String>=sqlx::query_scalar("SELECT exception_reason FROM field_service_orders WHERE id='cancel-exception'").fetch_one(db).await.unwrap();assert_eq!(reason.as_deref(),Some("Site blocked"));runtime.close().await;
 }).await;
}

#[cfg(feature = "private-development-api")]
#[tokio::test]
async fn private_agreement_review_full_transaction_and_readback() {
    account::with_system(|system| async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=setup(system).await;
        setup_agreement_review_facts(&p,now).await;
        let system=&p.review.system; let db=p.pool().raw();
        let central=account::login(system,"b@example.test",None).await;
        let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let mut doc=private_configuration::document(&system.control,&system.holdings).await;
        doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        doc["fieldServiceReservations"]=serde_json::json!(["h001"]);
        let activate=r#"{"entity":"agreement","id":"review-agreement","action":"activate","expectedVersion":1,"reason":"  Approved  "}"#;
        let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_eq!(send_as(&closed.router(),&cookie,"agreement-reviewer-b","review-disabled",activate).await.0,StatusCode::NOT_FOUND);
        closed.close().await;
        doc["fieldServiceAgreementReviews"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let post=|key:&str,body:&str| { let router=runtime.router(); let cookie=cookie.clone(); let key=key.to_owned(); let body=body.to_owned(); async move {send_as(&router,&cookie,"agreement-reviewer-b",&key,&body).await} };
        assert_ne!(send_as(&runtime.router(),"","agreement-reviewer-b","review-no-session",activate).await.0,StatusCode::OK);
        sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='agreement-reviewer-b'").bind(now-1).execute(db).await.unwrap();
        assert_ne!(post("review-expired",activate).await.0,StatusCode::OK);
        sqlx::query("UPDATE assignments SET valid_until=$1 WHERE id='agreement-reviewer-b'").bind(now+3_600_000).execute(db).await.unwrap();
        assert_eq!(post("review-stale",&activate.replace("\"expectedVersion\":1","\"expectedVersion\":2")).await.0,StatusCode::CONFLICT);
        assert_eq!(post("review-blocked",r#"{"entity":"agreement","id":"agreement","action":"retire","expectedVersion":2,"reason":"Close Agreement"}"#).await.0,StatusCode::CONFLICT);
        sqlx::query("UPDATE iam_members SET person_id='field-person' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();
        assert_eq!(post("review-alias",activate).await.0,StatusCode::FORBIDDEN);
        sqlx::query("UPDATE iam_members SET person_id='agreement-review-person' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION review_source_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='review-drift' THEN UPDATE field_service_agreements SET title='drift' WHERE id='review-agreement'; END IF; RETURN NEW; END $$;CREATE TRIGGER review_source_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION review_source_drift()").execute(db).await.unwrap();
        assert_ne!(post("review-drift",activate).await.0,StatusCode::OK);assert_eq!(p.counts("review-drift").await,(0,0,0,0,0));
        sqlx::raw_sql("DROP TRIGGER review_source_drift ON command_receipts;DROP FUNCTION review_source_drift()").execute(db).await.unwrap();
        sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT review_rollback CHECK(command_id<>'review-rollback')").execute(db).await.unwrap();
        assert_ne!(post("review-rollback",activate).await.0,StatusCode::OK);assert_eq!(p.counts("review-rollback").await,(0,0,0,0,0));
        sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT review_rollback").execute(db).await.unwrap();
        for (key,id) in [("review-long-ascii","x".repeat(210)),("review-long-cjk","界".repeat(90))] {
            sqlx::query("INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||jsonb_build_object('id',$1::text))).* FROM field_service_agreements g WHERE id='review-reject-agreement'").bind(&id).execute(db).await.unwrap();
            sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at)VALUES($1,$2,1,'create','fixture-a','Agreement drafted for independent review','draft',$3)").bind(format!("historical-{key}")).bind(&id).bind(now).execute(db).await.unwrap();
            sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES('company','field_service_agreement',$1,'visit_rate','2026-01-01','THB',2,10000,5000,'rate',false,$2)").bind(&id).bind(now).execute(db).await.unwrap();
            let body=serde_json::json!({"entity":"agreement","id":id,"action":"activate","expectedVersion":1,"reason":"Long original reference"}).to_string();
            let response=post(key,&body).await;assert_eq!(response.0,StatusCode::OK,"{}",response.1);assert_eq!(post(key,&body).await.0,StatusCode::OK);
            let request=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/agreements/reviews/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(request).await.unwrap().status(),StatusCode::OK);
        }
        // Conditional Asset purchase follows the same purpose as the Agreement action.
        sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('review-asset','company','branch','REVIEW-ASSET','Synthetic asset','synthetic','THB','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO field_service_agreements SELECT (jsonb_populate_record(NULL::field_service_agreements,to_jsonb(g)||jsonb_build_object('id','review-asset-agreement','asset_id','review-asset'))).* FROM field_service_agreements g WHERE id='review-reject-agreement'").execute(db).await.unwrap();
        sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES('review-asset-create','review-asset-agreement',1,'create','fixture-a','Agreement drafted for independent review','draft',$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES('company','field_service_agreement','review-asset-agreement','visit_rate','2026-01-01','THB',2,10000,5000,'rate',false,$1)").bind(now).execute(db).await.unwrap();
        let mut asset_drain=db.begin().await.unwrap();
        let before_asset:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service-parts-assets'").fetch_one(&mut *asset_drain).await.unwrap();
        sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service-parts-assets'").execute(&mut *asset_drain).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'review-asset-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service-parts-assets'").bind(before_asset).bind(now).execute(&mut *asset_drain).await.unwrap();asset_drain.commit().await.unwrap();
        let asset_activate=activate.replace("review-agreement","review-asset-agreement");assert_eq!(post("review-asset-activate",&asset_activate).await.0,StatusCode::FORBIDDEN);
        let asset_reject=r#"{"entity":"agreement","id":"review-asset-agreement","action":"reject","expectedVersion":1,"reason":"Asset review rejected"}"#;
        assert_eq!(post("review-asset-reject",asset_reject).await.0,StatusCode::OK);
        let result=post("review-activate",activate).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(result.1["data"]["status"],"active");
        assert_eq!(post("review-activate",activate).await.0,StatusCode::OK);
        assert_ne!(post("review-activate",&activate.replace("Approved","Changed")).await.0,StatusCode::OK);
        // Draining permits retained settlement and History replay, but cannot activate new work.
        let mut draining=db.begin().await.unwrap();
        let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *draining).await.unwrap();
        sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *draining).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'review-service-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(now).execute(&mut *draining).await.unwrap();
        draining.commit().await.unwrap();
        let denied_activation=activate.replace("review-agreement","review-reject-agreement");
        assert_eq!(post("review-draining-activation",&denied_activation).await.0,StatusCode::FORBIDDEN);
        assert_eq!(post("review-activate",activate).await.0,StatusCode::OK);
        let reject=r#"{"entity":"agreement","id":"review-reject-agreement","action":"reject","expectedVersion":1,"reason":"Not ready"}"#;
        assert_eq!(post("review-reject",reject).await.0,StatusCode::OK);assert_eq!(post("review-reject",reject).await.0,StatusCode::OK);
        let retire=r#"{"entity":"agreement","id":"review-agreement","action":"retire","expectedVersion":2,"reason":"Contract ended"}"#;
        // An order that reopens inside the owner unit must roll back retirement.
        sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,source_kind) VALUES('review-drift-ticket','company','branch','Synthetic cancelled order','Historical fixture','fixture-a','2027-01-01','fixture-a',$1,$1,'general')").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,created_by,version,created_at,updated_at) VALUES('review-drift-order','company','branch','review-drift-ticket',1,'review-agreement',2,'Historical issue','Historical site','fixture-a','cancelled','fixture-a',1,$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION agreement_late_order_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='review-retire-drift' THEN UPDATE field_service_orders SET status='open' WHERE id='review-drift-order'; END IF; RETURN NEW; END $$;CREATE TRIGGER agreement_late_order_drift AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION agreement_late_order_drift()").execute(db).await.unwrap();
        assert_ne!(post("review-retire-drift",retire).await.0,StatusCode::OK);assert_eq!(p.counts("review-retire-drift").await,(0,0,0,0,0));
        let stable:(String,String)=sqlx::query_as("SELECT (SELECT status FROM field_service_agreements WHERE id='review-agreement'),(SELECT status FROM field_service_orders WHERE id='review-drift-order')").fetch_one(db).await.unwrap();assert_eq!(stable,("active".into(),"cancelled".into()));
        sqlx::raw_sql("DROP TRIGGER agreement_late_order_drift ON audit_events;DROP FUNCTION agreement_late_order_drift()").execute(db).await.unwrap();
        let done=post("review-retire",retire).await;assert_eq!(done.0,StatusCode::OK,"{}",done.1);assert_eq!(post("review-retire",retire).await.0,StatusCode::OK);
        for (key,action,status) in [("review-activate","activate","active"),("review-reject","reject","rejected"),("review-retire","retire","retired")] {
            let req=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/agreements/reviews/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
            let response=runtime.router().oneshot(req).await.unwrap();let code=response.status();let body=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(code,StatusCode::OK,"{}",String::from_utf8_lossy(&body));let value:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(value["event"]["action"],action);assert_eq!(value["receipt"]["status"],status);assert_eq!(value["event"]["actorId"],"fixture-b");
        }
        let rows:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_agreement_events WHERE id IN ('service-agreement-event-review-activate','service-agreement-event-review-reject','service-agreement-event-review-retire')),(SELECT count(*) FROM command_receipts WHERE command_id IN ('review-activate','review-reject','review-retire')),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='field_service_agreement' AND source_id IN ('review-agreement','review-reject-agreement'))").fetch_one(db).await.unwrap();assert_eq!(rows,(3,3,2));
        runtime.close().await;
    }).await;
}

#[cfg(feature = "private-development-api")]
#[tokio::test]
async fn private_agreement_create_money_replay_and_rollback() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
  system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup(system).await;setup_agreement_review_facts(&p,now).await;let system=&p.review.system;let db=p.pool().raw();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let reviewer=account::login(system,"b@example.test",None).await;let reviewer_child=account::child(system,&reviewer,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&reviewer_child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer_cookie=format!("__Host-erpx-product={}",reviewer_child.token.expose());
  let mut doc=private_configuration::document(&system.control,&system.holdings).await;doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);doc["fieldServiceReservations"]=serde_json::json!(["h001"]);
  let body=r#"{"entity":"agreement","branchId":"branch","customerId":"customer","title":"  Native agreement  ","startDate":"2026-01-01","endDate":"2027-12-31","currency":"thb","visitRateMinor":10001,"warrantyDiscountBps":500}"#;
  let off=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(send(&off.router(),&cookie,"create-off",body).await.0,StatusCode::NOT_FOUND);off.close().await;
  doc["fieldServiceAgreementCreates"]=serde_json::json!(["h001"]);doc["fieldServiceAgreementReviews"]=serde_json::json!(["h001"]);
  let advancing=Arc::new(WriteClock(std::sync::atomic::AtomicI64::new(now)));let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),advancing,Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let post=|key:&str,body:&str|{let router=runtime.router();let cookie=cookie.clone();let key=key.to_owned();let body=body.to_owned();async move{send(&router,&cookie,&key,&body).await}};
  assert_ne!(post("create-invalid-date",&body.replace("2027-12-31","2025-12-31")).await.0,StatusCode::OK);
  assert_ne!(post("create-missing-branch",&body.replace("\"branch\"","\"missing\"")).await.0,StatusCode::OK);
  assert_ne!(post("create-missing-customer",&body.replace("\"customer\"","\"missing\"")).await.0,StatusCode::OK);
  sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('eur-customer','company','customer','EUR','Synthetic EUR','EUR',0,0),('jpy-customer','company','customer','JPY','Synthetic JPY','JPY',0,0)").execute(db).await.unwrap();
  assert_ne!(post("create-missing-fx",&body.replace("\"customer\"","\"eur-customer\"").replace("\"thb\"","\"eur\"")).await.0,StatusCode::OK);
  let money_first=body.replace("\"customer\"","\"missing\"").replace("\"thb\"","\"eur\"");let denial=post("create-money-first",&money_first).await;assert_eq!(denial.1["error"]["code"],"MANUAL_RATE_REQUIRED");
  // The original Money path rejects a fixed-scale mismatch before reading an oversized rate set.
  sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) SELECT 'jpy-over-'||n,'company','JPY','2026-01-01',2,'fixture-a',n FROM generate_series(1,20001) n").execute(db).await.unwrap();
  let scale=post("create-scale",&body.replace("\"customer\"","\"jpy-customer\"").replace("\"thb\"","\"jpy\"")).await;assert_eq!(scale.1["error"]["code"],"CURRENCY_SCALE_MISMATCH");
  assert_ne!(send(&runtime.router(),"","create-no-session",body).await.0,StatusCode::OK);
  assert_ne!(post("create-bad-warranty",&body.replace(r#""warrantyDiscountBps":500"#,r#""warrantyDiscountBps":10001"#)).await.0,StatusCode::OK);
  sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('create-asset','company','branch','CREATE-ASSET','Synthetic asset','synthetic','THB','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
  let asset_body=body.replace(r#""title":"#, r#""assetId":"create-asset","title":"#);
  let asset_result=post("create-with-asset",&asset_body).await;assert_eq!(asset_result.0,StatusCode::OK,"{}",asset_result.1);
  sqlx::raw_sql("CREATE FUNCTION agreement_customer_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='create-customer-drift' THEN UPDATE finance_parties SET status='inactive' WHERE id='customer'; END IF; RETURN NEW; END $$;CREATE TRIGGER agreement_customer_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION agreement_customer_drift()").execute(db).await.unwrap();
  assert_ne!(post("create-customer-drift",body).await.0,StatusCode::OK);assert_eq!(p.counts("create-customer-drift").await,(0,0,0,0,0));
  sqlx::raw_sql("DROP TRIGGER agreement_customer_drift ON command_receipts;DROP FUNCTION agreement_customer_drift()").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION agreement_money_tamper() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_id='service-agreement-create-money-tamper' THEN NEW.captured_at=NEW.captured_at+1; END IF; RETURN NEW; END $$;CREATE TRIGGER agreement_money_tamper BEFORE INSERT ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION agreement_money_tamper()").execute(db).await.unwrap();assert_ne!(post("create-money-tamper",body).await.0,StatusCode::OK);assert_eq!(p.counts("create-money-tamper").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER agreement_money_tamper ON money_transaction_snapshots;DROP FUNCTION agreement_money_tamper()").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION agreement_quote_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='create-quote-drift' THEN INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('drift-rate','company','THB','2026-01-01',3,'fixture-a',2); END IF; RETURN NEW; END $$;CREATE TRIGGER agreement_quote_drift AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION agreement_quote_drift()").execute(db).await.unwrap();assert_ne!(post("create-quote-drift",body).await.0,StatusCode::OK);assert_eq!(p.counts("create-quote-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER agreement_quote_drift ON audit_events;DROP FUNCTION agreement_quote_drift()").execute(db).await.unwrap();
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT create_agreement_rollback CHECK(command_id<>'create-rollback')").execute(db).await.unwrap();assert_ne!(post("create-rollback",body).await.0,StatusCode::OK);assert_eq!(p.counts("create-rollback").await,(0,0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT create_agreement_rollback").execute(db).await.unwrap();
  for key in ["create-money-tamper","create-quote-drift","create-rollback","create-customer-drift"] {
      let absent:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_agreements WHERE id=$1),(SELECT count(*) FROM field_service_agreement_events WHERE id=$2),(SELECT count(*) FROM money_transaction_snapshots WHERE company_id='company' AND source_type='field_service_agreement' AND source_id=$1)").bind(format!("service-agreement-{key}")).bind(format!("service-agreement-event-{key}")).fetch_one(db).await.unwrap();assert_eq!(absent,(0,0,0));
  }
  let result=post("create-normal",body).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(result.1["data"]["id"],"service-agreement-create-normal");assert_eq!(post("create-normal",body).await.0,StatusCode::OK);
  let timing:(i64,i64)=sqlx::query_as("SELECT g.created_at,m.captured_at FROM field_service_agreements g JOIN money_transaction_snapshots m ON m.company_id=g.company_id AND m.source_type='field_service_agreement' AND m.source_id=g.id AND m.amount_kind='visit_rate' WHERE g.id='service-agreement-create-normal'").fetch_one(db).await.unwrap();assert!(timing.1>timing.0,"Money capture must follow the original Agreement command timestamp");
  let get=|key:&str|Request::builder().uri(format!("/api/v1/holdings/acme/field-service/agreements/creates/{key}")).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
  let response=runtime.router().oneshot(get("create-normal")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["event"]["action"],"create");assert_eq!(value["money"]["amountUnits"],"10001");assert_eq!(value["money"]["usdUnits"],"5001");assert_eq!(value["money"]["fxRateId"],"rate");assert_eq!(value["money"]["usedLatestFallback"],false);
  let zero=body.replace("10001","0").replace("2026-01-01","2025-01-01");let result=post("create-zero",&zero).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);let response=runtime.router().oneshot(get("create-zero")).await.unwrap();let bytes=to_bytes(response.into_body(),65536).await.unwrap();let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["money"]["amountUnits"],"0");assert_eq!(value["money"]["usdUnits"],"0");assert_eq!(value["money"]["usedLatestFallback"],true);
  sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('newer-rate','company','THB','2026-01-01',4,'fixture-a',9)").execute(db).await.unwrap();
  assert_eq!(post("create-normal",body).await.0,StatusCode::OK);let response=runtime.router().oneshot(get("create-normal")).await.unwrap();let bytes=to_bytes(response.into_body(),65536).await.unwrap();let historical:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(historical["money"]["fxRateId"],"rate");assert_eq!(historical["money"]["usdUnits"],"5001");
  sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('usd-customer','company','customer','USD','Synthetic USD','USD',0,0)").execute(db).await.unwrap();
  let usd=body.replace(r#""customer""#,r#""usd-customer""#).replace(r#""thb""#,r#""usd""#);let result=post("create-usd",&usd).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);let response=runtime.router().oneshot(get("create-usd")).await.unwrap();let bytes=to_bytes(response.into_body(),65536).await.unwrap();let usd_evidence:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(usd_evidence["money"]["usdUnits"],"10001");assert!(usd_evidence["money"]["fxRateId"].is_null());
  let branch="b".repeat(210);sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES($1,'branch','long-agreement','Synthetic long branch','company','company',$1,'active')").bind(&branch).execute(db).await.unwrap();let long=body.replace("\"branch\"",&serde_json::to_string(&branch).unwrap());let result=post("create-long-branch",&long).await;assert_eq!(result.0,StatusCode::OK,"{}",result.1);assert_eq!(post("create-long-branch",&long).await.0,StatusCode::OK);assert_eq!(runtime.router().oneshot(get("create-long-branch")).await.unwrap().status(),StatusCode::OK);
  let review=r#"{"entity":"agreement","id":"service-agreement-create-long-branch","action":"activate","expectedVersion":1,"reason":"Independent approval"}"#;let reviewed=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","create-long-review",review).await;assert_eq!(reviewed.0,StatusCode::OK,"{}",reviewed.1);
  let customer="c".repeat(210);sqlx::query("INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||jsonb_build_object('id',$1::text,'code','LONG-CREATE'))).* FROM finance_parties p WHERE id='customer'").bind(&customer).execute(db).await.unwrap();
  let long_customer=body.replace("\"customer\"",&serde_json::to_string(&customer).unwrap());let created=post("create-long-customer",&long_customer).await;assert_eq!(created.0,StatusCode::OK,"{}",created.1);let stored:String=sqlx::query_scalar("SELECT customer_id FROM field_service_agreements WHERE id='service-agreement-create-long-customer'").fetch_one(db).await.unwrap();assert_eq!(stored,customer);
  let branch_login=account::login(system,"a@example.test",None).await;let branch_child=account::child(system,&branch_login,account::ACME).await;
  sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,scope_node_id)VALUES('agreement-branch-a','fixture-a','custom_field_parts','[\"field_service\"]','company',$1,$2,3,'branch','branch')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','agreement-branch-a',3,8,$2,$3)").bind(&branch_child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
  let branch_cookie=format!("__Host-erpx-product={}",branch_child.token.expose());let overridden=body.replace("\"branch\"","\"missing\"");
  let request=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",branch_cookie).header("idempotency-key","create-scope-override").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","agreement-branch-a").header("x-erpx-context-assignment-version","3").header("x-erpx-workspace-branch","north").body(Body::from(overridden)).unwrap();let response=runtime.router().oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let actual_branch:String=sqlx::query_scalar("SELECT branch_id FROM field_service_agreements WHERE id='service-agreement-create-scope-override'").fetch_one(db).await.unwrap();assert_eq!(actual_branch,"branch");
  let mut draining=db.begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *draining).await.unwrap();sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *draining).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'create-service-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(now).execute(&mut *draining).await.unwrap();draining.commit().await.unwrap();assert_eq!(post("create-draining-new",body).await.0,StatusCode::FORBIDDEN);assert_eq!(post("create-normal",body).await.0,StatusCode::OK);assert_eq!(runtime.router().oneshot(get("create-normal")).await.unwrap().status(),StatusCode::OK);
  runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn physical_scope_agreement_to_request_order_visit_parts_and_finance() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);
  let p=setup(system).await;setup_agreement_review_facts(&p,now).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let review=account::login(system,"b@example.test",None).await;let review_child=account::child(system,&review,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&review_child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer_cookie=format!("__Host-erpx-product={}",review_child.token.expose());
  let branch="b".repeat(210);let department="d".repeat(210);let customer="c".repeat(210);let asset="a".repeat(210);
  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES($1,'branch','physical-scope','Synthetic long branch','company','company',$1,'active')").bind(&branch).execute(db).await.unwrap();
  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES($1,'department','physical-department','Synthetic long department',$2,'company',$2,'active')").bind(&department).bind(&branch).execute(db).await.unwrap();
  sqlx::query("INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||jsonb_build_object('id',$1::text,'code','RAW-SCOPE'))).* FROM finance_parties p WHERE id='customer'").bind(&customer).execute(db).await.unwrap();
  sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,created_by,created_at,updated_at)VALUES($1,'company',$2,$3,'RAW-ASSET','Raw scope asset','synthetic','THB','fixture-a',$4,$4)").bind(&asset).bind(&branch).bind(&department).bind(now).execute(db).await.unwrap();
  sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name)VALUES('raw-warehouse','company',$1,'RAW','Raw scope warehouse')").bind(&branch).execute(db).await.unwrap();sqlx::query("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor)VALUES('raw-warehouse','product',5000,1000)").execute(db).await.unwrap();
  sqlx::query("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)VALUES('raw-sale','company',$1,'sales','RAW-SALE',$2,'raw-warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',$3,$3)").bind(&branch).bind(&customer).bind(now).execute(db).await.unwrap();
  sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at,version)VALUES('raw-ticket','company',$1,$4,'raw-sale',$2,'Raw source','Synthetic issue','fixture-a','2026-12-31','fixture-a',$3,$3,3)").bind(&branch).bind(&customer).bind(now).bind(&department).execute(db).await.unwrap();
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','raw_post_reviewer','Field journal reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','raw_post_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='raw_post_reviewer' WHERE id='finance-fixture-b'").execute(db).await.unwrap();
  let mut config=private_configuration::document(&system.control,&system.holdings).await;config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);for flag in ["fieldServiceAgreementCreates","fieldServiceAgreementReviews","fieldServiceRequests","fieldServiceOrderCreates","fieldServiceVisits","fieldServiceReservations","fieldServiceIssues","fieldServiceVisitCompletions","fieldServiceBillingReleases"]{config[flag]=serde_json::json!(["h001"]);}config["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["financeFieldServiceInvoiceReads"]=serde_json::json!(["h001"]);config["financeJournalPosts"]=serde_json::json!(["h001"]);config["financeFieldServiceJournalPosts"]=serde_json::json!(["h001"]);
  let mut closed_config=config.clone();closed_config["fieldServiceBillingReleases"]=serde_json::json!([]);closed_config["financeFieldServiceInvoiceReads"]=serde_json::json!([]);let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&closed_config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let create=serde_json::json!({"entity":"agreement","branchId":branch,"departmentId":department,"customerId":customer,"assetId":asset,"title":"Raw scope agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":10000}).to_string();let v=send(&runtime.router(),&cookie,"raw-scope-agreement",&create).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let activate=r#"{"entity":"agreement","id":"service-agreement-raw-scope-agreement","action":"activate","expectedVersion":1,"reason":"Independent approval"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","raw-scope-review",activate).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let request=r#"{"entity":"request","agreementId":"service-agreement-raw-scope-agreement","agreementVersion":2,"title":"Raw scope request","issue":"Broken equipment","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}"#;let v=send(&runtime.router(),&cookie,"raw-scope-request",request).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(send(&runtime.router(),&cookie,"raw-scope-request",request).await.0,StatusCode::OK);
  let get_request=Request::builder().uri("/api/v1/holdings/acme/field-service/requests/raw-scope-request").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(get_request).await.unwrap().status(),StatusCode::OK);
  let order=serde_json::json!({"ticketId":"raw-ticket","ticketVersion":3,"agreementId":"service-agreement-raw-scope-agreement","agreementVersion":2,"assetId":asset,"issue":"Broken unit","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}).to_string();let v=send(&runtime.router(),&cookie,"raw-scope-order",&order).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(send(&runtime.router(),&cookie,"raw-scope-order",&order).await.0,StatusCode::OK);
  let wrong_scope=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","raw-scope-wrong-workspace").header("x-erpx-context-company","company").header("x-erpx-context-revision","8").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-workspace-branch","north").body(Body::from(order.clone())).unwrap();assert_ne!(runtime.router().oneshot(wrong_scope).await.unwrap().status(),StatusCode::OK);assert_eq!(p.counts("raw-scope-wrong-workspace").await,(0,0,0,0,0));
  let get_order=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/raw-scope-order").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(get_order).await.unwrap().status(),StatusCode::OK);
  let schedule=serde_json::json!({"id":"service-order-raw-scope-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule visit"}).to_string();let v=send(&runtime.router(),&cookie,"raw-scope-schedule",&schedule).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let initial_stock_version:i64=sqlx::query_scalar("SELECT version FROM commerce_stock WHERE warehouse_id='raw-warehouse' AND product_id='product'").fetch_one(db).await.unwrap();
  let reserve=serde_json::json!({"id":"service-order-raw-scope-order","action":"reserve-part","expectedVersion":2,"warehouseId":"raw-warehouse","productId":"product","quantityMilli":1000,"expiresAt":now+3_600_000,"reason":"Reserve part"}).to_string();assert_ne!(send(&runtime.router(),"","raw-scope-unauth",&reserve).await.0,StatusCode::OK);assert_ne!(send(&runtime.router(),&cookie,"raw-scope-wrong-warehouse",&reserve.replace("raw-warehouse","warehouse")).await.0,StatusCode::OK);
  sqlx::raw_sql("CREATE FUNCTION raw_scope_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='raw-scope-drift' THEN UPDATE field_service_agreements SET title='changed' WHERE id='service-agreement-raw-scope-agreement'; END IF; RETURN NEW; END $$;CREATE TRIGGER raw_scope_drift AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION raw_scope_drift()").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"raw-scope-drift",&reserve).await.0,StatusCode::OK);assert_eq!(p.counts("raw-scope-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER raw_scope_drift ON command_receipts;DROP FUNCTION raw_scope_drift()").execute(db).await.unwrap();
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT raw_scope_rollback CHECK(command_id<>'raw-scope-rollback')").execute(db).await.unwrap();assert_ne!(send(&runtime.router(),&cookie,"raw-scope-rollback",&reserve).await.0,StatusCode::OK);assert_eq!(p.counts("raw-scope-rollback").await,(0,0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT raw_scope_rollback").execute(db).await.unwrap();
  let untouched:(i64,i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_part_reservations WHERE id='field-part-raw-scope-rollback'),(SELECT quantity_milli FROM commerce_stock WHERE warehouse_id='raw-warehouse' AND product_id='product'),(SELECT version FROM commerce_stock WHERE warehouse_id='raw-warehouse' AND product_id='product'),(SELECT title FROM field_service_agreements WHERE id='service-agreement-raw-scope-agreement')").fetch_one(db).await.unwrap();assert_eq!(untouched,(0,5000,initial_stock_version,"Raw scope agreement".into()));
  let v=send(&runtime.router(),&cookie,"raw-scope-reserve",&reserve).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(send(&runtime.router(),&cookie,"raw-scope-reserve",&reserve).await.0,StatusCode::OK);
  let dispatch=r#"{"id":"service-order-raw-scope-order","action":"dispatch","expectedVersion":3,"reason":"Dispatch visit"}"#;let v=send(&runtime.router(),&cookie,"raw-scope-dispatch",dispatch).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let start=r#"{"id":"service-order-raw-scope-order","action":"start","expectedVersion":4,"reason":"Start visit"}"#;let v=send(&runtime.router(),&cookie,"raw-scope-start",start).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let issue=r#"{"id":"service-order-raw-scope-order","action":"issue-part","expectedVersion":5,"reservationId":"field-part-raw-scope-reserve","quantityMilli":1000,"entryDate":"2026-01-10","allocations":[],"reason":"Issue part"}"#;let v=send(&runtime.router(),&cookie,"raw-scope-issue",issue).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(send(&runtime.router(),&cookie,"raw-scope-issue",issue).await.0,StatusCode::OK);
  let get=Request::builder().uri("/api/v1/holdings/acme/field-service/service-order-raw-scope-order/issues/raw-scope-issue").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let evidence:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let journal=evidence["event"]["journalId"].as_str().unwrap();
  let finance_cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());let finance_get=Request::builder().uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/workspace")).header("host","acme.example.test").header("cookie",&finance_cookie).body(Body::empty()).unwrap();assert_eq!(runtime.router().oneshot(finance_get).await.unwrap().status(),StatusCode::OK);
  let post=||Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/finance/journals/{journal}/post")).header("host","acme.example.test").header("cookie",&finance_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").header("idempotency-key","raw-scope-finance-post").header("content-type","application/json").body(Body::from(r#"{"expectedVersion":1}"#)).unwrap();let response=runtime.router().oneshot(post()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(runtime.router().oneshot(post()).await.unwrap().status(),StatusCode::OK);
  let posted:(String,i64)=sqlx::query_as("SELECT status,(SELECT count(*) FROM money_transaction_snapshots m WHERE m.company_id='company' AND m.source_type='finance_journal' AND m.source_id=j.id) FROM finance_journals j WHERE j.id=$1").bind(journal).fetch_one(db).await.unwrap();assert_eq!(posted,("posted".into(),1));
  sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at)VALUES('billing-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('billing-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0)").execute(db).await.unwrap();
  sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('billing-book-primary','company','Book primary','THB','maker',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('billing-book-ar','company','billing-book-primary','300','Book primary receivable','asset','THB',1,'receivable',0,0),('billing-book-revenue','company','billing-book-primary','430','Book primary revenue','revenue','THB',0,'none',0,0),('billing-ar-later','company','chart','121','Later receivable','asset','THB',1,'receivable',0,0),('billing-revenue-later','company','chart','421','Later revenue','revenue','THB',0,'none',0,0),('book-billing-ar','company','book-chart','001','Secondary receivable','asset','THB',1,'receivable',0,0),('book-billing-revenue','company','book-chart','002','Secondary revenue','revenue','THB',0,'none',0,0); INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at) VALUES('billing-rule','company','billing-book-primary','book-chart','2026-01-01','maker','Reviewed secondary book','[]','synthetic-opening-hash','period',0); INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id) VALUES('billing-rule','billing-book-ar','book-billing-ar'),('billing-rule','billing-book-revenue','book-billing-revenue'); UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer',reviewed_at=1 WHERE id='billing-rule'").execute(db).await.unwrap();
  let complete=r#"{"id":"service-order-raw-scope-order","action":"complete","expectedVersion":6,"laborMinutes":30,"evidence":"Actual completed work","customerConfirmation":"Customer accepted","reason":"Complete visit"}"#;let v=send(&runtime.router(),&cookie,"raw-scope-complete",complete).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let billing=r#"{"id":"service-order-raw-scope-order","action":"release-billing","expectedVersion":7,"reason":"Independent billing release"}"#;
  assert_ne!(send_as(&closed.router(),&reviewer_cookie,"agreement-reviewer-b","billing-closed",billing).await.0,StatusCode::OK);let closed_get=Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-billing-real/workspace").header("host","acme.example.test").header("cookie",&finance_cookie).body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(closed_get).await.unwrap().status(),StatusCode::NOT_FOUND);
  let closed_get=Request::builder().uri("/api/v1/holdings/acme/field-service/billings/billing-real").header("host","acme.example.test").header("cookie",&reviewer_cookie).body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(closed_get).await.unwrap().status(),StatusCode::NOT_FOUND);
  let own=send(&runtime.router(),&cookie,"billing-own",billing).await;assert_ne!(own.0,StatusCode::OK);
  sqlx::query("UPDATE iam_members SET person_id='field-person' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();let alias=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-person-alias",billing).await;assert_ne!(alias.0,StatusCode::OK,"{}",alias.1);sqlx::query("UPDATE iam_members SET person_id='agreement-review-person' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();
  sqlx::query("UPDATE field_service_agreements SET status='draft' WHERE id='service-agreement-raw-scope-agreement'").execute(db).await.unwrap();let inactive=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-inactive-agreement",billing).await;assert_ne!(inactive.0,StatusCode::OK,"{}",inactive.1);assert_eq!(p.counts("billing-inactive-agreement").await,(0,0,0,0,0));sqlx::query("UPDATE field_service_agreements SET status='active' WHERE id='service-agreement-raw-scope-agreement'").execute(db).await.unwrap();
  let stale=billing.replace("\"expectedVersion\":7","\"expectedVersion\":6");assert_ne!(send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-stale",&stale).await.0,StatusCode::OK);
  sqlx::raw_sql("CREATE FUNCTION billing_tamper_money() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.source_type='finance_invoice' AND NEW.source_id='field-invoice-billing-tamper' THEN NEW.captured_at=NEW.captured_at+1; END IF; RETURN NEW; END $$; CREATE TRIGGER billing_tamper_money BEFORE INSERT ON money_transaction_snapshots FOR EACH ROW EXECUTE FUNCTION billing_tamper_money()").execute(db).await.unwrap();
  let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-tamper",billing).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(p.counts("billing-tamper").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER billing_tamper_money ON money_transaction_snapshots; DROP FUNCTION billing_tamper_money()").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION billing_tamper_invoice() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='field-invoice-billing-invoice-tamper' THEN NEW.invoice_number=NEW.invoice_number||'-tampered'; END IF; RETURN NEW; END $$; CREATE TRIGGER billing_tamper_invoice BEFORE INSERT ON finance_invoices FOR EACH ROW EXECUTE FUNCTION billing_tamper_invoice()").execute(db).await.unwrap();let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-invoice-tamper",billing).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(p.counts("billing-invoice-tamper").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER billing_tamper_invoice ON finance_invoices; DROP FUNCTION billing_tamper_invoice()").execute(db).await.unwrap();
  sqlx::query("ALTER TABLE audit_events ADD CONSTRAINT billing_late_rollback CHECK(command_id<>'billing-rollback')").execute(db).await.unwrap();let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-rollback",billing).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(p.counts("billing-rollback").await,(0,0,0,0,0));sqlx::query("ALTER TABLE audit_events DROP CONSTRAINT billing_late_rollback").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION billing_drift_account() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='billing-account-drift' THEN UPDATE finance_accounts SET name='Drift' WHERE id='billing-ar'; END IF; RETURN NEW; END $$; CREATE TRIGGER billing_drift_account AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION billing_drift_account()").execute(db).await.unwrap();let drift=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-account-drift",billing).await;assert_ne!(drift.0,StatusCode::OK,"{}",drift.1);assert_eq!(p.counts("billing-account-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER billing_drift_account ON command_receipts;DROP FUNCTION billing_drift_account()").execute(db).await.unwrap();
  sqlx::raw_sql("CREATE FUNCTION billing_drift_quote() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='billing-fx-drift' THEN INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('billing-drift-rate','company','THB','2026-10-29',3,'fixture-b',9999999999999); END IF; RETURN NEW; END $$; CREATE TRIGGER billing_drift_quote AFTER INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION billing_drift_quote()").execute(db).await.unwrap();let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-fx-drift",billing).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(p.counts("billing-fx-drift").await,(0,0,0,0,0));sqlx::raw_sql("DROP TRIGGER billing_drift_quote ON command_receipts; DROP FUNCTION billing_drift_quote()").execute(db).await.unwrap();
  let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-real",billing).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-real",billing).await.0,StatusCode::OK);
  let get=Request::builder().uri("/api/v1/holdings/acme/field-service/billings/billing-real").header("host","acme.example.test").header("cookie",&reviewer_cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let evidence:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(evidence["release"]["amountMinor"],10000);assert_eq!(evidence["event"]["action"],"release-billing");
  sqlx::query("UPDATE finance_parties SET address_json='{}'::jsonb || jsonb_build_object('street','Verified address') WHERE id=$1").bind(&customer).execute(db).await.unwrap();
  sqlx::query("INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,subtotal_minor,tax_minor,total_minor,currency,account_id,source_snapshot_json,origin_kind,created_by,created_at) VALUES('billing-real-line','company','field-invoice-billing-real',1,0,'Actual retained line',10000,0,10000,'THB','billing-revenue','{}','historical_header','fixture-b',$1)").bind(now).execute(db).await.unwrap();
  sqlx::raw_sql("INSERT INTO finance_invoices SELECT (jsonb_populate_record(NULL::finance_invoices,to_jsonb(i)||jsonb_build_object('id','field-invoice-forged','invoice_number','FORGED'))).* FROM finance_invoices i WHERE id='field-invoice-billing-real';INSERT INTO money_transaction_snapshots SELECT (jsonb_populate_record(NULL::money_transaction_snapshots,to_jsonb(m)||jsonb_build_object('source_id','field-billing-forged'))).* FROM money_transaction_snapshots m WHERE company_id='company' AND source_type='field_service_billing_release' AND source_id='field-billing-billing-real';INSERT INTO money_transaction_snapshots SELECT (jsonb_populate_record(NULL::money_transaction_snapshots,to_jsonb(m)||jsonb_build_object('source_id','field-invoice-forged','carried_source_id','field-billing-forged'))).* FROM money_transaction_snapshots m WHERE company_id='company' AND source_type='finance_invoice' AND source_id='field-invoice-billing-real' AND amount_kind='draft:v1'").execute(db).await.unwrap();
  let finance_invoice_get=||Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-billing-real/workspace").header("host","acme.example.test").header("cookie",&finance_cookie).body(Body::empty()).unwrap();let response=runtime.router().oneshot(finance_invoice_get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let invoice:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(invoice["data"]["invoice"]["id"],"field-invoice-billing-real");assert_eq!(invoice["data"]["source"]["amountUnits"],"10000");assert_eq!(invoice["data"]["source"]["releaseId"],"field-billing-billing-real");assert_eq!(invoice["data"]["lines"][0]["description"],"Actual retained line");assert_eq!(invoice["data"]["lineHistory"][0]["line_count"],1);assert_eq!(invoice["data"]["options"]["parties"][0]["address_json"]["street"],"Verified address");assert_eq!(invoice["sourceOnly"],false);
  let wrong_actor=Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-billing-real/workspace").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(wrong_actor).await.unwrap().status(),StatusCode::OK);
  let forged=Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-forged/workspace").header("host","acme.example.test").header("cookie",&finance_cookie).body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(forged).await.unwrap().status(),StatusCode::OK);
  let effects:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_billing_releases WHERE id='field-billing-billing-real'),(SELECT count(*) FROM finance_invoices WHERE id='field-invoice-billing-real' AND status='draft' AND total_minor=10000 AND debit_account_id='billing-ar' AND credit_account_id='billing-revenue'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='field_service_billing_release' AND source_id='field-billing-billing-real' AND amount_units=10000),(SELECT count(*) FROM money_transaction_snapshots WHERE source_type='finance_invoice' AND source_id='field-invoice-billing-real' AND amount_units=10000 AND carried_source_id='field-billing-billing-real')").fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,1,1));
  // A Field-origin Support intake follows the real native visit and billing projection.
  for (key,body) in [("billing-intake-schedule",serde_json::json!({"id":"service-order-raw-scope-request","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule"})),("billing-intake-dispatch",serde_json::json!({"id":"service-order-raw-scope-request","action":"dispatch","expectedVersion":2,"reason":"Dispatch"})),("billing-intake-start",serde_json::json!({"id":"service-order-raw-scope-request","action":"start","expectedVersion":3,"reason":"Start"})),("billing-intake-complete",serde_json::json!({"id":"service-order-raw-scope-request","action":"complete","expectedVersion":4,"laborMinutes":10,"evidence":"Actual intake visit","customerConfirmation":"Customer accepted intake","reason":"Complete"}))] {let v=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);}
  let intake=r#"{"id":"service-order-raw-scope-request","action":"release-billing","expectedVersion":5,"reason":"Independent intake billing"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-intake",intake).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let ticket:(String,Option<String>,Option<String>,Option<String>,Option<String>,i64)=sqlx::query_as("SELECT status,resolution,resolved_by,confirmed_by,confirmation_reason,service_order_version FROM support_tickets WHERE id='ticket-raw-scope-request'").fetch_one(db).await.unwrap();assert_eq!(ticket,("closed".into(),Some("Actual intake visit".into()),Some("fixture-a".into()),Some("fixture-b".into()),Some("Independent intake billing".into()),6));
  // A second genuine native lifecycle covers warranty pricing and zero draft Money.
  sqlx::query("UPDATE operational_assets SET warranty_end='2099-12-31' WHERE id=$1").bind(&asset).execute(db).await.unwrap();
  let zero_agreement=serde_json::json!({"entity":"agreement","branchId":branch,"departmentId":department,"customerId":customer,"assetId":asset,"title":"Warranty agreement","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":10000,"warrantyDiscountBps":10000}).to_string();let v=send(&runtime.router(),&cookie,"billing-zero-agreement",&zero_agreement).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let activate=r#"{"entity":"agreement","id":"service-agreement-billing-zero-agreement","action":"activate","expectedVersion":1,"reason":"Independent approval"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-zero-activate",activate).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let order=serde_json::json!({"ticketId":"raw-ticket","ticketVersion":3,"agreementId":"service-agreement-billing-zero-agreement","agreementVersion":2,"assetId":asset,"issue":"Warranty repair","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}).to_string();let v=send(&runtime.router(),&cookie,"billing-zero-order",&order).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  for (key,body) in [("billing-zero-schedule",serde_json::json!({"id":"service-order-billing-zero-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule"})),("billing-zero-dispatch",serde_json::json!({"id":"service-order-billing-zero-order","action":"dispatch","expectedVersion":2,"reason":"Dispatch"})),("billing-zero-start",serde_json::json!({"id":"service-order-billing-zero-order","action":"start","expectedVersion":3,"reason":"Start"})),("billing-zero-complete",serde_json::json!({"id":"service-order-billing-zero-order","action":"complete","expectedVersion":4,"laborMinutes":30,"evidence":"Actual warranty work","customerConfirmation":"Customer accepted","reason":"Complete"}))] {let v=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);}
  let zero=r#"{"id":"service-order-billing-zero-order","action":"release-billing","expectedVersion":5,"reason":"Independent warranty billing"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-zero",zero).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM field_service_billing_releases WHERE id='field-billing-billing-zero' AND warranty_applied AND amount_minor=0),(SELECT count(*) FROM finance_invoices WHERE id='field-invoice-billing-zero' AND status='draft' AND total_minor=0),(SELECT count(*) FROM money_transaction_snapshots WHERE (source_id='field-billing-billing-zero' OR source_id='field-invoice-billing-zero') AND amount_units=0 AND carried_source_id IS NULL)").fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,2));
  let confirmations:i64=sqlx::query_scalar("SELECT count(*) FROM finance_service_no_charge_confirmations WHERE invoice_id='field-invoice-billing-zero'").fetch_one(db).await.unwrap();assert_eq!(confirmations,0);
  // A partial warranty uses the retained floor calculation and remains chargeable.
  let partial_agreement=serde_json::json!({"entity":"agreement","branchId":branch,"departmentId":department,"customerId":customer,"assetId":asset,"title":"Partial warranty","startDate":"2026-01-01","endDate":"2027-12-31","currency":"THB","visitRateMinor":10001,"warrantyDiscountBps":2500}).to_string();let v=send(&runtime.router(),&cookie,"billing-partial-agreement",&partial_agreement).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let activate=r#"{"entity":"agreement","id":"service-agreement-billing-partial-agreement","action":"activate","expectedVersion":1,"reason":"Independent approval"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-partial-activate",activate).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let order=serde_json::json!({"ticketId":"raw-ticket","ticketVersion":3,"agreementId":"service-agreement-billing-partial-agreement","agreementVersion":2,"assetId":asset,"issue":"Partial warranty repair","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}).to_string();let v=send(&runtime.router(),&cookie,"billing-partial-order",&order).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  for (key,body) in [("billing-partial-schedule",serde_json::json!({"id":"service-order-billing-partial-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule"})),("billing-partial-dispatch",serde_json::json!({"id":"service-order-billing-partial-order","action":"dispatch","expectedVersion":2,"reason":"Dispatch"})),("billing-partial-start",serde_json::json!({"id":"service-order-billing-partial-order","action":"start","expectedVersion":3,"reason":"Start"})),("billing-partial-complete",serde_json::json!({"id":"service-order-billing-partial-order","action":"complete","expectedVersion":4,"laborMinutes":30,"evidence":"Actual work","customerConfirmation":"Customer accepted","reason":"Complete"}))] {let v=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);}
  let partial=r#"{"id":"service-order-billing-partial-order","action":"release-billing","expectedVersion":5,"reason":"Independent partial warranty billing"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-partial",partial).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);let amount:i64=sqlx::query_scalar("SELECT amount_minor FROM field_service_billing_releases WHERE id='field-billing-billing-partial' AND warranty_applied").fetch_one(db).await.unwrap();assert_eq!(amount,7500);
  // Reviewer who actually completed this visit cannot release its billing.
  let second=serde_json::json!({"ticketId":"raw-ticket","ticketVersion":3,"agreementId":"service-agreement-billing-zero-agreement","agreementVersion":2,"assetId":asset,"issue":"Another visit","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}).to_string();let v=send(&runtime.router(),&cookie,"billing-reviewer-order",&second).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  for (key,body) in [("billing-reviewer-schedule",serde_json::json!({"id":"service-order-billing-reviewer-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule"})),("billing-reviewer-dispatch",serde_json::json!({"id":"service-order-billing-reviewer-order","action":"dispatch","expectedVersion":2,"reason":"Dispatch"})),("billing-reviewer-start",serde_json::json!({"id":"service-order-billing-reviewer-order","action":"start","expectedVersion":3,"reason":"Start"}))] {let v=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);}
  let complete=r#"{"id":"service-order-billing-reviewer-order","action":"complete","expectedVersion":4,"laborMinutes":10,"evidence":"Reviewer completed work","customerConfirmation":"Customer accepted","reason":"Complete"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-reviewer-complete",complete).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);let release=r#"{"id":"service-order-billing-reviewer-order","action":"release-billing","expectedVersion":5,"reason":"Reviewer cannot release own completion"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-reviewer-denied",release).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(p.counts("billing-reviewer-denied").await,(0,0,0,0,0));
  // Historical draft fixture allows an actual no-rate denial without fabricating an FX quote.
  sqlx::raw_sql("INSERT INTO finance_parties SELECT (jsonb_populate_record(NULL::finance_parties,to_jsonb(p)||jsonb_build_object('id','eur-customer','code','EUR-CUSTOMER','currency','EUR'))).* FROM finance_parties p WHERE id='customer'; INSERT INTO commerce_orders SELECT (jsonb_populate_record(NULL::commerce_orders,to_jsonb(o)||jsonb_build_object('id','eur-sale','number','EUR-SALE','party_id','eur-customer','currency','EUR'))).* FROM commerce_orders o WHERE id='raw-sale'; INSERT INTO support_tickets SELECT (jsonb_populate_record(NULL::support_tickets,to_jsonb(t)||jsonb_build_object('id','eur-ticket','order_id','eur-sale','customer_id','eur-customer'))).* FROM support_tickets t WHERE id='raw-ticket'; INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES('eur-chart','company','EUR chart','EUR','fixture-a',0,0); INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('eur-ar','company','eur-chart','120','Receivable','asset','EUR',1,'receivable',0,0),('eur-revenue','company','eur-chart','420','Revenue','revenue','EUR',0,'none',0,0)").execute(db).await.unwrap();
  sqlx::query("INSERT INTO field_service_agreements(id,company_id,branch_id,department_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES('eur-agreement','company',$1,$2,'eur-customer','Historical EUR draft','2026-01-01','2027-12-31','EUR',10000,'draft','fixture-a',1,$3,$3)").bind(&branch).bind(&department).bind(now).execute(db).await.unwrap();sqlx::query("INSERT INTO field_service_agreement_events(id,agreement_id,version,action,actor_id,reason,status,created_at) VALUES('eur-draft-event','eur-agreement',1,'create','fixture-a','Historical draft','draft',$1)").bind(now).execute(db).await.unwrap();
  let activate=r#"{"entity":"agreement","id":"eur-agreement","action":"activate","expectedVersion":1,"reason":"Independent approval"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","eur-activate",activate).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  let order=serde_json::json!({"ticketId":"eur-ticket","ticketVersion":3,"agreementId":"eur-agreement","agreementVersion":2,"assetId":null,"issue":"No exchange rate","site":"Customer site","technicianId":"fixture-a","scheduledAt":null}).to_string();let v=send(&runtime.router(),&cookie,"billing-eur-order",&order).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);
  for (key,body) in [("billing-eur-schedule",serde_json::json!({"id":"service-order-billing-eur-order","action":"schedule","expectedVersion":1,"technicianId":"fixture-a","scheduledAt":now+60000,"reason":"Schedule"})),("billing-eur-dispatch",serde_json::json!({"id":"service-order-billing-eur-order","action":"dispatch","expectedVersion":2,"reason":"Dispatch"})),("billing-eur-start",serde_json::json!({"id":"service-order-billing-eur-order","action":"start","expectedVersion":3,"reason":"Start"})),("billing-eur-complete",serde_json::json!({"id":"service-order-billing-eur-order","action":"complete","expectedVersion":4,"laborMinutes":10,"evidence":"Actual work","customerConfirmation":"Customer accepted","reason":"Complete"}))] {let v=send(&runtime.router(),&cookie,key,&body.to_string()).await;assert_eq!(v.0,StatusCode::OK,"{}",v.1);}
  let missing=r#"{"id":"service-order-billing-eur-order","action":"release-billing","expectedVersion":5,"reason":"Missing rate must fail"}"#;let v=send_as(&runtime.router(),&reviewer_cookie,"agreement-reviewer-b","billing-missing-fx",missing).await;assert_ne!(v.0,StatusCode::OK,"{}",v.1);assert_eq!(v.1["error"]["code"],"MANUAL_RATE_REQUIRED");assert_eq!(p.counts("billing-missing-fx").await,(0,0,0,0,0));
  closed.close().await;
  runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn finance_service_invoice_workspace_uses_real_included_history() {
    account::with_system(|system| async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=setup_without_finance_history(system).await;setup_agreement_review_facts(&p,now).await;
        let system=&p.review.system;let db=p.pool().raw();
        sqlx::raw_sql("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement';INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('invoice-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('invoice-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0)").execute(db).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
        let central=account::login(system,"b@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer=format!("__Host-erpx-product={}",child.token.expose());
        let finance=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());
        let mut config=private_configuration::document(&system.control,&system.holdings).await;config["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);for flag in ["fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases"]{config[flag]=serde_json::json!(["h001"]);}config["financeJournalReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);config["financeFieldServiceInvoiceReads"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&config.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        for (key,body) in [("included-dispatch",r#"{"id":"reserve-order","action":"dispatch","expectedVersion":1,"reason":"Dispatch"}"#),("included-start",r#"{"id":"reserve-order","action":"start","expectedVersion":2,"reason":"Start"}"#),("included-complete",r#"{"id":"reserve-order","action":"complete","expectedVersion":3,"laborMinutes":5,"evidence":"Actual work","customerConfirmation":"Accepted","reason":"Complete"}"#)] {let r=send(&runtime.router(),&cookie,key,body).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1)}
        let billing=r#"{"id":"reserve-order","action":"release-billing","expectedVersion":4,"reason":"Independent release"}"#;let r=send_as(&runtime.router(),&reviewer,"agreement-reviewer-b","included-billing",billing).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);
        let get=||Request::builder().uri("/api/v1/holdings/acme/finance/invoices/field-invoice-included-billing/workspace").header("host","acme.example.test").header("cookie",&finance).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(body["sourceOnly"],true);assert_eq!(body["data"]["source"]["releaseId"],"field-billing-included-billing");
        // This valid persisted later status is a fixture, not a claimed native posting.
        sqlx::query("UPDATE finance_invoices SET status='voided',version=version+1 WHERE id='field-invoice-included-billing'").execute(db).await.unwrap();let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(body["data"]["invoice"]["status"],"voided");
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_workspace_returns_real_scoped_rows_and_references() {
    account::with_system(|system| async move {
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);
  let p=setup_workspace(system).await;let system=&p.review.system;let db=p.pool().raw();let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  // Same-Company blocked customers and retired assets remain historical displays, but are excluded from active pickers.
  sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES('old-customer','company','customer','OLD','Historical customer','THB','blocked',0,0);INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,lifecycle_status,retirement_reason,created_by,created_at,updated_at) VALUES('old-asset','company','branch','OLD-ASSET','Historical asset','equipment','THB','retired','Historical retirement','fixture-a',0,0);INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES('old-agreement','company','branch','old-customer','old-asset','Historical agreement','2020-01-01','2021-01-01','THB',100,'draft','fixture-a',1,0,0)").execute(db).await.unwrap();
  let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();cfg["fieldServiceWorkspaceReads"]=serde_json::json!(["h001"]);cfg["fieldServiceWorkReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let get=||Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(get()).await.unwrap().status(),StatusCode::NOT_FOUND);
  let response=runtime.router().oneshot(get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let data=&value["data"];
  assert_eq!(data["agreements"].as_array().unwrap().len(),2);let historical=data["agreements"].as_array().unwrap().iter().find(|g|g["id"]=="old-agreement").unwrap();assert_eq!(historical["customer_name"],"Historical customer");assert_eq!(historical["asset_code"],"OLD-ASSET");assert!(data["customers"].as_array().unwrap().iter().all(|c|c["id"]!="old-customer"));assert!(data["assets"].as_array().unwrap().iter().all(|a|a["id"]!="old-asset"));assert_eq!(data["orders"].as_array().unwrap().len(),2);assert_eq!(data["customers"][0]["id"],"customer");assert_eq!(data["nodes"][0]["id"],"branch");assert_eq!(data["people"][0]["id"],"fixture-a");assert_eq!(data["canCreateAgreement"],true);assert_eq!(data["orders"][0]["technician_eligible"],1);assert_eq!(data["referenceScope"],serde_json::json!({"branchId":null,"departmentId":null}));assert_eq!(data["workspaceKey"],r#"["h001","company",null,null,"fixture-a","field-fixture-a","custom_field_parts",8]"#);
  let stale=Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(stale).await.unwrap().status(),StatusCode::OK);
  let unauth=Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(unauth).await.unwrap().status(),StatusCode::OK);
  // A selected-branch Order may retain a same-Company Agreement outside that branch.
  sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status)VALUES('elsewhere','branch','elsewhere','Elsewhere','company','company','elsewhere','active');INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES('elsewhere-agreement','company','elsewhere','customer','Elsewhere historical agreement','2020-01-01','2021-01-01','THB',100,'draft','fixture-a',1,0,0);INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('workspace-sale','company','branch','sales','WORKSPACE-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',0,0);INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES('workspace-ticket','company','branch','workspace-sale','customer','Historical sales ticket','Issue','fixture-a','2026-12-31','fixture-a',0,0);INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,created_by,created_at,updated_at) VALUES('workspace-linked-order','company','branch','workspace-ticket',1,'elsewhere-agreement',1,'Historical issue','Customer site','fixture-a','open','fixture-a',0,0)").execute(db).await.unwrap();
  let scoped=Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-workspace-branch","north").body(Body::empty()).unwrap();let response=runtime.router().oneshot(scoped).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let scoped:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(scoped["data"]["referenceScope"]["branchId"],"branch");assert!(scoped["data"]["agreements"].as_array().unwrap().iter().all(|g|g["id"]!="elsewhere-agreement"));let linked=scoped["data"]["orders"].as_array().unwrap().iter().find(|o|o["id"]=="workspace-linked-order").unwrap();assert_eq!(linked["agreement_title"],"Elsewhere historical agreement");
  let work=||Request::builder().uri("/api/v1/holdings/acme/field-service/work").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-workspace-branch","north").body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(work()).await.unwrap().status(),StatusCode::NOT_FOUND);let response=runtime.router().oneshot(work()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(value["data"].as_array().unwrap().iter().any(|i|i["id"]=="workspace-linked-order"));assert!(!value["workflowSourceIds"].as_array().unwrap().iter().any(|id|id=="workspace-linked-order"));assert!(value["workflowSourceIds"].as_array().unwrap().iter().any(|id|id=="issue-order"));let stale_work=Request::builder().uri("/api/v1/holdings/acme/field-service/work").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(stale_work).await.unwrap().status(),StatusCode::OK);
  setup_agreement_review_facts(&p,now).await;let central=account::login(system,"b@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer=format!("__Host-erpx-product={}",child.token.expose());let reviewer_get=||Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",&reviewer).body(Body::empty()).unwrap();let response=runtime.router().oneshot(reviewer_get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let row=value["data"]["agreements"].as_array().unwrap().iter().find(|g|g["id"]=="old-agreement").unwrap();assert_eq!(row["can_review"],1);assert_eq!(row["can_activate"],1);
  sqlx::query("UPDATE iam_members SET person_id='field-person' WHERE company_id='company' AND user_id='fixture-b'").execute(db).await.unwrap();let response=runtime.router().oneshot(reviewer_get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let row=value["data"]["agreements"].as_array().unwrap().iter().find(|g|g["id"]=="old-agreement").unwrap();assert_eq!(row["can_review"],0);assert_eq!(row["can_activate"],0);
  withdraw(&p,"service").await;system.scenario.clock.0.store(now+10_000,Ordering::SeqCst);let response=runtime.router().oneshot(get()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(!value["data"]["orders"].as_array().unwrap().is_empty());assert_eq!(value["data"]["canWrite"],false);assert_eq!(value["data"]["canCreateAgreement"],false);assert!(value["data"]["customers"].as_array().unwrap().is_empty());assert!(value["data"]["people"].as_array().unwrap().is_empty());assert!(value["data"]["tickets"].as_array().unwrap().is_empty());assert!(value["data"]["nodes"].as_array().unwrap().is_empty());
  let response=runtime.router().oneshot(work()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(value["data"].as_array().unwrap().iter().all(|i|i["canAct"]==false));
  // Existing retained parts History still permits historical display after withdrawal.
  withdraw(&p,"service-parts-assets").await;let response=runtime.router().oneshot(get()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
  // A foreign-Company customer attached to a scoped Field Agreement must fail closed.
  sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('foreign-customer','other-company','customer','FOREIGN','Foreign secret','THB',0,0);INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES('foreign-source','company','branch','foreign-customer','Bad legacy source','2020-01-01','2021-01-01','THB',100,'draft','fixture-a',1,0,0)").execute(db).await.unwrap();let response=runtime.router().oneshot(get()).await.unwrap();assert_ne!(response.status(),StatusCode::OK);
  closed.close().await;runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_workspace_rejects_oversized_historical_rows_without_truncation() {
    account::with_system(|system| async move {
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup(system).await;let system=&p.review.system;let db=p.pool().raw();let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceWorkspaceReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let get=Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let response=runtime.router().oneshot(get).await.unwrap();assert_eq!(response.status(),StatusCode::SERVICE_UNAVAILABLE);runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_workspace_masks_assets_without_parts_history() {
    account::with_system(|system| async move {
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace_without_parts(system).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('masked-asset','company','branch','MASKED','Covered asset','equipment','THB','fixture-a',0,0); INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,asset_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,version,created_at,updated_at) VALUES('masked-agreement','company','branch','customer','masked-asset','Covered agreement','2020-01-01','2021-01-01','THB',100,'draft','fixture-a',1,0,0)").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceWorkspaceReads"]=serde_json::json!(["h001"]);cfg["fieldServiceWorkReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let get=Request::builder().uri("/api/v1/holdings/acme/field-service/workspace").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let g=v["data"]["agreements"].as_array().unwrap().iter().find(|x|x["id"]=="masked-agreement").unwrap();assert!(g["asset_id"].is_null());assert!(g["asset_code"].is_null());assert_eq!(g["requires_asset_capability"],1);assert_eq!(v["data"]["canUseAssetReferences"],false);assert!(v["data"]["assets"].as_array().unwrap().is_empty());let work=Request::builder().uri("/api/v1/holdings/acme/field-service/work").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let r=runtime.router().oneshot(work).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let w:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(w["data"].as_array().unwrap().iter().any(|i|i["id"]=="masked-agreement" && i["canAct"]==false));assert!(!w["workflowSourceIds"].as_array().unwrap().iter().any(|id|id=="masked-agreement"));runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_order_detail_returns_real_parts_and_current_workspace() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace(system).await;let system=&p.review.system;let db=p.pool().raw();sqlx::query("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement'").execute(db).await.unwrap();p.issue("detail-original-issue").await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();cfg["fieldServiceDetailReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let get=||Request::builder().uri("/api/v1/holdings/acme/field-service/orders/issue-order/detail").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(closed.router().oneshot(get()).await.unwrap().status(),StatusCode::NOT_FOUND);let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let d=&v["data"];assert_eq!(d["order"]["id"],"issue-order");assert_eq!(d["order"]["version"],4);assert_eq!(d["parts"][0]["id"],"reserved-part");assert_eq!(d["parts"][0]["consumed_milli"],1000);assert_eq!(d["consumptions"][0]["id"],"operational-part-detail-original-issue");assert_eq!(d["consumptions"][0]["journal_status"],"draft");assert!(d["history"].as_array().unwrap().iter().any(|e|e["action"]=="issue-part"));assert_eq!(d["spares"]["stock"][0]["available_milli"],2000);assert_eq!(d["canEditVisitFiles"],true);
  // All owner changes commit together while the reader is waiting before its first source snapshot.
  let mut writer=db.begin().await.unwrap();sqlx::query("UPDATE commerce_stock SET quantity_milli=5000 WHERE warehouse_id='warehouse' AND product_id='product'").execute(&mut *writer).await.unwrap();sqlx::query("UPDATE commerce_products SET name='Changed actual spare' WHERE id='product'").execute(&mut *writer).await.unwrap();sqlx::query("UPDATE field_service_part_reservations SET released_milli=1000,release_reason='Concurrent release' WHERE id='reserved-part'").execute(&mut *writer).await.unwrap();sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,quantity_scope,held_quantity_milli)VALUES('detail-concurrent-hold','company','branch','warehouse','product','Concurrent hold','2027-12-31','fixture-a',0,0,'partial',500)").execute(&mut *writer).await.unwrap();let reader=runtime.router();let request=get();let pending=tokio::spawn(async move{reader.oneshot(request).await.unwrap()});tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!pending.is_finished());writer.commit().await.unwrap();let r=pending.await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(v["data"]["spares"]["stock"][0]["available_milli"],3500);assert_eq!(v["data"]["spares"]["stock"][0]["product_name"],"Changed actual spare");assert_eq!(v["data"]["parts"][0]["released_milli"],1000);
  sqlx::query("UPDATE field_service_part_reservations SET expires_at=$1 WHERE id='reserved-part'").bind(now-50).execute(db).await.unwrap();let r=runtime.router().oneshot(get()).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(v["data"]["spares"]["stock"][0]["available_milli"],4500);assert_eq!(v["data"]["parts"][0]["id"],"reserved-part");
  let missing=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/no-such-order/detail").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::OK);closed.close().await;runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_order_detail_invoice_link_requires_real_permission_and_provenance() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace(system).await;setup_agreement_review_facts(&p,now).await;let system=&p.review.system;let db=p.pool().raw();sqlx::raw_sql("UPDATE hr_engagements SET start_date='2020-01-01' WHERE id='field-engagement';INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES('detail-ar','company','chart','120','Receivable','asset','THB',1,'receivable',0,0),('detail-revenue','company','chart','420','Revenue','revenue','THB',0,'none',0,0)").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let maker=format!("__Host-erpx-product={}",child.token.expose());
  let central=account::login(system,"b@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','agreement-reviewer-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reviewer=format!("__Host-erpx-product={}",child.token.expose());
  let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);for flag in ["fieldServiceVisits","fieldServiceVisitCompletions","fieldServiceBillingReleases","fieldServiceDetailReads"]{cfg[flag]=serde_json::json!(["h001"]);}let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  for (key,body) in [("detail-link-dispatch",r#"{"id":"reserve-order","action":"dispatch","expectedVersion":1,"reason":"Dispatch"}"#),("detail-link-start",r#"{"id":"reserve-order","action":"start","expectedVersion":2,"reason":"Start"}"#),("detail-link-complete",r#"{"id":"reserve-order","action":"complete","expectedVersion":3,"laborMinutes":5,"evidence":"Actual work","customerConfirmation":"Accepted","reason":"Complete"}"#)]{let r=send(&runtime.router(),&maker,key,body).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1)}let billing=r#"{"id":"reserve-order","action":"release-billing","expectedVersion":4,"reason":"Independent billing"}"#;let r=send_as(&runtime.router(),&reviewer,"agreement-reviewer-b","detail-link-billing",billing).await;assert_eq!(r.0,StatusCode::OK,"{}",r.1);
  let get=|cookie:&str|Request::builder().uri("/api/v1/holdings/acme/field-service/orders/reserve-order/detail").header("host","acme.example.test").header("cookie",cookie).body(Body::empty()).unwrap();let r=runtime.router().oneshot(get(&maker)).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(v["data"]["billing"]["invoice_id"].is_null());
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','field_finance_reader','Service reader with Finance read',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','field_finance_reader',1,'[\"records.read\",\"records.write\",\"records.review\",\"module.field_service.read\",\"finance.read\"]','[]','[\"holding\"]','fixture',0)").execute(db).await.unwrap();sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('detail-finance-reader','fixture-b','field_finance_reader','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();let central=account::login(system,"b@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-b','company','detail-finance-reader',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let reader=format!("__Host-erpx-product={}",child.token.expose());let r=runtime.router().oneshot(get(&reader)).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(v["data"]["billing"]["invoice_id"],"field-invoice-detail-link-billing");assert_eq!(v["data"]["canEditVisitFiles"],false);// A source label, invoice and Money row without the actual Field event/receipt cannot fabricate a link.
  sqlx::raw_sql("INSERT INTO field_service_billing_releases SELECT (jsonb_populate_record(NULL::field_service_billing_releases,to_jsonb(b)||jsonb_build_object('id','field-billing-detail-forged','order_id','issue-order','source_order_version',3))).* FROM field_service_billing_releases b WHERE id='field-billing-detail-link-billing'; INSERT INTO finance_invoices SELECT (jsonb_populate_record(NULL::finance_invoices,to_jsonb(i)||jsonb_build_object('id','field-invoice-detail-forged','source_id','issue-order','invoice_number','FORGED-DETAIL'))).* FROM finance_invoices i WHERE id='field-invoice-detail-link-billing'; INSERT INTO money_transaction_snapshots SELECT (jsonb_populate_record(NULL::money_transaction_snapshots,to_jsonb(m)||jsonb_build_object('source_id','field-billing-detail-forged'))).* FROM money_transaction_snapshots m WHERE company_id='company' AND source_type='field_service_billing_release' AND source_id='field-billing-detail-link-billing'").execute(db).await.unwrap();let forged=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/issue-order/detail").header("host","acme.example.test").header("cookie",&reader).body(Body::empty()).unwrap();let r=runtime.router().oneshot(forged).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert!(v["data"]["billing"]["invoice_id"].is_null());runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_order_detail_respects_quality_holds_and_lot_choices() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace_tracking(system,"lot").await;let system=&p.review.system;let db=p.pool().raw();sqlx::query("INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at,quantity_scope,lot_id,held_quantity_milli)VALUES('detail-hold','company','branch','warehouse','product','Synthetic hold','2027-12-31','fixture-a',0,0,'partial','lot-1',1000)").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceDetailReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let request=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/issue-order/detail").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let r=runtime.router().oneshot(request).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(v["data"]["spares"]["stock"][0]["available_milli"],1000);let lots=v["data"]["spares"]["lots"].as_array().unwrap();assert_eq!(lots.len(),1);assert_eq!(lots[0]["lot_id"],"lot-2");runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_order_detail_hides_parts_without_history_and_gates_visit_files() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace_without_parts(system).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('detail-asset','company','branch','DETAIL','Asset','equipment','THB','fixture-a',0,0); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('detail-sale','company','branch','sales','DETAIL-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',0,0); INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES('detail-ticket','company','branch','detail-sale','customer','Service ticket','Issue','fixture-a','2026-12-31','fixture-a',0,0); INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,asset_id,issue,site,technician_id,status,created_by,created_at,updated_at) VALUES('detail-covered','company','branch','detail-ticket',1,'agreement',2,'detail-asset','Issue','Customer site','fixture-a','open','fixture-a',0,0); INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,details_json,created_at) VALUES('detail-old-reserve','detail-covered',1,'reserve-part','fixture-a','Historical parts','open','fixture-a','{}',0)").execute(db).await.unwrap();
  let long_id="x".repeat(210);sqlx::query("INSERT INTO field_service_orders SELECT (jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||jsonb_build_object('id',$1::text,'asset_id',NULL))).* FROM field_service_orders o WHERE id='detail-covered'").bind(&long_id).execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceDetailReads"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let request=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/detail-covered/detail").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let r=runtime.router().oneshot(request).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let v:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let d=&v["data"];assert_eq!(d["order"]["requires_asset_capability"],1);assert!(d["order"]["asset_id"].is_null());assert_eq!(d["canEditVisitFiles"],false);assert!(d["parts"].as_array().unwrap().is_empty());assert!(d["consumptions"].as_array().unwrap().is_empty());assert!(d["history"].as_array().unwrap().is_empty());assert!(d["spares"]["stock"].as_array().unwrap().is_empty());assert!(d["spares"]["lots"].as_array().unwrap().is_empty());let long_request=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/orders/{long_id}/detail")).header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",child.token.expose())).body(Body::empty()).unwrap();let r=runtime.router().oneshot(long_request).await.unwrap();let status=r.status();let bytes=to_bytes(r.into_body(),2*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let long:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(long["data"]["order"]["id"],long_id);runtime.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_field_attachment_withdraws_historical_draft_and_replays() {
    account::with_system(|system| async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=setup_workspace(system).await;let system=&p.review.system;let db=p.pool().raw();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('files-test-set','company','branch','field_service_visit','issue-order','record','technician_report',1,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('files-test-link','files-test-set','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(db).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldAttachmentLinks"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);cfg["fieldServiceOrderCancellations"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let post=|key:&str,reason:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-test-link/withdraw").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key",key).header("content-type","application/json").body(Body::from(serde_json::json!({"reason":reason}).to_string())).unwrap();
        let no_context=Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-test-link/withdraw").header("host","acme.example.test").header("cookie",&cookie).header("idempotency-key","fixture-missing-cas").body(Body::from(r#"{"reason":"No longer needed"}"#)).unwrap();assert_eq!(runtime.router().oneshot(no_context).await.unwrap().status(),StatusCode::CONFLICT);
        let stale_context=Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-test-link/withdraw").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").header("idempotency-key","fixture-stale-cas").body(Body::from(r#"{"reason":"No longer needed"}"#)).unwrap();assert_eq!(runtime.router().oneshot(stale_context).await.unwrap().status(),StatusCode::CONFLICT);
        let untouched:(String,i64)=sqlx::query_as("SELECT state,(SELECT count(*) FROM attachment_events WHERE link_id='files-test-link') FROM attachment_links WHERE id='files-test-link'").fetch_one(db).await.unwrap();assert_eq!(untouched,("draft".into(),0));
        sqlx::raw_sql("CREATE FUNCTION fixture_file_late_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE field_service_orders SET site='Changed by trigger' WHERE id='issue-order'; RETURN NEW; END $$; CREATE TRIGGER fixture_file_late_drift AFTER INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='link.withdrawn') EXECUTE FUNCTION fixture_file_late_drift()").execute(db).await.unwrap();
        assert_eq!(runtime.router().oneshot(post("fixture-withdraw-rollback","No longer needed")).await.unwrap().status(),StatusCode::CONFLICT);
        let before:(String,i64,String)=sqlx::query_as("SELECT l.state,(SELECT count(*) FROM attachment_events WHERE link_id=l.id),o.site FROM attachment_links l JOIN field_service_orders o ON o.id='issue-order' WHERE l.id='files-test-link'").fetch_one(db).await.unwrap();assert_eq!(before.0,"draft");assert_eq!(before.1,0);assert_ne!(before.2,"Changed by trigger");
        sqlx::raw_sql("DROP TRIGGER fixture_file_late_drift ON attachment_events; DROP FUNCTION fixture_file_late_drift()").execute(db).await.unwrap();
        let readonly_cfg={let mut c=cfg.clone();c["fieldAttachmentLinks"]=serde_json::json!([{"holdingId":"h001","mode":"read-only"}]);c};let readonly=assemble_development_api(DevelopmentApiConfiguration::parse(&readonly_cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(readonly.router().oneshot(post("fixture-readonly-key","No longer needed")).await.unwrap().status(),StatusCode::SERVICE_UNAVAILABLE);let status_req=Request::builder().uri("/api/v1/holdings/acme/attachment-links/files-test-link/status").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();assert_eq!(readonly.router().oneshot(status_req).await.unwrap().status(),StatusCode::NOT_FOUND);readonly.close().await;
        assert_eq!(runtime.router().oneshot(post("fixture-invalid-key","x")).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
        let no_actor=Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-test-link/withdraw").header("host","acme.example.test").header("idempotency-key","fixture-noactor-key").body(Body::from("{" )).unwrap();assert_eq!(runtime.router().oneshot(no_actor).await.unwrap().status(),StatusCode::UNAUTHORIZED);
        let finance_cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());let no_permission=Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-test-link/withdraw").header("host","acme.example.test").header("cookie",finance_cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","7").header("idempotency-key","fixture-noperm-key").body(Body::from(r#"{"reason":"x"}"#)).unwrap();assert_eq!(runtime.router().oneshot(no_permission).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);

        sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('files-other','files-test-set','draft','fixture-b',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id) VALUES('files-bound','files-test-set','bound','fixture-b',$1,$2,$1,'record','some-record')").bind(now).bind(now+3600000).execute(db).await.unwrap();
        let other=|id:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/attachment-links/{id}/withdraw")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key","fixture-other-key").body(Body::from(r#"{"reason":"No longer needed"}"#)).unwrap();assert_eq!(runtime.router().oneshot(other("files-other")).await.unwrap().status(),StatusCode::FORBIDDEN);assert_eq!(runtime.router().oneshot(other("files-bound")).await.unwrap().status(),StatusCode::CONFLICT);
        sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('files-concurrent','files-test-set','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();let concurrent=|key:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/files-concurrent/withdraw").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key",key).body(Body::from(r#"{"reason":"No longer needed"}"#)).unwrap();let (left,right)=tokio::join!(runtime.router().oneshot(concurrent("fixture-concurrent-1")),runtime.router().oneshot(concurrent("fixture-concurrent-2")));assert_eq!(left.unwrap().status(),StatusCode::OK);assert_eq!(right.unwrap().status(),StatusCode::OK);let concurrent_count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_events WHERE link_id='files-concurrent' AND action='link.withdrawn'").fetch_one(db).await.unwrap();assert_eq!(concurrent_count,1);
        let response=runtime.router().oneshot(post("fixture-withdraw-one","  No longer needed  ")).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(value["data"]["linkState"],"withdrawn");assert!(value["data"]["sourceId"].is_null());assert_eq!(value["data"]["scan"]["attempts"],0);
        assert_eq!(runtime.router().oneshot(post("fixture-withdraw-two","No longer needed")).await.unwrap().status(),StatusCode::OK);
        assert_eq!(runtime.router().oneshot(post("fixture-withdraw-three","Different reason")).await.unwrap().status(),StatusCode::CONFLICT);
        let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_events WHERE link_id='files-test-link' AND action='link.withdrawn'),(SELECT count(*) FROM attachment_access_events WHERE link_id='files-test-link'),(SELECT version FROM field_service_orders WHERE id='issue-order')").fetch_one(db).await.unwrap();assert_eq!(effects,(1,0,3));
        let long_id="f".repeat(210);sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES($1,'files-test-set','draft','fixture-a',$2,$3)").bind(&long_id).bind(now).bind(now+3600000).execute(db).await.unwrap();let long=Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/attachment-links/{long_id}/withdraw")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key","fixture-long-link").body(Body::from(r#"{"reason":"No longer needed"}"#)).unwrap();assert_eq!(runtime.router().oneshot(long).await.unwrap().status(),StatusCode::OK);
        // The mutation commits before its independent status audit; failure must remain recoverable.
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type) VALUES('gap-upload','field_service_visit','issue-order','signed_job_sheet','file.pdf','application/pdf',8,8,$1,'gap-stage','gap-final','ready','fixture-a',$2,$3,$3,1,'company','application/pdf')").bind("a".repeat(64)).bind(now+3600000).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state) VALUES('gap-source','gap-upload','field_service_visit','issue-order','signed_job_sheet','gap-final',1,'file.pdf','application/pdf',8,$1,'fixture-a',$2,'company','application/pdf','available')").bind("a".repeat(64)).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('gap-set','company','branch','field_service_visit','issue-order','record','signed_job_sheet',1,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('gap-link','gap-set','gap-source','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,scanner_signature,scanner_definition_at,completed_at,created_at) VALUES('gap-scan','gap-source',1,'clean','synthetic-signature',$1,$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_status_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic status failure'; END $$; CREATE TRIGGER fixture_status_failure BEFORE INSERT ON attachment_access_events FOR EACH ROW EXECUTE FUNCTION fixture_status_failure()").execute(db).await.unwrap();
        let gap=||Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-links/gap-link/withdraw").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key","fixture-gap-key").body(Body::from(r#"{"reason":"Corrected evidence"}"#)).unwrap();
        assert_eq!(runtime.router().oneshot(gap()).await.unwrap().status(),StatusCode::INTERNAL_SERVER_ERROR);
        let committed:(String,i64)=sqlx::query_as("SELECT l.state,(SELECT count(*) FROM attachment_events WHERE link_id=l.id AND action='link.withdrawn') FROM attachment_links l WHERE l.id='gap-link'").fetch_one(db).await.unwrap();assert_eq!(committed,("withdrawn".into(),1));
        sqlx::raw_sql("DROP TRIGGER fixture_status_failure ON attachment_access_events; DROP FUNCTION fixture_status_failure()").execute(db).await.unwrap();
        let response=runtime.router().oneshot(gap()).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let data:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(data["data"]["scan"]["lastResult"],"clean");assert_eq!(data["data"]["scan"]["signature"],"synthetic-signature");let audit:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_access_events WHERE link_id='gap-link' AND variant='status' AND outcome='allowed'").fetch_one(db).await.unwrap();assert_eq!(audit,1);
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('files-terminal-set','company','branch','field_service_visit','reserve-order','record','technician_report',1,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('files-terminal','files-terminal-set','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();let cancel=r#"{"id":"reserve-order","action":"cancel","expectedVersion":1,"reason":"Synthetic cancellation"}"#;let outcome=send(&runtime.router(),&cookie,"fixture-cancel-file",cancel).await;assert_eq!(outcome.0,StatusCode::OK,"{}",outcome.1);assert_eq!(runtime.router().oneshot(other("files-terminal")).await.unwrap().status(),StatusCode::CONFLICT);let state:String=sqlx::query_scalar("SELECT state FROM attachment_links WHERE id='files-terminal'").fetch_one(db).await.unwrap();assert_eq!(state,"draft");
        sqlx::query("INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at) VALUES('files-purchase','files-test-set','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        withdraw(&p,"service").await;system.scenario.clock.0.store(now+10000,Ordering::SeqCst);assert_ne!(runtime.router().oneshot(other("files-purchase")).await.unwrap().status(),StatusCode::OK);let state:String=sqlx::query_scalar("SELECT state FROM attachment_links WHERE id='files-purchase'").fetch_one(db).await.unwrap();assert_eq!(state,"draft");
        sqlx::query("UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=$1").bind(&child.result.session_id).execute(db).await.unwrap();let stale=Request::builder().uri("/api/v1/holdings/acme/attachment-links/files-test-link/status").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::empty()).unwrap();assert_ne!(runtime.router().oneshot(stale).await.unwrap().status(),StatusCode::OK);runtime.close().await;
    }).await;
}

// Component fixture: bytes and scan result are deterministic; real Clam is qualified separately.
#[cfg(feature = "private-development-api")]
#[path = "../../support/files/scan-facts.rs"]
#[allow(dead_code)]
mod bind_scan_facts;

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_field_attachment_bind_replays_component_scanned_manifest() {
    use erpx_server_files::public::attachment_scan::*;
    use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
    use erpx_server_shared::public::RecordId;
    use sha2::{Digest, Sha256};
    account::with_system(|system| async move {
        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        system.scenario.clock.0.store(now,Ordering::SeqCst);
        let p=setup_workspace(system).await; let system=&p.review.system; let db=p.pool().raw();
        let checksum="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(hex::encode(Sha256::digest(b"abc")),checksum);
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('bind-set','company','branch','field_service_visit','issue-order','record','technician_report',3,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,quarantine_key,status,created_by,expires_at,created_at,updated_at,company_id,detected_media_type) VALUES('bind-upload','field_service_visit','issue-order','technician_report','component.csv','text/csv',3,3,$1,'quarantine/bind','quarantine/bind','quarantine/bind','uploaded','fixture-a',$2,$3,$3,'company','text/csv')").bind(checksum).bind(now+3600000).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state,detected_media_type) VALUES('attachment-bind-upload','bind-upload','field_service_visit','issue-order','technician_report','quarantine/bind',1,'component.csv','text/csv',3,$1,'fixture-a',$2,'company','quarantined','text/csv')").bind(checksum).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,upload_intent_id,state,created_by,created_at,expires_at) VALUES('bind-link','bind-set','attachment-bind-upload','bind-upload','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        sqlx::query("UPDATE upload_intents SET attachment_set_id='bind-set',attachment_link_id='bind-link' WHERE id='bind-upload'").execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES('bind-original','company','attachment-bind-upload','original','quarantine/bind',3,'stored',$1)").bind(now).execute(db).await.unwrap();
        let details=serde_json::json!({"checksum":checksum,"bytes":3,"detectedMediaType":"text/csv"}).to_string();
        sqlx::query("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES('bind-origin','company','attachment-bind-upload','bind-link','bind-set','fixture-a','file.quarantined',$1,$2)").bind(details).bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_scan_attempts(id,attachment_id,attempt,state,created_at) VALUES('bind-scan','attachment-bind-upload',1,'pending',$1)").bind(now).execute(db).await.unwrap();
        let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(db).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",child.token.expose());let mut cfg=private_configuration::document(&system.control,&system.holdings).await;
        cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldAttachmentBinds"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);cfg["fieldServiceVisitRecoveries"]=serde_json::json!(["h001"]);cfg["fieldServiceOrderCancellations"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let post=|key:&str,version:i64|Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-sets/bind-set/bind").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key",key).body(Body::from(serde_json::json!({"expectedTargetVersion":version}).to_string())).unwrap();
        let post_set=|set:&str,key:&str,version:i64|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/attachment-sets/{set}/bind")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").header("idempotency-key",key).body(Body::from(serde_json::json!({"expectedTargetVersion":version}).to_string())).unwrap();
        let default_cfg={let mut c=cfg.clone();c.as_object_mut().unwrap().remove("fieldAttachmentBinds");c};
        let default_closed=assemble_development_api(DevelopmentApiConfiguration::parse(&default_cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_ne!(default_closed.router().oneshot(post("bind-default-closed",3)).await.unwrap().status(),StatusCode::OK);default_closed.close().await;
        let missing=Request::builder().method("POST").uri("/api/v1/holdings/acme/attachment-sets/bind-set/bind").header("host","acme.example.test").header("cookie",&cookie).header("idempotency-key","missing-cas-key").body(Body::from(r#"{"expectedTargetVersion":3}"#)).unwrap();assert_eq!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::CONFLICT);
        assert_eq!(runtime.router().oneshot(post("bind-pending-key",3)).await.unwrap().status(),StatusCode::CONFLICT);
        let clock=system.scenario.clock.clone();
        PgUnitOfWork::new(p.pool().clone()).run::<(),BusinessError,_>(TransactionOptions::default(),move |c| {let clock=clock.clone();Box::pin(async move {
            let scope=AttachmentScanScope{holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap()};
            let id=RecordId::new("attachment-bind-upload").unwrap();
            let claim=claim_attachment_scan(c,&scope,Some(&id),&*clock,&mut bind_scan_facts::Ids).await?.expect("pending source");
            let prepared=prepare_current_attachment_scan(c,claim,&*clock).await?;
            let evidence=AttachmentScanEvidence::Clean{engine:"deterministic component provider".into(),definition_updated_at:1,preview:None};
            let retained=retain_current_attachment_scan(c,&prepared,&evidence,&*clock,&mut bind_scan_facts::Ids).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;retained.require_current(c,&*clock).await?;Ok(())
        })}).await.unwrap();
        assert_eq!(runtime.router().oneshot(post("bind-stale-key",2)).await.unwrap().status(),StatusCode::CONFLICT);
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('bind-foreign','other-company','branch','field_service_visit','issue-order','record','technician_report',3,1,'confidential','{}','draft','fixture-a',$1,$1),('bind-withdrawn','company','branch','field_service_visit','issue-order','record','signed_job_sheet',3,1,'confidential','{}','draft','fixture-a',$1,$1),('bind-terminal','company','branch','field_service_visit','reserve-order','record','technician_report',2,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,withdrawn_at,withdrawn_by,withdrawal_reason) VALUES('bind-withdrawn-link','bind-withdrawn','attachment-bind-upload','withdrawn','fixture-a',$1,$2,$1,'fixture-a','component withdrawal')").bind(now).bind(now+3600000).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('bind-terminal-link','bind-terminal','attachment-bind-upload','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        assert_eq!(runtime.router().oneshot(post_set("bind-foreign","bind-foreign-key",3)).await.unwrap().status(),StatusCode::NOT_FOUND);
        assert_ne!(runtime.router().oneshot(post_set("bind-withdrawn","bind-withdrawn-key",3)).await.unwrap().status(),StatusCode::OK);
        let cancelled=send(&runtime.router(),&cookie,"bind-test-cancel",r#"{"id":"reserve-order","action":"cancel","expectedVersion":1,"reason":"Synthetic terminal state"}"#).await;assert_eq!(cancelled.0,StatusCode::OK,"{}",cancelled.1);
        assert_ne!(runtime.router().oneshot(post_set("bind-terminal","bind-terminal-key",2)).await.unwrap().status(),StatusCode::OK);
        let readonly_cfg={let mut c=cfg.clone();c["fieldAttachmentBinds"]=serde_json::json!([{"holdingId":"h001","mode":"read-only"}]);c};
        let readonly=assemble_development_api(DevelopmentApiConfiguration::parse(&readonly_cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        assert_eq!(readonly.router().oneshot(post("bind-readonly-key",3)).await.unwrap().status(),StatusCode::SERVICE_UNAVAILABLE);readonly.close().await;
        sqlx::raw_sql("CREATE FUNCTION bind_late_source_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_parties SET name='Unexpected late customer' WHERE id='customer'; RETURN NEW; END $$; CREATE TRIGGER bind_late_source_drift AFTER INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='set.bound') EXECUTE FUNCTION bind_late_source_drift()").execute(db).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("bind-drift-key",3)).await.unwrap().status(),StatusCode::OK);
        let rolled_back:(String,String,i64,i64)=sqlx::query_as("SELECT s.state,p.name,o.version,(SELECT count(*) FROM command_receipts WHERE command_id='bind-drift-key') FROM attachment_sets s JOIN field_service_orders o ON o.id=s.target_id JOIN finance_parties p ON p.id='customer' WHERE s.id='bind-set'").fetch_one(db).await.unwrap();assert_eq!(rolled_back.0,"draft");assert_ne!(rolled_back.1,"Unexpected late customer");assert_eq!((rolled_back.2,rolled_back.3),(3,0));
        sqlx::raw_sql("DROP TRIGGER bind_late_source_drift ON attachment_events; DROP FUNCTION bind_late_source_drift()").execute(db).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION bind_late_identity_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET valid_until=valid_until-1 WHERE id='field-fixture-a'; RETURN NEW; END $$; CREATE TRIGGER bind_late_identity_drift AFTER INSERT ON attachment_events FOR EACH ROW WHEN (NEW.action='set.bound') EXECUTE FUNCTION bind_late_identity_drift()").execute(db).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("bind-identity-key",3)).await.unwrap().status(),StatusCode::OK);
        let identity_rolled_back:(String,i64,i64)=sqlx::query_as("SELECT s.state,o.version,(SELECT count(*) FROM command_receipts WHERE command_id='bind-identity-key') FROM attachment_sets s JOIN field_service_orders o ON o.id=s.target_id WHERE s.id='bind-set'").fetch_one(db).await.unwrap();assert_eq!(identity_rolled_back,("draft".into(),3,0));
        sqlx::raw_sql("DROP TRIGGER bind_late_identity_drift ON attachment_events; DROP FUNCTION bind_late_identity_drift()").execute(db).await.unwrap();
        let (a,b)=tokio::join!(runtime.router().oneshot(post("bind-retry-key",3)),runtime.router().oneshot(post("bind-retry-key",3)));
        let a=a.unwrap();let b=b.unwrap();let ast=a.status();let bst=b.status();let ab=to_bytes(a.into_body(),65536).await.unwrap();let bb=to_bytes(b.into_body(),65536).await.unwrap();
        assert_eq!(ast,StatusCode::OK,"{}",String::from_utf8_lossy(&ab));assert_eq!(bst,StatusCode::OK,"{}",String::from_utf8_lossy(&bb));
        let value:serde_json::Value=serde_json::from_slice(&ab).unwrap();assert_eq!(value["data"]["manifest"][0]["checksumSha256"],checksum);assert_eq!(value["data"]["manifest"][0]["bytes"],3);
        assert_eq!(runtime.router().oneshot(post("bind-retry-key",2)).await.unwrap().status(),StatusCode::CONFLICT);
        let effects:(String,String,i64,i64,i64,i64)=sqlx::query_as("SELECT s.state,l.state,o.version,t.service_order_version,(SELECT count(*) FROM field_service_order_events WHERE id='service-file-event-bind-retry-key'),(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.bound') FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN field_service_orders o ON o.id=s.target_id JOIN support_tickets t ON t.id=o.ticket_id WHERE s.id='bind-set'").fetch_one(db).await.unwrap();assert_eq!(effects,("bound".into(),"bound".into(),4,4,1,1));
        let command_effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='bind-retry-key' AND operation='attachment.record.bind' AND record_id='issue-order' AND actor_id='fixture-a'),(SELECT count(*) FROM audit_events WHERE command_id='bind-retry-key' AND action='attachment.record.bind' AND record_version=4),(SELECT count(*) FROM outbox_events WHERE command_id='bind-retry-key' AND event_type='attachment.record.bind' AND aggregate_version=4)").fetch_one(db).await.unwrap();assert_eq!(command_effects,(1,1,1));
        let transition=send(&runtime.router(),&cookie,"bind-later-exception",r#"{"id":"issue-order","action":"exception","expectedVersion":4,"reason":"Synthetic later progress"}"#).await;assert_eq!(transition.0,StatusCode::OK,"{}",transition.1);
        assert_eq!(runtime.router().oneshot(post("bind-retry-key",3)).await.unwrap().status(),StatusCode::OK);
        sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,state,created_by,created_at,updated_at) VALUES('bind-purchase','company','branch','field_service_visit','issue-order','record','technician_report',5,1,'confidential','{}','draft','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
        sqlx::query("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('bind-purchase-link','bind-purchase','attachment-bind-upload','draft','fixture-a',$1,$2)").bind(now).bind(now+3600000).execute(db).await.unwrap();
        withdraw(&p,"service").await;system.scenario.clock.0.store(now+10000,Ordering::SeqCst);
        assert_ne!(runtime.router().oneshot(post_set("bind-purchase","bind-purchase-key",5)).await.unwrap().status(),StatusCode::OK);
        let purchase_unchanged:(String,i64)=sqlx::query_as("SELECT state,(SELECT count(*) FROM command_receipts WHERE command_id='bind-purchase-key') FROM attachment_sets WHERE id='bind-purchase'").fetch_one(db).await.unwrap();assert_eq!(purchase_unchanged,("draft".into(),0));
        sqlx::query("UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=$1").bind(&child.result.session_id).execute(db).await.unwrap();
        assert_ne!(runtime.router().oneshot(post("bind-retry-key",3)).await.unwrap().status(),StatusCode::OK);
        runtime.close().await;
    }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_field_source_record_preserves_modes_and_current_scope() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);
  let p=setup_workspace(system).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,title,issue,owner_id,due_date,source_kind,created_by,created_at,updated_at) VALUES('source-general','company','branch','General case','General issue','fixture-a','2027-12-31','general','fixture-a',$1,$1)").bind(now).execute(db).await.unwrap();
  sqlx::query("INSERT INTO field_service_orders SELECT (jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||jsonb_build_object('id','source-invalid','ticket_id','source-general'))).* FROM field_service_orders o WHERE id='issue-order'").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(db).await.unwrap();
  let cookie=format!("__Host-erpx-product={}",child.token.expose());let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"enforced"}]);
  let enforced=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let get=|id:&str,query:&str|Request::builder().uri(format!("/api/v1/holdings/acme/field-service/orders/{id}/source-record{query}")).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::empty()).unwrap();
  let response=enforced.router().oneshot(get("issue-order","?version=3")).await.unwrap();let status=response.status();let body=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&body));let value:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(value["data"]["id"],"issue-order");assert_eq!(value["data"]["version"],3);assert_eq!(value["data"]["companyId"],"company");assert_eq!(value["data"]["title"],"Synthetic service issue");
  let response=enforced.router().oneshot(get("issue-order","?version=4")).await.unwrap();assert_eq!(response.status(),StatusCode::CONFLICT);let body=to_bytes(response.into_body(),65536).await.unwrap();let value:serde_json::Value=serde_json::from_slice(&body).unwrap();assert_eq!(value["error"]["code"],"SOURCE_RECORD_CHANGED");
  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('other-branch','branch','other','Other branch','company','company','other-branch','active')").execute(db).await.unwrap();
  let outside=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/issue-order/source-record").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-workspace-branch","other").body(Body::empty()).unwrap();assert_eq!(enforced.router().oneshot(outside).await.unwrap().status(),StatusCode::NOT_FOUND);
  assert_eq!(enforced.router().oneshot(get("source-invalid","")).await.unwrap().status(),StatusCode::NOT_FOUND);assert_eq!(enforced.router().oneshot(get("issue-order","?version=0")).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
  let finance_cookie=format!("__Host-erpx-product={}",p.review.reviewer_token.expose());let unauthorized=Request::builder().uri("/api/v1/holdings/acme/field-service/orders/issue-order/source-record").header("host","acme.example.test").header("cookie",finance_cookie).body(Body::empty()).unwrap();assert_eq!(enforced.router().oneshot(unauthorized).await.unwrap().status(),StatusCode::FORBIDDEN);
  let default_cfg={let mut v=cfg.clone();v.as_object_mut().unwrap().remove("fieldServiceSourceRecordReads");v};let closed=assemble_development_api(DevelopmentApiConfiguration::parse(&default_cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_ne!(closed.router().oneshot(get("issue-order","")).await.unwrap().status(),StatusCode::OK);closed.close().await;
  let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
  erpx_server_field_service::public::source_record::lock_source_record(&mut tx).await.unwrap();erpx_server_support::public::field_service_workspace::lock_service_workspace_tickets(&mut tx).await.unwrap();
  let company=CompanyId::new("company").unwrap();let field=erpx_server_field_service::public::source_record::read_source_record(&mut tx,&company,None,None,"issue-order").await.unwrap();
  sqlx::query("UPDATE field_service_orders SET issue='Unexpected late change' WHERE id='issue-order'").execute(&mut *tx).await.unwrap();assert!(field.require_current(&mut tx).await.is_err());
  let absent=erpx_server_support::public::field_service_source_record::read_field_source_ticket(&mut tx,&company,"source-late-ticket").await.unwrap();assert!(absent.data().is_none());
  sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,title,issue,owner_id,due_date,source_kind,created_by,created_at,updated_at) VALUES('source-late-ticket','company','branch','Late source','Issue','fixture-a','2027-12-31','general','fixture-a',$1,$1)").bind(now).execute(&mut *tx).await.unwrap();assert!(absent.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
  withdraw(&p,"service").await;system.scenario.clock.0.store(now+10000,Ordering::SeqCst);assert_eq!(enforced.router().oneshot(get("issue-order","")).await.unwrap().status(),StatusCode::OK);enforced.close().await;
  {
   use erpx_server_commercial::public::purchases::{CapabilityPurpose,PurchaseRequest,PgCompanyPurchaseReader,current_package_catalogue,lock_commercial_authority_sources};
   use erpx_service::field_service_source_record::{SourceRecordMode,observe_source_purchase,write_source_purchase_observation};
   let mut tx=db.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_commercial_authority_sources(&mut tx).await.unwrap();
   let catalogue=current_package_catalogue().unwrap();let holding=HoldingId::new("h001").unwrap();let company=CompanyId::new("company").unwrap();let mut observations=Vec::new();
   for purpose in [CapabilityPurpose::NewWork,CapabilityPurpose::SettleExisting,CapabilityPurpose::History] {
    let inspected=PgCompanyPurchaseReader::new(holding.clone(),company.clone(),&mut tx,&catalogue).inspect(&PurchaseRequest{capabilities:vec!["service.orders".into()],purpose,mode:PackageEnforcementMode::Shadow},system.scenario.clock.now()).await.unwrap();
    assert!(inspected.require_allowed().is_ok());
    let before=observations.len();observe_source_purchase(SourceRecordMode::Disabled,&company,&inspected,&mut |v|observations.push(v));observe_source_purchase(SourceRecordMode::Enforced,&company,&inspected,&mut |v|observations.push(v));assert_eq!(observations.len(),before);
    observe_source_purchase(SourceRecordMode::Shadow,&company,&inspected,&mut |v|observations.push(v));
   }
   assert_eq!(observations.len(),3);assert_eq!(observations.iter().map(|x|x.purpose).collect::<Vec<_>>(),["new-work","settle-existing","history"]);assert_ne!(observations[0].code,"GRANTED");assert_ne!(observations[1].code,"GRANTED");assert_eq!(observations[2].code,"GRANTED");
   let item=serde_json::to_value(&observations[0]).unwrap();assert_eq!(item.as_object().unwrap().len(),7);assert_eq!(item["event"],"commercial.purchase.shadow");assert_eq!(item["phase"],"read");assert_eq!(item["companyId"],"company");assert_eq!(item["capability"],"service.orders");assert_eq!(item["purchaseRevision"].as_str().unwrap().len(),64);
   struct FailingOutput;impl std::io::Write for FailingOutput{fn write(&mut self,_:&[u8])->std::io::Result<usize>{Err(std::io::Error::other("synthetic sink failure"))}fn flush(&mut self)->std::io::Result<()>{Ok(())}}write_source_purchase_observation(&mut FailingOutput,&observations[0]);tx.rollback().await.unwrap();
  }
  for mode in ["disabled","shadow"]{cfg["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":mode}]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(runtime.router().oneshot(get("source-invalid","")).await.unwrap().status(),StatusCode::OK);assert_eq!(runtime.router().oneshot(get("issue-order","")).await.unwrap().status(),StatusCode::OK);runtime.close().await;}
  sqlx::query("UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=$1").bind(&child.result.session_id).execute(db).await.unwrap();
  cfg["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"disabled"}]);let stale=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_ne!(stale.router().oneshot(get("issue-order","")).await.unwrap().status(),StatusCode::OK);stale.close().await;
 }).await;
}

#[tokio::test]
#[cfg(feature = "private-development-api")]
async fn private_field_source_record_requires_actual_asset_history() {
    account::with_system(|system|async move{
  let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;system.scenario.clock.0.store(now,Ordering::SeqCst);let p=setup_workspace_without_parts(system).await;let system=&p.review.system;let db=p.pool().raw();
  sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at) VALUES('source-asset','company','branch','SOURCE','Source asset','equipment','THB','fixture-a',0,0); INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('source-sale','company','branch','sales','SOURCE-SALE','customer','warehouse','THB','2026-01-01','2026-12-31',100,0,'fixture-a',0,0); INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES('source-sale-ticket','company','branch','source-sale','customer','Source ticket','Issue','fixture-a','2027-12-31','fixture-a',0,0); INSERT INTO field_service_orders SELECT (jsonb_populate_record(NULL::field_service_orders,to_jsonb(o)||jsonb_build_object('id','source-asset-order','ticket_id','source-sale-ticket','asset_id','source-asset'))).* FROM field_service_orders o WHERE id='issue-order'").execute(db).await.unwrap();
  let central=account::login(system,"a@example.test",None).await;let child=account::child(system,&central,account::ACME).await;sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3600000).bind(now).execute(db).await.unwrap();
  let cookie=format!("__Host-erpx-product={}",child.token.expose());let get=||Request::builder().uri("/api/v1/holdings/acme/field-service/orders/source-asset-order/source-record").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
  let mut cfg=private_configuration::document(&system.control,&system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"enforced"}]);let enforced=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(enforced.router().oneshot(get()).await.unwrap().status(),StatusCode::NOT_FOUND);
  let mut grant=db.begin().await.unwrap();sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','service-parts-assets',1,'active',0,1,'Synthetic asset history','public synthetic evidence',$1)").bind(now).execute(&mut *grant).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'source-asset-grant',company_id,package_id,'fixture-a','synthetic-source-asset',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='service-parts-assets'").bind(now).execute(&mut *grant).await.unwrap();grant.commit().await.unwrap();assert_eq!(enforced.router().oneshot(get()).await.unwrap().status(),StatusCode::OK);enforced.close().await;
  cfg["fieldServiceSourceRecordReads"]=serde_json::json!([{"holdingId":"h001","mode":"disabled"}]);let disabled=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(disabled.router().oneshot(get()).await.unwrap().status(),StatusCode::OK);disabled.close().await;
 }).await;
}

#[tokio::test]
async fn service_posting_history_owner_images_match_original_postgres_text() {
    use erpx_server_commerce::public::service_parts_posting::{
        lock_service_parts_posting_sources, read_service_parts_posting,
    };
    use erpx_server_field_service::public::service_parts_posting::{
        lock_service_parts_field_history, read_service_parts_field_history,
    };
    use erpx_server_finance::public::service_parts_posting::{
        lock_service_parts_posting_references, read_service_parts_posting_references,
    };
    use erpx_server_inventory::public::service_parts_posting::{
        lock_service_parts_inventory_history, read_service_parts_inventory_history,
    };
    use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
    use erpx_service::service_parts_posting_setup::{PostingHistoryImages, retained_history_image};
    account::with_system(|system| async move {
        let p=setup_workspace(system).await;
        PgUnitOfWork::new(p.pool().clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_parts_posting_sources(c).await?;
            lock_service_parts_inventory_history(c).await?;
            lock_service_parts_field_history(c).await?;
            lock_service_parts_posting_references(c).await?;
            let company=CompanyId::new("company").unwrap();
            let commerce=read_service_parts_posting(c,&company).await?;
            let inventory=read_service_parts_inventory_history(c,&company,&commerce.facts().warehouse_ids).await?;
            let field=read_service_parts_field_history(c,&company).await?;
            let finance=read_service_parts_posting_references(c,&company,None).await?;
            let i=inventory.data();let f=field.data();let h=&finance.data().history_image;
            let actual=retained_history_image(c,&PostingHistoryImages {orders:&commerce.facts().commerce_orders_json,stock:&i.stock_json,ledger:&i.ledger_json,counts:&i.counts_json,consumptions:&i.consumptions_json,invoices:h.invoices.as_deref(),payments:h.payments.as_deref(),journals:h.journals.as_deref(),service:&f.service_json}).await?;
            let expected:String=sqlx::query_scalar("SELECT jsonb_build_object('orders',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM commerce_orders source WHERE source.company_id=$1),'stock',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.warehouse_id,source.product_id) FROM commerce_stock source JOIN commerce_warehouses warehouse ON warehouse.id=source.warehouse_id WHERE warehouse.company_id=$1),'ledger',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM inventory_ledger source JOIN commerce_warehouses warehouse ON warehouse.id=source.warehouse_id WHERE warehouse.company_id=$1),'counts',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM inventory_counts source WHERE source.company_id=$1),'consumptions',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM operational_inventory_consumptions source WHERE source.company_id=$1),'invoices',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM finance_invoices source WHERE source.company_id=$1),'payments',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM finance_payments source WHERE source.company_id=$1),'journals',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM finance_journals source WHERE source.company_id=$1),'service',(SELECT jsonb_agg(to_jsonb(source) ORDER BY source.id) FROM field_service_billing_releases source JOIN field_service_orders service ON service.id=source.order_id WHERE service.company_id=$1))::text").bind("company").fetch_one(&mut *c).await?;
            assert_eq!(actual,expected);
            commerce.require_current(c).await?;inventory.require_current(c).await?;field.require_current(c).await?;finance.require_current(c).await?;
            Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_posting_setup_reads_actual_selected_finance_and_owner_sources() {
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_service::service_parts_posting_setup::{
        ServicePostingEnvironment, ServicePostingSources, read_service_posting_setup,
    };
    account::with_system(|system| async move {
        let p = setup_workspace(system).await;
        sqlx::query("UPDATE active_work_contexts SET assignment_id='finance-fixture-a',revision=7 WHERE user_id='fixture-a'")
            .execute(p.pool().raw()).await.unwrap();
        let clock = p.review.system.scenario.clock.clone();
        let view = p
            .review
            .authorizer
            .execute_with_sources(
                "acme.example.test",
                p.review.maker.clone(),
                p.review.maker_selection.clone(),
                WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServicePostingSources,
                move |c, current| {
                    let clock = clock.clone();
                    Box::pin(async move {
                        read_service_posting_setup(
                            c,
                            current,
                            clock.as_ref(),
                            ServicePostingEnvironment::Development,
                        )
                        .await
                    })
                },
            )
            .await
            .unwrap();
        assert_eq!(view.company_id, "company");
        assert!(view.available);
        assert!(view.schema_available);
        assert_eq!(view.configured, Some(true));
        assert!(view.context_key.is_some());
    })
    .await;
}

async fn service_posting_finance_fixture(system: account::System) -> review::Prepared {
    let p = review::setup(system).await;
    let now = p.system.scenario.clock.now().get();
    let pool = &p.system.holdings[0];
    sqlx::raw_sql("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES ('part-ar','company','chart','111','AR','asset','THB','receivable',0,0),('part-ap','company','chart','211','AP','liability','THB','payable',0,0),('part-revenue','company','chart','411','Revenue','revenue','THB','none',0,0),('part-inventory','company','chart','121','Inventory','asset','THB','none',0,0),('part-clearing','company','chart','221','Clearing','liability','THB','none',0,0),('part-cogs','company','chart','511','COGS','expense','THB','none',0,0),('part-input','company','chart','131','Input','asset','THB','none',0,0),('part-output','company','chart','231','Output','liability','THB','none',0,0),('part-cash','company','chart','141','Cash','asset','THB','bank',0,0)").execute(pool.raw()).await.unwrap();
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','service',1,'active',0,1,'Synthetic source','synthetic',$1),('company','service-parts-assets',1,'active',0,1,'Synthetic source','synthetic',$1)").bind(now).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'setup-origin-'||package_id,company_id,package_id,'fixture-a','synthetic',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id IN('service','service-parts-assets')").bind(now).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    p
}

#[tokio::test]
async fn service_posting_setup_initializes_and_replays_actual_owner_transaction() {
    use erpx_server_commerce::public::service_parts_posting::PostingMode;
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_server_workflow::public::commands::CommandJson;
    use erpx_service::service_parts_posting_setup::{
        ServicePostingEnvironment, ServicePostingSources, execute_service_posting_setup,
        normalize_service_posting_command, read_service_posting_setup,
    };
    account::with_system(|system|async move{
        let p=service_posting_finance_fixture(system).await;let pool=&p.system.holdings[0];
        let access=||WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true};
        let clock=p.system.scenario.clock.clone();let c=clock.clone();
        let view=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=c.clone();Box::pin(async move{read_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development).await})}).await.unwrap();
        assert!(view.can_configure);let context=view.context_key.unwrap();
        let input=CommandJson::parse(&format!(r#"{{"expectedVersion":0,"contextKey":"{context}","chartId":"chart","currency":"THB","reason":"Set up parts","receivableAccountId":"part-ar","payableAccountId":"part-ap","revenueAccountId":"part-revenue","inventoryAccountId":"part-inventory","clearingAccountId":"part-clearing","cogsAccountId":"part-cogs","inputTaxAccountId":"part-input","outputTaxAccountId":"part-output","cashAccountId":"part-cash"}}"#)).unwrap();
        let input=normalize_service_posting_command(&input,PostingMode::Initialize).unwrap();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','posting_readonly','Synthetic Finance reader',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','posting_readonly',1,'[\"finance.read\",\"finance.approve\",\"module.finance.read\"]','[]','[\"holding\"]','synthetic',0)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('posting-reader-b','fixture-b','posting_readonly','[\"finance\"]','company',$1,$2,3,'holding')").bind(p.system.scenario.clock.now().get()-1000).bind(p.system.scenario.clock.now().get()+3_600_000).execute(pool.raw()).await.unwrap();
        sqlx::query("UPDATE active_work_contexts SET assignment_id='posting-reader-b',revision=8 WHERE user_id='fixture-b'").execute(pool.raw()).await.unwrap();
        let mut readonly_selection=p.reviewer_selection.clone();readonly_selection.expected=Some(erpx_server_identity_access::public::work_context::ExpectedWorkSelection{revision:8,assignment_id:erpx_server_shared::public::AssignmentId::new("posting-reader-b").unwrap(),assignment_version:3});
        let denied_input=input.clone();let denied_clock=clock.clone();
        let denied=p.authorizer.execute_with_sources("acme.example.test",p.reviewer.clone(),readonly_selection,access(),ServicePostingSources,move|db,current|{let clock=denied_clock.clone();let input=denied_input.clone();Box::pin(async move{execute_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development,"setup-readonly",&input).await})}).await;
        assert!(matches!(denied,Err(PgTransactionError::Business(e)) if e.kind==FailureKind::Forbidden));
        sqlx::raw_sql("CREATE FUNCTION fixture_setup_finance_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='setup-rollback' THEN UPDATE finance_charts SET name='changed' WHERE id='chart'; END IF; RETURN NEW; END $$; CREATE TRIGGER fixture_setup_finance_drift AFTER INSERT ON commerce_service_parts_posting_sources FOR EACH ROW EXECUTE FUNCTION fixture_setup_finance_drift()").execute(pool.raw()).await.unwrap();
        let drift_input=input.clone();let drift_clock=clock.clone();
        let failed=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=drift_clock.clone();let input=drift_input.clone();Box::pin(async move{execute_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development,"setup-rollback",&input).await})}).await;
        assert!(matches!(failed,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
        let rollback:(i64,i64,i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_settings WHERE company_id='company'),(SELECT count(*) FROM commerce_service_parts_posting_sources WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id='setup-rollback'),(SELECT count(*) FROM outbox_events WHERE command_id='setup-rollback'),(SELECT name FROM finance_charts WHERE id='chart')").fetch_one(pool.raw()).await.unwrap();assert_eq!(rollback,(0,0,0,0,"Primary".into()));
        sqlx::raw_sql("DROP TRIGGER fixture_setup_finance_drift ON commerce_service_parts_posting_sources; DROP FUNCTION fixture_setup_finance_drift()").execute(pool.raw()).await.unwrap();
        for attempt in 0..2 {let input=input.clone();let clock=clock.clone();let result=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=clock.clone();let input=input.clone();Box::pin(async move{execute_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development,"setup-real-test",&input).await})}).await.unwrap();assert_eq!(result.replayed,attempt==1);}
        let counts:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_settings WHERE company_id='company'),(SELECT count(*) FROM commerce_service_parts_posting_sources WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id='setup-real-test'),(SELECT count(*) FROM audit_events WHERE command_id='setup-real-test'),(SELECT count(*) FROM outbox_events WHERE command_id='setup-real-test')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(1,1,1,1,1));
    }).await;
}

#[tokio::test]
async fn service_posting_setup_confirms_existing_mapping_without_rewriting_it() {
    use erpx_server_commerce::public::service_parts_posting::PostingMode;
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_server_workflow::public::commands::CommandJson;
    use erpx_service::service_parts_posting_setup::{
        ServicePostingEnvironment, ServicePostingSources, execute_service_posting_setup,
        normalize_service_posting_command, read_service_posting_setup,
    };
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let pool=&p.system.holdings[0];
        sqlx::query("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','part-ar','part-ap','part-revenue','part-inventory','part-clearing','part-cogs','part-input','part-output','part-cash')").execute(pool.raw()).await.unwrap();
        let before:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap();
        let access=||WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true};
        let clock=p.system.scenario.clock.clone();let c=clock.clone();
        let view=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=c.clone();Box::pin(async move{read_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development).await})}).await.unwrap();
        assert!(view.can_confirm_existing);let context=view.context_key.unwrap();
        let body=CommandJson::parse(&format!(r#"{{"expectedVersion":0,"contextKey":"{context}","reason":"Confirm existing"}}"#)).unwrap();
        let input=normalize_service_posting_command(&body,PostingMode::ConfirmExisting).unwrap();
        let foreign_input=input.clone();let foreign_clock=clock.clone();
        let foreign=p.authorizer.execute_with_sources("northstar.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=foreign_clock.clone();let input=foreign_input.clone();Box::pin(async move{execute_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development,"setup-foreign",&input).await})}).await;
        assert!(foreign.is_err());
        for attempt in 0..2 {let input=input.clone();let clock=clock.clone();let result=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePostingSources,move|db,current|{let clock=clock.clone();let input=input.clone();Box::pin(async move{execute_service_posting_setup(db,current,clock.as_ref(),ServicePostingEnvironment::Development,"setup-existing",&input).await})}).await.unwrap();assert_eq!(result.replayed,attempt==1);assert_eq!(result.result.status,"confirmed-existing");}
        let after:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id='company'").fetch_one(pool.raw()).await.unwrap();assert_eq!(before,after);
        let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_service_parts_posting_sources WHERE company_id='company' AND mode='confirm-existing'),(SELECT count(*) FROM command_receipts WHERE command_id='setup-existing'),(SELECT count(*) FROM audit_events WHERE command_id='setup-existing'),(SELECT count(*) FROM outbox_events WHERE command_id='setup-existing')").fetch_one(pool.raw()).await.unwrap();assert_eq!(counts,(1,1,1,1));
    }).await;
}

#[tokio::test]
async fn service_posting_setup_selected_actor_without_finance_gets_unavailable_view() {
    use erpx_service::service_parts_posting_setup::{
        ServicePostingEnvironment, ServicePostingTarget, read_selected_service_posting_setup,
    };
    account::with_system(|system| async move {
        let p = setup_workspace(system).await;
        let result = read_selected_service_posting_setup(
            &p.review.authorizer,
            "acme.example.test",
            p.review.maker.clone(),
            {
                let selection = p.selection.clone();
                move || Ok(selection)
            },
            ServicePostingTarget {
                holding: HoldingId::new("h001").unwrap(),
                company: CompanyId::new("company").unwrap(),
                clock: p.review.system.scenario.clock.clone(),
                environment: ServicePostingEnvironment::Development,
            },
        )
        .await
        .unwrap();
        assert!(!result.data.available);
        assert_eq!(result.data.unavailable_reason, Some("authority-required"));
        assert_eq!(result.data.company_id, "company");
    })
    .await;
}

#[cfg(feature = "private-development-api")]
#[tokio::test]
async fn service_posting_setup_private_http_selected_actor_replay_and_context() {
    account::with_system(|system| async move {
        let p = service_posting_finance_fixture(system).await;
        let cookie = format!("__Host-erpx-product={}", p.reviewer_token.expose());
        let mut cfg = private_configuration::document(&p.system.control, &p.system.holdings).await;
        cfg["fieldServiceReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        let off = assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let get = || Request::builder().uri("/api/v1/holdings/acme/field-service/parts-setup").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        assert_eq!(off.router().oneshot(get()).await.unwrap().status(), StatusCode::NOT_FOUND);
        off.close().await;
        cfg["fieldServicePartsPostingSetups"] = serde_json::json!(["h001"]);
        let runtime = assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let response = runtime.router().oneshot(get()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let reply:serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
        assert_eq!(reply["context"]["userId"],"fixture-b");
        let context=reply["data"]["contextKey"].as_str().unwrap();
        let body=format!(r#"{{"expectedVersion":0,"contextKey":"{context}","chartId":"chart","currency":"THB","reason":"Set up parts","receivableAccountId":"part-ar","payableAccountId":"part-ap","revenueAccountId":"part-revenue","inventoryAccountId":"part-inventory","clearingAccountId":"part-clearing","cogsAccountId":"part-cogs","inputTaxAccountId":"part-input","outputTaxAccountId":"part-output","cashAccountId":"part-cash"}}"#);
        let post=|revision:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/parts-setup").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","setup-http-test").header("x-erpx-context-company","company").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").body(Body::from(body.clone())).unwrap();
        assert_eq!(runtime.router().oneshot(post("8")).await.unwrap().status(),StatusCode::CONFLICT);
        let missing=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/parts-setup").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","setup-missing-context").body(Body::from(body.clone())).unwrap();
        assert_ne!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::OK);
        let before:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_service_parts_posting_sources WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id IN ('setup-http-test','setup-missing-context'))").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(before,(0,0));
        for _ in 0..2 {let response=runtime.router().oneshot(post("7")).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(value["data"]["status"],"configured");assert_eq!(value["context"]["userId"],"fixture-b");}
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM commerce_service_parts_posting_sources WHERE company_id='company'),(SELECT count(*) FROM command_receipts WHERE command_id='setup-http-test'),(SELECT count(*) FROM outbox_events WHERE command_id='setup-http-test')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(1,1,1));
        let child=account::child(&p.system,&p.reviewer_central,account::ACME).await;let now=p.system.scenario.clock.now().get();let db=p.system.holdings[0].raw();
        sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('posting-branch','branch','posting-branch','Posting Branch','company','company','posting-branch','active')").execute(db).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('posting-branch-b','fixture-b','custom_finance_review','[\"finance\"]','company','posting-branch',$1,$2,3,'branch','posting-branch')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-b','company','posting-branch-b',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
        let branch_cookie=format!("__Host-erpx-product={}",child.token.expose());
        let branch=Request::builder().uri("/api/v1/holdings/acme/field-service/parts-setup").header("host","acme.example.test").header("cookie",branch_cookie).header("x-erpx-workspace-branch","posting-branch").body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(branch).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(value["data"]["available"],false);assert_eq!(value["data"]["unavailableReason"],"authority-required");
        runtime.close().await;
    }).await;
}

#[cfg(feature = "private-development-api")]
#[tokio::test]
async fn service_posting_setup_private_http_confirms_existing_mapping() {
    account::with_system(|system| async move {
        let p=service_posting_finance_fixture(system).await;
        let db=p.system.holdings[0].raw();
        sqlx::query("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','THB','part-ar','part-ap','part-revenue','part-inventory','part-clearing','part-cogs','part-input','part-output','part-cash')").execute(db).await.unwrap();
        let before:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id='company'").fetch_one(db).await.unwrap();
        let cookie=format!("__Host-erpx-product={}",p.reviewer_token.expose());
        let mut cfg=private_configuration::document(&p.system.control,&p.system.holdings).await;
        cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);cfg["fieldServicePartsPostingSetups"]=serde_json::json!(["h001"]);
        let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let req=Request::builder().uri("/api/v1/holdings/acme/field-service/parts-setup").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        let response=runtime.router().oneshot(req).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let view:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
        assert_eq!(view["data"]["configuration"]["chartId"],"chart");assert!(view["data"]["configuration"].get("chart_id").is_none());assert_eq!(view["data"]["canConfirmExisting"],true);
        let body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","reason":"Confirm existing"}}"#,view["data"]["contextKey"].as_str().unwrap());
        let post=||Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/parts-setup/existing").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key","setup-http-confirm").header("x-erpx-context-company","company").header("x-erpx-context-revision","7").header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").body(Body::from(body.clone())).unwrap();
        for _ in 0..2 { let response=runtime.router().oneshot(post()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let value:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),65536).await.unwrap()).unwrap();assert_eq!(value["data"]["status"],"confirmed-existing"); }
        let after:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id='company'").fetch_one(db).await.unwrap();assert_eq!(before,after);
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='setup-http-confirm'").fetch_one(db).await.unwrap();assert_eq!(count,1);
        runtime.close().await;
    }).await;
}

#[tokio::test]
async fn service_posting_context_matches_javascript_with_actual_same_unit_sources() {
    use erpx_server_commerce::public::service_parts_posting::read_service_parts_posting;
    use erpx_server_field_service::public::service_parts_posting::read_service_parts_field_history;
    use erpx_server_finance::public::service_parts_posting::read_service_parts_posting_references;
    use erpx_server_identity_access::public::{
        effective_authority::WorkAuthorizationRequest,
        selected_command_authority::{
            ServicePartsAuthorityPurpose, capture_service_parts_command_authority,
        },
    };
    use erpx_server_inventory::public::service_parts_posting::read_service_parts_inventory_history;
    use erpx_server_workflow::public::service_parts_posting::read_service_posting_receipt;
    use erpx_service::service_parts_posting_setup::{
        PostingHistoryImages, ServicePostingEnvironment, ServicePostingSources,
        read_service_posting_setup, retained_history_image,
    };
    use std::io::Write;
    use std::process::{Command, Stdio};
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Authority<'a> {
        session_id: &'a str,
        revision: i64,
        assignment_version: i64,
    }
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let clock=p.system.scenario.clock.clone();
        p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true},ServicePostingSources,move|c,current|{let clock=clock.clone();Box::pin(async move{
            let view=read_service_posting_setup(c,current,clock.as_ref(),ServicePostingEnvironment::Development).await?;
            let actor=capture_service_parts_command_authority(c,current,ServicePartsAuthorityPurpose::Read,clock.as_ref()).await?;
            let company=&current.selection().workspace().company_id;
            let commerce=read_service_parts_posting(c,company).await?;
            let inventory=read_service_parts_inventory_history(c,company,&commerce.facts().warehouse_ids).await?;
            let field=read_service_parts_field_history(c,company).await?;
            let finance=read_service_parts_posting_references(c,company,None).await?;
            let receipt=read_service_posting_receipt(c,company).await?;
            let inv=inventory.data();let fld=field.data();let fin=finance.data();let fh=&fin.history_image;
            let image=retained_history_image(c,&PostingHistoryImages{orders:&commerce.facts().commerce_orders_json,stock:&inv.stock_json,ledger:&inv.ledger_json,counts:&inv.counts_json,consumptions:&inv.consumptions_json,invoices:fh.invoices.as_deref(),payments:fh.payments.as_deref(),journals:fh.journals.as_deref(),service:&fld.service_json}).await?;
            let history=commerce.facts().has_commerce_history||inv.has_history||fld.has_history||fin.finance_history;
            let selection=current.selection();let grant=selection.assignment();
            let identity=CommandJson::from_serializable(&(company.as_str(),Option::<&str>::None,Option::<&str>::None,current.user_id().as_str(),grant.id.as_str(),grant.role.as_str(),Authority{session_id:current.session_id().as_str(),revision:selection.revision(),assignment_version:grant.version},actor.iam()))?;
            let mapping=commerce.facts().mapping_json.as_deref().unwrap_or("null");let proof=receipt.facts().map(|v|v.proof_json.as_str()).unwrap_or("null");
            let input=format!("[{},{{\"mapping\":{},\"proof\":{},\"history\":{},\"image\":{}}},{}]",identity.as_str(),mapping,proof,history,serde_json::to_string(&image).unwrap(),serde_json::to_string(&fin.reference_image).unwrap());
            let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());let mut child=Command::new(node).args(["-e","const [identity,state,refs]=JSON.parse(require('fs').readFileSync(0,'utf8')); process.stdout.write(require('crypto').createHash('sha256').update(JSON.stringify([JSON.stringify(identity),state,refs])).digest('hex'));"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("pinned Node oracle unavailable");child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();let result=child.wait_with_output().unwrap();assert!(result.status.success());assert_eq!(view.context_key.as_deref(),Some(std::str::from_utf8(&result.stdout).unwrap()));
            commerce.require_current(c).await?;inventory.require_current(c).await?;field.require_current(c).await?;finance.require_current(c).await?;receipt.require_current(c).await?;actor.require_current(c,clock.as_ref()).await?;
            Ok(())
        })}).await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_prerequisite_read_matches_actual_javascript_context_key() {
    use erpx_server_identity_access::public::{
        effective_authority::WorkAuthorizationRequest,
        selected_command_authority::{
            ServicePartsAuthorityPurpose, capture_service_parts_command_authority,
        },
    };
    use erpx_service::service_parts_posting_setup::ServicePostingEnvironment;
    use erpx_service::service_parts_prerequisite_setup::{
        ServicePrerequisiteSources, read_service_prerequisites,
    };
    use std::io::Write;
    use std::process::{Command, Stdio};
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Authority<'a> {
        session_id: &'a str,
        revision: i64,
        assignment_version: i64,
    }
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let clock=p.system.scenario.clock.clone();
        p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true},ServicePrerequisiteSources,move|c,current|{let clock=clock.clone();Box::pin(async move {
            let actual=read_service_prerequisites(c,current,clock.as_ref(),ServicePostingEnvironment::Development).await?;
            assert!(actual.available&&actual.can_create_chart&&actual.can_create_account&&actual.can_create_period&&actual.can_confirm_rate);
            let actor=capture_service_parts_command_authority(c,current,ServicePartsAuthorityPurpose::Read,clock.as_ref()).await?;
            let iam=serde_json::to_value(actor.iam()).unwrap();let selection=current.selection();let grant=selection.assignment();let company=&selection.workspace().company_id;
            let identity=CommandJson::from_serializable(&(current.holding_id().as_str(),company.as_str(),current.user_id().as_str(),grant.id.as_str(),grant.role.as_str(),Authority{session_id:current.session_id().as_str(),revision:selection.revision(),assignment_version:grant.version},iam.get("queuedAssignmentVersion"),iam.get("identityVersion"),iam.get("roleRevision"),iam.get("hrAppointmentVersion")))?;
            let charts:String=sqlx::query_scalar("SELECT COALESCE(json_agg(json_build_object('id',c.id,'name',c.name,'base_currency',c.base_currency,'status',c.status,'version',c.version,'source',to_jsonb(c)::text) ORDER BY c.id)::text,'[]') FROM finance_charts c WHERE c.company_id=$1").bind(company.as_str()).fetch_one(&mut *c).await?;
            let accounts:String=sqlx::query_scalar("SELECT COALESCE(json_agg(json_build_object('id',a.id,'chart_id',a.chart_id,'code',a.code,'name',a.name,'category',a.category,'currency',a.currency,'control_kind',a.control_kind,'status',a.status,'version',a.version,'source',to_jsonb(a)::text) ORDER BY a.id)::text,'[]') FROM finance_accounts a WHERE a.company_id=$1").bind(company.as_str()).fetch_one(&mut *c).await?;
            let periods:String=sqlx::query_scalar("SELECT COALESCE(json_agg(json_build_object('id',p.id,'name',p.name,'start_date',p.start_date,'end_date',p.end_date,'state',p.state,'version',p.close_version,'source',to_jsonb(p)::text) ORDER BY p.id)::text,'[]') FROM finance_periods p WHERE p.company_id=$1").bind(company.as_str()).fetch_one(&mut *c).await?;
            let rates:String=sqlx::query_scalar("SELECT COALESCE(json_agg(json_build_object('id',r.id,'currency',r.currency,'effective_date',r.effective_date::text,'currency_per_usd',r.currency_per_usd::text,'confirmed_at',r.confirmed_at,'source',to_jsonb(r)::text) ORDER BY r.id)::text,'[]') FROM company_fx_rates r WHERE r.company_id=$1").bind(company.as_str()).fetch_one(&mut *c).await?;
            let money:String=sqlx::query_scalar("SELECT COALESCE(json_agg(json_build_object('kind',kind,'source',source) ORDER BY kind)::text,'[]') FROM (SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=$1 UNION ALL SELECT 'scale:'||s.currency,to_jsonb(s)::text FROM company_currency_scales s WHERE s.company_id=$1) rows").bind(company.as_str()).fetch_one(&mut *c).await?;
            let image:String=sqlx::query_scalar("SELECT jsonb_build_object('orders',NULL,'stock',NULL,'ledger',NULL,'counts',NULL,'consumptions',NULL,'invoices',NULL,'payments',NULL,'journals',NULL,'service',NULL)::text").fetch_one(&mut *c).await?;
            let input=format!("[{},[{},{},{},{},{}],{}]",identity.as_str(),charts,accounts,periods,rates,money,serde_json::to_string(&image).unwrap());
            let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());let mut child=Command::new(&node).args(["-e","const [identity,rows,image]=JSON.parse(require('fs').readFileSync(0,'utf8')); const state={configured:false,history:false,chartId:null,mappingImage:'null',proofImage:'null',image,unsupportedExistingMapping:false}; process.stdout.write(require('crypto').createHash('sha256').update(JSON.stringify([...identity,JSON.stringify(rows),state])).digest('hex'));"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("pinned Node oracle unavailable");child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success());assert_eq!(actual.context_key.as_deref(),Some(std::str::from_utf8(&output.stdout).unwrap()));
            sqlx::raw_sql("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company','chart','USD','part-ar','part-ap','part-revenue','part-inventory','part-clearing','part-cogs','part-input','part-output','part-cash'); INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,created_by,created_at,updated_at) VALUES('prereq-history','company','chart','period','PREREQ-HISTORY','2026-01-01','THB','fixture-a',1,1)").execute(&mut *c).await?;
            let historical=read_service_prerequisites(c,current,clock.as_ref(),ServicePostingEnvironment::Development).await?;assert_eq!(historical.configured,Some(true));assert!(!historical.can_create_chart&&!historical.can_create_account);
            let mapping:String=sqlx::query_scalar("SELECT to_jsonb(s)::text FROM commerce_settings s WHERE company_id=$1").bind(company.as_str()).fetch_one(&mut *c).await?;
            let image:String=sqlx::query_scalar("SELECT jsonb_build_object('orders',NULL,'stock',NULL,'ledger',NULL,'counts',NULL,'consumptions',NULL,'invoices',NULL,'payments',NULL,'journals',(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM finance_journals j WHERE j.company_id=$1),'service',NULL)::text").bind(company.as_str()).fetch_one(&mut *c).await?;
            let historical_input=format!("[{},[{},{},{},{},{}],{},{}]",identity.as_str(),charts,accounts,periods,rates,money,serde_json::to_string(&image).unwrap(),mapping);
            let mut child=Command::new(&node).args(["-e","const [identity,rows,image,mapping]=JSON.parse(require('fs').readFileSync(0,'utf8'));const state={configured:true,history:true,chartId:mapping.chart_id,mappingImage:JSON.stringify(mapping),proofImage:'null',image,unsupportedExistingMapping:true};process.stdout.write(require('crypto').createHash('sha256').update(JSON.stringify([...identity,JSON.stringify(rows),state])).digest('hex'));"]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("pinned Node oracle unavailable");child.stdin.take().unwrap().write_all(historical_input.as_bytes()).unwrap();let output=child.wait_with_output().unwrap();assert!(output.status.success());assert_eq!(historical.context_key.as_deref(),Some(std::str::from_utf8(&output.stdout).unwrap()));actor.require_current(c,clock.as_ref()).await?;Ok(())
        })}).await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_prerequisite_read_rejects_unsafe_javascript_metadata() {
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_service::service_parts_posting_setup::ServicePostingEnvironment;
    use erpx_service::service_parts_prerequisite_setup::{
        ServicePrerequisiteSources, read_service_prerequisites,
    };
    account::with_system(|system| async move {
        let p = service_posting_finance_fixture(system).await;
        let clock = p.system.scenario.clock.clone();
        sqlx::query("UPDATE finance_periods SET close_version=9007199254740993 WHERE id='period'")
            .execute(p.system.holdings[0].raw()).await.unwrap();
        let read = || { let clock=clock.clone(); p.authorizer.execute_with_sources(
            "acme.example.test", p.maker.clone(), p.maker_selection.clone(),
            WorkAuthorizationRequest { permission:"finance.read".into(), module:None, destination:"services/field-service".into(), api:true },
            ServicePrerequisiteSources,
            move |c,current| { let clock=clock.clone(); Box::pin(async move {
                read_service_prerequisites(c,current,clock.as_ref(),ServicePostingEnvironment::Development).await
            }) },
        ) };
        assert!(read().await.is_err(), "unsafe period version must not be serialized");
        sqlx::raw_sql("UPDATE finance_periods SET close_version=0 WHERE id='period'; INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at) VALUES('unsafe-rate','company','THB','2026-01-01',2,'fixture-a',9007199254740993)")
            .execute(p.system.holdings[0].raw()).await.unwrap();
        assert!(read().await.is_err(), "unsafe rate confirmation must not be serialized");
    }).await;
}

#[tokio::test]
async fn service_prerequisite_commands_apply_replay_and_rollback_actual_owners() {
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_service::service_parts_posting_setup::ServicePostingEnvironment;
    use erpx_service::service_parts_prerequisite_setup::{
        ServicePrerequisiteSources, ServicePrerequisiteWriteSources, execute_service_prerequisite,
        normalize_service_prerequisite, read_service_prerequisites,
    };
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let clock=p.system.scenario.clock.clone();
        let access=||WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true};
        let read=||{let c=clock.clone();p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteSources,move|db,current|{let c=c.clone();Box::pin(async move{read_service_prerequisites(db,current,c.as_ref(),ServicePostingEnvironment::Development).await})})};
        let view=read().await.unwrap();assert!(view.can_create_chart);
        let chart_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"name":"New synthetic chart","baseCurrency":"thb"}}}}"#,view.context_key.unwrap());
        let chart=normalize_service_prerequisite(&CommandJson::parse(&chart_body).unwrap(),"chart").unwrap();
        let run=|key:&str,input:erpx_service::service_parts_prerequisite_setup::NormalizedPrerequisiteCommand|{let c=clock.clone();let key=key.to_owned();let variant=input.input.variant();p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteWriteSources(variant),move|db,current|{let c=c.clone();let key=key.clone();let input=input.clone();Box::pin(async move{execute_service_prerequisite(db,current,c.as_ref(),&key,&input).await})})};
        let created=run("prereq-chart",chart.clone()).await.unwrap();assert!(!created.replayed);let chart_value:serde_json::Value=serde_json::from_str(created.result.as_str()).unwrap();let chart_id=chart_value["id"].as_str().unwrap().to_owned();assert_eq!(chart_value["baseCurrency"],"THB");assert_eq!(chart_value["accountingBasis"],"IFRS");assert!(run("prereq-chart",chart.clone()).await.unwrap().replayed);
        let changed=normalize_service_prerequisite(&CommandJson::parse(&chart_body.replace("New synthetic chart","Changed chart")).unwrap(),"chart").unwrap();assert!(matches!(run("prereq-chart",changed).await,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let view=read().await.unwrap();let account_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"chartId":"{}","code":"121","name":"Synthetic bank","category":"asset","controlKind":"bank"}}}}"#,view.context_key.unwrap(),chart_id);
        let account=normalize_service_prerequisite(&CommandJson::parse(&account_body).unwrap(),"account").unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_charts SET name='changed by trigger' WHERE id='chart'; RETURN NEW; END $$; CREATE TRIGGER fixture_prereq_drift AFTER INSERT ON finance_accounts FOR EACH ROW EXECUTE FUNCTION fixture_prereq_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let failed=run("prereq-rollback",account.clone()).await;assert!(matches!(failed,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
        let counts:(i64,i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='prereq-rollback'),(SELECT count(*) FROM finance_accounts WHERE name='Synthetic bank'),(SELECT name FROM finance_charts WHERE id='chart')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0,"Primary".into()));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_drift ON finance_accounts; DROP FUNCTION fixture_prereq_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_deferred_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE finance_accounts SET updated_at=updated_at+1 WHERE id=NEW.id; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER fixture_prereq_deferred_drift AFTER INSERT ON finance_accounts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fixture_prereq_deferred_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        assert!(matches!(run("prereq-deferred-drift",account.clone()).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id='prereq-deferred-drift'),(SELECT count(*) FROM finance_accounts WHERE name='Synthetic bank')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_deferred_drift ON finance_accounts; DROP FUNCTION fixture_prereq_deferred_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let result=run("prereq-account",account.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["currency"],"THB");assert_eq!(value["isControl"],false);assert!(run("prereq-account",account).await.unwrap().replayed);
        let view=read().await.unwrap();let context=view.context_key.unwrap();let period_body=format!(r#"{{"expectedVersion":0,"contextKey":"{context}","input":{{"name":"Future period","startDate":"2031-01-01","endDate":"2031-12-31"}}}}"#);let period=normalize_service_prerequisite(&CommandJson::parse(&period_body).unwrap(),"period").unwrap();
        let stale=normalize_service_prerequisite(&CommandJson::parse(&period_body.replace(&context,&"0".repeat(64))).unwrap(),"period").unwrap();assert!(matches!(run("prereq-stale",stale).await,Err(PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_CONTEXT_CHANGED"));
        sqlx::raw_sql("CREATE FUNCTION fixture_prereq_iam_drift() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET version=version+1 WHERE id='finance-fixture-a'; RETURN NEW; END $$; CREATE TRIGGER fixture_prereq_iam_drift AFTER INSERT ON finance_periods FOR EACH ROW EXECUTE FUNCTION fixture_prereq_iam_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        assert!(matches!(run("prereq-iam-drift",period.clone()).await,Err(PgTransactionError::Business(_))));let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('prereq-stale','prereq-iam-drift')),(SELECT count(*) FROM finance_periods WHERE name='Future period'),(SELECT version FROM assignments WHERE id='finance-fixture-a')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(0,0,3));
        sqlx::raw_sql("DROP TRIGGER fixture_prereq_iam_drift ON finance_periods; DROP FUNCTION fixture_prereq_iam_drift()").execute(p.system.holdings[0].raw()).await.unwrap();
        let result=run("prereq-period",period.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["version"],0);assert_eq!(value["state"],"open");assert!(run("prereq-period",period).await.unwrap().replayed);
        let view=read().await.unwrap();let rate_body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"currency":"THB","effectiveDate":"2027-01-01","currencyPerUsd":"35.125"}}}}"#,view.context_key.unwrap());let rate=normalize_service_prerequisite(&CommandJson::parse(&rate_body).unwrap(),"rate").unwrap();let result=run("prereq-rate",rate.clone()).await.unwrap();let value:serde_json::Value=serde_json::from_str(result.result.as_str()).unwrap();assert_eq!(value["currencyPerUsd"],"35.125");assert!(run("prereq-rate",rate).await.unwrap().replayed);
        assert!(run("prereq-chart",chart.clone()).await.unwrap().replayed);
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate')),(SELECT count(*) FROM audit_events WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate')),(SELECT count(*) FROM outbox_events WHERE command_id IN ('prereq-chart','prereq-account','prereq-period','prereq-rate'))").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(4,4,4));
        let mut draining=p.system.holdings[0].raw().begin().await.unwrap();let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence) FROM company_package_grants WHERE company_id='company' AND package_id='service'").fetch_one(&mut *draining).await.unwrap();
        sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1 WHERE company_id='company' AND package_id='service'").execute(&mut *draining).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'prereq-draining','company',package_id,'fixture-a','synthetic-draining',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$2 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(before).bind(clock.now().get()).execute(&mut *draining).await.unwrap();draining.commit().await.unwrap();
        assert!(run("prereq-chart",chart).await.unwrap().replayed);let view=read().await.unwrap();assert!(!view.can_create_chart&&!view.can_confirm_rate);
        let new_rate=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"currency":"THB","effectiveDate":"2028-01-01","currencyPerUsd":"36"}}}}"#,view.context_key.unwrap());let new_rate=normalize_service_prerequisite(&CommandJson::parse(&new_rate).unwrap(),"rate").unwrap();assert!(matches!(run("prereq-draining-new",new_rate).await,Err(PgTransactionError::Business(e)) if e.kind==FailureKind::Forbidden));
    }).await;
}

#[tokio::test]
async fn service_prerequisite_commands_preserve_approve_only_and_legacy_authority() {
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    use erpx_service::service_parts_posting_setup::ServicePostingEnvironment;
    use erpx_service::service_parts_prerequisite_setup::{
        ServicePrerequisiteSources, ServicePrerequisiteWriteSources, execute_service_prerequisite,
        normalize_service_prerequisite, read_service_prerequisites,
    };
    account::with_system(|system|async move{
        let p=service_posting_finance_fixture(system).await;let db=p.system.holdings[0].raw();let clock=p.system.scenario.clock.clone();
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','prereq_approver','Synthetic Finance approver',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','prereq_approver',1,'[\"finance.read\",\"finance.approve\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0); UPDATE assignments SET role='prereq_approver' WHERE id='finance-fixture-b'; INSERT INTO finance_commands(command_id,company_id,operation,request_hash,created_at) VALUES('prereq-legacy','company','finance.chart.create','synthetic',1)").execute(db).await.unwrap();
        let access=||WorkAuthorizationRequest{permission:"finance.read".into(),module:None,destination:"services/field-service".into(),api:true};
        let c=clock.clone();let view=p.authorizer.execute_with_sources("acme.example.test",p.reviewer.clone(),p.reviewer_selection.clone(),access(),ServicePrerequisiteSources,move|db,current|{let c=c.clone();Box::pin(async move{read_service_prerequisites(db,current,c.as_ref(),ServicePostingEnvironment::Development).await})}).await.unwrap();assert!(!view.can_create_chart&&view.can_confirm_rate);
        let key=view.context_key.unwrap();let rate_body=format!(r#"{{"expectedVersion":0,"contextKey":"{key}","input":{{"currency":"THB","effectiveDate":"2027-02-01","currencyPerUsd":"35"}}}}"#);let rate=normalize_service_prerequisite(&CommandJson::parse(&rate_body).unwrap(),"rate").unwrap();let c=clock.clone();let result=p.authorizer.execute_with_sources("acme.example.test",p.reviewer.clone(),p.reviewer_selection.clone(),access(),ServicePrerequisiteWriteSources(rate.input.variant()),move|db,current|{let c=c.clone();let input=rate.clone();Box::pin(async move{execute_service_prerequisite(db,current,c.as_ref(),"prereq-approve-only",&input).await})}).await.unwrap();assert!(!result.replayed);
        let chart_body=format!(r#"{{"expectedVersion":0,"contextKey":"{key}","input":{{"name":"Forbidden chart","baseCurrency":"THB"}}}}"#);let chart=normalize_service_prerequisite(&CommandJson::parse(&chart_body).unwrap(),"chart").unwrap();let c=clock.clone();let forbidden=p.authorizer.execute_with_sources("acme.example.test",p.reviewer.clone(),p.reviewer_selection.clone(),access(),ServicePrerequisiteWriteSources(chart.input.variant()),move|db,current|{let c=c.clone();let input=chart.clone();Box::pin(async move{execute_service_prerequisite(db,current,c.as_ref(),"prereq-forbidden",&input).await})}).await;assert!(matches!(forbidden,Err(PgTransactionError::Business(e)) if e.kind==FailureKind::Forbidden));
        let c=clock.clone();let view=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteSources,move|db,current|{let c=c.clone();Box::pin(async move{read_service_prerequisites(db,current,c.as_ref(),ServicePostingEnvironment::Development).await})}).await.unwrap();let body=format!(r#"{{"expectedVersion":0,"contextKey":"{}","input":{{"name":"Legacy collision","baseCurrency":"THB"}}}}"#,view.context_key.unwrap());let chart=normalize_service_prerequisite(&CommandJson::parse(&body).unwrap(),"chart").unwrap();let c=clock.clone();let legacy=p.authorizer.execute_with_sources("acme.example.test",p.maker.clone(),p.maker_selection.clone(),access(),ServicePrerequisiteWriteSources(chart.input.variant()),move|db,current|{let c=c.clone();let input=chart.clone();Box::pin(async move{execute_service_prerequisite(db,current,c.as_ref(),"prereq-legacy",&input).await})}).await;assert!(matches!(legacy,Err(PgTransactionError::Business(e)) if e.code=="IDEMPOTENCY_CONFLICT"));
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('prereq-forbidden','prereq-legacy')),(SELECT count(*) FROM finance_charts WHERE name IN ('Forbidden chart','Legacy collision'))").fetch_one(db).await.unwrap();assert_eq!(counts,(0,0));
    }).await;
}

#[cfg(feature = "private-development-api")]
#[tokio::test]
async fn service_prerequisite_private_http_all_four_and_exact_replay() {
    account::with_system(|system|async move {
        let p=service_posting_finance_fixture(system).await;let cookie=format!("__Host-erpx-product={}",p.reviewer_token.expose());
        let mut cfg=private_configuration::document(&p.system.control,&p.system.holdings).await;cfg["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        let get=||Request::builder().uri("/api/v1/holdings/acme/field-service/parts-setup/prerequisites").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();
        let off=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(off.router().oneshot(get()).await.unwrap().status(),StatusCode::NOT_FOUND);off.close().await;
        cfg["fieldServicePartsPrerequisites"]=serde_json::json!(["h001"]);let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&cfg.to_string()).unwrap(),p.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
        let mut chart_id=String::new();
        for (kind,key) in [("charts","http-prereq-chart"),("accounts","http-prereq-account"),("periods","http-prereq-period"),("rates","http-prereq-rate")] {
            let response=runtime.router().oneshot(get()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let view:serde_json::Value=serde_json::from_slice(&to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();assert_eq!(view["context"]["userId"],"fixture-b");let context=view["data"]["contextKey"].as_str().unwrap();
            let input=match kind {"charts"=>serde_json::json!({"name":"HTTP chart","baseCurrency":"THB"}),"accounts"=>serde_json::json!({"chartId":chart_id,"code":"A1","name":"HTTP account","category":"asset","controlKind":"bank"}),"periods"=>serde_json::json!({"name":"HTTP period","startDate":"2032-01-01","endDate":"2032-12-31"}),_=>serde_json::json!({"currency":"THB","effectiveDate":"2028-01-01","currencyPerUsd":"36"})};
            let body=serde_json::json!({"expectedVersion":0,"contextKey":context,"input":input}).to_string();
            let post=|revision:&str|Request::builder().method("POST").uri(format!("/api/v1/holdings/acme/field-service/parts-setup/{kind}")).header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",&cookie).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-revision",revision).header("x-erpx-context-assignment","finance-fixture-b").header("x-erpx-context-assignment-version","3").body(Body::from(body.clone())).unwrap();
            if kind=="charts"{assert_eq!(runtime.router().oneshot(post("8")).await.unwrap().status(),StatusCode::CONFLICT);let missing=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/parts-setup/charts").header("host","acme.example.test").header("cookie",&cookie).header("idempotency-key","http-prereq-missing").body(Body::from(body.clone())).unwrap();assert_ne!(runtime.router().oneshot(missing).await.unwrap().status(),StatusCode::OK);let count:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id IN ('http-prereq-chart','http-prereq-missing')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(count,0);}
            let first=runtime.router().oneshot(post("7")).await.unwrap();assert_eq!(first.status(),StatusCode::OK);let result:serde_json::Value=serde_json::from_slice(&to_bytes(first.into_body(),65536).await.unwrap()).unwrap();if kind=="charts"{chart_id=result["data"]["id"].as_str().unwrap().into();}assert_eq!(result["context"]["userId"],"fixture-b");
            let replay=runtime.router().oneshot(post("7")).await.unwrap();assert_eq!(replay.status(),StatusCode::OK);let again:serde_json::Value=serde_json::from_slice(&to_bytes(replay.into_body(),65536).await.unwrap()).unwrap();assert_eq!(result["data"],again["data"]);
        }
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id LIKE 'http-prereq-%'),(SELECT count(*) FROM audit_events WHERE command_id LIKE 'http-prereq-%'),(SELECT count(*) FROM outbox_events WHERE command_id LIKE 'http-prereq-%')").fetch_one(p.system.holdings[0].raw()).await.unwrap();assert_eq!(counts,(4,4,4));runtime.close().await;
    }).await;
}
