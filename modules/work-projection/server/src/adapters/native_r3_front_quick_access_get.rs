//! Native read for `GET /api/v1/front-access/quick-access` (R3, lane L14).
//!
//! Additive owner-local ingress. The retired handler derives the Front app from
//! the registered host, resolves the verified Account session, and executes the
//! exact `quick_access_preferences` SELECT with surface `front-picker` and
//! context key `pos`/`pms`. It returns `{"data":{"pinnedIds":...,"version":...}}`
//! with `Cache-Control: private, no-store`; `canonicalDestinationLinks` is a
//! no-op on this shape because no field ends in `Href`/`_href`.
//!
//! No SQL is duplicated here: the preference SELECT is owned by
//! `adapters/quick_access_preferences.rs:44-51` and reused through
//! `public/quick_access_preferences::read_quick_access`. Verified Account
//! authority (session image, audience and the per-account preference database)
//! is supplied by composition through [`FrontQuickAccessAuthority`]; the router
//! never mints or widens a session and never consults TypeScript. POST stays on
//! the TypeScript route because its command half is not migrated (see blocker).

use crate::public::quick_access_preferences::{QuickAccessPreference, read_quick_access};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use sqlx::Connection;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain
// ---------------------------------------------------------------------------

/// The two Front apps accepted by the retired route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontApp {
    Pms,
    Pos,
}

impl FrontApp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pms => "pms",
            Self::Pos => "pos",
        }
    }
}

/// Registered Front hosts. Composition may add configured
/// `ERPX_FRONT_PMS_ORIGIN`/`ERPX_FRONT_POS_ORIGIN` hosts, but never a wildcard.
pub const DEFAULT_FRONT_HOSTS: &[(&str, FrontApp)] = &[
    ("pms.jesadakorn.com", FrontApp::Pms),
    ("pos.jesadakorn.com", FrontApp::Pos),
];

pub fn default_front_hosts() -> Vec<(String, FrontApp)> {
    DEFAULT_FRONT_HOSTS
        .iter()
        .map(|(host, app)| ((*host).to_owned(), *app))
        .collect()
}

/// Exact retired `QuickAccessPreference` wire projection (camelCase keys).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontQuickAccessPreferenceWire {
    pub pinned_ids: Option<Vec<String>>,
    pub version: i64,
}

impl From<QuickAccessPreference> for FrontQuickAccessPreferenceWire {
    fn from(value: QuickAccessPreference) -> Self {
        Self {
            pinned_ids: value.pinned_ids,
            version: value.version,
        }
    }
}

fn invalid_host() -> BusinessError {
    BusinessError::invalid(
        "FRONT_QUICK_ACCESS_INVALID",
        "Use a registered Front host.",
    )
}

fn missing_host() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FRONT_QUICK_ACCESS_UNAVAILABLE",
        "The native Front quick access is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

// ---------------------------------------------------------------------------
// application (no SQL, no authority minting)
// ---------------------------------------------------------------------------

pub type FrontQuickAccessFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Verified, SQL-free scope for one Front picker read. Composition derives it
/// from the current Account session and opens the per-account preference
/// database for that exact session/audience.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontQuickAccessScope {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
}

pub struct FrontQuickAccessScopeRequest<'a> {
    pub host: &'a str,
    pub app: FrontApp,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
}

/// Composition-supplied Account authority. It must re-assert the same session
/// image (`session`, headers, url, app) around the read exactly as the retired
/// `withCurrentSelfAccountPreferences`/`assertContext` pair did.
pub trait FrontQuickAccessAuthority: Send + Sync {
    fn scope<'a>(
        &'a self,
        request: FrontQuickAccessScopeRequest<'a>,
    ) -> FrontQuickAccessFuture<'a, FrontQuickAccessScope>;
}

pub trait FrontQuickAccessReader: Send + Sync {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> FrontQuickAccessFuture<'a, QuickAccessPreference>;
}

pub struct FrontQuickAccessRequest<'a> {
    pub host: &'a str,
    pub app: FrontApp,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
}

/// Obtain the verified scope and execute the owner preference read.
pub async fn read_front_quick_access<
    A: FrontQuickAccessAuthority + ?Sized,
    R: FrontQuickAccessReader + ?Sized,
>(
    authority: &A,
    reader: &R,
    request: FrontQuickAccessRequest<'_>,
) -> Result<FrontQuickAccessPreferenceWire, BusinessError> {
    let scope = authority
        .scope(FrontQuickAccessScopeRequest {
            host: request.host,
            app: request.app,
            headers: request.headers,
            query: request.query,
        })
        .await?;
    if scope.user_id.is_empty() || scope.surface.is_empty() || scope.context_key.is_empty() {
        return Err(unavailable());
    }
    let preference = reader
        .read(&scope.user_id, &scope.surface, &scope.context_key)
        .await?;
    Ok(preference.into())
}

// ---------------------------------------------------------------------------
// postgres adapter — reuses the owner SELECT, no duplicated SQL
// ---------------------------------------------------------------------------

/// Pool-backed reader. The pool is the per-account preference database opened
/// by composition for the verified session. The executed statement is
/// `adapters/quick_access_preferences.rs:44-51`.
pub struct PgFrontQuickAccess {
    pool: DatabasePool,
}

impl PgFrontQuickAccess {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl FrontQuickAccessReader for PgFrontQuickAccess {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> FrontQuickAccessFuture<'a, QuickAccessPreference> {
        Box::pin(async move {
            let mut connection = self
                .pool
                .raw()
                .acquire()
                .await
                .map_err(|_| unavailable())?;
            let mut tx = connection.begin().await.map_err(|_| unavailable())?;
            let result = read_quick_access(&mut tx, user_id, surface, context_key).await;
            match result {
                Ok(value) => {
                    tx.commit().await.map_err(|_| unavailable())?;
                    Ok(value)
                }
                Err(error) => {
                    let _ = tx.rollback().await;
                    Err(business(error))
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// http ingress
// ---------------------------------------------------------------------------

struct StateData {
    apps: BTreeMap<String, FrontApp>,
    authority: Arc<dyn FrontQuickAccessAuthority>,
    reader: Arc<dyn FrontQuickAccessReader>,
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

fn bounded(wire: FrontQuickAccessPreferenceWire) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data": wire})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_host());
    }
    first.map(|value| value.to_str().map_err(|_| invalid_host())).transpose()
}

async fn read(
    State(state): State<Arc<StateData>>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result: Result<FrontQuickAccessPreferenceWire, BusinessError> = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing_host)?;
        let app = state.apps.get(host).copied().ok_or_else(missing_host)?;
        read_front_quick_access(
            state.authority.as_ref(),
            state.reader.as_ref(),
            FrontQuickAccessRequest {
                host,
                app,
                headers: &headers,
                query: query.as_deref(),
            },
        )
        .await
    }
    .await;
    match result {
        Ok(wire) => bounded(wire),
        Err(error) => error_response(error),
    }
}

/// Build the native GET router over composition-supplied ports. `hosts` are
/// exact registered Front hosts (see [`default_front_hosts`]); no wildcard host
/// is accepted. POST is intentionally not served: it remains TypeScript-owned.
pub fn front_quick_access_router(
    hosts: Vec<(String, FrontApp)>,
    authority: Arc<dyn FrontQuickAccessAuthority>,
    reader: Arc<dyn FrontQuickAccessReader>,
) -> Result<Router, BusinessError> {
    let mut apps = BTreeMap::new();
    for (host, app) in hosts {
        if host.is_empty()
            || apps.insert(host, app).is_some()
        {
            return Err(invalid_host());
        }
    }
    if apps.is_empty() {
        return Err(invalid_host());
    }
    Ok(Router::new()
        .route("/api/v1/front-access/quick-access", get(read))
        .with_state(Arc::new(StateData {
            apps,
            authority,
            reader,
        })))
}

/// Convenience constructor binding the composition-opened per-account pool.
pub fn front_quick_access_router_with_pool(
    hosts: Vec<(String, FrontApp)>,
    authority: Arc<dyn FrontQuickAccessAuthority>,
    pool: DatabasePool,
) -> Result<Router, BusinessError> {
    front_quick_access_router(hosts, authority, Arc::new(PgFrontQuickAccess::new(pool)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Authority;
    impl FrontQuickAccessAuthority for Authority {
        fn scope<'a>(
            &'a self,
            request: FrontQuickAccessScopeRequest<'a>,
        ) -> FrontQuickAccessFuture<'a, FrontQuickAccessScope> {
            Box::pin(async move {
                Ok(FrontQuickAccessScope {
                    user_id: "user-1".into(),
                    surface: "front-picker".into(),
                    context_key: request.app.as_str().into(),
                })
            })
        }
    }

    struct Reader {
        preference: Option<QuickAccessPreference>,
        seen: Arc<Mutex<Vec<(String, String, String)>>>,
    }
    impl FrontQuickAccessReader for Reader {
        fn read<'a>(
            &'a self,
            user_id: &'a str,
            surface: &'a str,
            context_key: &'a str,
        ) -> FrontQuickAccessFuture<'a, QuickAccessPreference> {
            self.seen.lock().unwrap().push((
                user_id.to_owned(),
                surface.to_owned(),
                context_key.to_owned(),
            ));
            let preference = self.preference.clone().unwrap_or(QuickAccessPreference {
                pinned_ids: None,
                version: 0,
            });
            Box::pin(async move { Ok(preference) })
        }
    }

    fn headers(host: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, host.parse().unwrap());
        headers
    }

    fn router(seen: Arc<Mutex<Vec<(String, String, String)>>>) -> Router {
        let reader: Arc<dyn FrontQuickAccessReader> = Arc::new(Reader {
            preference: Some(QuickAccessPreference {
                pinned_ids: Some(vec!["pos-sale".into()]),
                version: 4,
            }),
            seen,
        });
        front_quick_access_router(
            default_front_hosts(),
            Arc::new(Authority),
            reader,
        )
        .unwrap()
    }

    #[test]
    fn default_hosts_map_to_the_two_front_apps() {
        let hosts = default_front_hosts();
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains(&("pms.jesadakorn.com".to_owned(), FrontApp::Pms)));
        assert!(hosts.contains(&("pos.jesadakorn.com".to_owned(), FrontApp::Pos)));
    }

    #[tokio::test]
    async fn read_uses_the_front_picker_scope_and_exact_wire() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Reader {
            preference: Some(QuickAccessPreference {
                pinned_ids: Some(vec!["pos-sale".into()]),
                version: 4,
            }),
            seen: seen.clone(),
        };
        let wire = read_front_quick_access(
            &Authority,
            &reader,
            FrontQuickAccessRequest {
                host: "pos.jesadakorn.com",
                app: FrontApp::Pos,
                headers: &headers("pos.jesadakorn.com"),
                query: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["pos-sale"], "version": 4})
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[(
                "user-1".to_owned(),
                "front-picker".to_owned(),
                "pos".to_owned()
            )]
        );
    }

    async fn exchange(addr: std::net::SocketAddr, request: String) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let head = text.split("\r\n\r\n").next().unwrap_or("").to_lowercase();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, head, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn router_serves_the_exact_envelope_and_fails_closed() {
        let router = router(Arc::new(Mutex::new(Vec::new())));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });

        let ok = exchange(
            addr,
            "GET /api/v1/front-access/quick-access HTTP/1.1\r\nHost: pos.jesadakorn.com\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["pinnedIds"][0], "pos-sale");
        assert_eq!(ok.2["data"]["version"], 4);
        assert_eq!(ok.2.as_object().unwrap().len(), 1);

        let unknown_host = exchange(
            addr,
            "GET /api/v1/front-access/quick-access HTTP/1.1\r\nHost: other.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(unknown_host.0, 404);
        assert_eq!(unknown_host.2["error"]["code"], "NOT_FOUND");

        // The command half is not served by this GET-only owner module.
        let post = exchange(
            addr,
            "POST /api/v1/front-access/quick-access HTTP/1.1\r\nHost: pos.jesadakorn.com\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(post.0, 405);
        server.abort();
    }

    #[test]
    fn router_rejects_empty_and_duplicate_hosts() {
        let reader: Arc<dyn FrontQuickAccessReader> = Arc::new(Reader {
            preference: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(front_quick_access_router(vec![], Arc::new(Authority), reader.clone()).is_err());
        assert!(front_quick_access_router(
            vec![("pos.jesadakorn.com".into(), FrontApp::Pos)],
            Arc::new(Authority),
            reader.clone(),
        )
        .is_ok());
        assert!(front_quick_access_router(
            vec![
                ("pos.jesadakorn.com".into(), FrontApp::Pos),
                ("pos.jesadakorn.com".into(), FrontApp::Pos),
            ],
            Arc::new(Authority),
            reader,
        )
        .is_err());
    }
}
