//! Owner-local axum HTTP ingress for the native engineering verification read
//! port's `prior` statement. Transport facts only: exact registered Host/Holding
//! target, exact configured origin, one bounded read request. The SQL and the
//! transaction stay in the owner port; no TS fallback and no authority decision
//! live here. The caller has already validated the isolated engineering context.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

pub type EngineeringVerificationReadHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineeringVerificationPriorRequest {
    pub company_id: String,
    pub run: String,
}

pub struct EngineeringVerificationReadHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub origin: &'a str,
    pub prior: &'a EngineeringVerificationPriorRequest,
}

#[derive(Clone, Debug)]
pub struct EngineeringVerificationReadHttpResponse {
    pub data: serde_json::Value,
}

impl EngineeringVerificationReadHttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait EngineeringVerificationReadHttpBackend: Send + Sync {
    fn prior<'a>(
        &'a self,
        request: EngineeringVerificationReadHttpRequest<'a>,
    ) -> EngineeringVerificationReadHttpFuture<'a, EngineeringVerificationReadHttpResponse>;
}

#[derive(Clone)]
pub struct EngineeringVerificationReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn EngineeringVerificationReadHttpBackend>,
}

struct HttpState {
    targets: BTreeMap<(String, String), EngineeringVerificationReadHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ENGINEERING_VERIFICATION_READ_REQUEST_INVALID",
        "Use one bounded engineering verification read.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered application origin.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ENGINEERING_VERIFICATION_READ_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

/// Same origin rule as the owner's engineering enrollment command: the
/// isolated local gateway origin or an exact registered https Holding origin.
fn valid_origin(origin: &str) -> bool {
    if matches!(origin, "http://127.0.0.1:3104" | "http://localhost:3104") {
        return true;
    }
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
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

fn bounded_response(body: serde_json::Value) -> Response {
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

fn target<'a>(
    state: &'a HttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a EngineeringVerificationReadHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

pub fn engineering_verification_read_http_router(
    targets: Vec<EngineeringVerificationReadHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/platform/engineering-verification/read/prior",
            post(prior),
        )
        .with_state(Arc::new(HttpState {
            targets: configured,
        })))
}

fn parse_prior(body: &serde_json::Value) -> Result<EngineeringVerificationPriorRequest, BusinessError> {
    let field = |name: &str| {
        body.get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(malformed)
    };
    let prior = EngineeringVerificationPriorRequest {
        company_id: field("companyId")?,
        run: field("run")?,
    };
    if prior.company_id.is_empty()
        || prior.company_id.len() > 200
        || prior.run.len() != 12
        || !prior
            .run
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(malformed());
    }
    Ok(prior)
}

async fn bounded_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    if request.uri().query().is_some() {
        return Err(malformed());
    }
    let body = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    let body: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
    if !body.is_object() {
        return Err(malformed());
    }
    Ok(body)
}

async fn prior(
    State(state): State<Arc<HttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let body = bounded_body(request).await?;
        let prior = parse_prior(&body)?;
        target
            .backend
            .prior(EngineeringVerificationReadHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                origin: &target.origin,
                prior: &prior,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded_response(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::body;
    use std::sync::Mutex;

    struct Noop;
    impl EngineeringVerificationReadHttpBackend for Noop {
        fn prior<'a>(
            &'a self,
            _: EngineeringVerificationReadHttpRequest<'a>,
        ) -> EngineeringVerificationReadHttpFuture<'a, EngineeringVerificationReadHttpResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    struct Probe {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl EngineeringVerificationReadHttpBackend for Probe {
        fn prior<'a>(
            &'a self,
            request: EngineeringVerificationReadHttpRequest<'a>,
        ) -> EngineeringVerificationReadHttpFuture<'a, EngineeringVerificationReadHttpResponse> {
            let seen = self.seen.clone();
            let run = request.prior.run.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(run);
                Ok(EngineeringVerificationReadHttpResponse {
                    data: serde_json::json!({"data":{"source_fingerprint":"a".repeat(64),"software_version":format!("release-{}","a".repeat(64)),"release_manifest_sha256":"b".repeat(64)}}),
                })
            })
        }
    }

    fn target(
        backend: Arc<dyn EngineeringVerificationReadHttpBackend>,
        host: &'static str,
    ) -> EngineeringVerificationReadHttpTarget {
        EngineeringVerificationReadHttpTarget {
            host: host.into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn request(host: &'static str, origin: &'static str, run: &str) -> Request {
        Request::builder()
            .method("POST")
            .uri("/api/v1/holdings/acme/platform/engineering-verification/read/prior")
            .header("host", host)
            .header("origin", origin)
            .body(body::Body::from(
                serde_json::json!({"companyId":"company-1","run":run}).to_string(),
            ))
            .unwrap()
    }

    #[test]
    fn configuration_requires_distinct_exact_holdings_and_valid_origins() {
        let backend = Arc::new(Noop);
        assert!(engineering_verification_read_http_router(vec![target(
            backend.clone(),
            "acme.example.test"
        )])
        .is_ok());
        assert!(engineering_verification_read_http_router(vec![
            target(backend.clone(), "acme.example.test"),
            target(backend.clone(), "acme.example.test"),
        ])
        .is_err());
        assert!(engineering_verification_read_http_router(vec![
            EngineeringVerificationReadHttpTarget {
                origin: "http://forged.example.test".into(),
                ..target(backend, "acme.example.test")
            }
        ])
        .is_err());
    }

    #[test]
    fn parse_prior_is_bounded() {
        let body = serde_json::json!({"companyId":"company-1","run":"abcdef123456"});
        assert_eq!(parse_prior(&body).unwrap().run, "abcdef123456");
        assert!(parse_prior(&serde_json::json!({"companyId":"","run":"abcdef123456"})).is_err());
        assert!(parse_prior(&serde_json::json!({"companyId":"c","run":"xyz"})).is_err());
        assert!(parse_prior(&serde_json::json!({"companyId":"c"})).is_err());
    }

    #[tokio::test]
    async fn registered_target_reaches_backend_and_response_is_bounded() {
        let seen = Arc::new(Mutex::new(vec![]));
        let backend: Arc<dyn EngineeringVerificationReadHttpBackend> = Arc::new(Probe {
            seen: seen.clone(),
        });
        let state = Arc::new(HttpState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                target(backend, "acme.example.test"),
            )]),
        });
        let response = prior(
            State(state),
            Path("acme".to_owned()),
            request("acme.example.test", "https://acme.example.test", "abcdef123456"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let bytes = body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["data"]["source_fingerprint"], "a".repeat(64));
        assert_eq!(seen.lock().unwrap().as_slice(), &["abcdef123456"]);
    }

    #[tokio::test]
    async fn unregistered_host_forged_origin_and_malformed_body_fail_closed() {
        let backend: Arc<dyn EngineeringVerificationReadHttpBackend> = Arc::new(Noop);
        let state = Arc::new(HttpState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                target(backend, "acme.example.test"),
            )]),
        });
        for response in [
            prior(
                State(state.clone()),
                Path("acme".to_owned()),
                request("forged.example.test", "https://acme.example.test", "abcdef123456"),
            )
            .await,
            prior(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.example.test", "http://forged.example.test", "abcdef123456"),
            )
            .await,
            prior(
                State(state.clone()),
                Path("acme".to_owned()),
                request("acme.example.test", "https://acme.example.test", "xyz"),
            )
            .await,
        ] {
            assert_ne!(response.status(), StatusCode::OK);
        }
    }
}
