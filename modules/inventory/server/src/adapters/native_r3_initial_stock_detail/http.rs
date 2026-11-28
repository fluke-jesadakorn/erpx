//! Owner-local Axum ingress for the R3 POS Initial Physical Stock detail read.
//!
//! One reader per exact registered Holding `(host, slug)` pair; one Company
//! header, one optional branch header and one actor header. Every SQL predicate
//! is owned by the Postgres adapter; this transport adds none.

use super::application::{read_detail, InitialStockDetailRead};
use super::domain::{valid_id, valid_slug, DetailQuery, READ_PATH};
use super::postgres::PgInitialStockDetailRead;
use erpx_platform_runtime_server::public::http_support::{
    header, routing::get, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request,
    Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailTarget {
    pub host: String,
    pub holding_slug: String,
}

struct DetailState {
    readers: BTreeMap<(String, String), Arc<dyn InitialStockDetailRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use one bounded native POS Initial Stock read.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_INITIAL_STOCK_DETAIL_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("POS_INITIAL_STOCK_DETAIL_REQUEST_INVALID"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("POS_INITIAL_STOCK_DETAIL_REQUEST_INVALID"))
        })
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

fn state(targets: Vec<DetailTarget>, reader: Arc<dyn InitialStockDetailRead>) -> Result<DetailState, BusinessError> {
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
    Ok(DetailState { readers })
}

/// Mount entry point with one shared pool-backed owner reader.
pub fn router(pool: DatabasePool, targets: Vec<DetailTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn InitialStockDetailRead> = Arc::new(PgInitialStockDetailRead::new(pool));
    Ok(Router::new()
        .route(READ_PATH, get(read))
        .with_state(Arc::new(state(targets, reader)?)))
}

/// Per-Holding reader injection: each `(target, pool)` pair owns its own pool.
pub fn router_for_holdings(
    targets: Vec<(DetailTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, pool) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    Arc::new(PgInitialStockDetailRead::new(pool)) as Arc<dyn InitialStockDetailRead>,
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
        .route(READ_PATH, get(read))
        .with_state(Arc::new(DetailState { readers })))
}

async fn read(
    State(state): State<Arc<DetailState>>,
    Path((slug, count_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) || !valid_id(&count_id) {
            return Err(invalid("POS_INITIAL_STOCK_DETAIL_REQUEST_INVALID"));
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .readers
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("POS_INITIAL_STOCK_DETAIL_COMPANY_INVALID"))?
            .to_owned();
        let branch = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| !value.is_empty())
            .map(|value| {
                if valid_id(value) {
                    Ok(value.to_owned())
                } else {
                    Err(invalid("POS_INITIAL_STOCK_DETAIL_BRANCH_INVALID"))
                }
            })
            .transpose()?;
        let actor_id = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("POS_INITIAL_STOCK_DETAIL_ACTOR_INVALID"))?
            .to_owned();
        let query = DetailQuery {
            company_id: company,
            branch_id: branch,
            count_id,
            actor_id,
        };
        read_detail(reader.as_ref(), &query)
            .await?
            .ok_or_else(missing)
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_context_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
        assert_eq!(
            unique_header(&headers, CONTEXT_BRANCH).unwrap(),
            None
        );
    }
}
