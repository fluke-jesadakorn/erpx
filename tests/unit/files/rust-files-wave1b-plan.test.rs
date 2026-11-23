//! Caller-unit statement plan for the native Files command (SR_058 slice).
//!
//! Pure owner tests: no database, no HTTP authority. The plan converts owner
//! `$n` SQL to the caller `?` dialect with one value per occurrence and builds
//! the receipt/audit/outbox rows with `jsonb_build_object` for execution in
//! the caller's already-open unit, so the files command never opens a second
//! transaction while the caller holds attachment row locks.
use erpx_server_files::public::files_wave1b::{
    FilesCommandInput, RD_PROJECT_ATTACHMENT_TARGET, is_read, plan,
};
use erpx_server_files::public::files_wave1b_http::{
    FilesCommandBackend, FilesCommandFuture, FilesCommandPlanFuture, FilesCommandRequest,
    FilesCommandTarget, files_command_router,
};
use erpx_server_runtime::public::http_support::{serve, serde_json};
use erpx_server_shared::public::CompanyId;
use std::sync::Arc;

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn input(kind: &str, values: serde_json::Value, now: i64) -> FilesCommandInput {
    let values = values.as_array().cloned().unwrap_or_default();
    FilesCommandInput {
        kind: kind.into(),
        values,
        now,
        company_id: "company".into(),
    }
}

#[test]
fn sr058_plan_converts_placeholders_and_keeps_ledger_shape() {
    let now = 1_700_000_000_000i64;
    let statements = plan(
        &company(),
        "test-key-0001",
        &input(
            "SR_058",
            serde_json::json!([now, "record", "mfg-event-k", "link-1", "company"]),
            now,
        ),
    )
    .unwrap();
    assert_eq!(statements.len(), 4);

    // Owner SQL: `$n` becomes caller `?`, one value per occurrence, in order.
    assert_eq!(
        statements[0].sql,
        "UPDATE attachment_links l SET state='bound',bound_at=?,binding_type=?,binding_id=? \
         FROM attachment_sets s WHERE l.id=? AND l.set_id=s.id AND s.company_id=? AND l.state='draft'"
    );
    assert_eq!(
        statements[0].values,
        vec![
            serde_json::json!(now),
            serde_json::json!("record"),
            serde_json::json!("mfg-event-k"),
            serde_json::json!("link-1"),
            serde_json::json!("company"),
        ]
    );

    // Receipt: same columns/conflict target as the executed path, jsonb payload.
    assert!(statements[1].sql.contains(
        "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json)"
    ));
    assert!(statements[1].sql.contains("ON CONFLICT(command_id) DO NOTHING"));
    assert!(statements[1]
        .sql
        .contains("jsonb_build_object('kind', 'SR_058', 'rows', 0)"));
    assert!(statements[1].sql.contains("'files-test-key-0001'"));
    assert!(statements[1].sql.contains("'files.command.SR_058'"));
    assert!(statements[1].sql.contains("'test-key-0001'"));
    assert!(statements[1].values.is_empty());

    // Audit: same columns/conflict target, authority read back from the
    // receipt written earlier in the same caller unit (executed-path shape).
    assert!(statements[2].sql.contains(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json)"
    ));
    assert!(statements[2].sql.contains("ON CONFLICT(id) DO NOTHING"));
    assert!(statements[2].sql.contains(
        "(SELECT authority_json FROM command_receipts WHERE command_id = 'files-test-key-0001')"
    ));
    assert!(statements[2].sql.contains("'files-audit-test-key-0001'"));
    assert!(statements[2].values.is_empty());

    // Outbox: same columns/conflict target, jsonb payload.
    assert!(statements[3].sql.contains(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at)"
    ));
    assert!(statements[3].sql.contains("ON CONFLICT(id) DO NOTHING"));
    assert!(statements[3]
        .sql
        .contains("jsonb_build_object('kind', 'SR_058', 'rows', 0)"));
    assert!(statements[3].sql.contains("'files-outbox-test-key-0001'"));
    assert!(statements[3].values.is_empty());
}

#[test]
fn rd_project_attachment_target_plans_owner_read_without_write_ledger() {
    // Native successor of the retired TypeScript project-attachment selector.
    // The owner SQL lives here, not in server-files TS; the caller's unit is
    // the read authority, so no receipt/audit/outbox statements are planned.
    let now = 1_700_000_000_000i64;
    assert!(is_read(RD_PROJECT_ATTACHMENT_TARGET));
    assert!(!is_read("SR_058"));
    let statements = plan(
        &company(),
        "test-key-project-target",
        &input(
            RD_PROJECT_ATTACHMENT_TARGET,
            serde_json::json!(["set-1", "company"]),
            now,
        ),
    )
    .unwrap();
    assert_eq!(statements.len(), 1);
    assert_eq!(
        statements[0].sql,
        "SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?"
    );
    assert_eq!(
        statements[0].values,
        vec![serde_json::json!("set-1"), serde_json::json!("company")]
    );
    let ledger = statements[0].sql.to_ascii_uppercase();
    assert!(!ledger.contains("INSERT"));
    assert!(!ledger.contains("UPDATE"));
    assert!(!ledger.contains("DELETE"));
}

#[test]
fn plan_expands_repeated_placeholders_one_value_per_occurrence() {
    // SR_065 reuses `$1` twice with a single distinct bind.
    let now = 1_700_000_000_000i64;
    let statements = plan(
        &company(),
        "test-key-0002",
        &input("SR_065", serde_json::json!([now]), now),
    )
    .unwrap();
    assert_eq!(statements.len(), 4);
    assert_eq!(statements[0].sql.matches('?').count(), 2);
    assert!(!statements[0].sql.contains('$'));
    assert_eq!(
        statements[0].values,
        vec![serde_json::json!(now), serde_json::json!(now)]
    );
}

#[test]
fn plan_preserves_casts_on_converted_placeholders() {
    // SR_001 carries a `$3::text` cast; the caller dialect keeps the cast.
    let now = 1_700_000_000_000i64;
    let statements = plan(
        &company(),
        "test-key-0003",
        &input(
            "SR_001",
            serde_json::json!(["event-1", "company", "set-9", "actor", "reused", "{}", now]),
            now,
        ),
    )
    .unwrap();
    assert!(statements[0].sql.contains("?::text"));
    assert!(!statements[0].sql.contains('$'));
}

#[test]
fn plan_rejects_unknown_kind_and_arity() {
    let now = 1_700_000_000_000i64;
    let error = plan(&company(), "test-key-0004", &input("NOPE", serde_json::json!([]), now))
        .unwrap_err();
    assert_eq!(error.code, "FILES_COMMAND_INVALID");
    // SR_058 binds five distinct placeholders; four values fail closed.
    let error = plan(
        &company(),
        "test-key-0005",
        &input("SR_058", serde_json::json!([now, "record", "mfg-event-k", "link-1"]), now),
    )
    .unwrap_err();
    assert_eq!(error.code, "FILES_COMMAND_INVALID");
    // SR_065 binds one distinct placeholder; two values fail closed.
    let error = plan(
        &company(),
        "test-key-0006",
        &input("SR_065", serde_json::json!([now, now]), now),
    )
    .unwrap_err();
    assert_eq!(error.code, "FILES_COMMAND_INVALID");
}

#[test]
fn brand_publication_kinds_plan_byte_identical_sql() {
    let now = 1_700_000_000_000i64;
    // 71publish: profile upsert for the primary publication column.
    let statements = plan(
        &company(),
        "test-key-brand-71p",
        &input(
            "BRAND_PUB_71P",
            serde_json::json!(["holding", "scope-1", "company", "holding", "user-1", now, "pub-1"]),
            now,
        ),
    )
    .unwrap();
    assert_eq!(statements.len(), 4);
    assert_eq!(
        statements[0].sql,
        "INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at,primary_publication_id) VALUES(?,?,?,?,?,?,?) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET primary_publication_id=excluded.primary_publication_id,updated_by=excluded.updated_by,updated_at=excluded.updated_at"
    );
    assert_eq!(statements[0].values.len(), 7);
    // 71 inverse writes the inverse column instead.
    let statements = plan(
        &company(),
        "test-key-brand-71i",
        &input(
            "BRAND_PUB_71I",
            serde_json::json!(["branch", "scope-2", "company", "branch", "user-1", now, "pub-2"]),
            now,
        ),
    )
    .unwrap();
    assert!(statements[0]
        .sql
        .contains("inverse_publication_id=excluded.inverse_publication_id"));
    // 72 publishes the storefront logo from the company row.
    let statements = plan(
        &company(),
        "test-key-brand-72",
        &input("BRAND_PUB_72", serde_json::json!(["user-1", now, "pub-3", "company"]), now),
    )
    .unwrap();
    assert!(statements[0].sql.contains("logo_publication_id"));
    assert_eq!(statements[0].values.len(), 4);
    // 73 revokes by exact publication match; 74 clears the storefront logo.
    let statements = plan(
        &company(),
        "test-key-brand-73p",
        &input("BRAND_PUB_73P", serde_json::json!(["holding", "scope-1", "pub-1"]), now),
    )
    .unwrap();
    assert!(statements[0].sql.contains("primary_publication_id=NULL"));
    let statements = plan(
        &company(),
        "test-key-brand-74",
        &input("BRAND_PUB_74", serde_json::json!(["company", "pub-3"]), now),
    )
    .unwrap();
    assert!(statements[0].sql.contains("logo_publication_id=NULL"));
    assert_eq!(statements[0].values.len(), 2);
    // Wrong arity fails closed per kind.
    let error = plan(
        &company(),
        "test-key-brand-arity",
        &input("BRAND_PUB_74", serde_json::json!(["company"]), now),
    )
    .unwrap_err();
    assert_eq!(error.code, "FILES_COMMAND_INVALID");
}

// ---------------------------------------------------------------------------
// Transport entrypoint: the plan route proves Host/Holding/Origin/key, then
// serves the owner plan for execution in the caller's unit.
// ---------------------------------------------------------------------------

struct Stub;

impl FilesCommandBackend for Stub {
    fn execute<'a>(&'a self, _: FilesCommandRequest<'a>) -> FilesCommandFuture<'a> {
        Box::pin(async {
            Err(erpx_server_shared::public::BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "FILES_COMMAND_UNAVAILABLE",
                "The native Files command unit is unavailable.",
            ))
        })
    }

    fn plan<'a>(&'a self, request: FilesCommandRequest<'a>) -> FilesCommandPlanFuture<'a> {
        Box::pin(async move {
            let company = request
                .headers
                .get("x-erpx-context-company")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            let company = CompanyId::new(company).map_err(|_| {
                erpx_server_shared::public::BusinessError::new(
                    erpx_server_shared::public::FailureKind::InvalidInput,
                    "FILES_WAVE1B_REQUEST_INVALID",
                    "Review the Files command request.",
                )
            })?;
            plan(&company, request.key.as_str(), request.input)
        })
    }
}

fn target(backend: Arc<dyn FilesCommandBackend>) -> FilesCommandTarget {
    FilesCommandTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend,
    }
}

async fn http(addr: std::net::SocketAddr, raw: &[u8]) -> (u16, serde_json::Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(raw).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
    (status, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
}

fn request(
    method: &str,
    path: &str,
    origin: Option<&str>,
    key: Option<&str>,
    body: &[u8],
) -> Vec<u8> {
    let mut raw = format!("{method} {path} HTTP/1.1\r\nHost: acme.example.test\r\n");
    if let Some(origin) = origin {
        raw.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(key) = key {
        raw.push_str(&format!("Idempotency-Key: {key}\r\n"));
    }
    raw.push_str("Content-Type: application/json\r\n");
    raw.push_str("x-erpx-context-company: company\r\n");
    raw.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = raw.into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

const PLAN: &str = "/api/v1/holdings/acme/files/command-plan";

#[tokio::test]
async fn command_plan_serves_the_owner_plan_for_the_caller_unit() {
    let router = files_command_router(vec![target(Arc::new(Stub))]).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });

    let body = serde_json::json!({
        "kind": "SR_058",
        "values": [1_700_000_000_000i64, "record", "mfg-event-k", "link-1", "company"],
        "now": 1_700_000_000_000i64,
        "companyId": "company",
    });
    let bytes = serde_json::to_vec(&body).unwrap();
    let (status, payload) = http(
        addr,
        &request(
            "POST",
            PLAN,
            Some("https://acme.example.test"),
            Some("test-key-0001"),
            &bytes,
        ),
    )
    .await;
    assert_eq!(status, 200);
    let statements = payload["data"]["statements"].as_array().unwrap();
    assert_eq!(statements.len(), 4);
    assert_eq!(statements[0]["sql"].as_str().unwrap().matches('?').count(), 5);
    assert_eq!(statements[0]["values"].as_array().unwrap().len(), 5);

    // A foreign origin never reaches the owner plan.
    let (status, payload) = http(
        addr,
        &request("POST", PLAN, Some("https://evil.example.test"), Some("test-key-0001"), &bytes),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(payload["error"]["code"], "ORIGIN_FORBIDDEN");

    // An unknown holding never reaches the owner plan.
    let (status, _) = http(
        addr,
        &request(
            "POST",
            "/api/v1/holdings/unknown/files/command-plan",
            Some("https://acme.example.test"),
            Some("test-key-0001"),
            &bytes,
        ),
    )
    .await;
    assert_eq!(status, 404);
}
