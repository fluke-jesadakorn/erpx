//! Lane 39 Brand evidence axum ingress. Transport facts only: exact
//! Host/Holding targets with one dedicated pool each, Company scope carried in
//! the bounded JSON body and the declared predicate families. The owner reader
//! executes the registered statement and projects BrandProfileRow values.

use crate::adapters::native_lane_39::evidence_application::{
    BrandEvidenceRead, read_brand_evidence,
};
use crate::adapters::native_lane_39::evidence_domain::EvidenceSearchRequest;
use crate::adapters::native_lane_39::evidence_postgres::PgBrandEvidenceRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const BODY_LIMIT: usize = 16 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<dyn BrandEvidenceRead>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CursorBody {
    branch_name: String,
    id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvidenceBody {
    company_id: String,
    branch_id: Option<String>,
    from: Option<i64>,
    to_exclusive: Option<i64>,
    query: Option<String>,
    cursor: Option<CursorBody>,
    limit: i64,
}

impl EvidenceBody {
    fn into_request(self) -> EvidenceSearchRequest {
        EvidenceSearchRequest {
            company_id: self.company_id,
            branch_id: self.branch_id,
            from: self.from,
            to_exclusive: self.to_exclusive,
            query: self.query,
            cursor: self.cursor.map(|cursor| {
                crate::adapters::native_lane_39::evidence_domain::EvidenceCursor {
                    branch_name: cursor.branch_name,
                    id: cursor.id,
                }
            }),
            limit: self.limit,
        }
    }
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "BRAND_EVIDENCE_SEARCH_REQUEST_INVALID",
        "Use one bounded Brand evidence search.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "BRAND_EVIDENCE_SEARCH_CONFIG_INVALID",
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
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

pub fn evidence_router(
    pool: DatabasePool,
    targets: Vec<EvidenceTarget>,
) -> Result<Router, BusinessError> {
    evidence_router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

pub fn evidence_router_for_holdings(
    targets: Vec<(EvidenceTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgBrandEvidenceRead::new(pool)) as Arc<dyn BrandEvidenceRead>,
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
            "/api/v1/holdings/{holding_slug}/branding/lane39-evidence-search",
            post(search),
        )
        .with_state(Arc::new(AppState { targets: configured })))
}

async fn search(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async move {
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?
            .to_owned();
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: EvidenceBody = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let mut request = body.into_request();
        // The verified Company header is authoritative over the body.
        request.company_id = company;
        let value = read_brand_evidence(reader.as_ref(), request).await?;
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
    fn target_slug_and_status_helpers_are_bounded() {
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
    }
}
