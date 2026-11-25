//! Native private-file denial authority witness HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding targets, a bounded JSON
//! actor and a 64-hex opaque token. The owner's transaction and closed fact
//! image stay behind [`PrivateFileDenialHttpBackend`]; this router mints no
//! authority and accepts no SQL from a caller.
use crate::domain::private_file_denial::PrivateFileDenialActor;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::post, to_bytes,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

const BODY_LIMIT: usize = 8 * 1024;

pub type PrivateFileDenialHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PrivateFileDenialHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub actor: &'a PrivateFileDenialActor,
}

#[derive(Clone, Debug)]
pub struct PrivateFileDenialHttpResponse {
    /// The opaque equality token; no source row is ever returned.
    pub witness: String,
}

pub trait PrivateFileDenialHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PrivateFileDenialHttpRequest<'a>,
    ) -> PrivateFileDenialHttpFuture<'a, PrivateFileDenialHttpResponse>;
}

pub struct PrivateFileDenialHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact registered Holding origin, for example `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: std::sync::Arc<dyn PrivateFileDenialHttpBackend>,
}

struct StateInner {
    targets: BTreeMap<(String, String), PrivateFileDenialHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "The private-file denial witness request is invalid.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PRIVATE_FILE_DENIAL_HTTP_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: header::HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"));
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID")))
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

fn target<'a>(
    state: &'a StateInner,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PrivateFileDenialHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn valid_witness(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

async fn strict_actor(request: Request) -> Result<PrivateFileDenialActor, BusinessError> {
    let content_type = unique_header(request.headers(), header::CONTENT_TYPE)?
        .ok_or_else(|| malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"))?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"));
    }
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"));
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"))?;
    if bytes.is_empty() {
        return Err(malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID"))?;
    PrivateFileDenialActor::parse(&value)
}

pub fn private_file_denial_http_router(
    targets: Vec<PrivateFileDenialHttpTarget>,
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
    Ok(Router::new().route(
        "/api/v1/holdings/{holding_slug}/identity-access/private-file-denial-witness",
        post(witness),
    )
    .with_state(std::sync::Arc::new(StateInner { targets: configured })))
}

async fn witness(
    State(state): State<std::sync::Arc<StateInner>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let target = target(&state, &headers, &slug)?;
        let actor = strict_actor(request).await?;
        let reply = target
            .backend
            .execute(PrivateFileDenialHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                actor: &actor,
            })
            .await?;
        if !valid_witness(&reply.witness) {
            return Err(malformed("PRIVATE_FILE_DENIAL_WITNESS_INVALID"));
        }
        Ok(reply.witness)
    }
    .await;
    match result {
        Ok(witness) => bounded(serde_json::json!({"data":{"witness":witness}})),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct Reject;
    impl PrivateFileDenialHttpBackend for Reject {
        fn execute<'a>(
            &'a self,
            _: PrivateFileDenialHttpRequest<'a>,
        ) -> PrivateFileDenialHttpFuture<'a, PrivateFileDenialHttpResponse> {
            Box::pin(async { Err(malformed("PRIVATE_FILE_DENIAL_REQUEST_INVALID")) })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let backend = Arc::new(Reject);
        let target = || PrivateFileDenialHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(private_file_denial_http_router(vec![target()]).is_ok());
        assert!(private_file_denial_http_router(vec![target(), target()]).is_err());
        assert!(private_file_denial_http_router(vec![PrivateFileDenialHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err());
        assert!(private_file_denial_http_router(vec![PrivateFileDenialHttpTarget {
            holding_slug: "Acme".into(),
            ..target()
        }])
        .is_err());
    }

    #[test]
    fn witness_shape_is_a_lowercase_64_hex_token() {
        assert!(valid_witness(&"a".repeat(64)));
        assert!(!valid_witness(&"A".repeat(64)));
        assert!(!valid_witness(&"a".repeat(63)));
        assert!(!valid_witness(&"g".repeat(64)));
    }
}
