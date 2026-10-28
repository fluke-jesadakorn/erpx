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
use erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext;
use erpx_server_runtime::public::postgres::{PersistenceError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_workflow::public::commands::CommandJson;
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
