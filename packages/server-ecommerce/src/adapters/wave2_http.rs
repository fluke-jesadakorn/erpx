//! Owner-local HTTP ingress for the native Ecommerce wave2 cart/checkout
//! commands. Transport facts only: exact registered Host/Holding target, exact
//! browser origin, one Idempotency-Key, a bounded exact JSON body and business
//! error mapping. Session/scope/transaction resolution is composition-owned.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 8 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type CheckoutWave2HttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct CheckoutWave2HttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a serde_json::Value,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct CheckoutWave2HttpResponse {
    pub data: serde_json::Value,
}

impl CheckoutWave2HttpResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait CheckoutWave2HttpBackend: Send + Sync {
    fn write<'a>(
        &'a self,
        request: CheckoutWave2HttpRequest<'a>,
    ) -> CheckoutWave2HttpFuture<'a, CheckoutWave2HttpResponse>;
}

pub struct CheckoutWave2HttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CheckoutWave2HttpBackend>,
}

struct State2 {
    targets: BTreeMap<(String, String), CheckoutWave2HttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_WAVE2_REQUEST_INVALID",
        "Use one bounded E-commerce check-out command.",
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
        "ECOMMERCE_WAVE2_CONFIGURATION_INVALID",
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

fn valid_origin(origin: &str) -> bool {
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
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

pub fn ecommerce_checkout_wave2_router(
    targets: Vec<CheckoutWave2HttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/ecommerce/checkout-wave2",
            post(write),
        )
        .with_state(Arc::new(State2 {
            targets: configured,
        })))
}

async fn write(
    State(state): State<Arc<State2>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = unique_header(request.headers(), KEY_HEADER)?
            .ok_or_else(|| {
                BusinessError::invalid(
                    "COMMAND_KEY_REQUIRED",
                    "Use a valid Idempotency-Key for this action.",
                )
            })
            .and_then(CommandKey::new)?;
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
        if !body.is_object() {
            return Err(malformed());
        }
        target
            .backend
            .write(CheckoutWave2HttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                body: &body,
                key: Some(&key),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::wave2::parse_owner_write;
    use serde_json::json;

    #[test]
    fn typed_body_parses_closed_kinds() {
        let value = json!({"kind":"cart.abandon","recordId":"c","values":[1,"c",2]});
        assert!(parse_owner_write(&value).is_err()); // arity 2 expected
        let value = json!({"kind":"cart.abandon","recordId":"c","values":[1,"c"]});
        assert!(parse_owner_write(&value).is_ok());
        let value = json!({"kind":"unknown","recordId":"c","values":[]});
        assert!(parse_owner_write(&value).is_err());
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        struct Noop;
        impl CheckoutWave2HttpBackend for Noop {
            fn write<'a>(
                &'a self,
                _: CheckoutWave2HttpRequest<'a>,
            ) -> CheckoutWave2HttpFuture<'a, CheckoutWave2HttpResponse> {
                Box::pin(async { Err(missing()) })
            }
        }
        let backend = Arc::new(Noop);
        let target = || CheckoutWave2HttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(ecommerce_checkout_wave2_router(vec![target()]).is_ok());
        assert!(ecommerce_checkout_wave2_router(vec![target(), target()]).is_err());
        assert!(
            ecommerce_checkout_wave2_router(vec![CheckoutWave2HttpTarget {
                origin: "http://acme.example.test".into(),
                ..target()
            }])
            .is_err()
        );
    }
}
