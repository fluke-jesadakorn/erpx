//! Owner-local axum ingress for the native assistant-view read. Transport
//! facts only: exact Host/Holding target, one bounded POST body and the exact
//! registered Origin. Authority resolution, the read-only unit and SQL stay
//! behind `AssistantViewHttpBackend` in the caller's composition.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const ORIGIN_HEADER: HeaderName = HeaderName::from_static("origin");

pub type AssistantViewHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistantViewHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub holding_id: &'a str,
    pub headers: &'a HeaderMap,
    pub path: &'a str,
    pub mode: &'a str,
}

pub trait AssistantViewHttpBackend: Send + Sync {
    fn view<'a>(
        &'a self,
        request: AssistantViewHttpRequest<'a>,
    ) -> AssistantViewHttpFuture<'a>;
}

pub struct AssistantViewHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub holding_id: String,
    pub origin: String,
    pub backend: Arc<dyn AssistantViewHttpBackend>,
}

struct StateData {
    targets: BTreeMap<(String, String), AssistantViewHttpTarget>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_VIEW_REQUEST_INVALID",
        "Use one bounded assistant view request.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the registered Holding origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_VIEW_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}
fn origin_host(value: &str) -> Option<&str> {
    let host = value.strip_prefix("https://")?;
    let (host, port) = match host.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (host, None),
    };
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    {
        return None;
    }
    if port.is_some_and(|port| {
        !(1..=5).contains(&port.len())
            || !port.bytes().all(|byte| byte.is_ascii_digit())
            || port.parse::<u16>().is_err()
    }) {
        return None;
    }
    Some(host)
}
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
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
fn respond(result: Result<Value, BusinessError>) -> Response {
    match result {
        Ok(data) => {
            let mut response = (StatusCode::OK, Json(serde_json::json!({ "data": data })))
                .into_response();
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
        Err(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
}

/// The exact accepted body: a bounded path and the optional conversation mode.
fn parse_body(value: &Value) -> Result<(&str, &str), BusinessError> {
    let object = value.as_object().ok_or_else(invalid)?;
    let keys: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    if !keys.is_subset(&BTreeSet::from(["path", "mode"])) {
        return Err(invalid());
    }
    let path = object.get("path").and_then(Value::as_str).ok_or_else(invalid)?;
    if path.is_empty() || path.len() > 1500 {
        return Err(invalid());
    }
    let mode = match object.get("mode") {
        None => "page",
        Some(Value::String(mode)) if matches!(mode.as_str(), "page" | "workspace") => mode.as_str(),
        _ => return Err(invalid()),
    };
    Ok((path, mode))
}

async fn view(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return respond(Err(invalid())),
    };
    let result: Result<Value, BusinessError> = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let origin = unique_header(&headers, ORIGIN_HEADER.clone())?;
        if origin != Some(target.origin.as_str()) {
            return Err(origin_forbidden());
        }
        if unique_header(&headers, header::CONTENT_TYPE)?
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(invalid());
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
        let (path, mode) = parse_body(&value)?;
        target
            .backend
            .view(AssistantViewHttpRequest {
                host,
                holding_slug: &slug,
                holding_id: &target.holding_id,
                headers: &headers,
                path,
                mode,
            })
            .await
    }
    .await;
    respond(result)
}

/// Build the owner read router from exact, distinct Holding targets.
pub fn assistant_view_router_for_holdings(
    targets: Vec<AssistantViewHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    let mut holding_ids = BTreeSet::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 255
            || target.host != origin_host(&target.origin).unwrap_or("")
            || !valid_slug(&target.holding_slug)
            || target.holding_id.is_empty()
            || target.holding_id.len() > 200
            || !origin_host(&target.origin).is_some()
            || !holding_ids.insert(target.holding_id.clone())
            || map
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assistant/view",
            post(view),
        )
        .with_state(Arc::new(StateData { targets: map })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_exact_client_bodies() {
        assert_eq!(
            parse_body(&serde_json::json!({"path":"/work"})).unwrap(),
            ("/work", "page")
        );
        assert_eq!(
            parse_body(&serde_json::json!({"path":"/work","mode":"workspace"})).unwrap(),
            ("/work", "workspace")
        );
        assert!(parse_body(&serde_json::json!({"path":""})).is_err());
        assert!(parse_body(&serde_json::json!({"path":"/work","mode":"other"})).is_err());
        assert!(parse_body(&serde_json::json!({"path":"/work","extra":1})).is_err());
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        struct Noop;
        impl AssistantViewHttpBackend for Noop {
            fn view<'a>(&'a self, _: AssistantViewHttpRequest<'a>) -> AssistantViewHttpFuture<'a> {
                Box::pin(async { Ok(serde_json::json!({})) })
            }
        }
        let backend: Arc<dyn AssistantViewHttpBackend> = Arc::new(Noop);
        let target = || AssistantViewHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            holding_id: "holding-1".into(),
            origin: "https://acme.example.test".into(),
            backend: backend.clone(),
        };
        assert!(assistant_view_router_for_holdings(vec![target()]).is_ok());
        assert!(assistant_view_router_for_holdings(vec![target(), target()]).is_err());
        assert!(assistant_view_router_for_holdings(vec![AssistantViewHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err());
    }
}
