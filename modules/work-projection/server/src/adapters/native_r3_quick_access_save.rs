//! Native command for `POST /api/v1/account/quick-access/{area}` (R3, lane L12).
//!
//! Additive owner-local ingress. The retired route validated
//! `{pinnedIds, expectedVersion}` plus an `Idempotency-Key`, resolved the
//! Account session/workspace scope and called
//! `saveSessionQuickAccessPreferences`. The SQL is owned by this crate at
//! `adapters/quick_access_preferences.rs:44,81,115,153`; this module adds only
//! the transport, the bounded body, the verified-scope port and the exact
//! `{"data":{"pinnedIds":...,"version":...}}` envelope with
//! `Cache-Control: private, no-store`. No TypeScript, no fallback and no
//! authority minting: composition supplies the verified Account scope.

use crate::adapters::native_r3_quick_access_get::{
    AccountArea, QuickAccessGetFuture, QuickAccessPreferenceWire, area_from, unavailable,
};
use crate::public::quick_access_preferences::{
    QuickAccessPreference, SaveQuickAccessInput, save_quick_access,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::Connection;
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const BODY_LIMIT: usize = 192_000;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn invalid_host() -> BusinessError {
    invalid(
        "ACCOUNT_QUICK_ACCESS_INVALID",
        "Use a registered Account host.",
    )
}

fn missing_host() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn command_key_required() -> BusinessError {
    invalid(
        "COMMAND_KEY_REQUIRED",
        "A valid Idempotency-Key is required.",
    )
}

fn invalid_json() -> BusinessError {
    invalid("INVALID_JSON", "Provide valid JSON.")
}

fn input_too_large() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "INPUT_TOO_LARGE",
        "Shorten the content before saving.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// `serviceCommandBody` accepts exactly `[A-Za-z0-9:_.-]`, 8..=180 characters.
pub fn valid_command_key(key: &str) -> bool {
    (8..=180).contains(&key.len())
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-'))
}

/// Strict retired body shape: unknown fields are rejected, pins are nullable,
/// version is a nonnegative integer.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuickAccessSaveBody {
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
}

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// application: ports, no SQL and no authority minting
// ---------------------------------------------------------------------------

pub type QuickAccessSaveFuture<'a, T> = QuickAccessGetFuture<'a, T>;

/// Verified, SQL-free scope for one account area save. Composition derives it
/// from the current Account session, workspace choices and, for `work-role`,
/// the validated `from` page plus `workRoleOptions`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessSaveScope {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
    pub available_ids: Vec<String>,
}

pub struct QuickAccessSaveScopeRequest<'a> {
    pub host: &'a str,
    pub area: AccountArea,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
}

/// Composition-supplied Account authority. The owner router transports only
/// registered host + area + browser headers; it never resolves a session.
pub trait QuickAccessSaveAuthority: Send + Sync {
    fn scope<'a>(
        &'a self,
        request: QuickAccessSaveScopeRequest<'a>,
    ) -> QuickAccessSaveFuture<'a, QuickAccessSaveScope>;
}

/// Owner write port. The Pg implementation executes this crate's preference
/// SQL; no SQL or session decision lives in the transport.
pub trait QuickAccessSaveWriter: Send + Sync {
    fn save<'a>(
        &'a self,
        input: &'a SaveQuickAccessInput,
        now: i64,
    ) -> QuickAccessSaveFuture<'a, QuickAccessPreference>;
}

pub struct QuickAccessSaveRequest<'a> {
    pub host: &'a str,
    pub area: &'a str,
    pub headers: &'a HeaderMap,
    pub query: Option<&'a str>,
    pub body: &'a [u8],
    pub key: &'a str,
    pub origin: Option<&'a str>,
    pub origin_expected: &'a str,
}

/// Verify transport guards, obtain the verified scope, execute the owner save
/// and return the exact retired wire shape.
pub async fn save_quick_access_command<
    A: QuickAccessSaveAuthority + ?Sized,
    W: QuickAccessSaveWriter + ?Sized,
>(
    authority: &A,
    writer: &W,
    request: QuickAccessSaveRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    let area = area_from(request.area)?;
    if request
        .origin
        .is_some_and(|origin| origin != request.origin_expected)
    {
        return Err(origin_forbidden());
    }
    if !valid_command_key(request.key) {
        return Err(command_key_required());
    }
    if request.body.len() > BODY_LIMIT {
        return Err(input_too_large());
    }
    let body: QuickAccessSaveBody =
        serde_json::from_slice(request.body).map_err(|_| invalid_json())?;
    if body.expected_version < 0 {
        return Err(invalid(
            "INVALID_SHORTCUTS",
            "Use a nonnegative shortcut version.",
        ));
    }
    let scope = authority
        .scope(QuickAccessSaveScopeRequest {
            host: request.host,
            area,
            headers: request.headers,
            query: request.query,
        })
        .await?;
    if scope.user_id.is_empty() || scope.surface.is_empty() || scope.context_key.is_empty() {
        return Err(unavailable());
    }
    let input = SaveQuickAccessInput {
        user_id: scope.user_id,
        surface: scope.surface,
        context_key: scope.context_key,
        pinned_ids: body.pinned_ids,
        expected_version: body.expected_version,
        available_ids: scope.available_ids,
        request_key: request.key.to_owned(),
    };
    let saved = writer.save(&input, now_millis()).await?;
    Ok(saved.into())
}

// ---------------------------------------------------------------------------
// postgres adapter — executes this crate's existing owner SQL
// ---------------------------------------------------------------------------

/// Pool-backed writer. Opens one short transaction because the owner
/// preference adapter requires a caller-owned unit; the executed statements
/// are `adapters/quick_access_preferences.rs:81,115,153`.
pub struct PgQuickAccessSave {
    pool: DatabasePool,
}

impl PgQuickAccessSave {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QuickAccessSaveWriter for PgQuickAccessSave {
    fn save<'a>(
        &'a self,
        input: &'a SaveQuickAccessInput,
        now: i64,
    ) -> QuickAccessSaveFuture<'a, QuickAccessPreference> {
        Box::pin(async move {
            let mut connection = self.pool.raw().acquire().await.map_err(|_| unavailable())?;
            let mut tx = connection.begin().await.map_err(|_| unavailable())?;
            let result = save_quick_access(&mut tx, input, now).await;
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

/// One exact registered Account host and its public origin.
#[derive(Clone, Debug)]
pub struct QuickAccessSaveTarget {
    pub host: String,
    pub origin: String,
}

struct StateData {
    hosts: BTreeSet<String>,
    origins: std::collections::BTreeMap<String, String>,
    authority: Arc<dyn QuickAccessSaveAuthority>,
    writer: Arc<dyn QuickAccessSaveWriter>,
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
    let mut response = (StatusCode::OK, Json(serde_json::json!({"data": wire}))).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_host());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_host()))
        .transpose()
}

async fn save(
    State(state): State<Arc<StateData>>,
    Path(area): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing_host)?;
        let origin_expected = state
            .origins
            .get(host)
            .map(String::as_str)
            .ok_or_else(missing_host)?;
        let origin = unique_header(&headers, header::ORIGIN)?;
        let key_header = HeaderName::from_static("idempotency-key");
        let key = unique_header(&headers, key_header)?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT + 1)
            .await
            .map_err(|_| input_too_large())?;
        save_quick_access_command(
            state.authority.as_ref(),
            state.writer.as_ref(),
            QuickAccessSaveRequest {
                host,
                area: &area,
                headers: &headers,
                query: query.as_deref(),
                body: &bytes,
                key,
                origin,
                origin_expected,
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

/// Build the native router over composition-supplied ports. `targets` are the
/// exact registered Account hosts and their public origins; no wildcard host
/// is accepted.
pub fn quick_access_save_router(
    targets: Vec<QuickAccessSaveTarget>,
    authority: Arc<dyn QuickAccessSaveAuthority>,
    writer: Arc<dyn QuickAccessSaveWriter>,
) -> Result<Router, BusinessError> {
    let mut hosts = BTreeSet::new();
    let mut origins = std::collections::BTreeMap::new();
    for target in targets {
        if target.host.is_empty() || target.origin.is_empty() || !hosts.insert(target.host.clone())
        {
            return Err(invalid(
                "ACCOUNT_QUICK_ACCESS_CONFIG_INVALID",
                "Use exact distinct registered Account hosts.",
            ));
        }
        origins.insert(target.host, target.origin);
    }
    if hosts.is_empty() {
        return Err(invalid(
            "ACCOUNT_QUICK_ACCESS_CONFIG_INVALID",
            "Use exact distinct registered Account hosts.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/account/quick-access/{area}", post(save))
        .with_state(Arc::new(StateData {
            hosts,
            origins,
            authority,
            writer,
        })))
}

/// Convenience constructor binding the owner pool-backed writer.
pub fn quick_access_save_router_with_pool(
    targets: Vec<QuickAccessSaveTarget>,
    authority: Arc<dyn QuickAccessSaveAuthority>,
    pool: DatabasePool,
) -> Result<Router, BusinessError> {
    quick_access_save_router(targets, authority, Arc::new(PgQuickAccessSave::new(pool)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Authority {
        seen: Arc<Mutex<Vec<(String, String, String, Vec<String>)>>>,
    }
    impl QuickAccessSaveAuthority for Authority {
        fn scope<'a>(
            &'a self,
            request: QuickAccessSaveScopeRequest<'a>,
        ) -> QuickAccessSaveFuture<'a, QuickAccessSaveScope> {
            self.seen.lock().unwrap().push((
                request.host.to_owned(),
                request.area.as_str().to_owned(),
                format!("{:?}", request.query),
                vec!["overview".to_owned(), "inbox".to_owned()],
            ));
            let scope = match request.area {
                AccountArea::WorkRole => QuickAccessSaveScope {
                    user_id: "user-1".into(),
                    surface: "account-work-role".into(),
                    context_key: "acme:holding-1".into(),
                    available_ids: vec!["overview".into(), "inbox".into()],
                },
                area => QuickAccessSaveScope {
                    user_id: "user-1".into(),
                    surface: format!("account-{}", area.as_str()),
                    context_key: "account".into(),
                    available_ids: vec!["overview".into(), "inbox".into()],
                },
            };
            Box::pin(async move { Ok(scope) })
        }
    }

    struct Writer {
        saved: Arc<Mutex<Vec<SaveQuickAccessInput>>>,
    }
    impl QuickAccessSaveWriter for Writer {
        fn save<'a>(
            &'a self,
            input: &'a SaveQuickAccessInput,
            _now: i64,
        ) -> QuickAccessSaveFuture<'a, QuickAccessPreference> {
            self.saved.lock().unwrap().push(input.clone());
            let value = QuickAccessPreference {
                pinned_ids: input.pinned_ids.clone(),
                version: input.expected_version + 1,
            };
            Box::pin(async move { Ok(value) })
        }
    }

    fn request(body: &str, key: &str, origin: Option<&str>) -> QuickAccessSaveRequest<'_> {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "account.example.test".parse().unwrap());
        QuickAccessSaveRequest {
            host: "account.example.test",
            area: "my-access",
            headers: &headers,
            query: None,
            body: body.as_bytes(),
            key,
            origin,
            origin_expected: "https://account.example.test",
        }
    }

    #[test]
    fn command_key_guard_matches_the_retired_regex() {
        assert!(valid_command_key("save-account-0001"));
        assert!(!valid_command_key("short"));
        assert!(!valid_command_key("bad key with spaces"));
        assert!(!valid_command_key(&"x".repeat(181)));
    }

    #[tokio::test]
    async fn save_projects_the_exact_retired_wire_and_scope() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let wire = save_quick_access_command(
            &Authority {
                seen: Arc::new(Mutex::new(Vec::new())),
            },
            &Writer {
                saved: saved.clone(),
            },
            request(
                r#"{"pinnedIds":["overview"],"expectedVersion":1}"#,
                "save-account-0001",
                None,
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["overview"], "version": 2})
        );
        let saved = saved.lock().unwrap();
        assert_eq!(saved[0].surface, "account-my-access");
        assert_eq!(saved[0].request_key, "save-account-0001");
        assert_eq!(saved[0].available_ids, vec!["overview", "inbox"]);
    }

    #[tokio::test]
    async fn guards_fail_closed_without_calling_the_writer() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer {
            saved: saved.clone(),
        };
        let authority = Authority {
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let unknown = save_quick_access_command(
            &authority,
            &writer,
            QuickAccessSaveRequest {
                area: "billing",
                ..request("{}", "save-account-0001", None)
            },
        )
        .await
        .unwrap_err();
        assert_eq!(unknown.code, "NOT_FOUND");

        let origin = save_quick_access_command(
            &authority,
            &writer,
            request("{}", "save-account-0001", Some("https://evil.example.test")),
        )
        .await
        .unwrap_err();
        assert_eq!(origin.code, "ORIGIN_FORBIDDEN");

        let key = save_quick_access_command(&authority, &writer, request("{}", "bad", None))
            .await
            .unwrap_err();
        assert_eq!(key.code, "COMMAND_KEY_REQUIRED");

        let json =
            save_quick_access_command(&authority, &writer, request("{", "save-account-0001", None))
                .await
                .unwrap_err();
        assert_eq!(json.code, "INVALID_JSON");

        let extra = save_quick_access_command(
            &authority,
            &writer,
            request(
                r#"{"pinnedIds":[],"expectedVersion":0,"extra":1}"#,
                "save-account-0001",
                None,
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(extra.code, "INVALID_JSON");
        assert!(saved.lock().unwrap().is_empty());
    }

    async fn exchange(
        addr: std::net::SocketAddr,
        request: String,
    ) -> (u16, String, serde_json::Value) {
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
        (
            status,
            headers,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn router_serves_the_exact_envelope_and_fails_closed() {
        let router = quick_access_save_router(
            vec![QuickAccessSaveTarget {
                host: "account.example.test".into(),
                origin: "https://account.example.test".into(),
            }],
            Arc::new(Authority {
                seen: Arc::new(Mutex::new(Vec::new())),
            }),
            Arc::new(Writer {
                saved: Arc::new(Mutex::new(Vec::new())),
            }),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ =
                erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });

        let ok = exchange(
            addr,
            "POST /api/v1/account/quick-access/my-access HTTP/1.1\r\nHost: account.example.test\r\nIdempotency-Key: save-account-0001\r\nContent-Length: 46\r\nConnection: close\r\n\r\n{\"pinnedIds\":[\"overview\"],\"expectedVersion\":1}"
                .to_owned(),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["pinnedIds"][0], "overview");
        assert_eq!(ok.2["data"]["version"], 2);

        let no_key = exchange(
            addr,
            "POST /api/v1/account/quick-access/my-access HTTP/1.1\r\nHost: account.example.test\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                .to_owned(),
        )
        .await;
        assert_eq!(no_key.0, 422);
        assert_eq!(no_key.2["error"]["code"], "COMMAND_KEY_REQUIRED");

        let unknown_host = exchange(
            addr,
            "POST /api/v1/account/quick-access/my-access HTTP/1.1\r\nHost: other.example.test\r\nIdempotency-Key: save-account-0001\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                .to_owned(),
        )
        .await;
        assert_eq!(unknown_host.0, 404);
        server.abort();
    }
}
