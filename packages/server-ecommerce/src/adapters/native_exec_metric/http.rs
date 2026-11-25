//! Native Ecommerce executive metric owner-local axum ingress.
//!
//! Transport facts only: exact Host/Holding targets with one dedicated pool
//! each, an authorised Company scope and optional Branch/Department selectors.
//! The wire body carries only the metric id, breakdown and two epoch bounds;
//! the owner reader owns the SQL, the scope and the projection. No statement or
//! relation name is accepted from the wire and every failure maps to a named
//! error so the TypeScript client fails closed.

use crate::adapters::native_exec_metric::application::{ExecMetricRead, read_exec_metric};
use crate::adapters::native_exec_metric::domain::{
    MAX_ROWS, Breakdown, Metric, Scope, validate_range, validate_scope,
};
use crate::adapters::native_exec_metric::postgres::PgExecMetricRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const WORKSPACE_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const WORKSPACE_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department");
const BODY_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecMetricTarget {
    pub host: String,
    pub holding_slug: String,
}

pub type MetricTarget = ExecMetricTarget;

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn ExecMetricRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID",
        "Use one bounded Ecommerce executive metric read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ECOMMERCE_EXEC_METRIC_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}

/// Build the router with one shared pool.
pub fn router(pool: DatabasePool, targets: Vec<ExecMetricTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

/// Build the router with one dedicated reader/pool per Holding, keyed by
/// `(host, holding_slug)`. Duplicate or malformed targets fail closed.
pub fn router_for_holdings(
    targets: Vec<(MetricTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgExecMetricRead::new(pool)) as Arc<dyn ExecMetricRead>,
                )
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
            "/api/v1/holdings/{holding_slug}/ecommerce/native-exec-metric/projection",
            post(read),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

fn bounded_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

fn wire(value: &serde_json::Value) -> Result<(Metric, Breakdown, i64, i64), BusinessError> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != 4 || object.keys().any(|key| !["metric", "breakdown", "from", "to"].contains(&key.as_str())) {
        return Err(invalid());
    }
    let metric = Metric::parse(object.get("metric").and_then(|v| v.as_str()).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    let breakdown = Breakdown::parse(
        object.get("breakdown").and_then(|v| v.as_str()).ok_or_else(invalid)?,
    );
    let from = object.get("from").and_then(|v| v.as_i64()).ok_or_else(invalid)?;
    let to = object.get("to").and_then(|v| v.as_i64()).ok_or_else(invalid)?;
    validate_range(from, to)?;
    Ok((metric, breakdown, from, to))
}

async fn read(State(state): State<Arc<AppState>>, request: Request) -> Response {
    // Own the transport facts before awaiting: a borrow of `Request<Body>`
    // across an await would make the handler future !Send.
    let (parts, body) = request.into_parts();
    let headers = parts.headers.clone();
    let slug = parts
        .uri
        .path()
        .split('/')
        .nth(4)
        .filter(|value| valid_slug(value))
        .map(str::to_owned)
        .ok_or_else(invalid);
    let result = async move {
        let slug = slug?;
        let bytes = to_bytes(body, BODY_LIMIT + 1).await.map_err(|_| invalid())?;
        if bytes.len() > BODY_LIMIT {
            return Err(invalid());
        }
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let (metric, breakdown, from, to) = wire(&body)?;
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| bounded_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(&headers, WORKSPACE_BRANCH)?
            .filter(|value| bounded_id(value));
        let department = unique_header(&headers, WORKSPACE_DEPARTMENT)?
            .filter(|value| bounded_id(value));
        let scope = Scope {
            company_id: company,
            branch_id: branch,
            department_id: department,
        };
        validate_scope(&scope)?;
        let value = read_exec_metric(reader.as_ref(), metric, breakdown, scope, from, to).await?;
        let rows = value.get("raw").and_then(|v| v.as_array()).map(Vec::len).unwrap_or(0);
        if rows > MAX_ROWS {
            return Err(invalid());
        }
        Ok::<_, BusinessError>(value)
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
    fn wire_body_accepts_only_the_four_owner_metrics() {
        for id in [
            "open-ecommerce-carts",
            "completed-ecommerce-checkouts",
            "ecommerce-return-links",
            "ecommerce-fulfillment-links",
        ] {
            let body = serde_json::json!({"metric": id, "breakdown": "status", "from": 1_767_225_600_000_i64, "to": 1_769_904_000_000_i64});
            assert!(wire(&body).is_ok());
        }
        let bad = serde_json::json!({"metric": "receivables", "breakdown": "status", "from": 1, "to": 2});
        assert_eq!(wire(&bad).unwrap_err().code, "ECOMMERCE_EXEC_METRIC_REQUEST_INVALID");
    }

    #[test]
    fn validation_helpers_reject_bad_targets() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert!(bounded_id("co-1"));
        assert!(!bounded_id("bad id"));
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
