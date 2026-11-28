//! Owner-local axum router for the lane 17 owner read family.
//!
//! Target, header and Company isolation follow the owner-read reference: a
//! request is served only on an exact registered Holding host/slug pair, with
//! one Company context header and one bounded body. No TypeScript SQL, no
//! fallback reader and no caller-supplied predicate exists behind this route.

use crate::adapters::native_lane_17::application::InventoryLane17Read;
use crate::adapters::native_lane_17::domain::{
    LaneReadKind, LaneReadRequest, READ_PATH, valid_id, valid_slug,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::adapters::native_lane_17::postgres::PgInventoryLane17Read as PgReader;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 16 * 1024;

/// Exact registered Holding target injected by composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    readers: BTreeMap<(String, String), Arc<dyn InventoryLane17Read>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(
        code,
        "Use one bounded native Inventory owner read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVENTORY_LANE17_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("INVENTORY_LANE17_REQUEST_INVALID"));
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid("INVENTORY_LANE17_REQUEST_INVALID")))
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
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

/// Build the owner-local router. Composition mounts it; this module never edits
/// composition and never accepts a reader from the request.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn InventoryLane17Read> = Arc::new(PgReader::new(pool));
    let mut readers = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(READ_PATH, post(read))
        .with_state(Arc::new(LaneState { readers })))
}

/// Build the router with one independent owner reader per Holding target.
///
/// Each `(LaneTarget, DatabasePool)` pair gets its own Pg reader keyed by the
/// exact `(host, holding_slug)` tuple, so a Holding can only reach its own
/// pool. Composition mounts this form when Holding pools are distinct; the
/// single-pool `router` above stays available for the shared dev pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgReader::new(pool)) as Arc<dyn InventoryLane17Read>,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(READ_PATH, post(read))
        .with_state(Arc::new(LaneState { readers })))
}

fn parse_body(bytes: &[u8]) -> Result<LaneReadRequest, BusinessError> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| invalid("INVENTORY_LANE17_REQUEST_INVALID"))?;
    let object = body.as_object().ok_or_else(|| invalid("INVENTORY_LANE17_REQUEST_INVALID"))?;
    if object.len() != 2 {
        return Err(invalid("INVENTORY_LANE17_REQUEST_INVALID"));
    }
    let kind = object
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .and_then(LaneReadKind::parse)
        .ok_or_else(|| invalid("INVENTORY_LANE17_KIND_INVALID"))?;
    let values = object
        .get("values")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .ok_or_else(|| invalid("INVENTORY_LANE17_REQUEST_INVALID"))?;
    Ok(LaneReadRequest {
        company: String::new(),
        kind,
        values,
    })
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("INVENTORY_LANE17_COMPANY_INVALID"))?
            .to_owned();
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid("INVENTORY_LANE17_REQUEST_INVALID"))?;
        let mut request = parse_body(&bytes)?;
        request.company = company;
        request.validate()?;
        let rows = reader.read(&request).await?;
        Ok::<_, BusinessError>(serde_json::json!({"rows": rows}))
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
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    #[test]
    fn body_parse_admits_only_the_closed_kind_and_scalar_values() {
        let request = parse_body(br#"{"kind":"landed_cost_scoped","values":["cost-1","company-1",null,null]}"#).unwrap();
        assert_eq!(request.kind, LaneReadKind::LandedCostScoped);
        assert_eq!(request.values.len(), 4);
        assert!(parse_body(br#"{"kind":"other","values":[]}"#).is_err());
        assert!(parse_body(br#"{"kind":"landed_cost_scoped"}"#).is_err());
        assert!(parse_body(br#"{"kind":"landed_cost_scoped","values":[],"extra":1}"#).is_err());
        assert!(parse_body(br#"{"kind":"landed_cost_scoped","values":[{"x":1},"company-1",null,null]}"#).is_ok());
        // A non-scalar is rejected by request validation, not by parsing.
        let bad = LaneReadRequest {
            company: "company-1".into(),
            kind: LaneReadKind::LandedCostScoped,
            values: vec![json!({"x": 1}), json!("company-1"), json!(null), json!(null)],
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn per_holding_router_requires_registered_targets() {
        assert!(router_for_holdings(vec![]).is_err());
    }

    #[test]
    fn error_status_mapping_is_closed() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
