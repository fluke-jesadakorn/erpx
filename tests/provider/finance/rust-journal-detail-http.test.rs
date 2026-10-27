#[allow(
    dead_code,
    reason = "actual Auth/Finance/Workflow fixture is shared with owner command tests"
)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{Router, body::Body, http::Request};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_shared::public::{Clock, CompanyId, HoldingId};
use erpx_service::{
    finance_journal_detail::*, finance_journal_detail_http::*,
    finance_journal_review::FinanceJournalEnvironment,
};
use facts::{Prepared, account, *};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{ChildStdout, Command},
    sync::Notify,
};
use tower::ServiceExt;
fn router(p: &Prepared) -> Router {
    let service = FinanceJournalDetailService::new(
        p.authorizer.clone(),
        FinanceJournalReadConfiguration::new(
            FinanceJournalEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    );
    finance_journal_read_router(
        Arc::new(account::fixture::account::policy()),
        account::fixture::account::holding_registry(),
        Arc::new(FinanceJournalReadScopeHeaders),
        vec![FinanceJournalReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            service: Arc::new(service),
        }],
    )
    .unwrap()
}
async fn phase(output: &mut BufReader<ChildStdout>, expected: &str) -> Result<(), &'static str> {
    let mut bytes = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(10),
        (&mut *output).take(32769).read_until(b'\n', &mut bytes),
    )
    .await
    .map_err(|_| "FINANCE_NODE_PROTOCOL_DEADLINE")?
    .map_err(|_| "FINANCE_NODE_PROTOCOL_IO")?;
    if bytes.len() > 32768 || !bytes.ends_with(b"\n") {
        return Err("FINANCE_NODE_PROTOCOL_LIMIT");
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "FINANCE_NODE_PROTOCOL_INVALID")?;
    if value["type"] != expected {
        if value["type"] == "failure" {
            eprintln!(
                "closed Finance Node code: {}",
                value["code"].as_str().unwrap_or("UNKNOWN")
            );
        }
        return Err("FINANCE_NODE_PROTOCOL_PHASE");
    }
    Ok(())
}
#[tokio::test]
async fn actual_node_finance_ssr_read_keeps_original_account_context_and_rejects_retired_cookie_after_normal_group_switch()
 {
    account::with_system(|system|async move{
  let p=setup(system).await;p.create("node-detail",false).await.unwrap();let app=router(&p);let cookie=format!("__Host-erpx-product={}",p.reviewer_token.expose());
  // Prove actual Router/authority/owner data before opening our ephemeral socket.
  let checked=app.clone().oneshot(Request::builder().uri("/api/v1/holdings/acme/finance/journals/journal-node-detail").header("host","acme.example.test").header("cookie",&cookie).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(checked.status(),200);
  let listener=TcpListener::bind(("127.0.0.1",0)).await.unwrap();let address=listener.local_addr().unwrap();let stop=Arc::new(Notify::new());let shutdown=stop.clone();let server=tokio::spawn(async move{axum::serve(listener,app).with_graceful_shutdown(async move{shutdown.notified().await}).await});
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();let node=std::env::var("ERPX_NODE_BIN").expect("Explicit pinned Node required");
  let mut child=Command::new(node).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).env("NODE_ENV","test").args(["--conditions=react-server"]).arg(root.join("tests/support/finance/native-journal-client.mjs")).arg(root.join("packages/feature-finance/src/adapters/server/native-journal-detail.ts")).kill_on_drop(true).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();let mut input=child.stdin.take().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());let diagnostics=child.stderr.take().unwrap();let errors=tokio::spawn(async move{let mut bytes=Vec::new();diagnostics.take(8193).read_to_end(&mut bytes).await.map(|_|bytes)});
  let result=async{
   let payload=serde_json::json!({"configuration":{"privateBaseUrl":format!("http://{address}"),"holdings":[{"origin":"https://acme.example.test","holdingSlug":"acme","holdingId":"h001"}],"timeoutMs":1500},"input":{"holdingSlug":"acme","journalId":"journal-node-detail"},"cookie":cookie});input.write_all((payload.to_string()+"\n").as_bytes()).await.map_err(|_|"FINANCE_NODE_INPUT")?;phase(&mut output,"read-complete").await?;
   let central=account::login(&p.system,"a@example.test",Some(erpx_server_identity_access::public::PresentedSecrets{browser:p.reviewer_central.browser.clone(),active:p.reviewer_central.active.clone(),saved:p.reviewer_central.saved.clone()})).await;let child=account::child(&p.system,&central,account::ACME).await;
   let now=p.system.scenario.clock.now().get();sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at)VALUES($1,'fixture-a','company','finance-fixture-a',3,7,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.system.holdings[0].raw()).await.map_err(|_|"FINANCE_CURRENT_SELECTION_FIXTURE")?;
   let payload=serde_json::json!({"cookie":format!("__Host-erpx-product={}",child.token.expose())});input.write_all((payload.to_string()+"\n").as_bytes()).await.map_err(|_|"FINANCE_NODE_INPUT")?;phase(&mut output,"done").await
  }.await;
  drop(input);if result.is_err(){let _=child.kill().await;}let status=tokio::time::timeout(Duration::from_secs(10),child.wait()).await;if status.is_err(){let _=child.kill().await;let _=child.wait().await;}let errors=errors.await.unwrap().unwrap();stop.notify_one();tokio::time::timeout(Duration::from_secs(10),server).await.unwrap().unwrap().unwrap();assert!(result.is_ok(),"closed Finance Node error: {result:?}");assert!(status.unwrap().unwrap().success());assert!(errors.len()<=8192);drop(TcpListener::bind(address).await.expect("owned Finance listener released"));
  for pool in [&p.system.control,&p.system.holdings[0],&p.system.holdings[1]]{assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND backend_type='client backend'AND xact_start IS NOT NULL)").fetch_one(pool.raw()).await.unwrap());}
 }).await;
}
