//! Owner-local Axum ingress for the native Work navigation read and the
//! Home read command. Transport facts only: registered Host/Holding target,
//! exact origin, bounded query/body, idempotency key and error mapping. The
//! Account session, current Work authority and the PostgreSQL unit remain
//! composition responsibilities supplied through the backend trait; the owner
//! SQL readers execute on that unit.
//!
//! The wire envelope is exactly `{data: ...}` with `private, no-store`, as the
//! retained Next route produced after `canonicalDestinationLinks`.
use crate::domain::navigation_wire::canonical_envelope;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type WorkNavigationHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct WorkNavigationReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub include_attention: bool,
    /// Captured at ingress, matching the retained route's `Date.now()`.
    pub read_through: i64,
}

pub struct WorkNavigationMarkHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub read_through: i64,
    pub key: &'a str,
}

#[derive(Clone, Debug)]
pub struct WorkNavigationReadHttpResponse {
    /// Exact `NavigationSnapshot` wire value from the owner SQL reader.
    pub data: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkNavigationMarkHttpResponse {
    pub home: i64,
}

/// Composition owns the Account session, Work authority and PostgreSQL unit.
pub trait WorkNavigationHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: WorkNavigationReadHttpRequest<'a>,
    ) -> WorkNavigationHttpFuture<'a, WorkNavigationReadHttpResponse>;

    fn mark_home_read<'a>(
        &'a self,
        request: WorkNavigationMarkHttpRequest<'a>,
    ) -> WorkNavigationHttpFuture<'a, WorkNavigationMarkHttpResponse>;
}

pub struct WorkNavigationHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn WorkNavigationHttpBackend>,
}

pub struct WorkNavigationHttpState {
    targets: BTreeMap<(String, String), WorkNavigationHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a WorkNavigationHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn input() -> BusinessError {
    BusinessError::invalid("NAVIGATION_REQUEST_INVALID", "Use the current Work navigation route.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin.")
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.")
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(input());
    }
    first
        .map(|value| value.to_str().map_err(|_| input()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "https" | "http") || authority.is_empty() || authority.len() > 255 {
        return false;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('.')
        || host.ends_with('.')
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
    {
        return false;
    }
    match port {
        None => true,
        Some(port) => {
            !port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|value| value > 0)
        }
    }
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
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
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
    json_response(status, json!({"error": {"code": error.code, "message": error.message}}))
}

fn now_millis() -> i64 {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

impl WorkNavigationHttpState {
    pub fn new(targets: Vec<WorkNavigationHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "WORK_NAVIGATION_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self { targets: configured })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
        mutation: bool,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if mutation || origin.is_some() {
            if origin != Some(target.origin.as_str()) {
                return Err(origin_forbidden());
            }
        }
        Ok(ParsedTarget { target, host })
    }

    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, query).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        let include_attention = match query {
            None => false,
            Some("include=attention") => true,
            Some(_) => return Err(input()),
        };
        let target = self.target(headers, holding_slug, false)?;
        let read_through = now_millis();
        let response = target
            .target
            .backend
            .read(WorkNavigationReadHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                include_attention,
                read_through,
            })
            .await?;
        validate_snapshot(&response.data, include_attention)?;
        Ok(json_response(StatusCode::OK, canonical_envelope(response.data)))
    }

    pub async fn mark(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.mark_inner(headers, holding_slug, query, body).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn mark_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(input());
        }
        let target = self.target(headers, holding_slug, true)?;
        let content_type = unique_header(headers, "content-type")?
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(input());
        }
        let key = unique_header(headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(command_key_required)?;
        if !(8..=180).contains(&key.len())
            || !key.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'.' | b'-')
            })
        {
            return Err(command_key_required());
        }
        if body.len() > BODY_LIMIT {
            return Err(input());
        }
        let body: Value = serde_json::from_slice(body).map_err(|_| input())?;
        if body != json!({"action": "read-home"}) {
            return Err(input());
        }
        let read_through = now_millis();
        let response = target
            .target
            .backend
            .mark_home_read(WorkNavigationMarkHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                read_through,
                key,
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": {"home": response.home}}),
        ))
    }
}

/// Fences the exact browser `NavigationSnapshot` keys; the owner reader must
/// supply every field the client consumes.
fn validate_snapshot(data: &Value, include_attention: bool) -> Result<(), BusinessError> {
    let invalid = || {
        BusinessError::new(
            FailureKind::Unavailable,
            "WORK_NAVIGATION_RESPONSE_INVALID",
            "The native Work navigation read is unavailable.",
        )
    };
    let Value::Object(map) = data else {
        return Err(invalid());
    };
    let mut keys = vec!["home", "work", "workViewAvailability"];
    if include_attention {
        keys.extend(["attention", "snapshotAt"]);
    }
    if keys.iter().any(|key| !map.contains_key(*key)) || map.len() != keys.len() {
        return Err(invalid());
    }
    Ok(())
}

async fn read_handler(
    State(state): State<Arc<WorkNavigationHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state.read(request.headers(), &holding_slug, query.as_deref()).await
}

async fn mark_handler(
    State(state): State<Arc<WorkNavigationHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(input()),
    };
    state.mark(&headers, &holding_slug, query.as_deref(), &body).await
}

/// Build the navigation router for exact registered Holding targets. The lead
/// mounts the returned Router into the private gateway.
pub fn work_navigation_router(
    targets: Vec<WorkNavigationHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(WorkNavigationHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/navigation",
            get(read_handler).post(mark_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        data: Value,
        seen: Mutex<Vec<String>>,
    }

    impl WorkNavigationHttpBackend for Fake {
        fn read<'a>(
            &'a self,
            request: WorkNavigationReadHttpRequest<'a>,
        ) -> WorkNavigationHttpFuture<'a, WorkNavigationReadHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "read:{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.include_attention,
                    request.read_through > 0
                ));
                Ok(WorkNavigationReadHttpResponse { data: self.data.clone() })
            })
        }

        fn mark_home_read<'a>(
            &'a self,
            request: WorkNavigationMarkHttpRequest<'a>,
        ) -> WorkNavigationHttpFuture<'a, WorkNavigationMarkHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "mark:{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.key,
                    request.read_through > 0
                ));
                Ok(WorkNavigationMarkHttpResponse { home: 0 })
            })
        }
    }

    fn snapshot() -> Value {
        json!({
            "home": 3,
            "work": 2,
            "attention": {
                "items": [{"href": "/support/ticket-1", "title": "Ticket", "updatedAt": 1}],
                "total": 2,
                "waiting": 0,
                "incomplete": false
            },
            "workViewAvailability": {"team": true, "routing": false},
            "snapshotAt": 1700000000000i64
        })
    }

    fn state() -> (WorkNavigationHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            data: snapshot(),
            seen: Mutex::new(Vec::new()),
        });
        let state = WorkNavigationHttpState::new(vec![WorkNavigationHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn headers(origin: Option<&str>, key: Option<&str>, mutation: bool) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "acme.jesadakorn.com".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        if let Some(key) = key {
            headers.insert("idempotency-key", key.parse().unwrap());
        }
        if mutation {
            headers.insert("content-type", "application/json".parse().unwrap());
        }
        headers
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn get_wraps_exact_snapshot_and_canonicalizes_only_hrefs() {
        let (state, fake) = state();
        let response = block_on(state.read(
            &headers(Some("https://acme.jesadakorn.com"), None, false),
            "acme",
            Some("include=attention"),
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let body = block_on(erpx_platform_runtime_server::public::http_support::body::to_bytes(
            response.into_body(),
            1 << 20,
        ))
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["home"], 3);
        assert_eq!(value["data"]["snapshotAt"], 1700000000000i64);
        assert_eq!(
            value["data"]["attention"]["items"][0]["href"],
            "/customers-sales/customers/service/cases/ticket-1"
        );
        assert_eq!(value["data"]["attention"]["items"][0]["title"], "Ticket");
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["read:acme.jesadakorn.com:acme:true:true"]
        );
    }

    #[test]
    fn post_requires_exact_body_and_key_and_returns_home_envelope() {
        let (state, fake) = state();
        let response = block_on(state.mark(
            &headers(Some("https://acme.jesadakorn.com"), Some("work:home:read:1"), true),
            "acme",
            None,
            br#"{"action":"read-home"}"#,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        let body = block_on(erpx_platform_runtime_server::public::http_support::body::to_bytes(
            response.into_body(),
            1 << 20,
        ))
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"data": {"home": 0}})
        );
        for (query, body, expected) in [
            (Some("include=attention"), br#"{"action":"read-home"}"#.as_slice(), StatusCode::UNPROCESSABLE_ENTITY),
            (None, br#"{"action":"other"}"#.as_slice(), StatusCode::UNPROCESSABLE_ENTITY),
            (None, br#"{"action":"read-home","extra":1}"#.as_slice(), StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            assert_eq!(
                block_on(state.mark(
                    &headers(Some("https://acme.jesadakorn.com"), Some("work:home:read:1"), true),
                    "acme",
                    query,
                    body,
                ))
                .status(),
                expected
            );
        }
        assert_eq!(fake.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn target_origin_key_and_query_are_fail_closed() {
        let (state, _) = state();
        assert_eq!(
            block_on(state.read(&headers(Some("https://evil.test"), None, false), "acme", None))
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            block_on(state.read(&headers(None, None, false), "other", None)).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            block_on(state.read(
                &headers(Some("https://acme.jesadakorn.com"), None, false),
                "acme",
                Some("include=other")
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.mark(
                &headers(Some("https://acme.jesadakorn.com"), None, true),
                "acme",
                None,
                br#"{"action":"read-home"}"#
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn router_builds_and_rejects_duplicate_targets() {
        let (_, fake) = state();
        let target = || WorkNavigationHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: fake.clone(),
        };
        assert!(work_navigation_router(vec![target()]).is_ok());
        assert!(work_navigation_router(vec![target(), target()]).is_err());
    }
}
