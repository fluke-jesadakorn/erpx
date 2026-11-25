//! Owner-local axum router for the People Payroll app-entrypoint reads. The
//! axum owner owns Company/Host isolation, the bounded request shape and error
//! mapping; PostgreSQL execution lives in `postgres`. No TypeScript fallback
//! SQL exists for this family: an unavailable native runtime returns 503.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::{AppEntrypointRead, read};
use super::domain::{bounded_id_error, envelope, invalid, valid_id, valid_slug};
use super::postgres::PgAppEntrypointRead;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const BODY_LIMIT: usize = 16 * 1024;

/// Exact registered Holding target the lead mounts. Host and slug must both
/// match; the Company is carried by an isolated context header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn AppEntrypointRead>>,
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

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

/// Build the native read router over an owner pool. The lead mounts it; no
/// composition file is edited here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn AppEntrypointRead> = Arc::new(PgAppEntrypointRead::new(pool));
    router_with_reader(reader, targets)
}

/// Same router over an injected owner reader; used by the module tests and by
/// the lead when a distinct owner reader is composed.
pub fn router_with_reader(
    reader: Arc<dyn AppEntrypointRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured: BTreeMap<(String, String), Arc<dyn AppEntrypointRead>> = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader.clone())
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PEOPLE_PAYROLL_LANE8_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "PEOPLE_PAYROLL_LANE8_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/app-entrypoint-read",
            post(entrypoint),
        )
        .with_state(Arc::new(LaneState { targets: configured })))
}

async fn entrypoint(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(bounded_id_error)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let kind = body
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(invalid)?;
        let values = body
            .get("values")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(invalid)?
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        envelope(read(reader.as_ref(), company, kind, &values).await?)
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
    use super::super::application::LaneReadFuture;
    use super::super::domain::{ApproverName, PayGroup, PersonOption, RosterShift};

    struct Noop;
    impl AppEntrypointRead for Noop {
        fn pay_groups<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<PayGroup>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn people<'a>(&'a self, _company: &'a str) -> LaneReadFuture<'a, Vec<PersonOption>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn approver_name<'a>(
            &'a self, _company: &'a str, _user: &'a str,
        ) -> LaneReadFuture<'a, Option<ApproverName>> {
            Box::pin(async { Ok(None) })
        }
        fn roster_shift<'a>(
            &'a self, _company: &'a str, _shift: &'a str, _engagement: &'a str,
        ) -> LaneReadFuture<'a, Option<RosterShift>> {
            Box::pin(async { Ok(None) })
        }
    }

    fn reader() -> Arc<dyn AppEntrypointRead> {
        Arc::new(Noop)
    }
    fn target() -> LaneTarget {
        LaneTarget { host: "acme.example.com".into(), holding_slug: "acme".into() }
    }

    #[test]
    fn router_rejects_empty_duplicate_and_malformed_targets() {
        assert!(router_with_reader(reader(), vec![]).is_err());
        assert!(router_with_reader(reader(), vec![target()]).is_ok());
        assert!(router_with_reader(
            reader(),
            vec![LaneTarget { host: "".into(), holding_slug: "acme".into() }],
        )
        .is_err());
        assert!(router_with_reader(
            reader(),
            vec![LaneTarget { host: "acme.example.com".into(), holding_slug: "Acme".into() }],
        )
        .is_err());
        assert!(router_with_reader(reader(), vec![target(), target()]).is_err());
    }

    #[test]
    fn header_helpers_fail_closed_on_duplicates_and_bad_values() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.com"));
        assert_eq!(unique_header(&headers, header::HOST).unwrap(), Some("acme.example.com"));
        headers.append(header::HOST, HeaderValue::from_static("evil.example.com"));
        assert_eq!(unique_header(&headers, header::HOST).unwrap_err().code, "PEOPLE_PAYROLL_LANE8_REQUEST_INVALID");
    }
}
