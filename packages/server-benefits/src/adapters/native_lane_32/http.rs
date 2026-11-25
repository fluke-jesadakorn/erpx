//! Owner-local axum router for the lane-32 Benefits payroll-input read.
//!
//! The router transports bounded identities only. It enforces registered
//! target/Host/Origin isolation, requires a well-formed Company scope and maps
//! owner errors to the shared BusinessError response shape. No SQL is present
//! in this module; the owner reader executes the statement.

use super::application::ReadPayrollInputs;
use super::domain::{
    LaneTarget, PayrollInputsRequest, invalid, unavailable, valid_id,
};
use super::postgres::PgPayrollInputRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_ENGAGEMENT: HeaderName = HeaderName::from_static("x-erpx-benefits-engagement");
const CONTEXT_CURRENCY: HeaderName = HeaderName::from_static("x-erpx-benefits-currency");
const CONTEXT_PERIOD_END: HeaderName = HeaderName::from_static("x-erpx-benefits-period-end");
const CONTEXT_PERIOD_START: HeaderName = HeaderName::from_static("x-erpx-benefits-period-start");

type Handler = ReadPayrollInputs<PgPayrollInputRead>;

struct ReadState {
    targets: BTreeMap<(String, String), LaneTarget>,
    handler: Arc<Handler>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host.bytes().any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
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

pub(crate) fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

pub(crate) fn error_response(error: BusinessError) -> Response {
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

/// Build the owner read router. The lead mounts it; no other module is edited.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
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
            "/api/v1/holdings/{holding_slug}/benefits/payroll-inputs",
            get(read),
        )
        .with_state(Arc::new(ReadState {
            targets: configured,
            handler: Arc::new(ReadPayrollInputs::new(PgPayrollInputRead::new(pool))),
        })))
}

fn required_header(
    headers: &HeaderMap,
    name: HeaderName,
) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| valid_id(value))
        .map(str::to_owned)
        .ok_or_else(invalid)
}

async fn read(
    State(state): State<Arc<ReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if let Some(origin) = unique_header(&headers, header::ORIGIN)? {
            if origin != target.origin {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "ORIGIN_FORBIDDEN",
                    "Use the registered application origin.",
                ));
            }
        }
        let company = required_header(&headers, CONTEXT_COMPANY)?;
        let engagement_id = required_header(&headers, CONTEXT_ENGAGEMENT)?;
        let currency = required_header(&headers, CONTEXT_CURRENCY)?;
        let period_end = required_header(&headers, CONTEXT_PERIOD_END)?;
        let period_start = required_header(&headers, CONTEXT_PERIOD_START)?;
        let body = state
            .handler
            .execute(PayrollInputsRequest {
                company_id: company,
                engagement_id,
                currency,
                period_end,
                period_start,
            })
            .await?;
        Ok::<_, BusinessError>(serde_json::to_value(body).map_err(|_| unavailable())?)
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
    fn rejects_duplicate_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }

    #[test]
    fn rejects_unregistered_origin_and_slug() {
        assert!(!valid_origin("http://acme.example.com"));
        assert!(!valid_origin("https://acme.example.com/evil"));
        assert!(valid_origin("https://acme.example.com"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug(""));
        assert!(valid_slug("acme-1"));
    }

    #[test]
    fn maps_failure_kinds_to_the_shared_status_contract() {
        assert_eq!(status(FailureKind::InvalidInput), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(status(FailureKind::Unavailable), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(status(FailureKind::Forbidden), StatusCode::FORBIDDEN);
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn error_response_pins_the_error_shape() {
        let response = error_response(unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
