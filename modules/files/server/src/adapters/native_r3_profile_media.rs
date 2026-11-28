//! R3 native read: `GET /api/v1/holdings/{holding_slug}/profiles/{user_id}/media/{kind}`.
//!
//! Additive owner module for the lead to mount; no router, composition, Cargo or
//! TypeScript file is changed here. The owner executes the retained
//! `member_profiles` SELECT with SQLx and resolves the stored object through a
//! composition-supplied holding FileStore, then writes the same byte response
//! and headers the retained Next handler returned. The two write routes
//! (`POST|DELETE .../profiles/me/media/{kind}`) are intentionally not served:
//! they still compose workflow command assertions, an organization owner write
//! and the object-store mutation in TypeScript.

use std::{collections::BTreeSet, future::Future, pin::Pin, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use sqlx::Row;

// domain: pure types and invariants -----------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaKind {
    Avatar,
    Cover,
}

impl MediaKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "avatar" => Some(Self::Avatar),
            "cover" => Some(Self::Cover),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Cover => "cover",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRow {
    pub key: Option<String>,
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaBytes {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PROFILE_MEDIA_NOT_FOUND",
        "Unknown profile image.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROFILE_MEDIA_UNAVAILABLE",
        "Profile image temporarily unavailable.",
    )
}

fn valid_media_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
}

// application: ports, no SQL ------------------------------------------------

pub type MediaFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition-resolved, session-backed Work context. Nothing here is read
/// from the caller's query string or body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileMediaContext {
    pub company: String,
    pub holding_id: String,
}

/// The lead resolves the current Account session and active Holding membership
/// before the owner is allowed to read a member profile.
pub trait ProfileMediaAuthority: Send + Sync {
    fn context<'a>(
        &'a self,
        host: &'a str,
        holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> MediaFuture<'a, ProfileMediaContext>;
}

/// The owner reads the retained media row from its own scoped pool.
pub trait ProfileMediaRowStore: Send + Sync {
    fn row<'a>(
        &'a self,
        context: &ProfileMediaContext,
        user_id: &'a str,
        kind: MediaKind,
    ) -> MediaFuture<'a, Option<MediaRow>>;
}

/// A holding-scoped object store; `get` returns the stored bytes only.
pub trait ProfileMediaFileStore: Send + Sync {
    fn get<'a>(&'a self, key: &'a str) -> MediaFuture<'a, Option<Vec<u8>>>;
}

/// Validate before any I/O, then execute the row read and object read. A
/// missing row, missing key or missing object maps to the retained 503.
pub async fn handle(
    rows: &dyn ProfileMediaRowStore,
    files: &dyn ProfileMediaFileStore,
    context: &ProfileMediaContext,
    user_id: &str,
    kind: MediaKind,
) -> Result<MediaBytes, BusinessError> {
    if RecordId::new(user_id).is_err() {
        return Err(not_found());
    }
    let row = rows
        .row(context, user_id, kind)
        .await?
        .ok_or_else(unavailable)?;
    let key = row.key.filter(|key| !key.is_empty()).ok_or_else(unavailable)?;
    let media_type = row
        .media_type
        .filter(|value| valid_media_type(value))
        .ok_or_else(unavailable)?;
    let bytes = files.get(&key).await?.ok_or_else(unavailable)?;
    Ok(MediaBytes { bytes, media_type })
}

// adapters: SQLx execution and axum transport -------------------------------

/// The retained statement with `?` replaced by `$1`. `kind` is restricted to
/// two owner constants above, never interpolated from caller input.
pub const AVATAR_SQL: &str =
    "SELECT avatar_key AS key, avatar_media_type AS media_type FROM member_profiles WHERE user_id=$1";
pub const COVER_SQL: &str =
    "SELECT cover_key AS key, cover_media_type AS media_type FROM member_profiles WHERE user_id=$1";

pub struct PgProfileMediaRowStore {
    pool: DatabasePool,
}

impl PgProfileMediaRowStore {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ProfileMediaRowStore for PgProfileMediaRowStore {
    fn row<'a>(
        &'a self,
        _context: &ProfileMediaContext,
        user_id: &'a str,
        kind: MediaKind,
    ) -> MediaFuture<'a, Option<MediaRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let user = user_id.to_owned();
        let sql = match kind {
            MediaKind::Avatar => AVATAR_SQL,
            MediaKind::Cover => COVER_SQL,
        };
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let user = user.clone();
                Box::pin(async move {
                    let found = sqlx::query(sql)
                        .bind(user)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(found.map(|row| MediaRow {
                        key: row.try_get::<Option<String>, _>("key").ok().flatten(),
                        media_type: row.try_get::<Option<String>, _>("media_type").ok().flatten(),
                    }))
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

#[derive(Clone)]
pub struct ProfileMediaTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ProfileMediaState {
    targets: BTreeSet<(String, String)>,
    authority: Arc<dyn ProfileMediaAuthority>,
    rows: Arc<dyn ProfileMediaRowStore>,
    files: Arc<dyn ProfileMediaFileStore>,
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

fn bytes_response(value: MediaBytes) -> Response {
    let mut response = (StatusCode::OK, value.bytes).into_response();
    let media_type = HeaderValue::from_str(&value.media_type).unwrap_or_else(|_| {
        HeaderValue::from_static("application/octet-stream")
    });
    for (name, header_value) in [
        (header::CONTENT_TYPE, media_type),
        (header::CACHE_CONTROL, HeaderValue::from_static("private, no-store")),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("default-src 'none'; sandbox"),
        ),
    ] {
        response.headers_mut().insert(name, header_value);
    }
    response
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(unavailable());
    }
    first
        .map(|value| value.to_str().map_err(|_| unavailable()))
        .transpose()
}

/// Exact distinct registered Holding targets; no wildcard host or slug.
pub fn router(
    targets: Vec<ProfileMediaTarget>,
    authority: Arc<dyn ProfileMediaAuthority>,
    rows: Arc<dyn ProfileMediaRowStore>,
    files: Arc<dyn ProfileMediaFileStore>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeSet::new();
    for target in targets {
        if target.host.is_empty() || !valid_slug(&target.holding_slug) {
            return Err(BusinessError::invalid(
                "FILES_PROFILE_MEDIA_ROUTE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
        if !configured.insert((target.host, target.holding_slug)) {
            return Err(BusinessError::invalid(
                "FILES_PROFILE_MEDIA_ROUTE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "FILES_PROFILE_MEDIA_ROUTE_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/profiles/{user_id}/media/{kind}",
            get(media),
        )
        .with_state(Arc::new(ProfileMediaState {
            targets: configured,
            authority,
            rows,
            files,
        })))
}

async fn media(
    State(state): State<Arc<ProfileMediaState>>,
    Path((slug, user_id, kind)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        if !state.targets.contains(&(host.to_owned(), slug.clone())) {
            return Err(BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."));
        }
        let kind = MediaKind::parse(&kind).ok_or_else(not_found)?;
        let context = state.authority.context(host, &slug, &headers).await?;
        handle(state.rows.as_ref(), state.files.as_ref(), &context, &user_id, kind).await
    }
    .await;
    match result {
        Ok(value) => bytes_response(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn row() -> MediaRow {
        MediaRow {
            key: Some("profile/avatar-1".into()),
            media_type: Some("image/png".into()),
        }
    }

    struct Store {
        seen: Arc<Mutex<Vec<String>>>,
        row: Option<MediaRow>,
    }

    impl ProfileMediaRowStore for Store {
        fn row<'a>(
            &'a self,
            context: &ProfileMediaContext,
            user_id: &'a str,
            kind: MediaKind,
        ) -> MediaFuture<'a, Option<MediaRow>> {
            let seen = self.seen.clone();
            let value = format!(
                "{}:{}:{}:{}",
                context.company, context.holding_id, user_id, kind.as_str()
            );
            let row = self.row.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(value);
                Ok(row)
            })
        }
    }

    struct Files {
        objects: Vec<(String, Vec<u8>)>,
    }

    impl ProfileMediaFileStore for Files {
        fn get<'a>(&'a self, key: &'a str) -> MediaFuture<'a, Option<Vec<u8>>> {
            let found = self
                .objects
                .iter()
                .find(|(stored, _)| stored == key)
                .map(|(_, bytes)| bytes.clone());
            Box::pin(async move { Ok(found) })
        }
    }

    struct Authority;

    impl ProfileMediaAuthority for Authority {
        fn context<'a>(
            &'a self,
            _host: &'a str,
            _slug: &'a str,
            _headers: &'a HeaderMap,
        ) -> MediaFuture<'a, ProfileMediaContext> {
            Box::pin(async {
                Ok(ProfileMediaContext {
                    company: "acme-co".into(),
                    holding_id: "holding-1".into(),
                })
            })
        }
    }

    fn target() -> ProfileMediaTarget {
        ProfileMediaTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }
    }

    fn build(row: Option<MediaRow>, objects: Vec<(String, Vec<u8>)>) -> Router {
        let store = Arc::new(Store {
            seen: Arc::new(Mutex::new(Vec::new())),
            row,
        });
        let files = Arc::new(Files { objects });
        router(vec![target()], Arc::new(Authority), store, files).expect("router")
    }

    async fn serve(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr, server)
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, String) {
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
        (status, head.to_owned(), body.to_owned())
    }

    fn get(host: &str, slug: &str, user: &str, kind: &str) -> String {
        format!(
            "GET /api/v1/holdings/{slug}/profiles/{user}/media/{kind} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
    }

    #[test]
    fn statement_and_kind_are_owner_bounded() {
        assert!(MediaKind::parse("avatar").is_some());
        assert!(MediaKind::parse("cover").is_some());
        assert!(MediaKind::parse("people-directory").is_none());
        assert_eq!(AVATAR_SQL.matches('$').count(), 1);
        assert_eq!(COVER_SQL.matches('$').count(), 1);
        assert!(AVATAR_SQL.contains("avatar_key AS key"));
        assert!(COVER_SQL.contains("cover_media_type AS media_type"));
    }

    #[tokio::test]
    async fn handle_requires_row_key_media_type_and_object() {
        let store = Store { seen: Arc::new(Mutex::new(Vec::new())), row: Some(row()) };
        let files = Files { objects: vec![("profile/avatar-1".into(), b"png-bytes".to_vec())] };
        let context = ProfileMediaContext { company: "acme-co".into(), holding_id: "holding-1".into() };
        let value = handle(&store, &files, &context, "user-1", MediaKind::Avatar).await.unwrap();
        assert_eq!(value.bytes, b"png-bytes");
        assert_eq!(value.media_type, "image/png");

        let empty = Files { objects: vec![] };
        assert_eq!(
            handle(&store, &empty, &context, "user-1", MediaKind::Avatar)
                .await
                .unwrap_err()
                .code,
            "PROFILE_MEDIA_UNAVAILABLE"
        );
        let no_row = Store { seen: Arc::new(Mutex::new(Vec::new())), row: None };
        assert!(handle(&no_row, &files, &context, "user-1", MediaKind::Avatar).await.is_err());
        assert!(handle(&store, &files, &context, "", MediaKind::Avatar).await.is_err());
    }

    #[tokio::test]
    async fn native_get_returns_exact_bytes_and_headers() {
        let (addr, server) = serve(build(
            Some(row()),
            vec![("profile/avatar-1".into(), b"png-bytes".to_vec())],
        ))
        .await;
        let (status, head, body) = http(
            addr,
            get("acme.jesadakorn.com", "acme", "user-1", "avatar"),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(body, "png-bytes");
        let lower = head.to_ascii_lowercase();
        assert!(lower.contains("content-type: image/png"));
        assert!(lower.contains("cache-control: private, no-store"));
        assert!(lower.contains("x-content-type-options: nosniff"));
        assert!(lower.contains("content-security-policy: default-src 'none'; sandbox"));
    }

    #[tokio::test]
    async fn wrong_host_unknown_kind_and_missing_object_fail_closed() {
        let (addr, server) = serve(build(
            Some(row()),
            vec![("profile/avatar-1".into(), b"png-bytes".to_vec())],
        ))
        .await;
        assert_eq!(
            http(addr, get("other.jesadakorn.com", "acme", "user-1", "avatar"))
                .await
                .0,
            404
        );
        let (status, _, body) = http(
            addr,
            get("acme.jesadakorn.com", "acme", "user-1", "banner"),
        )
        .await;
        assert_eq!(status, 404);
        assert!(body.contains("PROFILE_MEDIA_NOT_FOUND"));
        let (addr2, server2) = serve(build(Some(row()), vec![])).await;
        let (status, _, body) = http(
            addr2,
            get("acme.jesadakorn.com", "acme", "user-1", "avatar"),
        )
        .await;
        assert_eq!(status, 503);
        assert!(body.contains("PROFILE_MEDIA_UNAVAILABLE"));
        server.abort();
        server2.abort();
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(router(
            vec![],
            Arc::new(Authority),
            Arc::new(Store { seen: Arc::new(Mutex::new(Vec::new())), row: None }),
            Arc::new(Files { objects: vec![] }),
        )
        .is_err());
        let mut bad = target();
        bad.holding_slug = "ACME".into();
        assert!(router(
            vec![bad],
            Arc::new(Authority),
            Arc::new(Store { seen: Arc::new(Mutex::new(Vec::new())), row: None }),
            Arc::new(Files { objects: vec![] }),
        )
        .is_err());
    }
}
