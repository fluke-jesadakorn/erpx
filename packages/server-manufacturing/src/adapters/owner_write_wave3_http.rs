//! Owner-local HTTP ingress for the native Manufacturing owner-write registry.
//!
//! Transport facts only: exact registered Host/Holding, exact browser Origin,
//! one Idempotency-Key and bounded JSON containing only a typed `kind` and
//! positional `values`. Session, scope and the serializable unit are
//! composition responsibilities behind [`OwnerWriteBackend`]. This router
//! never executes SQL itself and accepts no caller-supplied SQL text.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 32 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");
const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

pub type OwnerWriteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct OwnerWriteRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a serde_json::Value,
    pub key: Option<&'a CommandKey>,
}

#[derive(Clone, Debug)]
pub struct OwnerWriteResponse {
    pub data: serde_json::Value,
}

impl OwnerWriteResponse {
    pub fn value(self) -> serde_json::Value {
        self.data
    }
}

pub trait OwnerWriteBackend: Send + Sync {
    fn write<'a>(&'a self, request: OwnerWriteRequest<'a>) -> OwnerWriteFuture<'a, OwnerWriteResponse>;
}

pub struct OwnerWriteTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn OwnerWriteBackend>,
}

struct AppState {
    targets: BTreeMap<(String, String), OwnerWriteTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_OWNER_WRITE_REQUEST_INVALID",
        "Use one bounded Manufacturing command.",
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
        "MANUFACTURING_OWNER_WRITE_CONFIG_INVALID",
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
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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

/// The single Manufacturing owner-write router.
pub fn manufacturing_owner_write_router(
    targets: Vec<OwnerWriteTarget>,
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
            "/api/v1/holdings/{holding_slug}/manufacturing/owner-write",
            post(write),
        )
        .with_state(Arc::new(AppState {
            targets: configured,
        })))
}

async fn write(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state.targets.get(&(host, slug.clone())).ok_or_else(missing)?;
        let origin = unique_header(request.headers(), ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let key = unique_header(request.headers(), KEY_HEADER)?
            .ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.")
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
            .write(OwnerWriteRequest {
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
    use crate::domain::owner_write_wave3::parse_owner_write;
    use erpx_server_runtime::public::http_support::body::Body;
    use serde_json::json;

    struct Noop;
    impl OwnerWriteBackend for Noop {
        fn write<'a>(
            &'a self,
            _: OwnerWriteRequest<'a>,
        ) -> OwnerWriteFuture<'a, OwnerWriteResponse> {
            Box::pin(async { Err(missing()) })
        }
    }

    #[test]
    fn registry_and_configuration_guards() {
        assert!(parse_owner_write(&json!({
            "kind":"order.transition","companyId":"c","recordId":"o",
            "values":["planned",1,"o"]
        }))
        .is_ok());
        let backend = Arc::new(Noop);
        let target = || OwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(manufacturing_owner_write_router(vec![target()]).is_ok());
        assert!(manufacturing_owner_write_router(vec![target(), target()]).is_err());
    }

    fn post(origin: &str, key: &str, body: &str) -> Request {
        Request::builder()
            .method("POST")
            .header("host", "acme.example.test")
            .header("origin", origin)
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn router_enforces_origin_key_and_routes_to_backend() {
        use erpx_server_runtime::public::http_support::StatusCode;
        let backend = Arc::new(Noop);
        let router = manufacturing_owner_write_router(vec![OwnerWriteTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }])
        .unwrap();
        let body = r#"{"kind":"order.transition","companyId":"c","recordId":"o","values":["planned",1,"o"]}"#;
        let response = write(
            State(Arc::new(AppState {
                targets: BTreeMap::from([(
                    ("acme.example.test".to_owned(), "acme".to_owned()),
                    OwnerWriteTarget {
                        host: "acme.example.test".into(),
                        holding_slug: "acme".into(),
                        origin: "https://acme.example.test".into(),
                        backend: Arc::new(Noop),
                    },
                )]),
            })),
            Path("acme".into()),
            post("https://evil.test", "owner-write-key-1", body),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let _ = router;
    }
}
