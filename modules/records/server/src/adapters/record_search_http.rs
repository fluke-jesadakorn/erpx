//! Owner-local axum ingress for the Records reference-search read family.
//!
//! Transport facts only: exact registered Host/Holding targets, a strict `q`
//! query, the signed scope headers injected by composition, and owner error
//! mapping. The handler and the owner SQL adapter below execute the read. No
//! TypeScript prepares or executes SQL on this path.

use crate::application::record_search::SearchFuture;
use crate::domain::record_search::{
    SearchQuery, SearchResponse, SearchTarget, configuration_invalid, request_invalid, valid_slug,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const QUERY_LIMIT: usize = 4_096;

/// Composition resolves the Holding cookie and current Work selection before reading.
pub trait RecordSearchBackend: Send + Sync {
    fn search<'a>(
        &'a self,
        host: &'a str,
        headers: &'a HeaderMap,
        query: &'a SearchQuery,
    ) -> SearchFuture<'a, SearchResponse>;
}

struct SearchHttpState {
    targets: BTreeMap<(String, String), Arc<dyn RecordSearchBackend>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
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

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
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

fn valid_target(target: &SearchTarget) -> bool {
    !target.host.is_empty() && valid_slug(&target.holding_slug)
}

/// Pure target check, exercised without a live pool.
pub fn validate_targets(targets: &[SearchTarget]) -> Result<(), BusinessError> {
    let mut seen = BTreeMap::new();
    for target in targets {
        if !valid_target(target)
            || seen
                .insert((&target.host, &target.holding_slug), ())
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(())
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes
                    .get(index + 1)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(request_invalid)?;
                let low = bytes
                    .get(index + 2)
                    .and_then(|value| (*value as char).to_digit(16))
                    .ok_or_else(request_invalid)?;
                out.push((high * 16 + low) as u8);
                index += 3;
            }
            value => {
                out.push(value);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| request_invalid())
}

/// Accept only `?q=` with one value and no other query keys.
fn query_term(query: Option<&str>) -> Result<String, BusinessError> {
    let raw = query.unwrap_or_default();
    if raw.len() > QUERY_LIMIT {
        return Err(request_invalid());
    }
    let mut term = None;
    for pair in raw.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').ok_or_else(request_invalid)?;
        if percent_decode(key)? != "q" || term.is_some() {
            return Err(request_invalid());
        }
        term = Some(percent_decode(value)?);
    }
    Ok(term.unwrap_or_default())
}

/// Bind a native session-backed search to each exact registered Holding.
pub fn router_for_holdings(
    targets: Vec<(SearchTarget, Arc<dyn RecordSearchBackend>)>,
) -> Result<Router, BusinessError> {
    validate_targets(
        &targets
            .iter()
            .map(|(target, _)| target.clone())
            .collect::<Vec<_>>(),
    )?;
    let targets = targets
        .into_iter()
        .map(|(target, backend)| ((target.host, target.holding_slug), backend))
        .collect();
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/records/native-search",
            get(read_search),
        )
        .with_state(Arc::new(SearchHttpState { targets })))
}

async fn ensure_empty_body(request: Request) -> Result<(), BusinessError> {
    let bytes = to_bytes(request.into_body(), 0)
        .await
        .map_err(|_| request_invalid())?;
    if bytes.is_empty() {
        Ok(())
    } else {
        Err(request_invalid())
    }
}

async fn read_search(
    State(state): State<Arc<SearchHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let backend = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        let term = query_term(query.as_deref())?;
        ensure_empty_body(request).await?;
        if term.is_empty() {
            return Ok(serde_json::json!({"data": []}));
        }
        let query = SearchQuery::new(&term)?;
        Ok::<_, BusinessError>(backend.search(&host, &headers, &query).await?.value())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_configuration_requires_exact_distinct_hosts() {
        let good = SearchTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        };
        assert!(validate_targets(&[good.clone()]).is_ok());
        let bad = SearchTarget {
            host: good.host.clone(),
            holding_slug: "BAD SLUG".into(),
        };
        assert_eq!(
            validate_targets(&[bad]).unwrap_err().code,
            "NATIVE_RECORDS_SEARCH_CONFIG_INVALID"
        );
        assert!(validate_targets(&[good.clone(), good]).is_err());
        assert!(validate_targets(&[]).is_err());
    }

    #[test]
    fn query_accepts_only_one_bounded_q() {
        assert_eq!(query_term(Some("q=needle")).unwrap(), "needle");
        assert_eq!(
            query_term(Some("q=needle+review")).unwrap(),
            "needle review"
        );
        assert!(query_term(Some("q=a&q=b")).is_err());
        assert!(query_term(Some("other=a")).is_err());
        assert!(query_term(Some("q")).is_err());
    }
}
