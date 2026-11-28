//! Native R3 read for `GET /api/v1/platform/page-layouts/{page_key}` (L05).
//!
//! The retained TypeScript route calls `requireManageErpxContext()` and then
//! `pageLayoutRequest(request, pageKey, undefined, false)`, which runs the exact
//! `page_layouts` SELECT on the Control database with the platform actor's
//! company/user/base. This module moves that read into the owner with real
//! SQLx. The statement and projection are the crate's already-tested owner
//! functions; no TypeScript call, plan-only SQL or synthesised scope remains.
//!
//! Composition resolves the platform session/work authority facts through
//! [`PlatformLayoutAuthority`] and injects the Control pool through
//! [`PgPlatformLayoutRead`]; the owner never reads a session from the request.
//! POST is intentionally not served here: the advisory-lock/receipt writer
//! stays TypeScript-owned until its own migration, so the lead routes POST to
//! the retained handler.

use crate::adapters::page_layouts_read::{
    read_page_layout_rows, select_layouts, PageLayoutRow, PAGE_LAYOUTS_READ_SQL,
};
use crate::domain::navigation_wire::canonical_envelope;
use crate::domain::page_layouts::{decode_layout, is_configurable_page, layout_snapshot_json};
use erpx_platform_runtime_server::public::http_support::{
    header, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Response, Router,
    State, StatusCode, get,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::Arc,
};

// ---------------------------------------------------------------------------
// domain: pure validation and projection
// ---------------------------------------------------------------------------

/// Bounded owner-scope identifier, identical shape to the Work reads.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn invalid() -> BusinessError {
    BusinessError::invalid("PLATFORM_LAYOUT_READ_INVALID", "Use one bounded page-layout read.")
}

/// Every authoritative fact the owner needs to execute the retained read.
/// Only `page_key` comes from the transport; all other facts are supplied by
/// the composition-resolved authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformLayoutQuery {
    pub company: String,
    pub page_key: String,
    pub user: String,
    pub base: String,
    pub allowed_widget_ids: BTreeSet<String>,
    pub can_publish: bool,
}

/// Fail closed before any SQL is planned: the page key must be an owner
/// allow-list entry, scope identifiers must be bounded, and at least one widget
/// must be available to the current role (the retained 403 parity).
pub fn validate(query: &PlatformLayoutQuery) -> Result<(), BusinessError> {
    if !valid_id(&query.company)
        || !valid_id(&query.user)
        || !is_configurable_page(&query.page_key)
        || query.page_key.len() > 128
        || query.base.is_empty()
        || query.base.len() > 200
        || query.allowed_widget_ids.is_empty()
        || query
            .allowed_widget_ids
            .iter()
            .any(|id| !valid_id(id))
    {
        return Err(invalid());
    }
    Ok(())
}

fn unknown_page() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PAGE_LAYOUT_UNKNOWN",
        "This page has no configurable widgets.",
    )
}

/// Project owner rows into the exact retained `LayoutSnapshot`. Versions come
/// from the rows; stored widgets outside the role allow-list are dropped.
pub fn project(rows: &[PageLayoutRow], query: &PlatformLayoutQuery) -> Result<Value, BusinessError> {
    validate(query)?;
    let (personal_row, organization_row) = select_layouts(rows, &query.user);
    let personal = decode_layout(
        personal_row.and_then(|row| row.layout_json.as_deref()),
        &query.allowed_widget_ids,
    )?;
    let organization = decode_layout(
        organization_row.and_then(|row| row.layout_json.as_deref()),
        &query.allowed_widget_ids,
    )?;
    Ok(layout_snapshot_json(
        personal.as_ref(),
        organization.as_ref(),
        personal_row.map_or(0, |row| row.version),
        organization_row.map_or(0, |row| row.version),
        query.can_publish,
    ))
}

// ---------------------------------------------------------------------------
// application: ports, no SQL
// ---------------------------------------------------------------------------

pub type LayoutFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;
pub type ContextFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader. Composition injects the sqlx-backed implementation bound to
/// the Control pool; the HTTP adapter never sees a connection.
pub trait PlatformLayoutRead: Send + Sync {
    fn read(&self, query: PlatformLayoutQuery) -> LayoutFuture<'_, Vec<PageLayoutRow>>;
}

/// Composition-resolved platform authority facts. Never caller input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformLayoutContext {
    pub company: String,
    pub user: String,
    pub base: String,
    pub allowed_widget_ids: BTreeSet<String>,
    pub can_publish: bool,
}

pub trait PlatformLayoutAuthority: Send + Sync {
    fn context<'a>(
        &'a self,
        host: &'a str,
        page_key: &'a str,
        headers: &'a HeaderMap,
    ) -> ContextFuture<'a, PlatformLayoutContext>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_LAYOUT_UNAVAILABLE",
        "The native Work layout is unavailable.",
    )
}

fn transaction(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

// ---------------------------------------------------------------------------
// adapters: sqlx execution on the Control pool
// ---------------------------------------------------------------------------

/// Pg-backed owner reader bound to the Control database pool.
pub struct PgPlatformLayoutRead {
    pool: DatabasePool,
}

impl PgPlatformLayoutRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PlatformLayoutRead for PgPlatformLayoutRead {
    fn read(&self, query: PlatformLayoutQuery) -> LayoutFuture<'_, Vec<PageLayoutRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company;
        let page_key = query.page_key;
        let user = query.user;
        let base = query.base;
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let page_key = page_key.clone();
                let user = user.clone();
                let base = base.clone();
                Box::pin(async move {
                    read_page_layout_rows(&mut *connection, &company, &page_key, &user, &base).await
                })
            })
            .await
            .map_err(transaction)
        })
    }
}

// ---------------------------------------------------------------------------
// http: exact envelope, headers and transport guards
// ---------------------------------------------------------------------------

struct PlatformLayoutState {
    hosts: BTreeSet<String>,
    read: Arc<dyn PlatformLayoutRead>,
    authority: Arc<dyn PlatformLayoutAuthority>,
}

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn secured(mut response: Response) -> Response {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(name, HeaderValue::from_static(value));
    }
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(status, serde_json::json!({"error": {"code": error.code, "message": error.message}}))
}

fn validate_snapshot(data: &Value) -> Result<(), BusinessError> {
    let Value::Object(map) = data else {
        return Err(unavailable());
    };
    if map.len() != 5
        || ["personal", "organization", "version", "organizationVersion", "canPublish"]
            .iter()
            .any(|key| !map.contains_key(*key))
    {
        return Err(unavailable());
    }
    Ok(())
}

impl PlatformLayoutState {
    async fn read(
        &self,
        headers: &HeaderMap,
        page_key: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(invalid());
        }
        if !is_configurable_page(page_key) || page_key.len() > 128 {
            return Err(unknown_page());
        }
        let host = unique_header(headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if !self.hosts.contains(host) {
            return Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."));
        }
        let context = self.authority.context(host, page_key, headers).await?;
        let query = PlatformLayoutQuery {
            company: context.company,
            page_key: page_key.to_owned(),
            user: context.user,
            base: context.base,
            allowed_widget_ids: context.allowed_widget_ids,
            can_publish: context.can_publish,
        };
        let rows = self.read.read(query.clone()).await?;
        let data = project(&rows, &query)?;
        validate_snapshot(&data)?;
        Ok(json_response(StatusCode::OK, canonical_envelope(data)))
    }
}

async fn read_handler(
    State(state): State<Arc<PlatformLayoutState>>,
    Path(page_key): Path<String>,
    request: erpx_platform_runtime_server::public::http_support::Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    match state.read(request.headers(), &page_key, query.as_deref()).await {
        Ok(response) => response,
        Err(error) => error_response(error),
    }
}

fn route(
    hosts: Vec<String>,
    read: Arc<dyn PlatformLayoutRead>,
    authority: Arc<dyn PlatformLayoutAuthority>,
) -> Result<Router, BusinessError> {
    let configured: BTreeSet<String> = hosts.into_iter().collect();
    if configured.is_empty() || configured.iter().any(|host| host.is_empty()) {
        return Err(BusinessError::invalid(
            "PLATFORM_LAYOUT_CONFIGURATION_INVALID",
            "Use exact distinct registered Platform hosts.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/platform/page-layouts/{page_key}", get(read_handler))
        .with_state(Arc::new(PlatformLayoutState {
            hosts: configured,
            read,
            authority,
        })))
}

/// Composition-facing constructor: exact registered Platform hosts share one
/// Control pool owned by this crate. POST stays on the retained TypeScript
/// handler and is deliberately not mounted on this router.
pub fn router(
    hosts: Vec<String>,
    pool: DatabasePool,
    authority: Arc<dyn PlatformLayoutAuthority>,
) -> Result<Router, BusinessError> {
    route(
        hosts,
        Arc::new(PgPlatformLayoutRead::new(pool)) as Arc<dyn PlatformLayoutRead>,
        authority,
    )
}

/// Test-facing constructor with a deterministic owner reader.
pub fn router_with_reader(
    hosts: Vec<String>,
    read: Arc<dyn PlatformLayoutRead>,
    authority: Arc<dyn PlatformLayoutAuthority>,
) -> Result<Router, BusinessError> {
    route(hosts, read, authority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serve;
    use serde_json::json;
    use std::sync::Mutex;

    struct Fixed {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl PlatformLayoutRead for Fixed {
        fn read(&self, query: PlatformLayoutQuery) -> LayoutFuture<'_, Vec<PageLayoutRow>> {
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    query.company, query.user, query.page_key, query.base
                ));
                Ok(vec![
                    PageLayoutRow {
                        owner_id: "user-1".into(),
                        layout_json: Some(
                            r#"{"schemaVersion":1,"widgets":[{"id":"w2","columns":12,"height":null,"hidden":false}]}"#
                                .into(),
                        ),
                        version: 2,
                    },
                    PageLayoutRow {
                        owner_id: "organization".into(),
                        layout_json: Some(
                            r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false},{"id":"gone","columns":6,"height":null,"hidden":false}]}"#
                                .into(),
                        ),
                        version: 5,
                    },
                ])
            })
        }
    }

    struct Authority;

    impl PlatformLayoutAuthority for Authority {
        fn context<'a>(
            &'a self,
            _host: &'a str,
            _page_key: &'a str,
            _headers: &'a HeaderMap,
        ) -> ContextFuture<'a, PlatformLayoutContext> {
            Box::pin(async {
                Ok(PlatformLayoutContext {
                    company: "erpx-platform".into(),
                    user: "user-1".into(),
                    base: "/manage-erpx/platform".into(),
                    allowed_widget_ids: BTreeSet::from(["w1".to_owned(), "w2".to_owned()]),
                    can_publish: true,
                })
            })
        }
    }

    fn router_with(seen: Arc<Mutex<Vec<String>>>) -> Router {
        let read: Arc<dyn PlatformLayoutRead> = Arc::new(Fixed { seen });
        router_with_reader(vec!["admin.jesadakorn.com".into()], read, Arc::new(Authority)).unwrap()
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Value) {
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
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let value = serde_json::from_str(body.trim()).unwrap_or(Value::Null);
        (status, head.to_owned(), value)
    }

    async fn spawn(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr, server)
    }

    fn get(host: &str, path: &str) -> String {
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
    }

    #[tokio::test]
    async fn native_get_returns_the_exact_data_envelope_and_headers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (addr, server) = spawn(router_with(seen.clone())).await;
        let (status, head, value) = http(
            addr,
            get("admin.jesadakorn.com", "/api/v1/platform/page-layouts/people-directory"),
        )
        .await;
        assert_eq!(status, 200);
        let head = head.to_ascii_lowercase();
        assert!(head.contains("cache-control: private, no-store"));
        assert!(head.contains("referrer-policy: no-referrer"));
        assert!(head.contains("x-content-type-options: nosniff"));
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(value["data"]["version"], 2);
        assert_eq!(value["data"]["organizationVersion"], 5);
        assert_eq!(value["data"]["canPublish"], true);
        assert_eq!(value["data"]["personal"]["widgets"].as_array().unwrap().len(), 1);
        assert_eq!(value["data"]["organization"]["widgets"][0]["id"], "w1");
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["erpx-platform:user-1:people-directory:/manage-erpx/platform"]
        );
        let _ = server.abort();
    }

    #[tokio::test]
    async fn unknown_page_query_and_wrong_host_fail_closed() {
        let (addr, server) = spawn(router_with(Arc::new(Mutex::new(Vec::new())))).await;
        assert_eq!(
            http(addr, get("admin.jesadakorn.com", "/api/v1/platform/page-layouts/platform-home"))
                .await
                .0,
            404
        );
        assert_eq!(
            http(
                addr,
                get("admin.jesadakorn.com", "/api/v1/platform/page-layouts/people-directory?x=1"),
            )
            .await
            .0,
            422
        );
        assert_eq!(
            http(addr, get("other.jesadakorn.com", "/api/v1/platform/page-layouts/people-directory"))
                .await
                .0,
            404
        );
        let _ = server.abort();
    }

    #[tokio::test]
    async fn post_is_not_served_by_the_read_module() {
        let (addr, server) = spawn(router_with(Arc::new(Mutex::new(Vec::new())))).await;
        let body = r#"{"action":"save","expectedVersion":2,"layout":null}"#;
        let raw = format!(
            "POST /api/v1/platform/page-layouts/people-directory HTTP/1.1\r\nHost: admin.jesadakorn.com\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        assert_eq!(http(addr, raw).await.0, 405);
        let _ = server.abort();
    }

    #[test]
    fn projection_matches_the_retained_snapshot_wire_shape() {
        let rows = vec![PageLayoutRow {
            owner_id: "organization".into(),
            layout_json: Some(
                r#"{"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false},{"id":"removed","columns":6,"height":null,"hidden":false}]}"#
                    .into(),
            ),
            version: 3,
        }];
        let query = PlatformLayoutQuery {
            company: "erpx-platform".into(),
            page_key: "people-directory".into(),
            user: "user-1".into(),
            base: "/manage-erpx/platform".into(),
            allowed_widget_ids: BTreeSet::from(["w1".to_owned()]),
            can_publish: false,
        };
        let value = project(&rows, &query).unwrap();
        assert_eq!(
            value,
            json!({
                "personal": null,
                "organization": {"schemaVersion":1,"widgets":[{"id":"w1","columns":6,"height":null,"hidden":false}]},
                "version": 0,
                "organizationVersion": 3,
                "canPublish": false
            })
        );
        assert_eq!(value.as_object().unwrap().len(), 5);
    }

    #[test]
    fn validation_fails_closed_on_scope_and_allow_list() {
        let query = PlatformLayoutQuery {
            company: "erpx-platform".into(),
            page_key: "people-directory".into(),
            user: "user-1".into(),
            base: "/manage-erpx/platform".into(),
            allowed_widget_ids: BTreeSet::from(["w1".to_owned()]),
            can_publish: true,
        };
        assert!(validate(&query).is_ok());
        let mut bad = query.clone();
        bad.page_key = "platform-home".into();
        assert_eq!(validate(&bad).unwrap_err().code, "PLATFORM_LAYOUT_READ_INVALID");
        let mut bad = query.clone();
        bad.allowed_widget_ids.clear();
        assert!(validate(&bad).is_err());
        let mut bad = query;
        bad.user = "../bad".into();
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn owner_statement_is_the_retained_predicate_and_projection() {
        assert_eq!(
            PAGE_LAYOUTS_READ_SQL,
            "SELECT owner_id,layout_json,version FROM page_layouts WHERE company_id=$1 AND page_key=$2 AND ((owner_id=$3 AND workspace_base=$4) OR (owner_id=$5 AND workspace_base=$6))"
        );
        assert_eq!(PAGE_LAYOUTS_READ_SQL.matches('$').count(), 6);
    }

    #[test]
    fn hosts_must_be_exact_and_distinct() {
        let read: Arc<dyn PlatformLayoutRead> =
            Arc::new(Fixed { seen: Arc::new(Mutex::new(Vec::new())) });
        assert!(route(vec![], read.clone(), Arc::new(Authority)).is_err());
        assert!(route(vec!["".into()], read, Arc::new(Authority)).is_err());
    }
}
