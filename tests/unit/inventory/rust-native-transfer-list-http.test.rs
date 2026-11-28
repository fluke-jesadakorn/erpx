//! Bounded HTTP contract tests for the native Inventory transfer-list read.
//!
//! The owner router is exercised without PostgreSQL through a reader-injected
//! constructor and a stub `InventoryTransferListRead`. The cases pin the exact
//! registered `(host, holding_slug)` pair, the Company/branch headers forwarded
//! to the reader, the empty-body boundary, the POST-only route, and the exact
//! status/JSON envelope.
//!
//! INTEGRATION BLOCKER: `modules/inventory/server/src/adapters/native_transfer_list`
//! currently exports only `router` and `router_for_holdings`, both of which
//! require a connected `DatabasePool`; there is no reader-injected constructor.
//! This target expects
//! `router_with_reader(Arc<dyn InventoryTransferListRead>, Vec<TransferTarget>)`
//! plus its `mod.rs` re-export, exactly as the other owner-local routers expose
//! it (see `modules/talent/server/src/adapters/native_talent_detail/http.rs`).
//! Until that hook is added this target does not compile.

use erpx_inventory_server::public::native_transfer_list::{
    router_with_reader, InventoryTransferListRead, TransferListFuture, TransferListRequest,
    TransferTarget,
};
use erpx_platform_runtime_server::public::http_support::{serde_json, serve, Router, Value};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

const ROUTE: &str = "/api/v1/holdings/acme/inventory/native-transfer-list/read";
const HOST: &str = "acme.example.test";
const OTHER_HOST: &str = "northstar.example.test";
const INVALID_MESSAGE: &str = "Use one bounded native Inventory transfer read.";
const NOT_FOUND_MESSAGE: &str = "Not found.";
const COMPANY_INVALID: &str = "INVENTORY_TRANSFER_LIST_COMPANY_INVALID";
const BRANCH_INVALID: &str = "INVENTORY_TRANSFER_LIST_BRANCH_INVALID";
const REQUEST_INVALID: &str = "INVENTORY_TRANSFER_LIST_REQUEST_INVALID";
const UNAVAILABLE: &str = "The native Inventory transfer reader is unavailable.";

type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

struct Probe {
    seen: Seen,
    rows: Vec<Value>,
    fail: Option<(FailureKind, &'static str, &'static str)>,
}

impl InventoryTransferListRead for Probe {
    fn list<'a>(&'a self, request: &'a TransferListRequest) -> TransferListFuture<'a, Vec<Value>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push((request.company.clone(), request.branch.clone()));
            match self.fail {
                Some((kind, code, message)) => Err(BusinessError::new(kind, code, message)),
                None => Ok(self.rows.clone()),
            }
        })
    }
}

fn probe(rows: Vec<Value>) -> (Arc<dyn InventoryTransferListRead>, Seen) {
    let seen = Seen::default();
    let reader: Arc<dyn InventoryTransferListRead> = Arc::new(Probe {
        seen: seen.clone(),
        rows,
        fail: None,
    });
    (reader, seen)
}

fn failing_probe(
    kind: FailureKind,
    code: &'static str,
    message: &'static str,
) -> Arc<dyn InventoryTransferListRead> {
    Arc::new(Probe {
        seen: Seen::default(),
        rows: Vec::new(),
        fail: Some((kind, code, message)),
    })
}

fn target(host: &str, slug: &str) -> TransferTarget {
    TransferTarget {
        host: host.into(),
        holding_slug: slug.into(),
    }
}

fn router(reader: Arc<dyn InventoryTransferListRead>, targets: Vec<TransferTarget>) -> Router {
    router_with_reader(reader, targets).expect("router configuration")
}

fn scope(host: &str, company: Option<&str>, branch: Option<&str>) -> Vec<(String, String)> {
    let mut headers = vec![("Host".to_string(), host.to_string())];
    if let Some(company) = company {
        headers.push(("x-erpx-context-company".to_string(), company.to_string()));
    }
    if let Some(branch) = branch {
        headers.push(("x-erpx-workspace-branch".to_string(), branch.to_string()));
    }
    headers
}

async fn start(router: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let addr = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let _ = serve(listener, router).await;
    });
    addr
}

struct Reply {
    status: u16,
    json: Value,
    head: String,
}

async fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &str,
) -> Reply {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut raw = format!("{method} {path} HTTP/1.1\r\n");
    for (name, value) in headers {
        raw.push_str(&format!("{name}: {value}\r\n"));
    }
    raw.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ));

    let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    stream.write_all(raw.as_bytes()).await.expect("write");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("read");
    let text = String::from_utf8_lossy(&bytes).to_string();
    let (raw_head, raw_body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let status = raw_head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
        .expect("status line");
    Reply {
        status,
        json: serde_json::from_str(raw_body).unwrap_or(Value::Null),
        head: raw_head.to_ascii_lowercase(),
    }
}

fn assert_error(reply: &Reply, status: u16, code: &str, message: &str) {
    assert_eq!(reply.status, status, "{}", reply.head);
    assert_eq!(
        reply.json,
        serde_json::json!({"error": {"code": code, "message": message}})
    );
}

#[tokio::test]
async fn registered_pair_returns_exact_rows_and_scope() {
    let rows = vec![serde_json::json!({
        "id": "transfer-1",
        "company_id": "company-1",
        "source_warehouse_id": "warehouse-a",
        "target_warehouse_id": "warehouse-b",
        "source_warehouse_name": "Main",
        "target_warehouse_name": "Shop",
        "currency": "THB",
        "status": "completed"
    })];
    let (reader, seen) = probe(rows.clone());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;

    let reply = call(
        addr,
        "POST",
        ROUTE,
        &scope(HOST, Some("company-1"), Some("branch-1")),
        "",
    )
    .await;

    assert_eq!(reply.status, 200, "{}", reply.head);
    assert_eq!(reply.json, serde_json::json!({"rows": rows}));
    assert!(reply.head.contains("content-type: application/json"));
    assert!(reply.head.contains("cache-control: private, no-store"));
    assert!(reply.head.contains("referrer-policy: no-referrer"));
    assert!(reply.head.contains("x-content-type-options: nosniff"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "company-1");
    assert_eq!(seen[0].1.as_deref(), Some("branch-1"));
}

#[tokio::test]
async fn holding_wide_scope_forwards_no_branch() {
    let (reader, seen) = probe(Vec::new());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;

    let reply = call(
        addr,
        "POST",
        ROUTE,
        &scope(HOST, Some("company-1"), None),
        "",
    )
    .await;

    assert_eq!(reply.status, 200, "{}", reply.head);
    assert_eq!(reply.json, serde_json::json!({"rows": []}));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, "company-1");
    assert!(seen[0].1.is_none());
}

#[tokio::test]
async fn target_isolation_requires_the_exact_host_and_slug_pair() {
    let (reader, seen) = probe(vec![serde_json::json!({"id": "transfer-1"})]);
    let addr = start(router(
        reader,
        vec![target(HOST, "acme"), target(OTHER_HOST, "northstar")],
    ))
    .await;

    let foreign_host = call(
        addr,
        "POST",
        ROUTE,
        &scope(OTHER_HOST, Some("company-1"), None),
        "",
    )
    .await;
    assert_error(&foreign_host, 404, "NOT_FOUND", NOT_FOUND_MESSAGE);

    let foreign_slug = call(
        addr,
        "POST",
        "/api/v1/holdings/northstar/inventory/native-transfer-list/read",
        &scope(HOST, Some("company-1"), None),
        "",
    )
    .await;
    assert_error(&foreign_slug, 404, "NOT_FOUND", NOT_FOUND_MESSAGE);

    let unregistered_host = call(
        addr,
        "POST",
        ROUTE,
        &scope("attacker.example.test", Some("company-1"), None),
        "",
    )
    .await;
    assert_error(&unregistered_host, 404, "NOT_FOUND", NOT_FOUND_MESSAGE);

    let case_variant = call(
        addr,
        "POST",
        ROUTE,
        &scope("ACME.EXAMPLE.TEST", Some("company-1"), None),
        "",
    )
    .await;
    assert_error(&case_variant, 404, "NOT_FOUND", NOT_FOUND_MESSAGE);

    assert!(seen.lock().unwrap().is_empty());

    let own = call(
        addr,
        "POST",
        ROUTE,
        &scope(HOST, Some("company-1"), None),
        "",
    )
    .await;
    assert_eq!(own.status, 200, "{}", own.head);
    assert_eq!(
        own.json,
        serde_json::json!({"rows": [{"id": "transfer-1"}]})
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn duplicate_scope_headers_are_rejected_before_the_reader() {
    let (reader, seen) = probe(Vec::new());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;

    let mut duplicate_company = scope(HOST, Some("company-1"), None);
    duplicate_company.push(("x-erpx-context-company".into(), "company-2".into()));
    let reply = call(addr, "POST", ROUTE, &duplicate_company, "").await;
    assert_error(&reply, 422, REQUEST_INVALID, INVALID_MESSAGE);

    let mut duplicate_branch = scope(HOST, Some("company-1"), Some("branch-1"));
    duplicate_branch.push(("x-erpx-workspace-branch".into(), "branch-2".into()));
    let reply = call(addr, "POST", ROUTE, &duplicate_branch, "").await;
    assert_error(&reply, 422, REQUEST_INVALID, INVALID_MESSAGE);

    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn company_and_branch_scope_are_bounded() {
    let (reader, seen) = probe(Vec::new());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;
    let long = "x".repeat(201);

    for (company, branch, code) in [
        (None, None, COMPANY_INVALID),
        (Some("bad company"), None, COMPANY_INVALID),
        (Some(long.as_str()), None, COMPANY_INVALID),
        (Some("company-1"), Some("bad branch"), BRANCH_INVALID),
        (Some("company-1"), Some(long.as_str()), BRANCH_INVALID),
    ] {
        let reply = call(addr, "POST", ROUTE, &scope(HOST, company, branch), "").await;
        assert_error(&reply, 422, code, INVALID_MESSAGE);
    }
    assert!(seen.lock().unwrap().is_empty());

    let empty_branch = call(
        addr,
        "POST",
        ROUTE,
        &scope(HOST, Some("company-1"), Some("")),
        "",
    )
    .await;
    assert_eq!(empty_branch.status, 200, "{}", empty_branch.head);
    assert_eq!(empty_branch.json, serde_json::json!({"rows": []}));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].1.is_none());
}

#[tokio::test]
async fn request_body_is_empty_or_an_empty_object_only() {
    let (reader, seen) = probe(Vec::new());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;
    let scope = scope(HOST, Some("company-1"), None);

    for body in ["", "{}"] {
        let reply = call(addr, "POST", ROUTE, &scope, body).await;
        assert_eq!(reply.status, 200, "body {body:?}: {}", reply.head);
        assert_eq!(reply.json, serde_json::json!({"rows": []}));
    }

    let oversized = format!(r#"{{"pad":"{}"}}"#, "x".repeat(1024));
    assert!(oversized.len() > 1024);
    for body in [
        r#"{"company":"company-2"}"#,
        r#"{"company":null}"#,
        "[]",
        "null",
        "not json",
        oversized.as_str(),
    ] {
        let reply = call(addr, "POST", ROUTE, &scope, body).await;
        assert_error(&reply, 422, REQUEST_INVALID, INVALID_MESSAGE);
    }

    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn reader_failures_keep_the_closed_status_envelope() {
    for (kind, code, message, status) in [
        (
            FailureKind::Unavailable,
            "INVENTORY_TRANSFER_LIST_UNAVAILABLE",
            UNAVAILABLE,
            503,
        ),
        (
            FailureKind::Forbidden,
            "INVENTORY_TRANSFER_LIST_FORBIDDEN",
            "Synthetic forbidden transfer read.",
            403,
        ),
        (
            FailureKind::Conflict,
            "INVENTORY_TRANSFER_LIST_CHANGED",
            "Synthetic changed transfer read.",
            409,
        ),
    ] {
        let addr = start(router(
            failing_probe(kind, code, message),
            vec![target(HOST, "acme")],
        ))
        .await;
        let reply = call(
            addr,
            "POST",
            ROUTE,
            &scope(HOST, Some("company-1"), None),
            "",
        )
        .await;
        assert_error(&reply, status, code, message);
        assert!(!reply.head.contains("cache-control"));
    }
}

#[tokio::test]
async fn route_is_post_only() {
    let (reader, seen) = probe(Vec::new());
    let addr = start(router(reader, vec![target(HOST, "acme")])).await;

    for method in ["GET", "PUT", "DELETE"] {
        let reply = call(
            addr,
            method,
            ROUTE,
            &scope(HOST, Some("company-1"), None),
            "",
        )
        .await;
        assert_eq!(reply.status, 405, "{method}: {}", reply.head);
    }

    let wrong_path = call(
        addr,
        "POST",
        "/api/v1/holdings/acme/inventory/native-transfer-list",
        &scope(HOST, Some("company-1"), None),
        "",
    )
    .await;
    assert_eq!(wrong_path.status, 404, "{}", wrong_path.head);
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn reader_injected_router_requires_distinct_exact_targets() {
    let reader = || probe(Vec::new()).0;

    assert!(router_with_reader(reader(), vec![]).is_err());
    assert!(
        router_with_reader(reader(), vec![target(HOST, "acme"), target(HOST, "acme")]).is_err()
    );
    assert!(router_with_reader(reader(), vec![target("", "acme")]).is_err());
    assert!(router_with_reader(reader(), vec![target(HOST, "Acme")]).is_err());
    assert!(router_with_reader(
        reader(),
        vec![target(HOST, "acme"), target(OTHER_HOST, "northstar")]
    )
    .is_ok());
}
