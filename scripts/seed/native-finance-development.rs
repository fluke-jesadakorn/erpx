//! Disposable native Finance development case. Never installs a service or uses
//! an existing application database; fixture cleanup verifies each database lease.
#[allow(dead_code)]
#[path = "../../tests/support/finance/review-composition.rs"]
mod finance;
#[allow(dead_code)]
#[path = "../../tests/support/runtime/private-session-config.rs"]
mod private_fixture;

use erpx_server_identity_access::public::{SystemAccountClock, SystemAuthenticationSecrets};
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api,
};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn write_private(dir: &Path, name: &str, value: &serde_json::Value) -> PathBuf {
    let path = dir.join(name);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .expect("private case file creation failed");
    serde_json::to_writer(&mut file, value).expect("private case file encoding failed");
    file.write_all(b"\n")
        .expect("private case file write failed");
    path
}
struct CaseFiles(Vec<PathBuf>);
impl Drop for CaseFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
#[tokio::main]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(
        args.len(),
        3,
        "usage: native-finance-development ABSOLUTE_PRIVATE_CASE_DIR LOOPBACK_LISTEN"
    );
    let dir = PathBuf::from(&args[1]);
    assert!(
        dir.is_absolute(),
        "absolute private case directory required"
    );
    let meta = fs::symlink_metadata(&dir).expect("private case directory required");
    assert!(
        meta.is_dir() && !meta.file_type().is_symlink() && meta.permissions().mode() & 0o077 == 0,
        "private case directory required"
    );
    let dir = dir.canonicalize().expect("private case directory required");
    let listen = args[2].clone();
    finance::account::with_system(move |system| async move {
        let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).expect("wall clock unavailable").as_millis()).expect("wall clock invalid");
        system.scenario.clock.0.store(now, Ordering::SeqCst);
        let prepared = finance::setup(system).await;
        prepared.create("development", false).await.expect("finance fixture unavailable");
        let system = &prepared.system;
        let control = private_fixture::database(&system.control).await;
        let h001 = private_fixture::database(&system.holdings[0]).await;
        let h002 = private_fixture::database(&system.holdings[1]).await;
        let doc = serde_json::json!({
            "version":1,"mode":"development","transport":"direct-loopback","listen":listen,
            "accountHost":"account-dev-erpx.jesadakorn.com","requireEmailVerification":true,
            "passwordWorkers":2,"maximumRequests":16,"requestTimeoutMs":5000,"control":control,
            "holdings":[
                {"holdingId":"h001","slug":"acme","host":"acme.jesadakorn.com","writeBinding":"HOLDING_H001_DB","readBinding":"HOLDING_H001_READ_DB","filesBinding":"HOLDING_H001_FILES","database":h001},
                {"holdingId":"h002","slug":"northstar","host":"northstar.jesadakorn.com","writeBinding":"HOLDING_H002_DB","readBinding":"HOLDING_H002_READ_DB","filesBinding":"HOLDING_H002_FILES","database":h002}
            ],"financeJournalReads":[{"holdingId":"h001","companyId":"company"}],"accountBootstrap":true
        });
        let configuration = DevelopmentApiConfiguration::parse(&doc.to_string()).expect("invalid explicit native case configuration");
        let manifest = serde_json::json!({"version":1,"databases":{
            "CONTROL_DB":doc["control"]["url"], "CONTROL_READ_DB":doc["control"]["url"],
            "HOLDING_H001_DB":doc["holdings"][0]["database"]["url"],"HOLDING_H001_READ_DB":doc["holdings"][0]["database"]["url"],
            "HOLDING_H002_DB":doc["holdings"][1]["database"]["url"],"HOLDING_H002_READ_DB":doc["holdings"][1]["database"]["url"]
        },"slots":[
            {"id":"h001","write":"HOLDING_H001_DB","read":"HOLDING_H001_READ_DB","files":"HOLDING_H001_FILES"},
            {"id":"h002","write":"HOLDING_H002_DB","read":"HOLDING_H002_READ_DB","files":"HOLDING_H002_FILES"}
        ]});
        let mut files = CaseFiles(Vec::new());
        files.0.push(write_private(&dir, "native-api.json", &doc));
        files.0.push(write_private(&dir, "databases.json", &manifest));
        files.0.push(write_private(&dir, "credentials.json", &serde_json::json!({"email":"b@example.test","password":finance::account::fixture::PASSWORD,"journalId":"journal-development"})));
        let runtime = assemble_development_api(configuration, Arc::new(SystemAccountClock), Arc::new(SystemAuthenticationSecrets)).await.expect("native case assembly failed");
        let listener = tokio::net::TcpListener::bind(runtime.listen_address()).await.expect("private case listener unavailable");
        println!("{}", dir.join("databases.json").display());
        println!("READY");
        let (stop, stopped) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || { let _ = std::io::stdin().read(&mut [0u8; 1]); let _ = stop.send(()); });
        let server = axum::serve(listener, runtime.router()).with_graceful_shutdown(async move {
            tokio::select! { _ = stopped => (), _ = tokio::signal::ctrl_c() => () }
        });
        let result = server.await;
        runtime.close().await;
        drop(files);
        result.expect("private case listener failed");
    }).await;
}
