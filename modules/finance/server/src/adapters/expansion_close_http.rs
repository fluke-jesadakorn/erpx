//! Native axum ingress for the Finance expansion-close pending source-task read.
//!
//! This is a company-keyed system read (no browser WorkContext): composition
//! resolves the Company scope and supplies the serializable owner unit through
//! `ExpansionCloseHttpBackend`. The route carries no SQL and no caller-supplied
//! predicate; the authoritative `EXPANSION_CLOSE_SQL` lives in the owner crate.

use crate::public::period_close::{ExpansionClosePendingRow, expansion_close_pending};
use erpx_platform_runtime_server::public::http_support::{
    HeaderValue, IntoResponse, Json, Path, Query, Response, Router, State, StatusCode, get, header,
    json,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};

/// Bounded read future; composition owns the exchange/connection lifetime.
pub type ExpansionClosePendingFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ExpansionClosePendingRow>, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpansionClosePendingQuery {
    pub start: String,
    pub end: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpansionClosePendingResponse {
    pub data: Vec<ExpansionClosePendingRow>,
}

/// Composition-owned Company scope and owner-unit adapter. The router never
/// mints a Company and never accepts caller SQL.
pub trait ExpansionCloseHttpBackend: Send + Sync {
    fn pending<'a>(
        &'a self,
        company_id: &'a str,
        start: &'a str,
        end: &'a str,
    ) -> ExpansionClosePendingFuture<'a>;
}

/// Convenience backend for composition: run the exported owner read on the
/// supplied connection. The connection is composition's owned unit.
pub async fn read_expansion_close_pending(
    c: &mut sqlx::PgConnection,
    company_id: &str,
    start: &str,
    end: &str,
) -> Result<Vec<ExpansionClosePendingRow>, BusinessError> {
    expansion_close_pending(c, company_id, start, end)
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_EXPANSION_CLOSE_UNAVAILABLE",
                "The expansion-close read is unavailable.",
            ),
        })
}

struct ExpansionCloseHttpState {
    backend: Arc<dyn ExpansionCloseHttpBackend>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "EXPANSION_CLOSE_PENDING_INVALID",
        "Use a bounded Company and close period boundary.",
    )
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
    let code = error.code;
    let message = error.message;
    (
        status(error.kind),
        Json(json!({"error":{"code":code,"message":message}})),
    )
        .into_response()
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '?' | '#' | '\\'))
}

fn valid_boundary(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
}

async fn pending(
    State(state): State<Arc<ExpansionCloseHttpState>>,
    Path(company_id): Path<String>,
    Query(query): Query<ExpansionClosePendingQuery>,
) -> Response {
    let result = async {
        if !valid_identifier(&company_id)
            || !valid_boundary(&query.start)
            || !valid_boundary(&query.end)
            || query.start > query.end
        {
            return Err(invalid());
        }
        let rows = state
            .backend
            .pending(&company_id, &query.start, &query.end)
            .await?;
        Ok(ExpansionClosePendingResponse { data: rows })
    }
    .await;
    match result {
        Ok(body) => {
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
        Err(error) => error_response(error),
    }
}

/// Owner-local router; composition mounts it on the private native API.
pub fn expansion_close_router(backend: Arc<dyn ExpansionCloseHttpBackend>) -> Router {
    Router::new()
        .route(
            "/api/v1/companies/{company_id}/finance/expansion-close/pending",
            get(pending),
        )
        .with_state(Arc::new(ExpansionCloseHttpState { backend }))
}
