//! Native ingress for `GET,POST /api/customer-portal/{holdingSlug}/quick-access`
//! (R3, lane L12).
//!
//! The retired route resolved a verified customer session and holding binding,
//! then read/wrote the same `quick_access_preferences` rows through
//! `readSessionQuickAccessPreferences`/`saveSessionQuickAccessPreferences`.
//! Those SQL statements are owned by this crate at
//! `adapters/quick_access_preferences.rs:44,81,115,153`; this module adds only
//! the exact-path transport, the strict body/key/origin guards, the
//! composition-supplied verified-scope port and the retired
//! `{"data":{...}}` envelope with `Cache-Control: private, no-store`.
//! No TypeScript, no fallback and no authority minting.

use crate::adapters::native_r3_quick_access_get::{
    QuickAccessGetReader, QuickAccessPreferenceWire, unavailable,
};
use crate::adapters::native_r3_quick_access_save::{
    QuickAccessSaveFuture, QuickAccessSaveWriter, now_millis,
};
use crate::public::quick_access_preferences::{
    QuickAccessPreference, SaveQuickAccessInput, save_quick_access,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use sqlx::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

const BODY_LIMIT: usize = 4096;
const SURFACE: &str = "customer_portal";

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn invalid_host() -> BusinessError {
    invalid(
        "PORTAL_QUICK_ACCESS_INVALID",
        "Use a registered Portal host.",
    )
}

fn missing_host() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid_slug_error() -> BusinessError {
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
    invalid("COMMAND_KEY_REQUIRED", "A command key is required.")
}

fn invalid_json() -> BusinessError {
    invalid("INVALID_JSON", "Invalid JSON.")
}

fn input_too_large() -> BusinessError {
    invalid("INPUT_TOO_LARGE", "Shorten the content before saving.")
}

fn invalid_shortcuts() -> BusinessError {
    invalid("INVALID_SHORTCUTS", "Choose up to six unique shortcuts.")
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// Strict retired body shape. The Portal schema bounds each pin id to 240
/// UTF-16 units (the Account surface uses 200); both cap pins at six.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortalQuickAccessBody {
    pub pinned_ids: Option<Vec<String>>,
    pub expected_version: i64,
}

fn validate_body(body: &PortalQuickAccessBody) -> Result<(), BusinessError> {
    if body.expected_version < 0 {
        return Err(invalid(
            "INVALID_SHORTCUTS",
            "Use a nonnegative shortcut version.",
        ));
    }
    if let Some(pins) = &body.pinned_ids {
        let mut seen = BTreeSet::new();
        if pins.len() > 6
            || pins.iter().any(|id| {
                id.is_empty() || id.encode_utf16().count() > 240 || !seen.insert(id.as_str())
            })
        {
            return Err(invalid_shortcuts());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// application: ports, no SQL and no authority minting
// ---------------------------------------------------------------------------

/// Verified, SQL-free customer scope. Composition derives the verified customer
/// identity and the holding binding id from the request headers; the owner
/// transport never resolves a session or a binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalQuickAccessScope {
    pub user_id: String,
    pub context_key: String,
    pub available_ids: Vec<String>,
}

pub struct PortalQuickAccessScopeRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub trait PortalQuickAccessAuthority: Send + Sync {
    fn scope<'a>(
        &'a self,
        request: PortalQuickAccessScopeRequest<'a>,
    ) -> QuickAccessSaveFuture<'a, PortalQuickAccessScope>;
}

pub struct PortalQuickAccessRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

/// Read through the existing owner SELECT (`surface=customer_portal`).
pub async fn portal_quick_access_get<
    A: PortalQuickAccessAuthority + ?Sized,
    R: QuickAccessGetReader + ?Sized,
>(
    authority: &A,
    reader: &R,
    request: PortalQuickAccessRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    if !valid_slug(request.holding_slug) {
        return Err(invalid_slug_error());
    }
    let scope = authority
        .scope(PortalQuickAccessScopeRequest {
            host: request.host,
            holding_slug: request.holding_slug,
            headers: request.headers,
        })
        .await?;
    if scope.user_id.is_empty() || scope.context_key.is_empty() {
        return Err(unavailable());
    }
    let preference = reader
        .read(&scope.user_id, SURFACE, &scope.context_key)
        .await?;
    Ok(preference.into())
}

pub struct PortalQuickAccessSaveRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a [u8],
    pub key: &'a str,
}

/// Validate the strict body/key, obtain the verified scope and execute the
/// existing owner save SQL with the projection-derived available ids.
pub async fn portal_quick_access_post<
    A: PortalQuickAccessAuthority + ?Sized,
    W: QuickAccessSaveWriter + ?Sized,
>(
    authority: &A,
    writer: &W,
    request: PortalQuickAccessSaveRequest<'_>,
) -> Result<QuickAccessPreferenceWire, BusinessError> {
    if !valid_slug(request.holding_slug) {
        return Err(invalid_slug_error());
    }
    if request.key.is_empty() || request.key.encode_utf16().count() > 200 {
        return Err(command_key_required());
    }
    if request.body.len() > BODY_LIMIT {
        return Err(input_too_large());
    }
    let body: PortalQuickAccessBody =
        serde_json::from_slice(request.body).map_err(|_| invalid_json())?;
    validate_body(&body)?;
    let scope = authority
        .scope(PortalQuickAccessScopeRequest {
            host: request.host,
            holding_slug: request.holding_slug,
            headers: request.headers,
        })
        .await?;
    if scope.user_id.is_empty() || scope.context_key.is_empty() {
        return Err(unavailable());
    }
    let input = SaveQuickAccessInput {
        user_id: scope.user_id,
        surface: SURFACE.to_owned(),
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

/// Pool-backed reader/writer. Opens one short transaction per call because the
/// owner preference adapters require a caller-owned unit.
pub struct PgPortalQuickAccess {
    pool: DatabasePool,
}

impl PgPortalQuickAccess {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl QuickAccessGetReader for PgPortalQuickAccess {
    fn read<'a>(
        &'a self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> QuickAccessSaveFuture<'a, QuickAccessPreference> {
        Box::pin(async move {
            let mut connection = self.pool.raw().acquire().await.map_err(|_| unavailable())?;
            let mut tx = connection.begin().await.map_err(|_| unavailable())?;
            let result = crate::public::quick_access_preferences::read_quick_access(
                &mut tx,
                user_id,
                surface,
                context_key,
            )
            .await;
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

impl QuickAccessSaveWriter for PgPortalQuickAccess {
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

#[derive(Clone, Debug)]
pub struct PortalQuickAccessTarget {
    pub host: String,
    pub origin: String,
}

struct StateData {
    origins: BTreeMap<String, String>,
    authority: Arc<dyn PortalQuickAccessAuthority>,
    reader: Arc<dyn QuickAccessGetReader>,
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

fn respond(result: Result<QuickAccessPreferenceWire, BusinessError>) -> Response {
    match result {
        Ok(wire) => {
            let mut response =
                (StatusCode::OK, Json(serde_json::json!({"data": wire}))).into_response();
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Err(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
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

fn target<'a>(
    state: &'a StateData,
    headers: &'a HeaderMap,
) -> Result<(&'a str, &'a str), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing_host)?;
    let origin = state
        .origins
        .get(host)
        .map(String::as_str)
        .ok_or_else(missing_host)?;
    Ok((host, origin))
}

async fn read(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async {
        let (host, _) = target(&state, &headers)?;
        portal_quick_access_get(
            state.authority.as_ref(),
            state.reader.as_ref(),
            PortalQuickAccessRequest {
                host,
                holding_slug: &slug,
                headers: &headers,
            },
        )
        .await
    }
    .await;
    respond(result)
}

async fn save(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async {
        let (host, expected_origin) = target(&state, &headers)?;
        if unique_header(&headers, header::ORIGIN)? != Some(expected_origin) {
            return Err(origin_forbidden());
        }
        let key_header = HeaderName::from_static("idempotency-key");
        let key = unique_header(&headers, key_header)?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT + 1)
            .await
            .map_err(|_| input_too_large())?;
        portal_quick_access_post(
            state.authority.as_ref(),
            state.writer.as_ref(),
            PortalQuickAccessSaveRequest {
                host,
                holding_slug: &slug,
                headers: &headers,
                body: &bytes,
                key,
            },
        )
        .await
    }
    .await;
    respond(result)
}

/// Build the exact-path Portal router from distinct registered targets. The
/// lead mounts the returned Router into the private gateway.
pub fn portal_quick_access_router(
    targets: Vec<PortalQuickAccessTarget>,
    authority: Arc<dyn PortalQuickAccessAuthority>,
    reader: Arc<dyn QuickAccessGetReader>,
    writer: Arc<dyn QuickAccessSaveWriter>,
) -> Result<Router, BusinessError> {
    let mut origins = BTreeMap::new();
    for target in targets {
        if target.host.is_empty() || target.origin.is_empty() || origins.contains_key(&target.host)
        {
            return Err(invalid(
                "PORTAL_QUICK_ACCESS_CONFIG_INVALID",
                "Use exact distinct registered Portal hosts.",
            ));
        }
        origins.insert(target.host, target.origin);
    }
    if origins.is_empty() {
        return Err(invalid(
            "PORTAL_QUICK_ACCESS_CONFIG_INVALID",
            "Use exact distinct registered Portal hosts.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/customer-portal/{holding_slug}/quick-access",
            get(read).post(save),
        )
        .with_state(Arc::new(StateData {
            origins,
            authority,
            reader,
            writer,
        })))
}

/// Convenience constructor binding one pool-backed owner reader and writer.
pub fn portal_quick_access_router_with_pool(
    targets: Vec<PortalQuickAccessTarget>,
    authority: Arc<dyn PortalQuickAccessAuthority>,
    pool: DatabasePool,
) -> Result<Router, BusinessError> {
    let source = Arc::new(PgPortalQuickAccess::new(pool));
    portal_quick_access_router(targets, authority, source.clone(), source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Authority;
    impl PortalQuickAccessAuthority for Authority {
        fn scope<'a>(
            &'a self,
            _request: PortalQuickAccessScopeRequest<'a>,
        ) -> QuickAccessSaveFuture<'a, PortalQuickAccessScope> {
            Box::pin(async move {
                Ok(PortalQuickAccessScope {
                    user_id: "customer-1".into(),
                    context_key: "binding-1".into(),
                    available_ids: vec!["invoices".into(), "tickets".into()],
                })
            })
        }
    }

    struct Reader {
        seen: Arc<Mutex<Vec<(String, String, String)>>>,
    }
    impl QuickAccessGetReader for Reader {
        fn read<'a>(
            &'a self,
            user_id: &'a str,
            surface: &'a str,
            context_key: &'a str,
        ) -> QuickAccessSaveFuture<'a, QuickAccessPreference> {
            self.seen.lock().unwrap().push((
                user_id.to_owned(),
                surface.to_owned(),
                context_key.to_owned(),
            ));
            Box::pin(async move {
                Ok(QuickAccessPreference {
                    pinned_ids: Some(vec!["invoices".into()]),
                    version: 1,
                })
            })
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
            Box::pin(async move {
                Ok(QuickAccessPreference {
                    pinned_ids: input.pinned_ids.clone(),
                    version: input.expected_version + 1,
                })
            })
        }
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "portal.example.test".parse().unwrap());
        headers
    }

    #[test]
    fn slug_and_body_guards_match_the_retired_route() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
        let body = PortalQuickAccessBody {
            pinned_ids: Some(vec!["invoices".into(), "tickets".into()]),
            expected_version: 0,
        };
        assert!(validate_body(&body).is_ok());
        let duplicate = PortalQuickAccessBody {
            pinned_ids: Some(vec!["invoices".into(), "invoices".into()]),
            expected_version: 0,
        };
        assert_eq!(
            validate_body(&duplicate).unwrap_err().code,
            "INVALID_SHORTCUTS"
        );
        let negative = PortalQuickAccessBody {
            pinned_ids: None,
            expected_version: -1,
        };
        assert_eq!(
            validate_body(&negative).unwrap_err().code,
            "INVALID_SHORTCUTS"
        );
    }

    #[tokio::test]
    async fn read_uses_the_customer_portal_surface_and_binding() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let wire = portal_quick_access_get(
            &Authority,
            &Reader { seen: seen.clone() },
            PortalQuickAccessRequest {
                host: "portal.example.test",
                holding_slug: "acme",
                headers: &headers(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["invoices"], "version": 1})
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[(
                "customer-1".to_owned(),
                "customer_portal".to_owned(),
                "binding-1".to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn save_uses_projection_available_ids_and_exact_wire() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let wire = portal_quick_access_post(
            &Authority,
            &Writer {
                saved: saved.clone(),
            },
            PortalQuickAccessSaveRequest {
                host: "portal.example.test",
                holding_slug: "acme",
                headers: &headers(),
                body: br#"{"pinnedIds":["invoices"],"expectedVersion":0}"#,
                key: "portal-key-0001",
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(&wire).unwrap(),
            serde_json::json!({"pinnedIds": ["invoices"], "version": 1})
        );
        let saved = saved.lock().unwrap();
        assert_eq!(saved[0].surface, "customer_portal");
        assert_eq!(saved[0].context_key, "binding-1");
        assert_eq!(saved[0].available_ids, vec!["invoices", "tickets"]);
        assert_eq!(saved[0].request_key, "portal-key-0001");
    }

    #[tokio::test]
    async fn save_guards_fail_closed_without_calling_the_writer() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer {
            saved: saved.clone(),
        };
        let no_key = portal_quick_access_post(
            &Authority,
            &writer,
            PortalQuickAccessSaveRequest {
                host: "portal.example.test",
                holding_slug: "acme",
                headers: &headers(),
                body: b"{}",
                key: "",
            },
        )
        .await
        .unwrap_err();
        assert_eq!(no_key.code, "COMMAND_KEY_REQUIRED");

        let bad_slug = portal_quick_access_post(
            &Authority,
            &writer,
            PortalQuickAccessSaveRequest {
                host: "portal.example.test",
                holding_slug: "Acme",
                headers: &headers(),
                body: b"{}",
                key: "portal-key-0001",
            },
        )
        .await
        .unwrap_err();
        assert_eq!(bad_slug.code, "NOT_FOUND");

        let extra = portal_quick_access_post(
            &Authority,
            &writer,
            PortalQuickAccessSaveRequest {
                host: "portal.example.test",
                holding_slug: "acme",
                headers: &headers(),
                body: br#"{"pinnedIds":[],"expectedVersion":0,"extra":1}"#,
                key: "portal-key-0001",
            },
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
    async fn router_serves_both_methods_and_fails_closed() {
        let router = portal_quick_access_router(
            vec![PortalQuickAccessTarget {
                host: "portal.example.test".into(),
                origin: "https://portal.example.test".into(),
            }],
            Arc::new(Authority),
            Arc::new(Reader {
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

        let ok_get = exchange(
            addr,
            "GET /api/customer-portal/acme/quick-access HTTP/1.1\r\nHost: portal.example.test\r\nConnection: close\r\n\r\n"
                .to_owned(),
        )
        .await;
        assert_eq!(ok_get.0, 200);
        assert!(ok_get.1.contains("cache-control: private, no-store"));
        assert_eq!(ok_get.2["data"]["pinnedIds"][0], "invoices");

        let ok_post = exchange(
            addr,
            "POST /api/customer-portal/acme/quick-access HTTP/1.1\r\nHost: portal.example.test\r\nOrigin: https://portal.example.test\r\nIdempotency-Key: portal-key-0001\r\nContent-Length: 46\r\nConnection: close\r\n\r\n{\"pinnedIds\":[\"invoices\"],\"expectedVersion\":0}"
                .to_owned(),
        )
        .await;
        assert_eq!(ok_post.0, 200);
        assert_eq!(ok_post.2["data"]["version"], 1);

        let no_origin = exchange(
            addr,
            "POST /api/customer-portal/acme/quick-access HTTP/1.1\r\nHost: portal.example.test\r\nIdempotency-Key: portal-key-0001\r\nContent-Length: 46\r\nConnection: close\r\n\r\n{\"pinnedIds\":[\"invoices\"],\"expectedVersion\":0}"
                .to_owned(),
        )
        .await;
        assert_eq!(no_origin.0, 403);
        assert_eq!(no_origin.2["error"]["code"], "ORIGIN_FORBIDDEN");
        server.abort();
    }
}
