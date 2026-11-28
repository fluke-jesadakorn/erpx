//! Bounded GET transport for the native assistant page/workspace read. The
//! backend is the owner's SQLx-backed assistant-view port; this module only
//! validates query values and writes the exact `{data}` envelope.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get as get_route, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

use crate::adapters::assistant_view_http::{AssistantViewHttpRequest, AssistantViewHttpTarget};

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use a valid assistant page request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
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
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Err(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    }
}

/// Exact query values: `path` 1..1500 chars and an optional `page|workspace`
/// mode. Duplicate keys are rejected rather than silently taking the first.
pub fn parse_query(request: &Request) -> Result<(String, String), BusinessError> {
    let query = request.uri().query().ok_or_else(invalid)?;
    let mut path: Option<&str> = None;
    let mut mode: Option<&str> = None;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some("path"), Some(value)) if path.is_none() => path = Some(value),
            (Some("mode"), Some(value)) if mode.is_none() => mode = Some(value),
            _ => return Err(invalid()),
        }
    }
    let path = path
        .filter(|value| (1..=1500).contains(&value.len()))
        .ok_or_else(invalid)?
        .to_owned();
    let mode = match mode {
        None => "page",
        Some("page") => "page",
        Some("workspace") => "workspace",
        _ => return Err(invalid()),
    }
    .to_owned();
    Ok((path, mode))
}

struct StateData {
    targets: BTreeMap<(String, String), AssistantViewHttpTarget>,
}

async fn get(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let host = match unique_header(&headers, header::HOST) {
        Ok(Some(host)) if !host.is_empty() => host.to_owned(),
        _ => return respond(Err(missing())),
    };
    let target = match state.targets.get(&(host.clone(), slug.clone())) {
        Some(target) => target,
        None => return respond(Err(missing())),
    };
    let (path, mode) = match parse_query(&request) {
        Ok(values) => values,
        Err(error) => return respond(Err(error)),
    };
    let result = {
        let call = AssistantViewHttpRequest {
            host: &host,
            holding_slug: &slug,
            holding_id: &target.holding_id,
            headers: &headers,
            path: &path,
            mode: &mode,
        };
        target.backend.view(call).await
    };
    respond(result)
}

/// Build the exact GET router from the owner's already-validated view targets.
pub fn assistant_get_router(targets: Vec<AssistantViewHttpTarget>) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || target.holding_id.is_empty()
            || !slugs.insert(target.holding_slug.clone())
            || map
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(invalid());
        }
    }
    if map.is_empty() {
        return Err(invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assistant",
            get_route(get),
        )
        .with_state(Arc::new(StateData { targets: map })))
}

/// Convenience constructor mirroring `assistant_view_router_for_holdings`.
pub fn assistant_get_router_for_holdings(
    targets: Vec<AssistantViewHttpTarget>,
) -> Result<Router, BusinessError> {
    assistant_get_router(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::body::Body;

    fn request(query: &str) -> Request {
        Request::builder()
            .uri(format!("/api/v1/holdings/acme/assistant?{}", query))
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn query_accepts_exact_page_values() {
        assert_eq!(
            parse_query(&request("path=/work")).unwrap(),
            ("/work".to_owned(), "page".to_owned())
        );
        assert_eq!(
            parse_query(&request("path=/work&mode=workspace")).unwrap(),
            ("/work".to_owned(), "workspace".to_owned())
        );
    }

    #[test]
    fn query_rejects_missing_duplicate_or_unknown_values() {
        assert!(parse_query(&request("mode=page")).is_err());
        assert!(parse_query(&request("path=/work&path=/home")).is_err());
        assert!(parse_query(&request("path=/work&mode=other")).is_err());
        assert!(parse_query(&request("path=/work&extra=1")).is_err());
    }

    #[test]
    fn envelope_is_the_next_handler_shape() {
        let response = respond(Ok(serde_json::json!({"title":"Today"})));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "private, no-store");
        let denied = respond(Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )));
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    }
}
