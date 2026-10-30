//! Actual assembled private Field Files ingress and native scanner over leased DBs.
#[allow(dead_code)]
#[path = "../../support/runtime/private-session-config.rs"]
mod configuration;
#[allow(dead_code, unused_imports)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api, run_field_files_worker_file,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    future::IntoFuture,
    os::unix::fs::PermissionsExt,
    sync::{Arc, atomic::Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct AbortServer(tokio::task::JoinHandle<std::io::Result<()>>);
impl Drop for AbortServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn send(address: std::net::SocketAddr, request: Request<Body>) -> axum::http::Response<Body> {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 10 * 1024 * 1024).await.unwrap();
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let mut head = format!("{} {} HTTP/1.1\r\n", parts.method, parts.uri);
    for (name, value) in &parts.headers {
        head.push_str(&format!("{}: {}\r\n", name, value.to_str().unwrap()));
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let split = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let header = std::str::from_utf8(&bytes[..split]).unwrap();
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let mut builder = axum::http::Response::builder().status(status);
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':').unwrap();
        if name.eq_ignore_ascii_case("transfer-encoding")
            && value.trim().eq_ignore_ascii_case("chunked")
        {
            chunked = true;
        }
        builder = builder.header(name, value.trim());
    }
    let raw = &bytes[split + 4..];
    let decoded = if chunked {
        let mut out = Vec::new();
        let mut pos = 0;
        loop {
            let end = raw[pos..].windows(2).position(|w| w == b"\r\n").unwrap() + pos;
            let size = usize::from_str_radix(
                std::str::from_utf8(&raw[pos..end])
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap(),
                16,
            )
            .unwrap();
            pos = end + 2;
            if size == 0 {
                break;
            }
            out.extend_from_slice(&raw[pos..pos + size]);
            pos += size + 2;
        }
        out
    } else {
        raw.to_vec()
    };
    builder.body(Body::from(decoded)).unwrap()
}

#[tokio::test]
async fn actual_private_field_files_and_worker_publish_only_scanned_content() {
    facts::account::with_system(|system|async move {
  let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64;
  system.scenario.clock.0.store(now,Ordering::SeqCst);
  let p=facts::setup_workspace(system).await;
  let central=facts::account::login(&p.review.system,"a@example.test",None).await;
  let child=facts::account::child(&p.review.system,&central,facts::account::ACME).await;
  sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
  let root=tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap();
  let store=root.path().join("files");let tmp=root.path().join("temporary");for path in [&store,&tmp]{fs::create_dir(path).unwrap();fs::set_permissions(path,fs::Permissions::from_mode(0o700)).unwrap();}
  let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
  let mut doc=configuration::document(&p.review.system.control,&p.review.system.holdings).await;
  doc["listen"]=address.to_string().into();
  doc["fieldServiceReads"]=serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
  doc["fieldAttachmentLinks"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);
  doc["fieldAttachmentBinds"]=serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);
  doc["fieldFilesPolicy"]=serde_json::json!({"mode":"enabled","uploadsFrozen":false});
  doc["fieldFilesProviders"]=serde_json::json!({"targets":[{"holdingId":"h001","companyId":"company","filesBinding":"HOLDING_H001_FILES","filesRoot":store,"temporaryRoot":tmp,"previewBinary":std::env::var("ERPX_FILES_PREVIEW_BIN").unwrap(),"heifConvert":std::env::var("ERPX_HEIF_CONVERT").unwrap(),"pdftoppm":std::env::var("ERPX_PDFTOPPM_BIN").unwrap(),"clamavSocket":std::env::var("ERPX_CLAMD_SOCKET").unwrap(),"clamavDatabaseDirectory":std::env::var("ERPX_CLAMAV_DATABASE_DIR").unwrap(),"maximumDefinitionAgeMs":86400000,"commandTimeoutMs":5000,"scanTimeoutMs":30000,"concurrentScans":1}]});
  let config_path=root.path().join("private.json");fs::write(&config_path,serde_json::to_vec(&doc).unwrap()).unwrap();fs::set_permissions(&config_path,fs::Permissions::from_mode(0o600)).unwrap();
  let cfg=DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap();
  let runtime=assemble_development_api(cfg,p.review.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let server=AbortServer(tokio::spawn(axum::serve(listener,runtime.router()).into_future()));let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let req=|method:&str,path:&str,key:&str,body:Vec<u8>|Request::builder().method(method).uri(path).header("host","acme.example.test").header("cookie",&cookie).header("origin","https://acme.example.test").header("content-type",if method=="PUT"{"image/png"}else{"application/json"}).header("idempotency-key",key).header("x-erpx-context-company","company").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::from(body)).unwrap();
  let base="/api/v1/holdings/acme/field-service/attachment-sets";
  let anonymous=Request::builder().method("POST").uri(base).header("host","acme.example.test").body(Body::from("{" )).unwrap();assert_eq!(send(address,anonymous).await.status(),StatusCode::UNAUTHORIZED);
  let created=send(address,req("POST",base,"real-field-create",br#"{"targetType":"field_service_visit","targetId":"issue-order","componentKind":"record","purposeKey":"site_photo","expectedTargetVersion":3}"#.to_vec())).await;
  let status=created.status();let bytes=to_bytes(created.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let set:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let id=set["data"]["id"].as_str().unwrap();
  let input=include_bytes!("../../../fixtures/files/preview/png-alpha.input");let reserve=format!("{base}/{id}/files");let body=serde_json::to_vec(&serde_json::json!({"filename":"photo.png","mediaType":"image/png","bytes":input.len()})).unwrap();
  let r=send(address,req("POST",&reserve,"real-field-reserve",body)).await;let status=r.status();let bytes=to_bytes(r.into_body(),65536).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let reservation:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let path=reservation["data"]["uploadUrl"].as_str().unwrap();let link=reservation["data"]["attachmentLinkId"].as_str().unwrap();
  let uploaded=send(address,req("PUT",path,"unused",input.to_vec())).await;let code=uploaded.status();let body=to_bytes(uploaded.into_body(),65536).await.unwrap();assert_eq!(code,StatusCode::ACCEPTED,"{}",String::from_utf8_lossy(&body));let uploaded:serde_json::Value=serde_json::from_slice(&body).unwrap();let source=uploaded["data"]["sourceId"].as_str().unwrap();
  let status_path=format!("/api/v1/holdings/acme/field-service/attachment-links/{link}/status");
  let status=send(address,req("GET",&status_path,"unused",vec![])).await;assert_eq!(status.status(),StatusCode::OK);let status:serde_json::Value=serde_json::from_slice(&to_bytes(status.into_body(),65536).await.unwrap()).unwrap();assert_eq!(status["data"]["sourceState"],"quarantined");
  let content=format!("/api/v1/holdings/acme/field-service/attachment-links/{link}/content?variant=original");
  let malformed=Request::builder().uri(format!("/api/v1/holdings/acme/field-service/attachment-links/{link}/content?variant=invalid")).header("host","acme.example.test").body(Body::empty()).unwrap();assert_eq!(send(address,malformed).await.status(),StatusCode::UNAUTHORIZED);assert_eq!(send(address,req("GET",&content,"unused",vec![])).await.status(),StatusCode::LOCKED);
  // Separate synthetic draft: exercise real cleanup using only expiry timestamps.
  let expiry_set=send(address,req("POST",base,"synthetic-expiry-create",br#"{"targetType":"field_service_visit","targetId":"issue-order","componentKind":"record","purposeKey":"technician_report","expectedTargetVersion":3}"#.to_vec())).await;assert_eq!(expiry_set.status(),StatusCode::CREATED);
  let expiry_set:serde_json::Value=serde_json::from_slice(&to_bytes(expiry_set.into_body(),65536).await.unwrap()).unwrap();let expiry_id=expiry_set["data"]["id"].as_str().unwrap();
  let expiry_reserve=format!("{base}/{expiry_id}/files");let expiry_body=serde_json::to_vec(&serde_json::json!({"filename":"expiry.png","mediaType":"image/png","bytes":input.len()})).unwrap();
  let expiry=send(address,req("POST",&expiry_reserve,"synthetic-expiry-reserve",expiry_body)).await;assert_eq!(expiry.status(),StatusCode::CREATED);let expiry:serde_json::Value=serde_json::from_slice(&to_bytes(expiry.into_body(),65536).await.unwrap()).unwrap();let expiry_link=expiry["data"]["attachmentLinkId"].as_str().unwrap();let expiry_upload=expiry["data"]["uploadIntentId"].as_str().unwrap();
  sqlx::query("UPDATE attachment_links SET expires_at=$2 WHERE id=$1").bind(expiry_link).bind(now-1000).execute(p.pool().raw()).await.unwrap();sqlx::query("UPDATE upload_intents SET expires_at=$2 WHERE id=$1").bind(expiry_upload).bind(now-1000).execute(p.pool().raw()).await.unwrap();
  let (stop_tx,stop_rx)=tokio::sync::oneshot::channel::<()>();let path=config_path.clone();let worker=tokio::spawn(async move {run_field_files_worker_file(&path,async{let _=stop_rx.await;}).await});
  let mut available=false;for _ in 0..120 {let state:Option<String>=sqlx::query_scalar("SELECT source_state FROM attachments WHERE id=$1").bind(source).fetch_optional(p.pool().raw()).await.unwrap();if state.as_deref()==Some("available"){available=true;break;}
if worker.is_finished(){break;}tokio::time::sleep(Duration::from_millis(500)).await;}assert!(available,"actual scanner did not publish available content");
  let mut expired=false;for _ in 0..120 {let facts:(String,String,String,i64,i64,i64)=sqlx::query_as("SELECT s.state,l.state,u.status,(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='link.expired'),(SELECT count(*) FROM attachment_events WHERE set_id=s.id AND action='set.expired'),(SELECT count(*) FROM upload_intents WHERE id=$2) FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN upload_intents u ON u.id=$2 WHERE s.id=$1 AND l.id=$3").bind(expiry_id).bind(expiry_upload).bind(expiry_link).fetch_one(p.pool().raw()).await.unwrap();if facts==("expired".into(),"expired".into(),"expired".into(),1,1,1){expired=true;break;}
if worker.is_finished(){break;}tokio::time::sleep(Duration::from_millis(500)).await;}assert!(expired,"actual worker did not expire synthetic draft or retain its upload history");
  let response=send(address,req("GET",&content,"unused",vec![])).await;assert_eq!(response.status(),StatusCode::OK);let checksum=response.headers().get("x-erpx-checksum-sha256").unwrap().to_str().unwrap().to_owned();let actual=to_bytes(response.into_body(),10*1024*1024).await.unwrap();assert_eq!(&actual[..],input);assert_eq!(checksum,hex::encode(Sha256::digest(input)));
  // Exercise global Field Files policy without changing the clean source.
  for (mode,frozen,read_status) in [("read-only",false,StatusCode::NOT_FOUND),("enabled",true,StatusCode::OK),("disabled",false,StatusCode::NOT_FOUND)] {
   let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let candidate=listener.local_addr().unwrap();let mut variant=doc.clone();variant["listen"]=candidate.to_string().into();variant["fieldFilesPolicy"]=serde_json::json!({"mode":mode,"uploadsFrozen":frozen});
   let candidate_runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&variant.to_string()).unwrap(),p.review.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
   let candidate_server=AbortServer(tokio::spawn(axum::serve(listener,candidate_runtime.router()).into_future()));
   assert_eq!(send(candidate,req("GET",&content,"unused",vec![])).await.status(),read_status);
   let reserved_body=serde_json::to_vec(&serde_json::json!({"filename":"blocked.png","mediaType":"image/png","bytes":input.len()})).unwrap();
   assert_eq!(send(candidate,req("POST",&reserve,"policy-blocked-reserve",reserved_body)).await.status(),if frozen{StatusCode::SERVICE_UNAVAILABLE}else{StatusCode::NOT_FOUND},"policy mode={mode} frozen={frozen}");
   if mode!="enabled" {
    let policy_bind=format!("/api/v1/holdings/acme/attachment-sets/{id}/bind");let blocked=send(candidate,req("POST",&policy_bind,"blocked-bind",br#"{"expectedTargetVersion":3}"#.to_vec())).await;assert_ne!(blocked.status(),StatusCode::OK);
    let policy_withdraw=format!("/api/v1/holdings/acme/attachment-links/{link}/withdraw");let blocked=send(candidate,req("POST",&policy_withdraw,"blocked-withdraw",br#"{"reason":"Synthetic policy check"}"#.to_vec())).await;assert_ne!(blocked.status(),StatusCode::OK);
   }
   candidate_server.0.abort();drop(candidate_server);candidate_runtime.close().await;
  }
  // A second draft reservation can be withdrawn and replayed without inventing bytes.
  let body=serde_json::to_vec(&serde_json::json!({"filename":"unused.png","mediaType":"image/png","bytes":input.len()})).unwrap();
  let second=send(address,req("POST",&reserve,"real-second-reservation",body)).await;assert_eq!(second.status(),StatusCode::CREATED);
  let second:serde_json::Value=serde_json::from_slice(&to_bytes(second.into_body(),65536).await.unwrap()).unwrap();let withdrawn_link=second["data"]["attachmentLinkId"].as_str().unwrap();
  let withdraw=format!("/api/v1/holdings/acme/attachment-links/{withdrawn_link}/withdraw");let reason=br#"{"reason":"Synthetic unused file"}"#.to_vec();
  for _ in 0..2 {let response=send(address,req("POST",&withdraw,"real-withdraw",reason.clone())).await;assert_eq!(response.status(),StatusCode::OK);}
  let withdrawn_content=format!("/api/v1/holdings/acme/field-service/attachment-links/{withdrawn_link}/content?variant=original");assert_ne!(send(address,req("GET",&withdrawn_content,"unused",vec![])).await.status(),StatusCode::OK);
  // Bind the clean first file to the actual Field source; replay preserves one version.
  let bind=format!("/api/v1/holdings/acme/attachment-sets/{id}/bind");let binding=br#"{"expectedTargetVersion":3}"#.to_vec();
  for _ in 0..2 {let response=send(address,req("POST",&bind,"real-field-bind",binding.clone())).await;let code=response.status();let body=to_bytes(response.into_body(),65536).await.unwrap();assert_eq!(code,StatusCode::OK,"{}",String::from_utf8_lossy(&body));}
  let facts:(i64,i64,i64)=sqlx::query_as("SELECT version,(SELECT count(*) FROM attachment_events WHERE set_id=$1 AND action='set.bound'),(SELECT count(*) FROM attachment_events WHERE link_id=$2 AND action='link.withdrawn') FROM field_service_orders WHERE id='issue-order'").bind(id).bind(withdrawn_link).fetch_one(p.pool().raw()).await.unwrap();assert_eq!(facts,(4,1,1));
  assert_eq!(send(address,req("GET",&content,"unused",vec![])).await.status(),StatusCode::OK);
  let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let readonly_address=listener.local_addr().unwrap();let mut readonly_doc=doc.clone();readonly_doc["listen"]=readonly_address.to_string().into();readonly_doc["fieldFilesPolicy"]=serde_json::json!({"mode":"read-only","uploadsFrozen":false});
  let readonly_runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&readonly_doc.to_string()).unwrap(),p.review.system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();let readonly_server=AbortServer(tokio::spawn(axum::serve(listener,readonly_runtime.router()).into_future()));
  assert_eq!(send(readonly_address,req("GET",&content,"unused",vec![])).await.status(),StatusCode::OK);readonly_server.0.abort();drop(readonly_server);readonly_runtime.close().await;
  let audit_before:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events").fetch_one(p.pool().raw()).await.unwrap();
  let wrong=Request::builder().uri(&content).header("host","acme.example.test").header("cookie",&cookie).header("x-erpx-context-company","foreign").header("x-erpx-context-assignment","field-fixture-a").header("x-erpx-context-assignment-version","3").header("x-erpx-context-revision","8").body(Body::empty()).unwrap();assert_ne!(send(address,wrong).await.status(),StatusCode::OK);let audit_after:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events").fetch_one(p.pool().raw()).await.unwrap();assert_eq!(audit_after,audit_before);
  let _=stop_tx.send(());worker.await.unwrap().unwrap();server.0.abort();drop(server);runtime.close().await;
 }).await;
}
