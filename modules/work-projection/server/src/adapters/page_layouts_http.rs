//! Owner-local Axum ingress for native Work page layouts.
//!
//! Transport facts only: exact Host/Holding target, exact browser origin for
//! mutations, configured page key, bounded command body, idempotency key and
//! error mapping. The Account session, Company scope, work authority, widget
//! allow-list and PostgreSQL unit are supplied by composition through the
//! backend trait; the owner SQL reader executes the retained `page_layouts`
//! statement on that unit.
//!
//! The wire envelope is exactly `{data: LayoutSnapshot}`, matching the
//! retained Next route after `canonicalDestinationLinks`.
use crate::domain::navigation_wire::canonical_envelope;
use crate::domain::page_layouts::{is_configurable_page, parse_page_layout_input, PageLayoutInput};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type PageLayoutsHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PageLayoutsReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub page_key: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct PageLayoutsCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub page_key: &'a str,
    pub headers: &'a HeaderMap,
    pub key: &'a str,
    pub input: PageLayoutInput,
}

#[derive(Clone, Debug)]
pub struct PageLayoutsHttpResponse {
    /// Exact `LayoutSnapshot` wire value from the owner reader/writer.
    pub data: Value,
}

/// Composition owns the Account session, work authority and PostgreSQL unit.
pub trait PageLayoutsHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: PageLayoutsReadHttpRequest<'a>,
    ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse>;

    fn command<'a>(
        &'a self,
        request: PageLayoutsCommandHttpRequest<'a>,
    ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse>;
}

pub struct PageLayoutsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn PageLayoutsHttpBackend>,
}

pub struct PageLayoutsHttpState {
    targets: BTreeMap<(String, String), PageLayoutsHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a PageLayoutsHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn input() -> BusinessError {
    BusinessError::invalid("LAYOUT_REQUEST_INVALID", "Use the current page-layout route.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin.")
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key for this action.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_LAYOUT_UNAVAILABLE",
        "The native Work layout is unavailable.",
    )
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

impl PageLayoutsHttpState {
    pub fn new(targets: Vec<PageLayoutsHttpTarget>) -> Result<Self, BusinessError> {
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
                    "PAGE_LAYOUTS_HTTP_CONFIGURATION_INVALID",
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

    fn page_key(page_key: &str) -> Result<(), BusinessError> {
        if !is_configurable_page(page_key) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "PAGE_LAYOUT_UNKNOWN",
                "This page has no configurable widgets.",
            ));
        }
        Ok(())
    }

    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        page_key: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, page_key, query).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        page_key: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(input());
        }
        Self::page_key(page_key)?;
        let target = self.target(headers, holding_slug, false)?;
        let response = target
            .target
            .backend
            .read(PageLayoutsReadHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                page_key,
                headers,
            })
            .await?;
        validate_snapshot(&response.data)?;
        Ok(json_response(StatusCode::OK, canonical_envelope(response.data)))
    }

    pub async fn command(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        page_key: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.command_inner(headers, holding_slug, page_key, query, body).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn command_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        page_key: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(input());
        }
        Self::page_key(page_key)?;
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
        let value: Value = serde_json::from_slice(body).map_err(|_| input())?;
        let parsed = parse_page_layout_input(&value)?;
        let response = target
            .target
            .backend
            .command(PageLayoutsCommandHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                page_key,
                headers,
                key,
                input: parsed,
            })
            .await?;
        validate_snapshot(&response.data)?;
        Ok(json_response(StatusCode::OK, canonical_envelope(response.data)))
    }
}

async fn read_handler(
    State(state): State<Arc<PageLayoutsHttpState>>,
    Path((holding_slug, page_key)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state.read(request.headers(), &holding_slug, &page_key, query.as_deref()).await
}

async fn command_handler(
    State(state): State<Arc<PageLayoutsHttpState>>,
    Path((holding_slug, page_key)): Path<(String, String)>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(input()),
    };
    state.command(&headers, &holding_slug, &page_key, query.as_deref(), &body).await
}

/// Build the page-layout router for exact registered Holding targets. The lead
/// mounts the returned Router into the private gateway.
pub fn page_layouts_router(
    targets: Vec<PageLayoutsHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(PageLayoutsHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/page-layouts/{page_key}",
            get(read_handler).post(command_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        seen: Mutex<Vec<String>>,
    }

    impl PageLayoutsHttpBackend for Fake {
        fn read<'a>(
            &'a self,
            request: PageLayoutsReadHttpRequest<'a>,
        ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "read:{}:{}:{}",
                    request.host, request.holding_slug, request.page_key
                ));
                Ok(PageLayoutsHttpResponse {
                    data: json!({
                        "personal": null,
                        "organization": null,
                        "version": 2,
                        "organizationVersion": 3,
                        "canPublish": true
                    }),
                })
            })
        }

        fn command<'a>(
            &'a self,
            request: PageLayoutsCommandHttpRequest<'a>,
        ) -> PageLayoutsHttpFuture<'a, PageLayoutsHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "command:{}:{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.page_key,
                    request.key,
                    request.input.action == crate::domain::page_layouts::PageLayoutAction::Save
                ));
                Ok(PageLayoutsHttpResponse {
                    data: json!({
                        "personal": null,
                        "organization": null,
                        "version": 3,
                        "organizationVersion": 3,
                        "canPublish": true
                    }),
                })
            })
        }
    }

    fn state() -> (PageLayoutsHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake { seen: Mutex::new(Vec::new()) });
        let state = PageLayoutsHttpState::new(vec![PageLayoutsHttpTarget {
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

    async fn body(response: Response) -> Value {
        let bytes = erpx_platform_runtime_server::public::http_support::body::to_bytes(
            response.into_body(),
            1 << 20,
        )
        .await
        .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn get_wraps_exact_snapshot() {
        let (state, fake) = state();
        let response = block_on(state.read(
            &headers(Some("https://acme.jesadakorn.com"), None, false),
            "acme",
            "people-directory",
            None,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "private, no-store");
        let value = block_on(body(response));
        assert_eq!(value["data"]["version"], 2);
        assert_eq!(value["data"]["organizationVersion"], 3);
        assert_eq!(value["data"]["canPublish"], true);
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert_eq!(fake.seen.lock().unwrap().as_slice(), ["read:acme.jesadakorn.com:acme:people-directory"]);
    }

    #[test]
    fn unknown_page_and_query_are_rejected() {
        let (state, _) = state();
        assert_eq!(
            block_on(state.read(&headers(None, None, false), "acme", "platform-home", None)).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            block_on(state.read(&headers(None, None, false), "acme", "people-directory", Some("x=1"))).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn post_validates_command_key_body_and_origin() {
        let (state, fake) = state();
        let valid = br#"{"action":"save","expectedVersion":2,"layout":{"schemaVersion":1,"widgets":[]}}"#;
        let response = block_on(state.command(
            &headers(Some("https://acme.jesadakorn.com"), Some("wp-layout:save:1"), true),
            "acme",
            "people-directory",
            None,
            valid,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(block_on(body(response))["data"]["version"], 3);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["command:acme.jesadakorn.com:acme:people-directory:wp-layout:save:1:true"]
        );
        assert_eq!(
            block_on(state.command(
                &headers(Some("https://acme.jesadakorn.com"), Some("wp-layout:save:1"), true),
                "acme", "people-directory", None,
                br#"{"action":"publish","expectedVersion":2,"layout":null}"#
            ))
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            block_on(state.command(
                &headers(Some("https://acme.jesadakorn.com"), Some("wp-layout:save:1"), true),
                "acme", "people-directory", None,
                br#"{"action":"save","expectedVersion":2,"layout":{"schemaVersion":1,"widgets":[]},"extra":1}"#
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.command(
                &headers(Some("https://evil.test"), Some("wp-layout:save:1"), true),
                "acme", "people-directory", None, valid
            ))
            .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn router_builds_and_rejects_duplicate_targets() {
        let (_, fake) = state();
        let target = || PageLayoutsHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: fake.clone(),
        };
        assert!(page_layouts_router(vec![target()]).is_ok());
        assert!(page_layouts_router(vec![target(), target()]).is_err());
    }
}
