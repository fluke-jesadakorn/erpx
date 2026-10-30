//! Private, leased Field Files fixture held for a single Vitest lifecycle case.
//! The caller supplies a private output path and closes stdin to release the lease.
#[allow(dead_code)]
#[path = "../runtime/private-session-config.rs"]
mod configuration;
#[allow(dead_code, unused_imports)]
#[path = "../field_service/parts.rs"]
mod facts;
use erpx_server_identity_access::public::SystemAuthenticationSecrets;
use erpx_service::private_development_api::{
    DevelopmentApiConfiguration, assemble_development_api, run_field_files_worker_file,
};
use sqlx::Row;
use std::{
    fs,
    future::IntoFuture,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    sync::{Arc, atomic::Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct AbortServer(tokio::task::JoinHandle<std::io::Result<()>>);
impl Drop for AbortServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::main]
async fn main() {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("signal handler");
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .expect("signal handler");
    tokio::spawn(async move {
        tokio::select! {_=term.recv()=>{},_=interrupt.recv()=>{}};
        let _ = shutdown_tx.send(true);
    });
    facts::account::with_system(|system| async move {
        if std::env::var_os("ERPX_FIELD_FILES_BROWSER_PROFILE").is_some() {
            facts::account::database::with_database(3, move |read1| async move {
                facts::account::database::with_database(3, move |read2| async move {
                    run_fixture(system, Some([read1, read2]), shutdown_rx).await
                })
                .await;
            })
            .await;
        } else {
            run_fixture(system, None, shutdown_rx).await;
        }
    })
    .await;
}

async fn run_fixture(
    system: facts::account::System,
    reads: Option<[erpx_server_runtime::public::postgres::DatabasePool; 2]>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    if *shutdown.borrow() {
        return;
    }
    let mixed = std::env::var_os("ERPX_FIELD_FILES_MIXED_PROFILE").is_some();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    system.scenario.clock.0.store(now, Ordering::SeqCst);
    let p = facts::setup_workspace(system).await;
    let central = facts::account::login(&p.review.system, "a@example.test", None).await;
    let child = facts::account::child(&p.review.system, &central, facts::account::ACME).await;
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
    let foreign_central = facts::account::login(&p.review.system, "b@example.test", None).await;
    let foreign =
        facts::account::child(&p.review.system, &foreign_central, facts::account::ACME).await;
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('foreign','company','foreign','Synthetic foreign company',NULL,'foreign',NULL,'active'); INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('foreign','fixture-b','local:fixture-b','active','local',1)").execute(p.pool().raw()).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('field-foreign','fixture-b','custom_field_parts','[\"field_service\"]','foreign',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(p.pool().raw()).await.unwrap();
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-b','foreign','field-foreign',3,8,$2,$3)").bind(&foreign.result.session_id).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let store = root.path().join("files");
    let tmp = root.path().join("temporary");
    for path in [&store, &tmp] {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        for line in BufReader::new(std::io::stdin()).lines() {
            match line {
                Ok(line) if line.len() <= 65536 => {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
                _ => break,
            }
        }
    });
    let mut mixed_company = None;
    let mut sessions = serde_json::Map::new();
    if mixed {
        assert!(reads.is_none(), "profiles are mutually exclusive");
        let seed_path = format!(
            "{}.seed",
            std::env::var("ERPX_FIELD_FILES_FIXTURE_OUTPUT").unwrap()
        );
        write_private(
            &seed_path,
            &serde_json::json!({"nonce":std::env::var("ERPX_FIELD_FILES_FIXTURE_NONCE").unwrap(), "database":configuration::database(p.pool()).await, "store":store}),
        );
        let input = tokio::select! { _ = shutdown.changed() => return, value = rx.recv() => match value { Some(value) => value, None => return } };
        let activation: serde_json::Value =
            serde_json::from_str(&input).expect("mixed activation JSON");
        let company = activation["companyId"].as_str().expect("companyId");
        let actors = activation["actors"].as_array().expect("actors");
        assert!(!actors.is_empty() && actors.len() <= 16);
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organization_nodes WHERE id=$1 AND company_id=$1 AND kind='company' AND status='active')").bind(company).fetch_one(p.pool().raw()).await.unwrap();
        assert!(exists, "mixed company must be active in the leased Holding");
        let password: String =
            sqlx::query_scalar("SELECT password FROM account WHERE id='fixture-a'")
                .fetch_one(p.review.system.control.raw())
                .await
                .unwrap();
        for (index, actor) in actors.iter().enumerate() {
            let label = actor["label"].as_str().expect("label");
            let user = actor["userId"].as_str().expect("userId");
            let assignment = actor["assignmentId"].as_str().expect("assignmentId");
            let version = actor["assignmentVersion"]
                .as_i64()
                .expect("assignmentVersion");
            let role = actor["role"].as_str().expect("role");
            assert!(actor["branchId"].is_null() && actor["departmentId"].is_null());
            assert!(!label.is_empty() && label.len() <= 80 && !sessions.contains_key(label));
            let row = sqlx::query("SELECT role, modules_json, version FROM assignments WHERE id=$1 AND user_id=$2 AND company_id=$3 AND scope_kind='holding' AND valid_from <= $4 AND valid_until > $4").bind(assignment).bind(user).bind(company).bind(now).fetch_one(p.pool().raw()).await.expect("seeded active assignment");
            let modules: serde_json::Value =
                serde_json::from_str(row.get::<String, _>("modules_json").as_str()).unwrap();
            assert_eq!(row.get::<String, _>("role"), role);
            assert_eq!(row.get::<i64, _>("version"), version);
            assert_eq!(modules, actor["modules"]);
            let membership = sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) SELECT $1,$2,$3,id,'active','local',1 FROM hr_people WHERE id=$4 AND company_id=$1 AND user_id=$2 ON CONFLICT(company_id,user_id) DO UPDATE SET version=iam_members.version WHERE iam_members.principal_id=EXCLUDED.principal_id AND iam_members.person_id=EXCLUDED.person_id AND iam_members.status='active' AND iam_members.login_policy='local'").bind(company).bind(user).bind(format!("local:{user}")).bind(format!("person-{user}")).execute(p.pool().raw()).await.unwrap();
            assert_eq!(
                membership.rows_affected(),
                1,
                "seeded person must match actor"
            );
            let existing_email: Option<String> =
                sqlx::query_scalar("SELECT email FROM \"user\" WHERE id=$1")
                    .bind(user)
                    .fetch_optional(p.review.system.control.raw())
                    .await
                    .unwrap();
            let email = existing_email
                .clone()
                .unwrap_or_else(|| format!("mixed-{index}@example.test"));
            if existing_email.is_none() {
                sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES($1,$1,$2,1,$3,$3)").bind(user).bind(&email).bind(now).execute(p.review.system.control.raw()).await.unwrap();
                sqlx::query("INSERT INTO account(id,\"accountId\",\"providerId\",issuer,\"userId\",password,\"createdAt\",\"updatedAt\") VALUES($1,$1,'credential','local:credential',$1,$2,$3,$3)").bind(user).bind(&password).bind(now).execute(p.review.system.control.raw()).await.unwrap();
            }
            let central = facts::account::login(&p.review.system, &email, None).await;
            let child =
                facts::account::child(&p.review.system, &central, facts::account::ACME).await;
            sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,$2,$3,$4,$5,8,$6,$7)").bind(&child.result.session_id).bind(user).bind(company).bind(assignment).bind(version).bind(now+3_600_000).bind(now).execute(p.pool().raw()).await.unwrap();
            sessions.insert(label.to_owned(), serde_json::json!({"cookie":format!("__Host-erpx-product={}",child.token.expose()),"companyId":company,"assignmentId":assignment,"assignmentVersion":version,"revision":8}));
        }
        mixed_company = Some(company.to_owned());
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut doc =
        configuration::document(&p.review.system.control, &p.review.system.holdings).await;
    doc["listen"] = address.to_string().into();
    if let Some(ref pools) = reads {
        use erpx_server_runtime::public::postgres::{
            MigrationTrack, load_migrations, migrate_database,
        };
        let files = load_migrations(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
            MigrationTrack::HoldingRead,
        )
        .await
        .unwrap();
        for pool in pools {
            migrate_database(pool, MigrationTrack::HoldingRead, &files)
                .await
                .unwrap();
        }
        doc["accountBootstrap"] = true.into();
        doc["accountHost"] = "account-dev-erpx.jesadakorn.com".into();
        doc["holdings"][0]["host"] = "acme.jesadakorn.com".into();
        doc["holdings"][1]["host"] = "northstar.jesadakorn.com".into();
        doc["fieldServiceWorkspaceReads"] = serde_json::json!(["h001"]);
        doc["fieldServiceDetailReads"] = serde_json::json!(["h001"]);
    }

    doc["fieldServiceReads"] = serde_json::json!([{"holdingId":"h001","companyId":"company"}]);
    doc["fieldAttachmentLinks"] = serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);
    doc["fieldAttachmentBinds"] = serde_json::json!([{"holdingId":"h001","mode":"enabled"}]);
    doc["fieldFilesPolicy"] = serde_json::json!({"mode":"enabled","uploadsFrozen":false});
    doc["fieldFilesProviders"] = serde_json::json!({"targets":[{"holdingId":"h001","companyId":"company","filesBinding":"HOLDING_H001_FILES","filesRoot":store,"temporaryRoot":tmp,"previewBinary":std::env::var("ERPX_FILES_PREVIEW_BIN").unwrap(),"heifConvert":std::env::var("ERPX_HEIF_CONVERT").unwrap(),"pdftoppm":std::env::var("ERPX_PDFTOPPM_BIN").unwrap(),"clamavSocket":std::env::var("ERPX_CLAMD_SOCKET").unwrap(),"clamavDatabaseDirectory":std::env::var("ERPX_CLAMAV_DATABASE_DIR").unwrap(),"maximumDefinitionAgeMs":86400000,"commandTimeoutMs":5000,"scanTimeoutMs":30000,"concurrentScans":1}]});
    let foreign_tmp = root.path().join("foreign-temporary");
    fs::create_dir(&foreign_tmp).unwrap();
    fs::set_permissions(&foreign_tmp, fs::Permissions::from_mode(0o700)).unwrap();
    let mut foreign_provider = doc["fieldFilesProviders"]["targets"][0].clone();
    foreign_provider["companyId"] = "foreign".into();
    foreign_provider["temporaryRoot"] = serde_json::json!(foreign_tmp);
    doc["fieldFilesProviders"]["targets"]
        .as_array_mut()
        .unwrap()
        .push(foreign_provider);
    if let Some(ref company) = mixed_company {
        let mixed_tmp = root.path().join("mixed-temporary");
        fs::create_dir(&mixed_tmp).unwrap();
        fs::set_permissions(&mixed_tmp, fs::Permissions::from_mode(0o700)).unwrap();
        let mut provider = doc["fieldFilesProviders"]["targets"][0].clone();
        provider["companyId"] = company.clone().into();
        provider["temporaryRoot"] = serde_json::json!(mixed_tmp);
        doc["fieldFilesProviders"]["targets"]
            .as_array_mut()
            .unwrap()
            .push(provider);
    }
    let config_path = root.path().join("private.json");
    fs::write(&config_path, serde_json::to_vec(&doc).unwrap()).unwrap();
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let runtime = assemble_development_api(
        DevelopmentApiConfiguration::parse(&doc.to_string()).unwrap(),
        p.review.system.scenario.clock.clone(),
        Arc::new(SystemAuthenticationSecrets),
    )
    .await
    .unwrap();
    let server = AbortServer(tokio::spawn(
        axum::serve(listener, runtime.router()).into_future(),
    ));
    if *shutdown.borrow() {
        server.0.abort();
        drop(server);
        runtime.close().await;
        return;
    }
    let output = std::env::var("ERPX_FIELD_FILES_FIXTURE_OUTPUT").unwrap();
    let mut private = serde_json::json!({"nonce":std::env::var("ERPX_FIELD_FILES_FIXTURE_NONCE").unwrap(),"address":address.to_string(),"cookie":format!("__Host-erpx-product={}",child.token.expose()),"foreignCookie":format!("__Host-erpx-product={}",foreign.token.expose()),"database":configuration::database(p.pool()).await,"store":store});
    if mixed {
        private["sessions"] = sessions.into();
    }
    if let Some(ref pools) = reads {
        private["browser"] = serde_json::json!({"control":configuration::database(&p.review.system.control).await,"holding1":configuration::database(&p.review.system.holdings[0]).await,"holding2":configuration::database(&p.review.system.holdings[1]).await,"holding1Read":configuration::database(&pools[0]).await,"holding2Read":configuration::database(&pools[1]).await,"maker":{"email":"a@example.test","password":facts::account::fixture::PASSWORD},"companyId":"company","orderId":"issue-order","privateConfiguration":config_path});
    }

    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)
        .unwrap();
    out.write_all(&serde_json::to_vec(&private).unwrap())
        .unwrap();
    out.sync_all().unwrap();
    let mut worker = None;
    loop {
        let command = tokio::select! { _=shutdown.changed()=>break, value=rx.recv()=>match value {Some(value)=>value,None=>break}};
        if command == "scan" && worker.is_none() {
            let (stop, wait) = tokio::sync::oneshot::channel::<()>();
            let path = config_path.clone();
            let task = tokio::spawn(async move {
                run_field_files_worker_file(&path, async {
                    let _ = wait.await;
                })
                .await
            });
            worker = Some((stop, task));
        } else if mixed && command == "advance-clock" {
            let advanced = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                + 300_000;
            p.review
                .system
                .scenario
                .clock
                .0
                .store(advanced, Ordering::SeqCst);
            let path = format!(
                "{}.clock",
                std::env::var("ERPX_FIELD_FILES_FIXTURE_OUTPUT").unwrap()
            );
            write_private(
                &path,
                &serde_json::json!({"nonce":std::env::var("ERPX_FIELD_FILES_FIXTURE_NONCE").unwrap()}),
            );
        } else if command == "quit" {
            break;
        }
    }
    if let Some((stop, task)) = worker {
        let _ = stop.send(());
        task.await.unwrap().unwrap();
    }
    server.0.abort();
    drop(server);
    runtime.close().await;
}

fn write_private(path: &str, value: &serde_json::Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();
    file.sync_all().unwrap();
}
