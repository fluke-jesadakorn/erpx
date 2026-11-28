//! R3 native read: generic attachment links-by-set.
//!
//! Owner-local successor of the `GET` branch of the retained public route
//! `/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/links`. That route
//! only served Project-owned sets (via the native Projects reader) and returned
//! `404` for every other owner. This module executes the Files-owned SQL for
//! any set in the selected Company, applies the same exact-link row shape and
//! returns the same `{ data: [...] }` envelope and `Cache-Control` header the
//! retained TypeScript read returned. No TypeScript SQL, no fallback, no stub.
//!
//! The module is intentionally standalone: a lead mounts it by adding the
//! `pub mod` lines reported in the worker handoff. It introduces no change to
//! composition, the gateway, Cargo or the migration ledger.

use std::{future::Future, pin::Pin, collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;

/// Exact retained Project/Files link row shape, made public so the owner's
/// public module can re-export the DTO without a private alias.
pub use super::project_attachment::ProjectAttachmentLinkRead as LinkRow;

/// Maximum rows and serialized bytes of one links-by-set read, matching the
/// bounded native attachment list reader.
pub const MAX_LINKS: usize = 100;
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

// ---------------------------------------------------------------------------
// domain: pure types and invariants
// ---------------------------------------------------------------------------

/// Exact retained response envelope. The public TS read returned `Response.json`
/// of `{ data }`; no `context`, witness or other field crossed that boundary.
#[derive(Debug, Serialize)]
pub struct LinkSetResponse {
    pub data: Vec<LinkRow>,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

pub fn valid_set_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b'?' | b'#'))
}

fn set_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_SET_NOT_FOUND",
        "File set not found.",
    )
}

fn malformed() -> BusinessError {
    BusinessError::invalid("ATTACHMENT_REQUEST_INVALID", "Review the file request.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ATTACHMENT_READ_INVALID",
        "The file metadata could not be read safely.",
    )
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type LinkSetFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Bounded transport input. The owner reader verifies the work context from
/// `headers` (Host, Company, assignment, revision, cookie) exactly as it does
/// for the generic attachment list read; this router never mints authority.
#[derive(Clone, Copy)]
pub struct LinkSetRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub set_id: &'a str,
}

/// Composition injects the owner reader behind this port. The reader owns the
/// SQL and the Company scope; a missing or failing reader has no fallback.
pub trait LinkSetRead: Send + Sync {
    fn links<'a>(&'a self, request: LinkSetRequest<'a>) -> LinkSetFuture<'a, Vec<LinkRow>>;
}

/// Bound the projection before it reaches the response, independently of any
/// bound the reader already applied.
pub fn handle(rows: Vec<LinkRow>) -> Result<LinkSetResponse, BusinessError> {
    if rows.len() > MAX_LINKS {
        return Err(unavailable());
    }
    let response = LinkSetResponse { data: rows };
    let bytes = serde_json::to_vec(&response).map_err(|_| unavailable())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(unavailable());
    }
    Ok(response)
}

// ---------------------------------------------------------------------------
// adapters: SQLx execution, row mapping and axum transport
// ---------------------------------------------------------------------------

/// One Files-owned statement. `$1` = set id, `$2` = company id, `$3` = response
/// byte limit, `$4` = row limit. Only Files relations are read; no owner table
/// is touched and no transaction is opened by this adapter.
pub const LINKS_BY_SET_SQL: &str = r#"
SELECT CASE WHEN octet_length(image::text) <= $3 THEN image END
FROM (
  SELECT COALESCE(jsonb_agg(jsonb_build_object(
    'id', links.id,
    'sourceId', links.attachment_id,
    'filename', COALESCE(links.original_name, links.intent_original_name, 'file'),
    'mediaType', COALESCE(links.media_type, links.intent_media_type, 'application/octet-stream'),
    'bytes', COALESCE(links.bytes, links.declared_bytes),
    'sourceState', links.source_state,
    'linkState', links.state,
    'caption', links.caption,
    'previewAvailable', links.preview_object_key IS NOT NULL,
    'createdAt', links.created_at,
    'expiresAt', links.expires_at,
    'boundAt', links.bound_at
  ) ORDER BY links.created_at, links.id), '[]'::jsonb) AS image
  FROM (
    SELECT l.id,l.attachment_id,l.state,l.caption,l.created_at,l.expires_at,l.bound_at,
           a.original_name,a.media_type,a.bytes,a.source_state,a.preview_object_key,
           u.original_name intent_original_name,u.media_type intent_media_type,
           u.declared_bytes
    FROM attachment_links l
    JOIN attachment_sets s ON s.id=l.set_id
    LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id
    LEFT JOIN upload_intents u ON u.id=l.upload_intent_id AND u.company_id=s.company_id
    WHERE l.set_id=$1
      AND s.company_id=$2
      AND (l.attachment_id IS NULL OR a.id IS NOT NULL)
      AND (l.upload_intent_id IS NULL OR u.id IS NOT NULL)
    ORDER BY l.created_at, l.id
    LIMIT $4
  ) links
) sized
"#;

const SET_EXISTS_SQL: &str =
    "SELECT EXISTS(SELECT 1 FROM attachment_sets WHERE id=$1 AND company_id=$2)";

/// Files-owned PostgreSQL reader for the generic links-by-set projection.
#[derive(Clone)]
pub struct PgLinkSetRead {
    pool: DatabasePool,
}

impl PgLinkSetRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub fn links<'a>(&'a self, company: &'a str, set_id: &'a str) -> LinkSetFuture<'a, Vec<LinkRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = company.to_owned();
        let set_id = set_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let set_id = set_id.clone();
                Box::pin(async move {
                    let exists: bool = sqlx::query_scalar(SET_EXISTS_SQL)
                        .bind(set_id.as_str())
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if !exists {
                        return Err(set_missing().into());
                    }
                    let value: Option<serde_json::Value> = sqlx::query_scalar(LINKS_BY_SET_SQL)
                        .bind(set_id.as_str())
                        .bind(company.as_str())
                        .bind(MAX_RESPONSE_BYTES as i64)
                        .bind(MAX_LINKS as i64)
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let value = value.ok_or_else(unavailable)?;
                    let rows = serde_json::from_value::<Vec<LinkRow>>(value).map_err(|_| unavailable())?;
                    Ok(rows)
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

/// One exact registered Holding target.
#[derive(Clone)]
pub struct LinkSetTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LinkSetState {
    targets: BTreeMap<(String, String), Arc<dyn LinkSetRead>>,
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

fn bounded(body: LinkSetResponse) -> Response {
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

/// Exact distinct `(host, holding_slug)` targets; no wildcard or suffix match.
/// Composition mounts the returned router; this module never edits composition.
pub fn router(
    reader: Arc<dyn LinkSetRead>,
    targets: Vec<LinkSetTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FILES_LINK_SET_ROUTE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "FILES_LINK_SET_ROUTE_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/attachment-sets/{set_id}/links",
            get(links),
        )
        .with_state(Arc::new(LinkSetState {
            targets: configured,
        })))
}

async fn links(
    State(state): State<Arc<LinkSetState>>,
    Path((slug, set_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if request.uri().query().is_some_and(|value| !value.is_empty()) {
            return Err(malformed());
        }
        if !valid_set_id(&set_id) {
            return Err(set_missing());
        }
        let bytes = to_bytes(request.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|_| malformed())?;
        if !bytes.is_empty() {
            return Err(malformed());
        }
        let rows = reader
            .links(LinkSetRequest {
                host,
                holding_slug: &slug,
                headers: &headers,
                set_id: &set_id,
            })
            .await?;
        handle(rows)
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
    use std::sync::Mutex;

    fn row(id: &str) -> LinkRow {
        LinkRow {
            id: id.to_owned(),
            source_id: Some(format!("src-{id}")),
            filename: "a.pdf".to_owned(),
            media_type: "application/pdf".to_owned(),
            bytes: Some(10),
            source_state: Some("available".to_owned()),
            link_state: "bound".to_owned(),
            caption: None,
            preview_available: false,
            created_at: 1,
            expires_at: 2,
            bound_at: Some(1),
        }
    }

    struct Probe {
        calls: Mutex<Vec<String>>,
        rows: Vec<LinkRow>,
    }
    impl LinkSetRead for Probe {
        fn links<'a>(&'a self, request: LinkSetRequest<'a>) -> LinkSetFuture<'a, Vec<LinkRow>> {
            Box::pin(async move {
                self.calls.lock().expect("mutex").push(request.set_id.to_owned());
                Ok(self.rows.clone())
            })
        }
    }
    fn target() -> LinkSetTarget {
        LinkSetTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }
    }
    async fn serve(reader: Arc<Probe>) -> (u16, tokio::task::JoinHandle<()>) {
        let router = router(reader, vec![target()]).expect("router");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let port = listener.local_addr().expect("addr").port();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (port, server)
    }
    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
        stream.write_all(raw.as_bytes()).await.expect("write");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .expect("status");
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or(("", ""));
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
            head.to_owned(),
        )
    }
    fn request(host: &str, slug: &str, set: &str, query: &str) -> String {
        format!(
            "GET /api/v1/holdings/{slug}/attachment-sets/{set}/links{query} HTTP/1.1\r\n\
             Host: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    }

    #[tokio::test]
    async fn serves_exact_data_envelope_and_headers() {
        let reader = Arc::new(Probe {
            calls: Mutex::new(Vec::new()),
            rows: vec![row("l1")],
        });
        let (port, server) = serve(reader.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let (status, body, head) = http(addr, request("acme.example.test", "acme", "set-1", "")).await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"][0]["id"], "l1");
        assert_eq!(body["data"][0]["sourceId"], "src-l1");
        assert!(body.get("context").is_none());
        assert!(head.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert_eq!(*reader.calls.lock().expect("mutex"), vec!["set-1".to_owned()]);
    }

    #[tokio::test]
    async fn fails_closed_on_wrong_host_slug_set_and_query() {
        let reader = Arc::new(Probe {
            calls: Mutex::new(Vec::new()),
            rows: vec![row("l1")],
        });
        let (port, server) = serve(reader.clone()).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for raw in [
            request("attacker.example.test", "acme", "set-1", ""),
            request("acme.example.test", "other", "set-1", ""),
        ] {
            let (status, body, _) = http(addr, raw).await;
            assert_eq!(status, 404);
            assert_eq!(body["error"]["code"], "NOT_FOUND");
        }
        let (status, body, _) = http(addr, request("acme.example.test", "acme", "bad%5Cset", "")).await;
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "ATTACHMENT_SET_NOT_FOUND");
        let (status, body, _) = http(addr, request("acme.example.test", "acme", "set-1", "?x=1")).await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "ATTACHMENT_REQUEST_INVALID");
        server.abort();
        assert!(reader.calls.lock().expect("mutex").is_empty());
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let reader: Arc<dyn LinkSetRead> = Arc::new(Probe {
            calls: Mutex::new(Vec::new()),
            rows: Vec::new(),
        });
        assert!(router(reader.clone(), vec![target()]).is_ok());
        assert!(router(reader.clone(), vec![target(), target()]).is_err());
        assert!(router(reader.clone(), vec![]).is_err());
        assert!(router(
            reader,
            vec![LinkSetTarget {
                host: "a".to_owned(),
                holding_slug: "BAD".to_owned(),
            }]
        )
        .is_err());
    }

    #[test]
    fn domain_invariants() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(valid_set_id("set-1"));
        assert!(!valid_set_id("bad/set"));
        assert!(handle(Vec::new()).is_ok());
        assert!(handle((0..MAX_LINKS + 1).map(|i| row(&i.to_string())).collect()).is_err());
    }
}
