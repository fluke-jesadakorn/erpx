//! Owner-local axum router for the native media-publication read/create.
//!
//! A registered Holding host + slug pair is the only target. Company, user and
//! Holding identity arrive as unique bounded headers; the POST additionally
//! proves the browser Origin and one Idempotency-Key. The router transports the
//! owner payload and never plans SQL or accepts a caller-supplied statement.

use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind, RecordId};

use super::application::{
    CreateRequest, PublicationFileStore, PublicationStore, create_publication,
};
use super::domain::CreatePublicationInput;
use super::postgres::PgPublicationStore;

const BODY_LIMIT: usize = 64 * 1024;
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_HOLDING: HeaderName = HeaderName::from_static("x-erpx-context-holding");
const MANAGE_HEADER: HeaderName = HeaderName::from_static("x-erpx-files-publications-manage");

/// Exact registered Holding target. No wildcard or fallback host is accepted.
pub struct MediaPublicationsTarget {
    pub host: String,
    pub holding_slug: String,
    /// Expected browser origin, normally `https://<host>`.
    pub origin: String,
}

struct TargetState {
    origin: String,
    store: Arc<dyn PublicationStore>,
    files: Arc<dyn PublicationFileStore>,
}

struct AppState {
    targets: BTreeMap<(String, String), TargetState>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MEDIA_PUBLICATIONS_CONFIG_INVALID",
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

fn valid_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"));
    rest.is_some_and(|host| {
        !host.is_empty()
            && !host.bytes().any(|b| {
                b.is_ascii_whitespace()
                    || matches!(b, b'/' | b'?' | b'#' | b'@' | b'\\')
            })
    })
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("MEDIA_PUBLICATIONS_INVALID", "Use one bounded request."));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("MEDIA_PUBLICATIONS_INVALID", "Use one bounded request."))
        })
        .transpose()
}

fn identity(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique_header(headers, name)?
        .filter(|value| RecordId::new(*value).is_ok())
        .ok_or_else(|| invalid("MEDIA_PUBLICATIONS_INVALID", "Use one bounded request."))?;
    Ok(value.to_owned())
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

fn target_for<'a>(
    state: &'a AppState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a TargetState, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn from_state(
    entries: impl IntoIterator<
        Item = (
            MediaPublicationsTarget,
            Arc<dyn PublicationStore>,
            Arc<dyn PublicationFileStore>,
        ),
    >,
) -> Result<Arc<AppState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, store, files) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    TargetState {
                        origin: target.origin,
                        store,
                        files,
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(AppState {
        targets: configured,
    }))
}

fn now_millis() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(0)
}

/// Build the native router from pre-built owner ports. The lead mounts it in
/// the Holding gateway alongside the other Files owner routers.
pub fn media_publications_router(
    store: Arc<dyn PublicationStore>,
    files: Arc<dyn PublicationFileStore>,
    targets: Vec<MediaPublicationsTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/media-publications",
            get(list).post(create),
        )
        .with_state(from_state(targets.into_iter().map(|target| {
            (target, store.clone(), files.clone())
        }))?))
}

/// Per-holding constructor: each `(target, pool)` pair owns its own store while
/// the byte store is shared at composition. Two Holdings never share a pool.
pub fn media_publications_router_for_holdings(
    targets: Vec<(MediaPublicationsTarget, DatabasePool)>,
    files: Arc<dyn PublicationFileStore>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/media-publications",
            get(list).post(create),
        )
        .with_state(from_state(targets.into_iter().map(|(target, pool)| {
            let store: Arc<dyn PublicationStore> = Arc::new(PgPublicationStore::new(pool));
            (target, store, files.clone())
        }))?))
}

async fn list(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target_for(&state, request.headers(), &slug)?;
        let company = identity(request.headers(), CONTEXT_COMPANY)?;
        let target_type = query_value(&request, "targetType")
            .filter(|value| !value.is_empty() && value.len() <= 80)
            .ok_or_else(|| invalid("PUBLICATION_TARGET_REQUIRED", "Choose a public media owner."))?;
        let target_id = query_value(&request, "targetId")
            .filter(|value| !value.is_empty() && value.len() <= 180)
            .ok_or_else(|| invalid("PUBLICATION_TARGET_REQUIRED", "Choose a public media owner."))?;
        let manage = matches!(unique_header(request.headers(), MANAGE_HEADER)?, Some("true"));
        let payload = target
            .store
            .list(&company, &target_type, &target_id, manage)
            .await?;
        serde_json::to_value(payload)
            .map_err(|_| BusinessError::new(FailureKind::Unavailable, "MEDIA_PUBLICATIONS_UNAVAILABLE", "The native media publication service is unavailable."))
    }
    .await;
    match result {
        Ok(payload) => bounded(serde_json::json!({"data": payload})),
        Err(error) => error_response(error),
    }
}

async fn create(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target_for(&state, request.headers(), &slug)?;
        let headers = request.headers().clone();
        if let Some(origin) = unique_header(&headers, header::ORIGIN)? {
            if origin != target.origin {
                return Err(forbidden());
            }
        }
        let key = unique_header(&headers, KEY_HEADER)?
            .ok_or_else(|| invalid("COMMAND_KEY_REQUIRED", "A valid Idempotency-Key is required."))
            .and_then(CommandKey::new)?;
        let company = identity(&headers, CONTEXT_COMPANY)?;
        let user = identity(&headers, CONTEXT_USER)?;
        let holding_id = identity(&headers, CONTEXT_HOLDING)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid("VALIDATION_FAILED", "Invalid publication."))?;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("VALIDATION_FAILED", "Invalid publication."))?;
        let input = CreatePublicationInput::parse(&parsed)?;
        let row = create_publication(
            target.store.as_ref(),
            target.files.clone(),
            CreateRequest {
                company,
                user,
                holding_id,
                input,
                key: key.as_str().to_owned(),
                now: now_millis(),
            },
        )
        .await?;
        serde_json::to_value(row).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "MEDIA_PUBLICATIONS_UNAVAILABLE",
                "The native media publication service is unavailable.",
            )
        })
    }
    .await;
    match result {
        Ok(row) => (StatusCode::CREATED, Json(serde_json::json!({"data": row}))).into_response(),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_media_publications::application::{
        PreparedCreate, PublicationFuture, PublicationObject, SourceLink,
    };
    use crate::adapters::native_media_publications::domain::{PublicationList, PublicationRow, stable_id};
    use erpx_platform_runtime_server::public::http_support::serve;
    use std::sync::Mutex;

    fn row() -> PublicationRow {
        PublicationRow {
            id: stable_id("company-1", "user-1", "key-00000001"),
            company_id: Some("company-1".into()),
            source_link_id: "link-1".into(),
            target_type: "holding_brand".into(),
            target_id: "acme".into(),
            derivative_object_key: "holding-1/published/pub-1/r1".into(),
            derivative_media_type: "image/webp".into(),
            derivative_checksum_sha256: "abc123".into(),
            derivative_bytes: 8,
            alt_text: "Acme mark".into(),
            ordinal: 0,
            revision: 1,
            state: "draft".into(),
            created_by: "user-1".into(),
            created_at: 1_760_000_000_000,
            approved_by: None,
            approved_at: None,
            revoked_by: None,
            revoked_at: None,
            revocation_reason: None,
        }
    }

    fn source() -> SourceLink {
        SourceLink {
            id: "link-1".into(),
            attachment_id: "attachment-1".into(),
            preview_object_key: "holding-1/quarantine/upload-1/original".into(),
            media_type: "image/webp".into(),
            target_type: "holding_brand".into(),
            target_id: "acme".into(),
            purpose_key: "primary".into(),
            source_state: "available".into(),
        }
    }

    #[derive(Clone)]
    struct Probe {
        writes: Arc<Mutex<u32>>,
        stored: Arc<Mutex<Vec<String>>>,
    }

    impl PublicationStore for Probe {
        fn list(
            &self,
            _: &str,
            target_type: &str,
            target_id: &str,
            _: bool,
        ) -> PublicationFuture<'_, PublicationList> {
            let (target_type, target_id) = (target_type.to_owned(), target_id.to_owned());
            Box::pin(async move {
                Ok(PublicationList {
                    target_type,
                    target_id,
                    order_version: 1,
                    items: vec![row()],
                })
            })
        }
        fn source(&self, _: &str, _: &str) -> PublicationFuture<'_, Option<SourceLink>> {
            Box::pin(async { Ok(Some(source())) })
        }
        fn publication(&self, _: &str, _: &str) -> PublicationFuture<'_, Option<PublicationRow>> {
            Box::pin(async { Ok(None) })
        }
        fn create(
            &self,
            files: Arc<dyn PublicationFileStore>,
            request: &PreparedCreate,
        ) -> PublicationFuture<'_, PublicationRow> {
            *self.writes.lock().unwrap() += 1;
            let mut result = row();
            result.id = request.id.clone();
            let object_key = format!(
                "{}/published/{}/r1",
                request.holding_id, request.id
            );
            Box::pin(async move {
                files
                    .put_once(&object_key, &[1, 2, 3, 4, 5, 6, 7, 8], "image/webp", "abc123")
                    .await?;
                Ok(result)
            })
        }
    }

    struct Files(Arc<Mutex<Vec<String>>>);

    impl PublicationFileStore for Files {
        fn get(&self, _: &str) -> PublicationFuture<'_, Option<PublicationObject>> {
            Box::pin(async {
                Ok(Some(PublicationObject {
                    bytes: vec![1, 2, 3, 4, 5, 6, 7, 8],
                    media_type: "image/webp".into(),
                }))
            })
        }
        fn put_once(&self, key: &str, _: &[u8], _: &str, _: &str) -> PublicationFuture<'_, ()> {
            self.0.lock().unwrap().push(key.to_owned());
            Box::pin(async { Ok(()) })
        }
    }

    fn target() -> MediaPublicationsTarget {
        MediaPublicationsTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
        }
    }

    async fn serve_probe(
        probe: Probe,
        files: Files,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let router = media_publications_router(
            Arc::new(probe),
            Arc::new(files),
            vec![target()],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    async fn http(addr: std::net::SocketAddr, raw: &str) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        let parsed = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        (status, text, parsed)
    }

    fn headers(host: &str) -> String {
        format!(
            "Host: {host}\r\n\
             x-erpx-context-company: company-1\r\n\
             x-erpx-context-user: user-1\r\n\
             x-erpx-context-holding: holding-1\r\n"
        )
    }

    fn get(host: &str, query: &str) -> String {
        format!(
            "GET /api/v1/holdings/acme/media-publications?{query} HTTP/1.1\r\n{}\
             Accept: application/json\r\nConnection: close\r\n\r\n",
            headers(host)
        )
    }

    fn post(host: &str, origin: &str, key: &str, body: &str) -> String {
        format!(
            "POST /api/v1/holdings/acme/media-publications HTTP/1.1\r\n{}\
             Origin: {origin}\r\nIdempotency-Key: {key}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            headers(host),
            body.len()
        )
    }

    fn body() -> String {
        serde_json::json!({
            "sourceLinkId": "link-1",
            "targetType": "holding_brand",
            "targetId": "acme",
            "altText": "Acme mark",
            "ordinal": 0,
        })
        .to_string()
    }

    #[tokio::test]
    async fn get_returns_data_envelope_and_private_cache() {
        let writes = Arc::new(Mutex::new(0));
        let stored = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(
            Probe { writes, stored: stored.clone() },
            Files(stored),
        )
        .await;
        let (status, raw, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            &get("acme.jesadakorn.com", "targetType=holding_brand&targetId=acme"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body["data"]["targetType"], "holding_brand");
        assert_eq!(body["data"]["orderVersion"], 1);
        assert_eq!(body["data"]["items"][0]["source_link_id"], "link-1");
        assert!(raw.to_ascii_lowercase().contains("cache-control: private, no-store"));
    }

    #[tokio::test]
    async fn get_requires_target() {
        let stored = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(
            Probe { writes: Arc::new(Mutex::new(0)), stored: stored.clone() },
            Files(stored),
        )
        .await;
        let (status, _, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            &get("acme.jesadakorn.com", "targetId=acme"),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PUBLICATION_TARGET_REQUIRED");
    }

    #[tokio::test]
    async fn post_returns_201_and_stores_one_object() {
        let writes = Arc::new(Mutex::new(0));
        let stored = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(Probe { writes: writes.clone(), stored: stored.clone() }, Files(stored.clone())).await;
        let (status, _, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            &post("acme.jesadakorn.com", "https://acme.jesadakorn.com", "key-00000001", &body()),
        )
        .await;
        server.abort();
        assert_eq!(status, 201);
        assert_eq!(body["data"]["id"], stable_id("company-1", "user-1", "key-00000001"));
        assert_eq!(*writes.lock().unwrap(), 1);
        let expected = format!(
            "holding-1/published/{}/r1",
            body["data"]["id"].as_str().unwrap()
        );
        assert_eq!(stored.lock().unwrap().as_slice(), [expected]);
    }

    #[tokio::test]
    async fn post_rejects_origin_and_key_before_writing() {
        let writes = Arc::new(Mutex::new(0));
        let stored = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(Probe { writes: writes.clone(), stored: stored.clone() }, Files(stored)).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let (status, _, response) = http(
            addr,
            &post("acme.jesadakorn.com", "https://attacker.example.test", "key-00000001", &body()),
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(response["error"]["code"], "ORIGIN_FORBIDDEN");
        let (status, _, response) = http(
            addr,
            &post("acme.jesadakorn.com", "https://acme.jesadakorn.com", "bad", &body()),
        )
        .await;
        server.abort();
        assert_eq!(status, 422);
        assert_eq!(response["error"]["code"], "COMMAND_KEY_REQUIRED");
        assert_eq!(*writes.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn unknown_host_fails_closed() {
        let stored = Arc::new(Mutex::new(Vec::new()));
        let (port, server) = serve_probe(
            Probe { writes: Arc::new(Mutex::new(0)), stored: stored.clone() },
            Files(stored),
        )
        .await;
        let (status, _, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            &get("attacker.example.test", "targetType=holding_brand&targetId=acme"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let stored = Arc::new(Mutex::new(Vec::new()));
        let store: Arc<dyn PublicationStore> = Arc::new(Probe {
            writes: Arc::new(Mutex::new(0)),
            stored: stored.clone(),
        });
        let files: Arc<dyn PublicationFileStore> = Arc::new(Files(stored));
        assert!(media_publications_router(store.clone(), files.clone(), vec![target()]).is_ok());
        assert!(media_publications_router(store.clone(), files.clone(), vec![target(), target()]).is_err());
        assert!(media_publications_router(store, files, vec![]).is_err());
    }
}
