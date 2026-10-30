//! The actual Field-only private assembly over leased PostgreSQL and loopback HTTP.
#[allow(dead_code)]
#[path = "../../support/rust-account-revocation.rs"]
mod account;
#[allow(dead_code)]
#[path = "../../support/runtime/private-session-config.rs"]
mod configuration;
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_server_shared::public::Clock;
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api,
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn status(address: std::net::SocketAddr, path: &str, host: &str, cookie: &str) -> u16 {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test]
async fn field_only_private_listener_enforces_config_and_current_authority() {
    account::with_system(|system| async move {
        let proof = account::login(&system, "a@example.test", None).await;
        let child = account::child(&system, &proof, account::ACME).await;
        let mut doc = configuration::document(&system.control, &system.holdings).await;
        doc["listen"] = "127.0.0.1:43122".into();
        let mut unsupported = doc.clone();
        unsupported["financeJournalReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        unsupported["financeJournalPosts"] = serde_json::json!(["h001"]);
        assert_eq!(DevelopmentApiConfiguration::parse(&unsupported.to_string()).is_ok(), cfg!(feature = "business-http"));
        doc["fieldServiceReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        doc["fieldServiceWorkspaceReads"] = serde_json::json!(["h001"]);
        doc["financeJournalReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
        doc["fieldServiceReservations"] = serde_json::json!(["h001"]);
        doc["fieldServiceIssues"] = serde_json::json!(["h001"]);
        let db = system.holdings[0].raw();
        let now = system.scenario.clock.now().get();
        sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES('company','company','acme','Synthetic Acme',NULL,'company','active'); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','field_reader','Synthetic Field reader',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','field_reader',1,'[\"records.read\",\"module.field_service.read\"]','[]','[\"holding\"]','synthetic',0); INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('field-person','fixture-a','company','Synthetic reader','TH','fixture-a',0,0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company','fixture-a','local:fixture-a','field-person','active','local',1)").execute(db).await.unwrap();
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('field-reader','fixture-a','field_reader','[\"field_service\"]','company',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
        sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-reader',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
        let mut tx = db.begin().await.unwrap();
        sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','service',1,'active',0,1,'Synthetic source','synthetic',$1)").bind(now).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'field-origin-service',company_id,package_id,'fixture-a','synthetic',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id='service'").bind(now).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        let cfg = DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
        let runtime = assemble_development_api(
            cfg,
            system.scenario.clock.clone(),
            Arc::new(SystemAuthenticationSecrets),
        )
        .await
        .unwrap();
        let address = runtime.listen_address();
        let listener = tokio::net::TcpListener::bind(address)
            .await
            .expect("owned test port must be free");
        let server = tokio::spawn(axum::serve(listener, runtime.router()).into_future());
        let cookie = format!("__Host-erpx-product={}", child.token.expose());
        assert_eq!(
            status(address, "/api/health", "account.example.test", "").await,
            200
        );
        assert_ne!(
            status(
                address,
                "/api/v1/holdings/acme/field-service/workspace",
                "acme.example.test",
                ""
            )
            .await,
            200
        );
        let positive = tokio::time::timeout(std::time::Duration::from_secs(10), status(address, "/api/v1/holdings/acme/field-service/workspace", "acme.example.test", &cookie)).await;
        let finance = status(address, "/api/v1/holdings/acme/finance/journals/unrelated", "acme.example.test", &cookie).await;
        assert_ne!(
            status(
                address,
                "/api/v1/holdings/acme/field-service/workspace",
                "other.example.test",
                &cookie
            )
            .await,
            200
        );
        server.abort();
        let _ = server.await;
        runtime.close().await;
        assert_eq!(positive.expect("workspace response timed out"), 200);
        assert_eq!(finance, 403);
    })
    .await;
}

#[tokio::test]
async fn assembled_private_count_routes_reject_anonymous_before_reading_body() {
 use std::sync::atomic::{AtomicUsize,Ordering};
 use tower::ServiceExt;
 struct Held(Arc<AtomicUsize>);
 impl tokio::io::AsyncRead for Held {
  fn poll_read(self:std::pin::Pin<&mut Self>,_:&mut std::task::Context<'_>,_:&mut tokio::io::ReadBuf<'_>)->std::task::Poll<std::io::Result<()>> {self.0.fetch_add(1,Ordering::SeqCst);std::task::Poll::Pending}
 }
 account::with_system(|system|async move{
  let mut doc=configuration::document(&system.control,&system.holdings).await;
  doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
  doc["fieldServiceInitialCounts"]=serde_json::json!([{"holdingId":"h001","filesEnabled":false,"filesReadOnly":false,"uploadsFrozen":false}]);
  let cfg=DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
  let runtime=assemble_development_api(cfg,system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let base="/api/v1/holdings/acme/field-service/initial-physical-stock";
  let mut paths=vec![format!("{base}/id/update")];
  for action in ["flow","submit","approve","post","reject","cancel"] {paths.push(format!("{base}/id/stock-count/{action}"));}
  for path in paths {
   let polls=Arc::new(AtomicUsize::new(0));let body=axum::body::Body::from_stream(tokio_util::codec::FramedRead::new(Held(polls.clone()),tokio_util::codec::BytesCodec::new()));
   let request=axum::http::Request::builder().method("POST").uri(&path).header("host","acme.example.test").header("idempotency-key","held").body(body).unwrap();
   let response=tokio::time::timeout(std::time::Duration::from_secs(5),runtime.router().oneshot(request)).await.unwrap().unwrap();
   assert_eq!(response.status(),axum::http::StatusCode::UNAUTHORIZED,"{path}");assert_eq!(polls.load(Ordering::SeqCst),0,"{path}");
  }
  let detail=format!("{base}/id/stock-count");
  let request=axum::http::Request::builder().method("GET").uri(&detail).header("host","acme.example.test").body(axum::body::Body::empty()).unwrap();
  let response=runtime.router().oneshot(request).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::UNAUTHORIZED,"{detail}");
  for (method,path) in [("POST",detail),("POST",format!("{base}/id/stock-count/unknown")),("GET",format!("{base}/id/update")),("DELETE",format!("{base}/id/stock-count/flow"))] {
   let request=axum::http::Request::builder().method(method).uri(&path).header("host","acme.example.test").body(axum::body::Body::empty()).unwrap();
   let response=runtime.router().oneshot(request).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::NOT_FOUND,"{method} {path}");
  }
  runtime.close().await;
 }).await;
}
