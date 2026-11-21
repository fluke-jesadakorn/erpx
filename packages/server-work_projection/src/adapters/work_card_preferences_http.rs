//! Owner-local HTTP ingress for native work-card preferences.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! Account/Platform session, current work authority and PostgreSQL unit stay
//! composition responsibilities supplied through the backend trait.
use crate::domain::work_card_preferences::{
    SaveWorkCardPreferences, WorkCardPreferences, invalid, pinned_json,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, get, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type WorkCardPreferencesHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct WorkCardPreferencesReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

pub struct WorkCardPreferencesSaveHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub input: SaveWorkCardPreferences,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct WorkCardPreferencesHttpResponse {
    /// Current preferences (`{pinnedIds, version}`).
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the Account session, current work role and PostgreSQL unit.
pub trait WorkCardPreferencesHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: WorkCardPreferencesReadHttpRequest<'a>,
    ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse>;

    fn save<'a>(
        &'a self,
        request: WorkCardPreferencesSaveHttpRequest<'a>,
    ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse>;
}

pub struct WorkCardPreferencesHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn WorkCardPreferencesHttpBackend>,
}

pub struct WorkCardPreferencesHttpState {
    targets: BTreeMap<(String, String), WorkCardPreferencesHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a WorkCardPreferencesHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
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
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
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
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
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

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "WORK_CARD_PREFERENCES_SOURCE_INVALID",
        "The native work-card preference is unavailable.",
    )
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
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
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

fn validate_preferences(value: &Value) -> Result<WorkCardPreferences, BusinessError> {
    serde_json::from_value::<WorkCardPreferences>(value.clone()).map_err(|_| source_invalid())
}

impl WorkCardPreferencesHttpState {
    pub fn new(targets: Vec<WorkCardPreferencesHttpTarget>) -> Result<Self, BusinessError> {
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
                    "WORK_CARD_PREFERENCES_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
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
        if query.is_some() {
            return Err(invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let response = target
            .target
            .backend
            .read(WorkCardPreferencesReadHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
            })
            .await?;
        let data = validate_preferences(&response.data)?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": data, "context": response.context}),
        ))
    }

    pub async fn save(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.save_inner(headers, holding_slug, query, body).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn save_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let content_type = unique_header(headers, "content-type")?
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json" {
            return Err(invalid());
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
            return Err(invalid());
        }
        let raw = std::str::from_utf8(body).map_err(|_| invalid())?;
        let input = SaveWorkCardPreferences::parse(raw)?;
        let _ = pinned_json(input.pinned_ids.as_ref())?;
        let response = target
            .target
            .backend
            .save(WorkCardPreferencesSaveHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                input,
                headers,
            })
            .await?;
        let data = validate_preferences(&response.data)?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": data, "context": response.context}),
        ))
    }
}

async fn read_handler(
    State(state): State<Arc<WorkCardPreferencesHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state
        .read(request.headers(), &holding_slug, query.as_deref())
        .await
}

async fn save_handler(
    State(state): State<Arc<WorkCardPreferencesHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(invalid()),
    };
    state
        .save(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

pub fn work_card_preferences_router(
    targets: Vec<WorkCardPreferencesHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(WorkCardPreferencesHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/work/card-preferences",
            get(read_handler).post(save_handler),
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

    impl WorkCardPreferencesHttpBackend for Fake {
        fn read<'a>(
            &'a self,
            request: WorkCardPreferencesReadHttpRequest<'a>,
        ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse> {
            Box::pin(async move {
                self.seen
                    .lock()
                    .unwrap()
                    .push(format!("read:{}:{}", request.host, request.holding_slug));
                Ok(WorkCardPreferencesHttpResponse {
                    data: self.data.clone(),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }

        fn save<'a>(
            &'a self,
            request: WorkCardPreferencesSaveHttpRequest<'a>,
        ) -> WorkCardPreferencesHttpFuture<'a, WorkCardPreferencesHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "save:{}:{}:{}:{:?}",
                    request.host,
                    request.holding_slug,
                    request.input.expected_version,
                    request.input.pinned_ids
                ));
                Ok(WorkCardPreferencesHttpResponse {
                    data: self.data.clone(),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }
    }

    fn state() -> (WorkCardPreferencesHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            data: json!({"pinnedIds": ["tasks", "expenses"], "version": 3}),
            seen: Mutex::new(Vec::new()),
        });
        let state = WorkCardPreferencesHttpState::new(vec![WorkCardPreferencesHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    fn headers(origin: Option<&str>, key: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        if let Some(key) = key {
            headers.insert("idempotency-key", key.parse().unwrap());
        }
        headers.insert("content-type", "application/json".parse().unwrap());
        headers
    }

    #[test]
    fn read_and_save_forward_exact_target_and_bounded_input() {
        let (state, fake) = state();
        let read =
            block_on(state.read(&headers(Some("http://127.0.0.1:3102"), None), "acme", None));
        assert_eq!(read.status(), StatusCode::OK);
        let saved = block_on(state.save(
            &headers(Some("http://127.0.0.1:3102"), Some("work:cards:save:1")),
            "acme",
            None,
            br#"{"pinnedIds":["tasks"],"expectedVersion":2}"#,
        ));
        assert_eq!(saved.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            [
                "read:holding.erpxbrowser.test:3102:acme",
                "save:holding.erpxbrowser.test:3102:acme:2:Some([\"tasks\"])"
            ]
        );
    }

    #[test]
    fn rejects_wrong_origin_unknown_holding_missing_key_and_bad_body() {
        let (state, _) = state();
        assert_eq!(
            block_on(state.read(&headers(Some("https://evil.test"), None), "acme", None)).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            block_on(state.read(&headers(Some("http://127.0.0.1:3102"), None), "other", None))
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            block_on(state.save(
                &headers(Some("http://127.0.0.1:3102"), None),
                "acme",
                None,
                br#"{"pinnedIds":[],"expectedVersion":1}"#
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            block_on(state.save(
                &headers(Some("http://127.0.0.1:3102"), Some("work:cards:save:1")),
                "acme",
                None,
                br#"{"pinnedIds":["tasks"],"expectedVersion":1,"extra":true}"#
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn router_builds_and_rejects_duplicate_targets() {
        let (_, fake) = state();
        let target = || WorkCardPreferencesHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        };
        assert!(work_card_preferences_router(vec![target()]).is_ok());
        assert!(work_card_preferences_router(vec![target(), target()]).is_err());
    }
}
