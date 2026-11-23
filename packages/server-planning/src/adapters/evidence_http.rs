//! Native private HTTP ingress for the Planning evidence SEARCH port.
//!
//! Transport facts only: an exact registered Host/Holding target, exact Origin,
//! no query ambiguity, a bounded closed request body and a validated owner
//! response. Account session, current IAM, package history, the eligible
//! scenario set and the owner transaction remain composition responsibilities
//! behind [`EvidenceHttpBackend`]. No TypeScript fallback or caller SQL exists.
use crate::domain::evidence::{EvidenceQuery, EvidenceQueryInput};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const MAX_ROWS: usize = 51;
const ORIGIN_HEADER: HeaderName = header::ORIGIN;

pub type EvidenceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Validated transport facts plus the already-parsed bounded query. The
/// backend resolves the current actor and purchase admission; the router never
/// mints a session, Company, permission or owner row.
pub struct EvidenceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub query: &'a EvidenceQuery,
}

#[derive(Debug)]
pub struct EvidenceHttpResponse {
    pub data: serde_json::Value,
}

impl EvidenceHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data})
    }
}

pub trait EvidenceHttpBackend: Send + Sync {
    fn search<'a>(
        &'a self,
        request: EvidenceHttpRequest<'a>,
    ) -> EvidenceHttpFuture<'a, EvidenceHttpResponse>;
}

pub struct EvidenceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn EvidenceHttpBackend>,
}

pub struct EvidenceHttpState {
    targets: BTreeMap<(String, String), EvidenceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "EVIDENCE_REQUEST_INVALID",
        "Use one unambiguous Planning evidence request.",
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
        "PLANNING_EVIDENCE_CONFIG_INVALID",
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

/// Exact owner row shape required before any response leaves the port.
pub fn valid_rows(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let Some(rows) = object.get("rows").and_then(serde_json::Value::as_array) else {
        return false;
    };
    if rows.len() > MAX_ROWS {
        return false;
    }
    const TEXT: [&str; 11] = [
        "id", "scenario_id", "branch_id", "warehouse_id", "specification_id",
        "demand_quantity_milli", "due_date", "capacity_minutes", "scenario_status",
        "shortage_count", "shortage_items_json",
    ];
    rows.iter().all(|row| {
        let Some(row) = row.as_object() else {
            return false;
        };
        TEXT.iter().all(|key| row.get(*key).is_some_and(serde_json::Value::is_string))
            && row.get("specification_version").is_some_and(serde_json::Value::is_i64)
            && row.get("scenario_version").is_some_and(serde_json::Value::is_i64)
            && row.get("scenario_created_at").is_some_and(serde_json::Value::is_i64)
            && row.get("scenario_updated_at").is_some_and(serde_json::Value::is_i64)
    })
}

impl EvidenceHttpState {
    pub fn new(targets: Vec<EvidenceHttpTarget>) -> Result<Self, BusinessError> {
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
        Ok(Self { targets: configured })
    }

    /// Exact POST handler used by both the Router and owner tests.
    pub async fn search(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Response {
        match self.search_inner(headers, holding_slug, query, body).await {
            Ok(value) => bounded(serde_json::json!({"data": value})),
            Err(error) => error_response(error),
        }
    }

    async fn search_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<serde_json::Value, BusinessError> {
        if query.is_some() || body.len() > BODY_LIMIT {
            return Err(malformed());
        }
        let host = unique_header(headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let origin = unique_header(headers, ORIGIN_HEADER)?.ok_or_else(forbidden)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .or_else(|| {
                origin
                    .strip_prefix("https://")
                    .and_then(|rest| match rest.rsplit_once(':') {
                        Some((value, _)) => Some(value),
                        None => Some(rest),
                    })
                    .and_then(|value| self.targets.get(&(value.to_owned(), holding_slug.to_owned())))
            })
            .ok_or_else(missing)?;
        if origin != target.origin {
            return Err(forbidden());
        }
        let input: EvidenceQueryInput =
            serde_json::from_slice(body).map_err(|_| malformed())?;
        let parsed = input.into_query()?;
        let response = target
            .backend
            .search(EvidenceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers,
                query: &parsed,
            })
            .await?;
        if !valid_rows(&response.data) {
            return Err(BusinessError::invalid(
                "EVIDENCE_VALUE_INVALID",
                "The native Planning evidence returned an invalid owner row.",
            ));
        }
        Ok(response.data)
    }
}

pub fn planning_evidence_router(targets: Vec<EvidenceHttpTarget>) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/planning/evidence/search",
            post(search_evidence),
        )
        .with_state(Arc::new(EvidenceHttpState::new(targets)?)))
}

async fn search_evidence(
    State(state): State<Arc<EvidenceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(ToOwned::to_owned);
    let result = async move {
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        state
            .search_inner(&headers, &slug, query.as_deref(), &bytes)
            .await
    }
    .await;
    match result {
        Ok(value) => bounded(serde_json::json!({"data": value})),
        Err(error) => error_response(error),
    }
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_guards() {
        let target = || EvidenceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Noop),
        };
        struct Noop;
        impl EvidenceHttpBackend for Noop {
            fn search<'a>(
                &'a self,
                _: EvidenceHttpRequest<'a>,
            ) -> EvidenceHttpFuture<'a, EvidenceHttpResponse> {
                Box::pin(async { Err(malformed()) })
            }
        }
        assert!(EvidenceHttpState::new(vec![target()]).is_ok());
        assert!(EvidenceHttpState::new(vec![target(), target()]).is_err());
        assert!(EvidenceHttpState::new(vec![EvidenceHttpTarget {
            origin: "http://acme.example.test".into(),
            ..target()
        }])
        .is_err());
    }

    #[test]
    fn row_shape_is_strict() {
        let good = serde_json::json!({
            "rows": [{
                "id": "r", "scenario_id": "s", "recommendation_id": null,
                "branch_id": "b", "branch_name": null, "warehouse_id": "w",
                "warehouse_name": null, "specification_id": "sp",
                "specification_name": null, "output_product_id": null,
                "output_product_name": null, "specification_version": 2,
                "demand_quantity_milli": "2000", "due_date": "2026-10-04",
                "capacity_minutes": "120", "scenario_status": "calculated",
                "scenario_version": 1, "scenario_created_at": 100, "scenario_updated_at": 300,
                "recommendation_status": null, "recommended_quantity_milli": null,
                "required_minutes": null, "feasible_date": null,
                "recommendation_version": null, "scenario_version_ref": null,
                "recommendation_created_at": null, "recommendation_updated_at": null,
                "shortage_count": "1", "shortage_items_json": "[]"
            }]
        });
        assert!(valid_rows(&good));
        let bad = serde_json::json!({"rows":[{"id":"r"}]});
        assert!(!valid_rows(&bad));
        assert!(!valid_rows(&serde_json::json!({"rows":"not-an-array"})));
    }
}
