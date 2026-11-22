//! Native PMS refund-context read through a booted private API on disposable DBs.
//!
//! End-to-end cutover proof for the TS combined-row retirement: seed a Company,
//! session, assignment and one failed refund request, boot
//! `erpx-server private-api --test-config`, then exercise
//! `GET /api/v1/holdings/acme/pms/booking-refunds/context` over real loopback
//! HTTP. Asserts the native payload, private headers, the ABSENT 404 contract
//! and malformed-query rejection.
#![allow(dead_code)]
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, MigrationTrack, PoolLimits, connect_pool, load_migrations,
    migrate_database,
};
use sqlx::{AssertSqlSafe, Connection, postgres::PgConnectOptions};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const HOST: &str = "acme.e2e.test";
const ORIGIN: &str = "https://acme.e2e.test";
const COMPANY: &str = "acme-co";
const PORT: u16 = 44991;
const TOKEN_PARENT: &str = "pppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppppp";
const TOKEN_CHILD: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

async fn migrate(pool: &DatabasePool, track: MigrationTrack) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, track).await.unwrap();
    migrate_database(pool, track, &migrations).await.unwrap();
}

fn session_digest(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"erpx-account-v1\0");
    hash.update(b"session");
    hash.update([0u8]);
    hash.update(raw.as_bytes());
    hex::encode(hash.finalize())
}

async fn seed(control: &DatabasePool, holding: &DatabasePool, now: i64) {
    let mut c = control.raw().acquire().await.unwrap();
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES('user','Synthetic','user@example.test',1,$1,$1)")
        .bind(now).execute(&mut *c).await.unwrap();
    // session.token stores the session-purpose digest, not the raw cookie secret.
    let parent_digest = session_digest(TOKEN_PARENT);
    let child_digest = session_digest(TOKEN_CHILD);
    sqlx::query("INSERT INTO session(id,\"userId\",token,\"expiresAt\",\"createdAt\",\"updatedAt\") VALUES('parent','user',$1,to_timestamp(($2::bigint+3600000)::double precision/1000)::text,to_timestamp($2::double precision/1000)::text,to_timestamp($2::double precision/1000)::text),('session','user',$3,to_timestamp(($2::bigint+86400000)::double precision/1000)::text,to_timestamp($2::double precision/1000)::text,to_timestamp($2::double precision/1000)::text)")
        .bind(parent_digest).bind(now).bind(child_digest).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('parent','account','development',NULL,$1,$2),('session',$3,'development','parent',$1,$2)")
        .bind(now).bind(now + 3600000).bind(format!("holding:development:h001:{HOST}")).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('browser','parent',1,$1)")
        .bind(now).execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('parent','browser','saved-parent')")
        .execute(&mut *c).await.unwrap();
    sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','acme','E2E','HOLDING_H001_DB','HOLDING_H001_READ_DB','HOLDING_H001_FILES','active')")
        .execute(&mut *c).await.unwrap();

    let mut h = holding.raw().acquire().await.unwrap();
    sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES($1,'company','acme','Acme',NULL,$1,NULL,'active')")
        .bind(COMPANY).execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','e2e_pms_reader','E2E',false,1,'active')")
        .execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','e2e_pms_reader',1,'[\"pms.read\",\"module.pms.read\"]','[]','[\"holding\"]','fixture',0)")
        .execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('grant','user','e2e_pms_reader','[\"pms\"]',$1,$2,$3,3,'holding')")
        .bind(COMPANY).bind(now - 1000).bind(now + 3600000).execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('session','user',$1,'grant',3,7,$2,$3)")
        .bind(COMPANY).bind(now + 3600000).bind(now).execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,state,config_json,version,created_by,updated_at) VALUES('prop-1',$1,'branch','Riverside','2026-11-04','open','{}',1,'maker',1)")
        .bind(COMPANY).execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES('res-1','prop-1','reservation','{\"number\":\"BK-1\"}',1,1)")
        .execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,checkout_id,checkout_url,status,expires_at,created_at,updated_at) VALUES('pay-1','prop-1','res-1',$1,'acme','acct-1',false,5000,'THB','cs-1','https://pay.example/cs-1','confirmed',99,1,1)")
        .bind(COMPANY).execute(&mut *h).await.unwrap();
    sqlx::query("INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,generation,created_at,updated_at) VALUES('req-1','pay-1',2000,'[]','Guest asked','user-1','failed',0,1,1)")
        .execute(&mut *h).await.unwrap();
}

async fn call(port: u16, path: &str, cookie: Option<&str>) -> (u16, serde_json::Value, String) {
    let mut headers = format!(
        "GET {path} HTTP/1.1\r\nHost: {HOST}\r\nOrigin: {ORIGIN}\r\nAccept: application/json\r\n"
    );
    if let Some(cookie) = cookie {
        headers.push_str(&format!("Cookie: __Host-erpx-product={cookie}\r\n"));
    }
    headers.push_str("Connection: close\r\n\r\n");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(headers.as_bytes()).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let response = String::from_utf8_lossy(&bytes);
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap();
    (
        status,
        serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        head.to_owned(),
    )
}

#[tokio::test]
async fn refund_context_end_to_end_against_native_service() {
    let admin_url = std::env::var("ERPX_TEST_ADMIN_URL")
        .expect("ERPX_TEST_ADMIN_URL required for isolated native Postgres tests");
    // Test-profile database URLs require a non-empty password; mint a random
    // one for this run on the pre-created erpx_test_admin role.
    let db_password: String = uuid::Uuid::new_v4().simple().to_string()[..16].to_string();
    let token: String = uuid::Uuid::new_v4().simple().to_string()[..10].to_string();
    let control_name = format!("erpx_test_{token}_control_db");
    let holding_name = format!("erpx_test_{token}_h001_db");
    let admin_options = PgConnectOptions::from_str(&admin_url).expect("invalid test admin URL");
    let mut admin = sqlx::PgConnection::connect_with(&admin_options)
        .await
        .unwrap();
    sqlx::raw_sql(AssertSqlSafe(format!(
        "ALTER ROLE erpx_test_admin WITH PASSWORD '{}'",
        db_password.replace('\'', "''")
    )))
    .execute(&mut admin)
    .await
    .unwrap();
    for name in [&control_name, &holding_name] {
        sqlx::raw_sql(AssertSqlSafe(format!("CREATE DATABASE \"{name}\" TEMPLATE template0")))
            .execute(&mut admin)
            .await
            .unwrap();
    }
    let control_config =
        DatabaseConfig::from_options(admin_options.clone().database(&control_name)).unwrap();
    let holding_config =
        DatabaseConfig::from_options(admin_options.clone().database(&holding_name)).unwrap();
    let limits = PoolLimits {
        max_connections: 4,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = connect_pool(&control_config, limits).await.unwrap();
    let holding = connect_pool(&holding_config, limits).await.unwrap();
    migrate(&control, MigrationTrack::Control).await;
    migrate(&holding, MigrationTrack::HoldingWrite).await;
    let now = now_ms();
    seed(&control, &holding, now).await;

    let config = serde_json::json!({
        "version": 1,
        "mode": "test",
        "environment": "development",
        "transport": "direct-loopback",
        "listen": format!("127.0.0.1:{PORT}"),
        "accountHost": "account.e2e.test",
        "requireEmailVerification": false,
        "passwordWorkers": 1,
        "maximumRequests": 16,
        "requestTimeoutMs": 10000,
        "control": { "url": format!("postgresql://erpx_test_admin:{db_password}@127.0.0.1:5432/{control_name}") },
        "holdings": [{
            "holdingId": "h001",
            "slug": "acme",
            "host": HOST,
            "writeBinding": "HOLDING_H001_DB",
            "readBinding": "HOLDING_H001_READ_DB",
            "filesBinding": "HOLDING_H001_FILES",
            "database": { "url": format!("postgresql://erpx_test_admin:{db_password}@127.0.0.1:5432/{holding_name}") }
        }],
        "operationsCompanies": [{ "holdingId": "h001", "companyId": COMPANY }],
        "posCompanies": [{ "holdingId": "h001", "companyId": COMPANY }],
        "crmCompanies": [{ "holdingId": "h001", "companyId": COMPANY }],
        "commerceCompanies": [{ "holdingId": "h001", "companyId": COMPANY }]
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private.json");
    std::fs::write(&path, serde_json::to_string(&config).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug/erpx-server");
    let mut child = std::process::Command::new(&binary)
        .arg("private-api")
        .arg("--test-config")
        .arg(&path)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("native test service spawns");
    let stderr = Arc::new(std::sync::Mutex::new(String::new()));
    {
        let stderr = stderr.clone();
        let mut err = child.stderr.take();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            if let Some(err) = err.as_mut() {
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut guard = stderr.lock().unwrap();
                    guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if guard.len() > 8000 {
                        let overflow = guard.len() - 8000;
                        guard.drain(..overflow);
                    }
                }
            }
        });
    }
    let dead = Arc::new(AtomicBool::new(false));
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            let log = stderr.lock().unwrap().clone();
            panic!("native test service exited during boot: {log}");
        }
        match TcpStream::connect(("127.0.0.1", PORT)).await {
            Ok(_) => break,
            Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(error) => panic!("native test service never listened: {error}"),
        }
    }
    assert!(!dead.load(Ordering::SeqCst), "native test service exited during boot");
    // Health gate before exercising the route.
    let (health, _, _) = call(PORT, "/api/health", None).await;
    assert_eq!(health, 200);

    let (status, value, head) = call(
        PORT,
        "/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1",
        Some(TOKEN_CHILD),
    )
    .await;
    assert_eq!(status, 200, "{value}");
    assert_eq!(value["data"]["id"], "req-1");
    assert_eq!(value["data"]["property_id"], "prop-1");
    assert_eq!(value["data"]["account_id"], "acct-1");
    assert_eq!(value["data"]["livemode"], false);
    assert_eq!(value["data"]["state"], "failed");
    assert!(head.contains("cache-control: private, no-store"));
    assert!(head.contains("referrer-policy: no-referrer"));

    let (absent, absent_value, _) = call(
        PORT,
        "/api/v1/holdings/acme/pms/booking-refunds/context?request=nope",
        Some(TOKEN_CHILD),
    )
    .await;
    assert_eq!(absent, 404, "{absent_value}");
    assert_eq!(absent_value["error"]["code"], "PMS_REFUND_CONTEXT_ABSENT");

    let (bad, _, _) = call(
        PORT,
        "/api/v1/holdings/acme/pms/booking-refunds/context?unknown=1",
        Some(TOKEN_CHILD),
    )
    .await;
    assert_eq!(bad, 422);

    let (denied, _, _) = call(
        PORT,
        "/api/v1/holdings/acme/pms/booking-refunds/context?request=req-1",
        None,
    )
    .await;
    assert!(denied == 401 || denied == 404, "unauthenticated must not read: {denied}");

    let _ = child.kill();
    let _ = child.wait();
    control.close().await;
    holding.close().await;
    for name in [&control_name, &holding_name] {
        sqlx::raw_sql(AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{name}\"")))
            .execute(&mut admin)
            .await
            .unwrap();
    }
}
