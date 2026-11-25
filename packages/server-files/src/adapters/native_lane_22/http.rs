//! Owner-local axum router for the native Files work-evidence read.
//!
//! Mirrors `server-commerce::adapters::quality_product_read`: a registered
//! Holding host + slug pair is the only target, Company and record identities
//! arrive as unique headers, and every failure is mapped to a bounded error
//! body. The router transports the owner reader's complete payload; it never
//! plans SQL and never accepts a caller-supplied statement.

use std::collections::BTreeMap;
use std::sync::Arc;

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};

use super::application::WorkEvidenceRead;
use super::domain::{WorkEvidenceKind, parse_read_only, valid_id};
use super::postgres::PgWorkEvidenceRead;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_RECORD: HeaderName = HeaderName::from_static("x-erpx-context-record");
const READ_ONLY: HeaderName = HeaderName::from_static("x-erpx-files-read-only");

/// Exact registered Holding target. No wildcard or fallback host is accepted.
pub struct WorkEvidenceTarget {
    pub host: String,
    pub holding_slug: String,
}

/// Alias used by the lead's composition contract.
pub type LaneTarget = WorkEvidenceTarget;

struct WorkEvidenceReadState {
    targets: BTreeMap<(String, String), Arc<dyn WorkEvidenceRead>>,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_EVIDENCE_READ_UNAVAILABLE",
        "The native Files work-evidence service is unavailable.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "WORK_EVIDENCE_READ_REQUEST_INVALID",
        "Use one bounded Files work-evidence read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "WORK_EVIDENCE_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn query_value(request: &Request, key: &str) -> Option<String> {
    request.uri().query().and_then(|raw| {
        let mut found = None;
        for part in raw.split('&') {
            let mut pair = part.splitn(2, '=');
            if pair.next() == Some(key) {
                if found.is_some() {
                    return None;
                }
                found = pair.next().map(str::to_owned);
            }
        }
        found
    })
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Validate the registered targets and build the state map keyed by
/// `(host, holding_slug)`. Every entry carries its own reader; callers decide
/// whether that reader is shared (`router`) or holding-specific
/// (`router_for_holdings`).
fn build_state(
    entries: impl IntoIterator<Item = (WorkEvidenceTarget, Arc<dyn WorkEvidenceRead>)>,
) -> Result<Arc<WorkEvidenceReadState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(WorkEvidenceReadState {
        targets: configured,
    }))
}

fn router_from_state(state: Arc<WorkEvidenceReadState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/files/work-evidence",
            get(read),
        )
        .with_state(state)
}

/// Build the native work-evidence router. The lead mounts it alongside the
/// other Files owner routers; no composition code needs to know the SQL.
pub fn router(
    reader: Arc<dyn WorkEvidenceRead>,
    targets: Vec<WorkEvidenceTarget>,
) -> Result<Router, BusinessError> {
    Ok(router_from_state(build_state(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

/// Convenience form matching the literal router contract: the lead passes the
/// owner pool and the registered targets and receives the mounted axum router.
pub fn router_with_pool(
    pool: DatabasePool,
    targets: Vec<WorkEvidenceTarget>,
) -> Result<Router, BusinessError> {
    router(Arc::new(PgWorkEvidenceRead::new(pool)), targets)
}

/// Per-holding constructor: each `(target, pool)` pair produces its own
/// `PgWorkEvidenceRead`, inserted into the state map under that target's
/// `(host, holding_slug)`. Two Holdings never share a reader or a pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(router_from_state(build_state(
        targets.into_iter().map(|(target, pool)| {
            let reader: Arc<dyn WorkEvidenceRead> = Arc::new(PgWorkEvidenceRead::new(pool));
            (target, reader)
        }),
    )?))
}

async fn read(
    State(state): State<Arc<WorkEvidenceReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value) && RecordId::new(*value).is_ok())
            .ok_or_else(invalid)?;
        let record = unique_header(request.headers(), CONTEXT_RECORD)?
            .filter(|value| valid_id(value) && RecordId::new(*value).is_ok())
            .ok_or_else(invalid)?;
        let raw_kind = query_value(&request, "kind").ok_or_else(invalid)?;
        let kind = WorkEvidenceKind::parse(&raw_kind).ok_or_else(invalid)?;
        let read_only = parse_read_only(unique_header(request.headers(), READ_ONLY)?)
            .map_err(|_| invalid())?;
        let response = reader.read(company, record, kind, read_only).await?;
        let body = serde_json::to_value(response).map_err(|_| unavailable())?;
        Ok::<_, BusinessError>(body)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::domain::{WorkEvidenceResponse, WorkEvidenceRow};
    use erpx_server_runtime::public::http_support::serve;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Probe {
        calls: Arc<Mutex<Vec<(String, String, String, bool)>>>,
        rows: Vec<WorkEvidenceRow>,
    }

    impl WorkEvidenceRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            record: &'a str,
            kind: WorkEvidenceKind,
            read_only: bool,
        ) -> Pin<Box<dyn Future<Output = Result<WorkEvidenceResponse, BusinessError>> + Send + 'a>>
        {
            let calls = self.calls.clone();
            let rows = self.rows.clone();
            let company = company.to_owned();
            let record = record.to_owned();
            Box::pin(async move {
                calls.lock().unwrap().push((
                    company,
                    record,
                    kind.as_str().to_owned(),
                    read_only,
                ));
                Ok(WorkEvidenceResponse {
                    kind: kind.as_str().to_owned(),
                    rows,
                })
            })
        }
    }

    fn target() -> WorkEvidenceTarget {
        WorkEvidenceTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    fn probe() -> Arc<Probe> {
        Arc::new(Probe {
            calls: Arc::new(Mutex::new(Vec::new())),
            rows: vec![WorkEvidenceRow {
                id: "link-1".into(),
                set_id: Some("set-1".into()),
                purpose_key: "medical_certificate".into(),
                original_name: "certificate.pdf".into(),
                bytes: Some(4096),
                media_type: Some("application/pdf".into()),
                source_state: Some("available".into()),
                link_state: "bound".into(),
                preview_available: true,
                created_at: 1_760_000_000_000,
            }],
        })
    }

    fn probe_with_id(id: &str) -> Arc<Probe> {
        let mut reader = (*probe()).clone();
        reader.rows[0].id = id.to_owned();
        Arc::new(reader)
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
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

    async fn serve_probe(
        reader: Arc<Probe>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let router = router(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(host: &str, company: &str, record: &str, kind: &str, read_only: Option<&str>) -> String {
        let flag = read_only
            .map(|value| format!("x-erpx-files-read-only: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "GET /api/v1/holdings/acme/files/work-evidence?kind={kind} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\
             x-erpx-context-record: {record}\r\n\
             {flag}\r\n"
        )
    }

    fn request_at(slug: &str, host: &str, company: &str, record: &str, kind: &str) -> String {
        format!(
            "GET /api/v1/holdings/{slug}/files/work-evidence?kind={kind} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-context-company: {company}\r\n\
             x-erpx-context-record: {record}\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn complete_payload_and_company_scope() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company-1", "leave-1", "sets", Some("true")),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["kind"], "sets");
        assert_eq!(body["rows"][0]["id"], "link-1");
        assert_eq!(body["rows"][0]["preview_available"], true);
        assert_eq!(body["rows"][0]["bytes"], 4096);
        assert_eq!(
            *reader.calls.lock().unwrap(),
            vec![("company-1".to_owned(), "leave-1".to_owned(), "sets".to_owned(), true)]
        );
    }

    #[tokio::test]
    async fn legacy_kind_is_bounded() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("acme.example.test", "company-1", "leave-1", "legacy", None),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["kind"], "legacy");
        assert_eq!(
            reader.calls.lock().unwrap()[0].2,
            "legacy"
        );
    }

    #[tokio::test]
    async fn wrong_host_fails_closed() {
        let (port, server) = serve_probe(probe()).await;
        let (status, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("attacker.example.test", "company-1", "leave-1", "sets", None),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn invalid_request_fails_closed() {
        let reader = probe();
        let (port, server) = serve_probe(reader.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for raw in [
            request("acme.example.test", "bad company!", "leave-1", "sets", None),
            request("acme.example.test", "company-1", "leave-1", "drop", None),
            request("acme.example.test", "company-1", "leave-1", "sets", Some("1")),
        ] {
            let (status, body) = http(addr, raw).await;
            assert_eq!(status, 422);
            assert_eq!(body["error"]["code"], "WORK_EVIDENCE_READ_REQUEST_INVALID");
        }
        server.abort();
        assert!(reader.calls.lock().unwrap().is_empty());
    }

    /// `router_for_holdings` builds one `PgWorkEvidenceRead` per (target, pool)
    /// through `build_state`. This test drives that exact state map with two
    /// distinguishable probe readers and proves the key routes to its own
    /// reader, so no Holding can reach another Holding's pool.
    #[tokio::test]
    async fn per_holding_targets_route_to_distinct_readers() {
        let acme = probe_with_id("acme-row");
        let northstar = probe_with_id("northstar-row");
        let state = build_state(vec![
            (
                target(),
                acme.clone() as Arc<dyn WorkEvidenceRead>,
            ),
            (
                WorkEvidenceTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                northstar.clone() as Arc<dyn WorkEvidenceRead>,
            ),
        ])
        .unwrap();
        assert_eq!(state.targets.len(), 2);
        let a = state
            .targets
            .get(&("acme.example.test".into(), "acme".into()))
            .unwrap();
        let b = state
            .targets
            .get(&("northstar.example.test".into(), "northstar".into()))
            .unwrap();
        assert!(!Arc::ptr_eq(a, b), "each holding must own a distinct reader");

        let router = router_from_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });

        let (status_a, body_a) = http(
            addr,
            request_at("acme", "acme.example.test", "company-1", "leave-1", "sets"),
        )
        .await;
        let (status_b, body_b) = http(
            addr,
            request_at("northstar", "northstar.example.test", "company-2", "leave-2", "sets"),
        )
        .await;
        server.abort();

        assert_eq!(status_a, 200);
        assert_eq!(status_b, 200);
        assert_eq!(body_a["rows"][0]["id"], "acme-row");
        assert_eq!(body_b["rows"][0]["id"], "northstar-row");
        assert_eq!(acme.calls.lock().unwrap().len(), 1);
        assert_eq!(northstar.calls.lock().unwrap().len(), 1);
        assert_eq!(acme.calls.lock().unwrap()[0].0, "company-1");
        assert_eq!(northstar.calls.lock().unwrap()[0].0, "company-2");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let reader = probe();
        assert!(router(reader.clone(), vec![target()]).is_ok());
        assert!(router(reader.clone(), vec![target(), target()]).is_err());
        assert!(router(reader.clone(), vec![]).is_err());
        assert!(router(
            reader,
            vec![WorkEvidenceTarget { host: "a".into(), holding_slug: "BAD".into() }]
        )
        .is_err());
    }
}
