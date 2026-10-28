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
        // Synthetic reviewer, with a real published scoped workflow role and
        // an active HR engagement. The journal remains created by fixture-a.
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','journal_flow_reviewer','Journal workflow reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','journal_flow_reviewer',1,'[\"finance.read\",\"finance.write\",\"finance.approve\",\"module.finance.read\",\"workflow.finance_approver\"]','[]','[\"holding\"]','fixture',0);UPDATE assignments SET role='journal_flow_reviewer' WHERE id='finance-fixture-b';INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer','fixture-b','company','Reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-engagement','person-reviewer','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(system.holdings[0].raw()).await.expect("journal reviewer fixture unavailable");
        // Third synthetic account uses the same fixture credential hash, while
        // all sessions and SSO child credentials still come from normal login.
        sqlx::raw_sql("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") SELECT 'fixture-c','fixture-c','c@example.test',\"emailVerified\",\"createdAt\",\"updatedAt\" FROM \"user\" WHERE id='fixture-b'; INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") SELECT 'fixture-c','fixture-c',\"providerId\",issuer,'fixture-c',password,\"createdAt\",\"updatedAt\" FROM account WHERE id='fixture-b'").execute(system.control.raw()).await.expect("third synthetic account unavailable");
        sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) SELECT 'finance-fixture-c','fixture-c',role,modules_json,company_id,valid_from,valid_until,version,scope_kind FROM assignments WHERE id='finance-fixture-b'").execute(system.holdings[0].raw()).await.expect("third synthetic assignment unavailable");
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('person-reviewer-c','fixture-c','company','Independent reviewer','TH','active','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('reviewer-engagement-c','person-reviewer-c','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0)").execute(system.holdings[0].raw()).await.expect("third synthetic reviewer unavailable");
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
            ],"financeJournalReads":[{"holdingId":"h001","companyId":"company"}],"financeJournalCreates":["h001"],"financeJournalEdits":["h001"],"financeJournalPosts":["h001"],"financeJournalReversals":["h001"],"accountBootstrap":true
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
        files.0.push(write_private(&dir, "credentials.json", &serde_json::json!({"email":"b@example.test","creatorEmail":"a@example.test","reviewerEmail":"c@example.test","password":finance::account::fixture::PASSWORD,"journalId":"journal-development"})));
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
