//! Native read for `GET /api/v1/account/quick-access/{area}` (R3, lane L12).
//!
//! Additive owner-local ingress. The exact retired response is
//! `{"data":{"pinnedIds":...,"version":...}}` with `Cache-Control: private,
//! no-store`; `canonicalDestinationLinks` is a no-op on this shape because no
//! field name ends in `Href`/`_href`.
//!
//! No SQL is repeated here: the preference SELECT is owned by the existing
//! `read_quick_access` adapter (`adapters/quick_access_preferences.rs:44`), and
//! this module only adds the transport, the area guard and the wire projection.
//! Verified Account authority (session, workspace choices, work-role scope) is
//! supplied by composition through [`QuickAccessGetAuthority`]; the router
//! never mints or widens a session and never consults TypeScript.

use crate::public::quick_access_preferences::{
    QuickAccessPreference, read_quick_access,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Serialize;
use sqlx::Connection;
use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

// ---------------------------------------------------------------------------
// domain
// ---------------------------------------------------------------------------

/// The three account quick-access areas accepted by the retired route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountArea {
    MyAccess,
    Security,
    WorkRole,
}

impl AccountArea {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MyAccess => "my-access",
            Self::Security => "security",
            Self::WorkRole => "work-role",
        }
    }
}

/// Parse the bounded `{area}` path value. Unknown areas are a 404, matching
/// the retired `areaFrom` guard ("Unknown account area.").
pub fn area_from(value: &str) -> Result<AccountArea, BusinessError> {
    match value {
        "my-access" => Ok(AccountArea::MyAccess),
        "security" => Ok(AccountArea::Security),
        "work-role" => Ok(AccountArea::WorkRole),
        _ => Err(BusinessError::new(
            FailureKind::NotFound,
            "NOT_FOUND",
            "Unknown account area.",
        )),
    }
}

/// Exact retired `QuickAccessPreference` wire projection (camelCase keys).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickAccessPreferenceWire {
    pub pinned_ids: Option<Vec<String>>,
    pub version: i64,
}

impl From<QuickAccessPreference> for QuickAccessPreferenceWire {
    fn from(value: QuickAccessPreference) -> Self {
        Self {
            pinned_ids: value.pinned_ids,
            version: value.version,
        }
    }
}

pub fn invalid_host() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_QUICK_ACCESS_INVALID",
        "Use a registered Account host.",
    )
}

pub fn missing_host() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_QUICK_ACCESS_UNAVAILABLE",
        "The native account quick access is unavailable.",
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

pub type QuickAccessGetFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Verified, SQL-free scope for one account area read. Composition derives it
/// from the current session (`my-access`/`security`) or, for `work-role`, from
/// the validated `from` workspace plus `workRoleOptions`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessGetScope {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
}

pub struct QuickAccessGetScopeRequest<'a> {
    pub host: &'a str,
    pub area: AccountArea,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
}

/// Composition-supplied Account authority. The owner router transports only
/// registered host + area + browser headers; it never resolves a session.
pub trait QuickAccessGetAuthority: Send + Sync {
    fn scope<'a>(
        &'a self,
        request: QuickAccessGetScopeRequest<'a>,
    ) -> QuickAccessGetFuture<'a, QuickAccessGetScope>;
}

/// Owner read port. The Pg implementation reuses the existing preference SQL;
/// no SQL or session decision lives in the transport.
pub trait QuickAccessGetReader: Send + Sync {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> QuickAccessGetFuture<'a, QuickAccessPreference>;
}

pub struct QuickAccessGetRequest<'a> {
    pub host: &'a str,
    pub area: &'a str,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
}

/// Validate the area, obtain the verified scope, execute the owner preference
/// read and return the exact retired wire shape.
pub async fn read_quick_access_get<A: QuickAccessGetAuthority + ?Sized, R: QuickAccessGetReader + ?Sized>(
    authority: &A,
    reader: &R,
    request: QuickAccessGetRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    let area = area_from(request.area)?;
    let scope = authority
        .scope(QuickAccessGetScopeRequest {
            host: request.host,
            area,
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
// postgres adapter — reuses the existing owner SELECT, no duplicated SQL
// ---------------------------------------------------------------------------

/// Pool-backed reader. Opens a short read transaction because the owner
/// preference adapter requires a caller-owned transaction; the executed
/// statement is `adapters/quick_access_preferences.rs:44-51`.
pub struct PgQuickAccessGet {
    pool: DatabasePool,
}

impl PgQuickAccessGet {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QuickAccessGetReader for PgQuickAccessGet {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> QuickAccessGetFuture<'a, QuickAccessPreference> {
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
    hosts: BTreeSet<String>,
    authority: Arc<dyn QuickAccessGetAuthority>,
    reader: Arc<dyn QuickAccessGetReader>,
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

fn bounded(wire: QuickAccessPreferenceWire) -> Response {
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
    Path(area): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing_host)?;
        if !state.hosts.contains(host) {
            return Err(missing_host());
        }
        read_quick_access_get(
            state.authority.as_ref(),
            state.reader.as_ref(),
            QuickAccessGetRequest {
                host,
                area: &area,
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

/// Build the native router over composition-supplied ports. `hosts` are the
/// exact registered Account hosts; no wildcard host is accepted.
pub fn quick_access_get_router(
    hosts: Vec<String>,
    authority: Arc<dyn QuickAccessGetAuthority>,
    reader: Arc<dyn QuickAccessGetReader>,
) -> Result<Router, BusinessError> {
    let hosts: BTreeSet<String> = hosts.into_iter().collect();
    if hosts.is_empty() || hosts.iter().any(|host| host.is_empty()) {
        return Err(BusinessError::invalid(
            "ACCOUNT_QUICK_ACCESS_CONFIG_INVALID",
            "Use exact distinct registered Account hosts.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/account/quick-access/{area}", get(read))
        .with_state(Arc::new(StateData {
            hosts,
            authority,
            reader,
        })))
}

/// Convenience constructor binding the owner pool-backed reader.
pub fn quick_access_get_router_with_pool(
    hosts: Vec<String>,
    authority: Arc<dyn QuickAccessGetAuthority>,
    pool: DatabasePool,
) -> Result<Router, BusinessError> {
    quick_access_get_router(hosts, authority, Arc::new(PgQuickAccessGet::new(pool)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Authority;
    impl QuickAccessGetAuthority for Authority {
        fn scope<'a>(
            &'a self,
            request: QuickAccessGetScopeRequest<'a>,
        ) -> QuickAccessGetFuture<'a, QuickAccessGetScope> {
            let scope = match request.area {
                AccountArea::WorkRole => QuickAccessGetScope {
                    user_id: "user-1".into(),
                    surface: "account-work-role".into(),
                    context_key: "acme:holding-1".into(),
                },
                area => QuickAccessGetScope {
                    user_id: "user-1".into(),
                    surface: format!("account-{}", area.as_str()),
                    context_key: "account".into(),
                },
            };
            Box::pin(async move { Ok(scope) })
        }
    }

    struct Reader {
        preference: Option<QuickAccessPreference>,
        seen: Arc<Mutex<Vec<(String, String, String)>>>,
    }
    impl QuickAccessGetReader for Reader {
        fn read<'a>(
            &'a self,
            user_id: &'a str,
            surface: &'a str,
            context_key: &'a str,
        ) -> QuickAccessGetFuture<'a, QuickAccessPreference> {
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

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "account.example.test".parse().unwrap());
        headers
    }

    #[test]
    fn area_guard_pins_the_three_retired_areas() {
        assert_eq!(area_from("my-access").unwrap(), AccountArea::MyAccess);
        assert_eq!(area_from("security").unwrap(), AccountArea::Security);
        assert_eq!(area_from("work-role").unwrap(), AccountArea::WorkRole);
        assert_eq!(area_from("billing").unwrap_err().code, "NOT_FOUND");
    }

    #[tokio::test]
    async fn query_projects_the_exact_retired_wire_and_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Reader {
            preference: Some(QuickAccessPreference {
                pinned_ids: Some(vec!["overview".into(), "inbox".into()]),
                version: 3,
            }),
            seen: seen.clone(),
        };
        let wire = read_quick_access_get(
            &Authority,
            &reader,
            QuickAccessGetRequest {
                host: "account.example.test",
                area: "my-access",
                headers: &headers(),
                query: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["overview", "inbox"], "version": 3})
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[("user-1".to_owned(), "account-my-access".to_owned(), "account".to_owned())]
        );
    }

    #[tokio::test]
    async fn work_role_uses_the_composed_workspace_context() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Reader {
            preference: None,
            seen: seen.clone(),
        };
        let wire = read_quick_access_get(
            &Authority,
            &reader,
            QuickAccessGetRequest {
                host: "account.example.test",
                area: "work-role",
                headers: &headers(),
                query: Some("from=%2Fholdings%2Facme%2Ffinance"),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": null, "version": 0})
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[(
                "user-1".to_owned(),
                "account-work-role".to_owned(),
                "acme:holding-1".to_owned()
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
        let headers = text.split("\r\n\r\n").next().unwrap_or("").to_lowercase();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (status, headers, serde_json::from_str(body).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn router_serves_the_exact_envelope_and_fails_closed() {
        let router = quick_access_get_router(
            vec!["account.example.test".into()],
            Arc::new(Authority),
            Arc::new(Reader {
                preference: Some(QuickAccessPreference {
                    pinned_ids: Some(vec!["overview".into()]),
                    version: 2,
                }),
                seen: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });

        let ok = exchange(
            addr,
            "GET /api/v1/account/quick-access/my-access HTTP/1.1\r\nHost: account.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["pinnedIds"][0], "overview");
        assert_eq!(ok.2["data"]["version"], 2);

        let unknown_area = exchange(
            addr,
            "GET /api/v1/account/quick-access/billing HTTP/1.1\r\nHost: account.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(unknown_area.0, 404);
        assert_eq!(unknown_area.2["error"]["code"], "NOT_FOUND");

        let unknown_host = exchange(
            addr,
            "GET /api/v1/account/quick-access/my-access HTTP/1.1\r\nHost: other.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(unknown_host.0, 404);
        server.abort();
    }
}
