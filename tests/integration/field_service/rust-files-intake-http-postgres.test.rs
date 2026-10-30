//! Real selected Account and Holding transaction for private Field intake HTTP.
#[allow(dead_code, unused_imports)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, HoldingId};
use erpx_service::{
    field_attachment_intake_http::{
        FieldIntakeMode, FieldIntakePolicy, FieldIntakeProviderReadiness,
        FieldIntakeProviderStatus, FieldIntakeTarget, field_intake_router,
    },
    field_service_reservation_read_http::FieldServiceReadScopeHeaders,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;
struct HeldBody(Arc<AtomicUsize>);
impl tokio::io::AsyncRead for HeldBody {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::task::Poll::Pending
    }
}
struct Provider {
    ready: AtomicBool,
    calls: AtomicUsize,
}
impl FieldIntakeProviderReadiness for Provider {
    fn probe(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<FieldIntakeProviderStatus, BusinessError>> + Send + '_>>
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let ready = self.ready.load(Ordering::SeqCst);
        Box::pin(async move {
            Ok(FieldIntakeProviderStatus {
                scanner_enabled: true,
                scanner_ready: ready,
                preview_ready: ready,
            })
        })
    }
}
#[tokio::test]
async fn private_route_enforces_account_and_replay_readiness() {
    facts::account::with_system(|system| async move {
  let p=facts::setup_workspace(system).await;
  let central=facts::account::login(&p.review.system,"a@example.test",None).await;
  let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
  let now=p.review.system.scenario.clock.now().get();
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
  let provider=Arc::new(Provider{ready:AtomicBool::new(true),calls:AtomicUsize::new(0)});
  let target=FieldIntakeTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),holding:HoldingId::new("h001").unwrap(),company:CompanyId::new("company").unwrap(),authorizer:p.review.authorizer.clone(),catalogue:Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap()),clock:p.review.system.scenario.clock.clone(),policy:FieldIntakePolicy{mode:FieldIntakeMode::Enabled,uploads_frozen:false},readiness:provider.clone()};
  let router=field_intake_router(Arc::new(facts::account::fixture::account::policy()),facts::account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),vec![target]).unwrap();
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let body=r#"{"targetType":"field_service_visit","targetId":"issue-order","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":3}"#;
  let post=|key:&str,body:&str|Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/attachment-sets").header("host","acme.example.test").header("cookie",&cookie).header("origin","https://acme.example.test").header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::from(body.to_owned())).unwrap();
  let reads=Arc::new(AtomicUsize::new(0));
  let held=|authenticated:bool| {let mut b=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/attachment-sets").header("host","acme.example.test").header("idempotency-key","held-body");if authenticated {b=b.header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","9");}b.body(Body::from_stream(tokio_util::codec::FramedRead::new(HeldBody(reads.clone()),tokio_util::codec::BytesCodec::new()))).unwrap()};
  for (auth,status) in [(false,StatusCode::UNAUTHORIZED),(true,StatusCode::CONFLICT)] {let r=tokio::time::timeout(std::time::Duration::from_secs(5),router.clone().oneshot(held(auth))).await.unwrap().unwrap();assert_eq!(r.status(),status);assert_eq!(reads.load(Ordering::SeqCst),0);}
  let anonymous_bad=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/attachment-sets").header("host","acme.example.test").header("idempotency-key","bad-body").body(Body::from("{")).unwrap();assert_eq!(router.clone().oneshot(anonymous_bad).await.unwrap().status(),StatusCode::UNAUTHORIZED);
  let stale_bad=Request::builder().method("POST").uri("/api/v1/holdings/acme/field-service/attachment-sets").header("host","acme.example.test").header("cookie",&cookie).header("idempotency-key","stale-bad").header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","9").body(Body::from("{")).unwrap();assert_eq!(router.clone().oneshot(stale_bad).await.unwrap().status(),StatusCode::CONFLICT);
  let stale_query=Request::builder().uri("/api/v1/holdings/acme/field-service/attachment-sets?targetType=other").header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","9").body(Body::empty()).unwrap();assert_eq!(router.clone().oneshot(stale_query).await.unwrap().status(),StatusCode::CONFLICT);
  let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM audit_events WHERE action='attachment.set.create')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(effects,(0,0));
  let bad_query=Request::builder().uri("/api/v1/holdings/acme/field-service/attachment-sets?targetType=other").header("host","acme.example.test").body(Body::empty()).unwrap();assert_eq!(router.clone().oneshot(bad_query).await.unwrap().status(),StatusCode::UNAUTHORIZED);
  assert_eq!(provider.calls.load(Ordering::SeqCst),0);

  let first=router.clone().oneshot(post("private-create",body)).await.unwrap();assert_eq!(first.status(),StatusCode::CREATED);let data:serde_json::Value=serde_json::from_slice(&to_bytes(first.into_body(),65536).await.unwrap()).unwrap();let typed:erpx_service::field_attachment_intake_contracts::FieldIntakeCreateResponse=serde_json::from_value(data.clone()).unwrap();assert_eq!(serde_json::to_value(typed).unwrap(),data);assert!(data["data"]["id"].is_string());assert_eq!(provider.calls.load(Ordering::SeqCst),1);
  provider.ready.store(false,Ordering::SeqCst);
  assert_eq!(router.clone().oneshot(post("private-create",body)).await.unwrap().status(),StatusCode::CREATED);assert_eq!(provider.calls.load(Ordering::SeqCst),1);
  assert_eq!(router.clone().oneshot(post("private-reuse",body)).await.unwrap().status(),StatusCode::SERVICE_UNAVAILABLE);assert_eq!(provider.calls.load(Ordering::SeqCst),2);
  sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('unsafe-existing','company','branch','field_service_visit','issue-order','record','site_photo',4,1,'internal','{}','fixture-a',9007199254740992,1)").execute(p.pool().raw()).await.unwrap();
  let corrupt_get=Request::builder().uri("/api/v1/holdings/acme/field-service/attachment-sets?targetType=field_service_visit&targetId=issue-order").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let corrupt_get=router.clone().oneshot(corrupt_get).await.unwrap();assert_eq!(corrupt_get.status(),StatusCode::INTERNAL_SERVER_ERROR);let error:serde_json::Value=serde_json::from_slice(&to_bytes(corrupt_get.into_body(),65536).await.unwrap()).unwrap();assert_eq!(error["error"]["code"],"INTERNAL_ERROR");
  sqlx::query("DELETE FROM attachment_sets WHERE id='unsafe-existing'").execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('other-version','company','branch','field_service_visit','issue-order','record','site_photo',4,1,'internal','{}','fixture-a',1,1)").execute(p.pool().raw()).await.unwrap();
  let get=Request::builder().uri("/api/v1/holdings/acme/field-service/attachment-sets?targetType=field_service_visit&targetId=issue-order").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let list=router.clone().oneshot(get).await.unwrap();assert_eq!(list.status(),StatusCode::OK);let data:serde_json::Value=serde_json::from_slice(&to_bytes(list.into_body(),65536).await.unwrap()).unwrap();assert_eq!(data["data"].as_array().unwrap().len(),2);
  for (number,count) in [("0x3",1),("0b11",1),("%EF%BB%BF3",1),("%C2%853",2),("junk",2),("NaN",2),("0",2),("-0",2),("%20",2),("4",1)] {let uri=format!("/api/v1/holdings/acme/field-service/attachment-sets?targetType=field_service_visit&targetId=issue-order&expectedTargetVersion={number}");let req=Request::builder().uri(uri).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let resp=router.clone().oneshot(req).await.unwrap();assert_eq!(resp.status(),StatusCode::OK,"{number}");let data:serde_json::Value=serde_json::from_slice(&to_bytes(resp.into_body(),65536).await.unwrap()).unwrap();assert_eq!(data["data"].as_array().unwrap().len(),count,"{number}");}
  for number in ["1.5","Infinity","9223372036854775808"] {let uri=format!("/api/v1/holdings/acme/field-service/attachment-sets?targetType=field_service_visit&targetId=issue-order&expectedTargetVersion={number}");let req=Request::builder().uri(uri).header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap();let resp=router.clone().oneshot(req).await.unwrap();assert_eq!(resp.status(),StatusCode::INTERNAL_SERVER_ERROR,"{number}");let data:serde_json::Value=serde_json::from_slice(&to_bytes(resp.into_body(),65536).await.unwrap()).unwrap();assert_eq!(data["error"]["code"],"INTERNAL_ERROR");}
  let invalid=post("private-invalid",&body.replace("field_service_visit","other"));assert_eq!(router.clone().oneshot(invalid).await.unwrap().status(),StatusCode::UNPROCESSABLE_ENTITY);
  provider.ready.store(true,Ordering::SeqCst);
  sqlx::query("INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES('unsafe-draft','company','branch','field_service_visit','issue-order','record','technician_report',3,1,'internal','{}','fixture-a',9007199254740992,1)").execute(p.pool().raw()).await.unwrap();
  sqlx::query("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES('unsafe-draft-event','company','unsafe-draft','fixture-a','set.created','{\"compatibilityAdapter\":false}',1)").execute(p.pool().raw()).await.unwrap();
  let corrupt_create=router.clone().oneshot(post("unsafe-created",&body.replace("site_photo","technician_report"))).await.unwrap();assert_eq!(corrupt_create.status(),StatusCode::INTERNAL_SERVER_ERROR);let error:serde_json::Value=serde_json::from_slice(&to_bytes(corrupt_create.into_body(),65536).await.unwrap()).unwrap();assert_eq!(error["error"]["code"],"INTERNAL_ERROR");
  let effects:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM attachment_sets WHERE target_id='issue-order'),(SELECT count(*) FROM attachment_events WHERE action='set.reused'),(SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events WHERE action='attachment.set.create'),(SELECT count(*) FROM outbox_events WHERE event_type='attachment.set.create')").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(effects,(3,0,1,1,1));

  let anonymous=Request::builder().uri("/api/v1/holdings/acme/field-service/attachment-sets?targetType=field_service_visit&targetId=issue-order").header("host","acme.example.test").body(Body::empty()).unwrap();assert_eq!(router.oneshot(anonymous).await.unwrap().status(),StatusCode::UNAUTHORIZED);
 }).await;
}
