#[path = "../../support/runtime/private-session-api.rs"]
mod fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_service::private_development_api::*;
use fixture::{account, cookie, document};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{ChildStdout, Command},
    sync::Notify,
};
use tower::ServiceExt;
async fn message(reader: &mut BufReader<ChildStdout>) -> Result<serde_json::Value, &'static str> {
    let mut bytes = Vec::new();
    let mut bounded = (&mut *reader).take(32769);
    tokio::time::timeout(
        Duration::from_secs(10),
        bounded.read_until(b'\n', &mut bytes),
    )
    .await
    .map_err(|_| "NODE_SESSION_PROTOCOL_DEADLINE")?
    .map_err(|_| "NODE_SESSION_PROTOCOL_IO")?;
    if bytes.len() > 32768 || !bytes.ends_with(b"\n") {
        return Err("NODE_SESSION_PROTOCOL_LIMIT");
    }
    serde_json::from_slice(&bytes).map_err(|_| "NODE_SESSION_PROTOCOL_INVALID")
}
fn phase(value: &serde_json::Value, expected: &str) -> Result<(), &'static str> {
    if value["type"] == expected {
        Ok(())
    } else {
        if value["type"] == "failure" {
            eprintln!(
                "closed Node protocol code: {}",
                value["code"].as_str().unwrap_or("UNKNOWN")
            );
        }
        Err("NODE_SESSION_UNEXPECTED_PHASE")
    }
}
#[tokio::test]
async fn actual_node_ssr_transport_reads_current_three_database_sessions_bypasses_ambient_proxy_and_cleans_up_abort()
 {
    account::with_system(|system|async move{
  let central=account::login(&system,"a@example.test",None).await;let first=account::child(&system,&central,account::ACME).await;let second=account::child(&system,&central,account::NORTHSTAR).await;
  let mut doc=document(&system).await;
  // First prove the Router in process before this test opens ANY listener.
  let checked=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();
  let response=checked.router().oneshot(Request::builder().uri("/api/auth/get-session").header("host","account.example.test").header("cookie",cookie(&central)).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);drop(response);checked.close().await;
  let listener=TcpListener::bind(("127.0.0.1",0)).await.unwrap();let address=listener.local_addr().unwrap();assert!(address.ip().is_loopback());doc["listen"]=address.to_string().into();
  let runtime=assemble_development_api(DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),system.scenario.clock.clone(),Arc::new(SystemAuthenticationSecrets)).await.unwrap();assert_eq!(runtime.listen_address(),address);
  let stop=Arc::new(Notify::new());let shutdown=stop.clone();let app=runtime.router();let server=tokio::spawn(async move{axum::serve(listener,app).with_graceful_shutdown(async move{shutdown.notified().await}).await});
  let trap=TcpListener::bind(("127.0.0.1",0)).await.unwrap();let trap_address=trap.local_addr().unwrap();let trap_stop=Arc::new(Notify::new());let trap_shutdown=trap_stop.clone();let proxy_requests=Arc::new(AtomicUsize::new(0));let seen=proxy_requests.clone();let cookie_leak=Arc::new(AtomicBool::new(false));let leaked=cookie_leak.clone();
  let proxy=tokio::spawn(async move{loop{tokio::select!{_ =trap_shutdown.notified()=>break,accepted=trap.accept()=>{
   let(mut stream,_)=accepted.unwrap();let mut request=Vec::new();let mut byte=[0u8;1];
   let _=tokio::time::timeout(Duration::from_secs(2),async{while request.len()<8192&&!request.ends_with(b"\r\n\r\n"){if stream.read(&mut byte).await?==0{break}request.push(byte[0]);}Ok::<_,std::io::Error>(())}).await;
   seen.fetch_add(1,Ordering::SeqCst);if String::from_utf8_lossy(&request).to_ascii_lowercase().contains("\ncookie:"){leaked.store(true,Ordering::SeqCst)}
   let body="proxy-control-ok";let response=format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());let _=stream.write_all(response.as_bytes()).await;
  }}}});
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();let node=std::env::var("ERPX_NODE_BIN").expect("Explicit pinned Node required");
  let mut child=Command::new(node).env_clear().env("PATH",std::env::var("PATH").unwrap_or_default()).env("NODE_ENV","test").env("NODE_USE_ENV_PROXY","1").env("HTTP_PROXY",format!("http://{trap_address}")).env("HTTPS_PROXY",format!("http://{trap_address}")).args(["--conditions=react-server","--use-env-proxy"]).arg(root.join("tests/support/runtime/native-session-client.mjs")).arg(root.join("packages/feature-runtime/src/adapters/server/native-session-http.ts")).kill_on_drop(true).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
  let mut input=child.stdin.take().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());let errors=child.stderr.take().unwrap();let error_reader=tokio::spawn(async move{let mut bytes=Vec::new();errors.take(8193).read_to_end(&mut bytes).await.map(|_|bytes)});
  let payload=serde_json::json!({"configuration":{"privateBaseUrl":format!("http://{address}"),"accountOrigin":"https://account.example.test","holdingOrigins":["https://acme.example.test","https://northstar.example.test"],"timeoutMs":1500},"cases":[{"host":"account.example.test","cookie":cookie(&central),"expectedUser":"fixture-a"},{"host":"acme.example.test","cookie":format!("__Host-erpx-product={}",first.token.expose()),"expectedUser":"fixture-a","expectedSession":first.result.session_id},{"host":"northstar.example.test","cookie":format!("__Host-erpx-product={}",second.token.expose()),"expectedUser":"fixture-a","expectedSession":second.result.session_id}]});
  let result=async {
   input.write_all((payload.to_string()+"\n").as_bytes()).await.map_err(|_|"NODE_SESSION_INPUT")?;
   phase(&message(&mut output).await?,"first-complete")?;
   sqlx::query("UPDATE holding_bindings SET status='suspended'WHERE holding_id='h001'").execute(system.control.raw()).await.map_err(|_|"SOURCE_WITHDRAWAL_FAILED")?;input.write_all(b"withdraw\n").await.map_err(|_|"NODE_SESSION_INPUT")?;
   phase(&message(&mut output).await?,"ready-for-abort")?;
   let mut blocker=system.control.raw().begin().await.map_err(|_|"BLOCKER_UNAVAILABLE")?;sqlx::query("LOCK TABLE session IN EXCLUSIVE MODE").execute(&mut *blocker).await.map_err(|_|"BLOCKER_UNAVAILABLE")?;input.write_all(b"blocked\n").await.map_err(|_|"NODE_SESSION_INPUT")?;
   phase(&message(&mut output).await?,"abort-complete")?;blocker.rollback().await.map_err(|_|"BLOCKER_RELEASE_FAILED")?;input.write_all(b"released\n").await.map_err(|_|"NODE_SESSION_INPUT")?;let done=message(&mut output).await?;phase(&done,"done")?;if done["freshAfterAbort"]!=true{return Err("NODE_FRESH_READ_FAILED")}Ok::<_,&'static str>(())
  }.await;
  drop(input);if result.is_err(){let _=child.kill().await;}let status=tokio::time::timeout(Duration::from_secs(10),child.wait()).await;
  if status.is_err(){let _=child.kill().await;let _=child.wait().await;}
  let diagnostics=error_reader.await.unwrap().unwrap();stop.notify_one();trap_stop.notify_one();tokio::time::timeout(Duration::from_secs(10),server).await.unwrap().unwrap().unwrap();proxy.await.unwrap();runtime.close().await;
  assert!(result.is_ok(),"closed protocol error: {result:?}");assert!(status.unwrap().unwrap().success());assert!(diagnostics.len()<=8192);assert_eq!(proxy_requests.load(Ordering::SeqCst),1,"only the explicit no-cookie proxy control may reach the trap");assert!(!cookie_leak.load(Ordering::SeqCst));
  for pool in [&system.control,&system.holdings[0],&system.holdings[1]]{assert!(!sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND backend_type='client backend'AND xact_start IS NOT NULL)").fetch_one(pool.raw()).await.unwrap());}
 }).await;
}

async fn cli_read(
    address: std::net::SocketAddr,
    path: &str,
    host: &str,
) -> Result<(u16, serde_json::Value), &'static str> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|_| "CLI_TEST_CONNECT")?;
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .map_err(|_| "CLI_TEST_WRITE")?;
        let mut bytes = Vec::new();
        stream
            .take(32769)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "CLI_TEST_READ")?;
        if bytes.len() > 32768 {
            return Err("CLI_TEST_RESPONSE_LIMIT");
        }
        let split = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("CLI_TEST_HEADERS")?;
        let headers = std::str::from_utf8(&bytes[..split]).map_err(|_| "CLI_TEST_HEADERS")?;
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .ok_or("CLI_TEST_STATUS")?;
        if !headers
            .to_ascii_lowercase()
            .contains("cache-control: private, no-store")
        {
            return Err("CLI_TEST_PRIVATE_RESPONSE");
        }
        let body = serde_json::from_slice(&bytes[split + 4..]).map_err(|_| "CLI_TEST_JSON")?;
        Ok((status, body))
    })
    .await
    .map_err(|_| "CLI_TEST_READ_DEADLINE")?
}

#[cfg(unix)]
#[tokio::test]
async fn explicit_development_cli_owns_only_its_disposable_listener_and_closes_pools_after_signal()
{
    account::with_system(|system| async move {
        let mut document = document(&system).await;
        // First prove real leased schema/registry assembly before opening a port.
        let checked = assemble_development_api(
            DevelopmentApiConfiguration::parse(&document.to_string()).unwrap(),
            system.scenario.clock.clone(), Arc::new(SystemAuthenticationSecrets),
        ).await.unwrap();
        let response = checked.router().oneshot(Request::builder()
            .uri("/api/auth/get-session").header("host", "account.example.test")
            .body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        checked.close().await;

        let reservation = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = reservation.local_addr().unwrap();
        document["listen"] = address.to_string().into();
        // The exact same public parser must admit the allocated disposable port.
        DevelopmentApiConfiguration::parse(&document.to_string()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private-api.json");
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path).unwrap();
        file.write_all(document.to_string().as_bytes()).unwrap();
        drop(file);
        drop(reservation);
        let mut child = Command::new(env!("CARGO_BIN_EXE_erpx-server"))
            .env_clear().env("PATH", std::env::var("PATH").unwrap_or_default())
            .args(["private-api", "--development-config"]).arg(&path)
            .kill_on_drop(true).stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped())
            .spawn().unwrap();
        let mut diagnostics = BufReader::new(child.stderr.take().unwrap());
        let result = async {
            let mut line = Vec::new();
            tokio::time::timeout(Duration::from_secs(10), (&mut diagnostics).take(1025).read_until(b'\n', &mut line))
                .await.map_err(|_| "CLI_STARTUP_DEADLINE")?.map_err(|_| "CLI_STARTUP_IO")?;
            if line != format!("Private development session API listening on {address}; readiness remains unavailable.\n").as_bytes() {
                return Err("CLI_STARTUP_CLOSED_FAILURE");
            }
            for host in ["account.example.test", "acme.example.test", "northstar.example.test"] {
                let (status, body) = cli_read(address, "/api/auth/get-session", host).await?;
                if status != 200 || !body.is_null() { return Err("CLI_ANONYMOUS_SESSION_FAILED"); }
            }
            if cli_read(address, "/api/health", "account.example.test").await?.0 != 200
                || cli_read(address, "/api/ready", "account.example.test").await?.0 != 503 {
                return Err("CLI_FOUNDATION_BOUNDARY_FAILED");
            }
            Ok::<_, &'static str>(())
        }.await;
        if result.is_ok() {
            use nix::{sys::signal::{kill, Signal}, unistd::Pid};
            kill(Pid::from_raw(i32::try_from(child.id().unwrap()).unwrap()), Signal::SIGTERM).unwrap();
        } else { let _ = child.kill().await; }
        let status = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
            Ok(status) => status.unwrap(),
            Err(_) => { let _ = child.kill().await; let _ = child.wait().await; panic!("CLI_SHUTDOWN_DEADLINE"); }
        };
        let mut tail = Vec::new();
        diagnostics.take(4097).read_to_end(&mut tail).await.unwrap();
        assert!(result.is_ok(), "closed CLI error: {result:?}");
        assert!(status.success());
        assert!(tail.is_empty(), "unexpected private CLI diagnostics");
        drop(TcpListener::bind(address).await.expect("CLI listener must be released"));
        for pool in [&system.control, &system.holdings[0], &system.holdings[1]] {
            assert!(!sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND pid<>pg_backend_pid()AND backend_type='client backend'AND xact_start IS NOT NULL)")
                .fetch_one(pool.raw()).await.unwrap());
        }
    }).await;
}
