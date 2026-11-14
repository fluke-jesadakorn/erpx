//! Owner-local HTTP ingress for the native `rate-target` command.
//!
//! Transport facts only: registered Host/Holding target, exact origin,
//! Idempotency-Key, bounded exact JSON input and business-error mapping. The
//! Account/Platform session, current Holding authority and serializable
//! Control unit remain composition responsibilities supplied through the
//! backend trait.
use crate::domain::target_rate_command::RateTarget;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;

pub type BillingRateTargetHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingRateTargetHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub command: RateTarget,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct BillingRateTargetHttpResponse {
    /// Exact `{ok:true}` command receipt.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the session, authority and serializable Control unit.
pub trait BillingRateTargetHttpBackend: Send + Sync {
    fn rate<'a>(
        &'a self,
        request: BillingRateTargetHttpRequest<'a>,
    ) -> BillingRateTargetHttpFuture<'a, BillingRateTargetHttpResponse>;
}

pub struct BillingRateTargetHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BillingRateTargetHttpBackend>,
}

pub struct BillingRateTargetHttpState {
    targets: BTreeMap<(String, String), BillingRateTargetHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingRateTargetHttpTarget,
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

fn request_invalid() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Use a valid target rating request.")
}

fn command_key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_TARGET_RATE_SOURCE_INVALID",
        "The native target rating response is unavailable.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    }) || origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
        })
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RateBody {
    version: i64,
    reason: String,
    target_month: String,
}

impl BillingRateTargetHttpState {
    pub fn new(targets: Vec<BillingRateTargetHttpTarget>) -> Result<Self, BusinessError> {
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
                    "BILLING_TARGET_RATE_HTTP_CONFIGURATION_INVALID",
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
        if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(ParsedTarget { target, host })
    }

    pub async fn rate(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.rate_inner(headers, holding_slug, query, body).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn rate_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(request_invalid());
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
            return Err(request_invalid());
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
            return Err(request_invalid());
        }
        let parsed = serde_json::from_slice::<RateBody>(body).map_err(|_| request_invalid())?;
        let command = RateTarget {
            key: CommandKey::new(key)?,
            expected_version: parsed.version,
            reason: parsed.reason,
            target_month: parsed.target_month,
        };
        command.validate()?;
        let response = target
            .target
            .backend
            .rate(BillingRateTargetHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                command,
                headers,
            })
            .await?;
        if response.data != json!({"ok": true}) {
            return Err(source_invalid());
        }
        Ok(json_response(
            StatusCode::OK,
            json!({"data": {"ok": true}, "context": response.context}),
        ))
    }
}

async fn rate_handler(
    State(state): State<Arc<BillingRateTargetHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(request_invalid()),
    };
    state
        .rate(&headers, &holding_slug, query.as_deref(), &body)
        .await
}

pub fn billing_rate_target_router(
    targets: Vec<BillingRateTargetHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingRateTargetHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/target/rate",
            post(rate_handler),
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

    impl BillingRateTargetHttpBackend for Fake {
        fn rate<'a>(
            &'a self,
            request: BillingRateTargetHttpRequest<'a>,
        ) -> BillingRateTargetHttpFuture<'a, BillingRateTargetHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request.command.target_month,
                    request.command.expected_version
                ));
                Ok(BillingRateTargetHttpResponse {
                    data: json!({"ok": true}),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }
    }

    fn state() -> (BillingRateTargetHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
        });
        let state = BillingRateTargetHttpState::new(vec![BillingRateTargetHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
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

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    const BODY: &[u8] =
        br#"{"version":2,"reason":"Rated from reviewed target evidence","targetMonth":"2027-01"}"#;

    #[test]
    fn rate_forwards_exact_target_and_month() {
        let (state, fake) = state();
        let response = block_on(state.rate(
            &headers(Some("http://127.0.0.1:3102"), Some("billing:rate:1")),
            "acme",
            None,
            BODY,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["holding.erpxbrowser.test:3102:acme:2027-01:2"]
        );
    }

    #[test]
    fn rejects_wrong_origin_holding_key_and_body() {
        let (state, _) = state();
        for (origin, slug, key, body) in [
            (Some("https://evil.test"), "acme", Some("billing:rate:1"), BODY),
            (Some("http://127.0.0.1:3102"), "other", Some("billing:rate:1"), BODY),
            (Some("http://127.0.0.1:3102"), "acme", None, BODY),
            (
                Some("http://127.0.0.1:3102"),
                "acme",
                Some("billing:rate:1"),
                br#"{"version":2,"reason":"Rated from reviewed target evidence","targetMonth":"2027-01","extra":true}"#,
            ),
        ] {
            let response = block_on(state.rate(&headers(origin, key), slug, None, body));
            assert!(response.status().is_client_error(), "{origin:?} {slug} {key:?}");
        }
    }
}
