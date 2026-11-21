//! Native axum ingress for the Company Money rate-list and currency-scale reads.
//!
//! Company-keyed system reads (no browser WorkContext). Composition resolves
//! the Company scope and supplies the serializable owner unit through
//! `CompanyMoneyHttpBackend`. The routes carry no SQL and no caller-supplied
//! predicate; the authoritative SQL lives in the owner crate.

use crate::domain::company_money::ManualRate;
use crate::public::company_money::{company_currency_scale, company_fx_rates};
use erpx_server_runtime::public::http_support::{
    HeaderValue, IntoResponse, Json, Path, Query, Response, Router, State, StatusCode, get, header,
    json,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};

pub type CompanyMoneyRatesFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<ManualRate>, BusinessError>> + Send + 'a>>;
pub type CompanyMoneyScaleFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<i64>, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanyMoneyScaleQuery {
    pub currency: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanyMoneyRatesResponse {
    pub data: Vec<ManualRate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CompanyMoneyScaleResponse {
    pub data: Option<i64>,
}

/// Composition-owned Company scope and owner-unit adapter. The router never
/// mints a Company and never accepts caller SQL.
pub trait CompanyMoneyHttpBackend: Send + Sync {
    fn rates<'a>(&'a self, company_id: &'a str) -> CompanyMoneyRatesFuture<'a>;
    fn scale<'a>(
        &'a self,
        company_id: &'a str,
        currency: &'a str,
    ) -> CompanyMoneyScaleFuture<'a>;
}

fn to_business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "COMPANY_MONEY_READ_UNAVAILABLE",
            "The Company Money read is unavailable.",
        ),
    }
}

/// Convenience backends for composition: run the exported owner reads on the
/// supplied connection, which is composition's owned unit.
pub async fn read_company_fx_rates(
    c: &mut sqlx::PgConnection,
    company_id: &str,
) -> Result<Vec<ManualRate>, BusinessError> {
    company_fx_rates(c, company_id).await.map_err(to_business)
}

pub async fn read_company_currency_scale(
    c: &mut sqlx::PgConnection,
    company_id: &str,
    currency: &str,
) -> Result<Option<i64>, BusinessError> {
    company_currency_scale(c, company_id, currency)
        .await
        .map_err(to_business)
}

struct CompanyMoneyHttpState {
    backend: Arc<dyn CompanyMoneyHttpBackend>,
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMPANY_MONEY_READ_INVALID",
        "Use a bounded Company and ISO 4217 currency.",
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

fn bounded(body: impl Serialize) -> Response {
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

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '?' | '#' | '\\'))
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

async fn rates(
    State(state): State<Arc<CompanyMoneyHttpState>>,
    Path(company_id): Path<String>,
) -> Response {
    let result = async {
        if !valid_identifier(&company_id) {
            return Err(invalid());
        }
        let rows = state.backend.rates(&company_id).await?;
        Ok(CompanyMoneyRatesResponse { data: rows })
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn scale(
    State(state): State<Arc<CompanyMoneyHttpState>>,
    Path(company_id): Path<String>,
    Query(query): Query<CompanyMoneyScaleQuery>,
) -> Response {
    let result = async {
        if !valid_identifier(&company_id) || !valid_currency(&query.currency) {
            return Err(invalid());
        }
        let scale = state.backend.scale(&company_id, &query.currency).await?;
        Ok(CompanyMoneyScaleResponse { data: scale })
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

/// Owner-local router; composition mounts it on the private native API.
pub fn company_money_read_router(backend: Arc<dyn CompanyMoneyHttpBackend>) -> Router {
    Router::new()
        .route(
            "/api/v1/companies/{company_id}/finance/company-money/rates",
            get(rates),
        )
        .route(
            "/api/v1/companies/{company_id}/finance/company-money/scale",
            get(scale),
        )
        .with_state(Arc::new(CompanyMoneyHttpState { backend }))
}
