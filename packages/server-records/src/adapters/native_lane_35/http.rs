//! Lane 35 owner-local axum ingress for the Records work-report read family.
//!
//! Transport facts only: one exact registered Host/Holding target, a Company
//! header that must equal the body scope, strict body validation and owner
//! error mapping. The handler and Postgres reader below own the query and the
//! projection. No fallback SQL exists on the TypeScript side.

use crate::adapters::native_lane_35::application::LaneQueryHandler;
use crate::adapters::native_lane_35::domain::{
    LaneQuery, LaneTarget, configuration_invalid, invalid, valid_id, valid_slug,
};
use crate::adapters::native_lane_35::postgres::PgWorkReportSource;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 64 * 1024;

type Handler = LaneQueryHandler<PgWorkReportSource>;

struct LaneHttpState {
    targets: BTreeMap<(String, String), Arc<Handler>>,
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

fn target<'a>(
    state: &'a LaneHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a Arc<Handler>, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

/// Pure target check, exercised without a live pool.
pub fn validate_targets(targets: &[LaneTarget]) -> Result<(), BusinessError> {
    let mut seen = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || seen.insert((&target.host, &target.holding_slug), ()).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if targets.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(())
}

/// Build the owner router. The lead mounts it; no composition or shared module
/// is edited from this lane. Targets must be exact and distinct.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    validate_targets(&targets)?;
    let handler = Arc::new(LaneQueryHandler::new(PgWorkReportSource::new(pool)));
    let mut configured = BTreeMap::new();
    for target in targets {
        configured.insert(
            (target.host.clone(), target.holding_slug.clone()),
            handler.clone(),
        );
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/records/native-lane-35/work-report",
            post(read_report),
        )
        .with_state(Arc::new(LaneHttpState {
            targets: configured,
        })))
}

async fn read_report(
    State(state): State<Arc<LaneHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let handler = target(&state, request.headers(), &slug)?;
        let company = unique_header(request.headers(), COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        match request.uri().query() {
            None | Some("") => Ok(()),
            Some(_) => Err(invalid()),
        }?;
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let query: LaneQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        // A caller cannot read a Company other than the one in the signed scope.
        if query.company_id() != company {
            return Err(invalid());
        }
        let body = handler.execute(query).await?.value();
        Ok::<_, BusinessError>(body)
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
        let good = LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(validate_targets(&[good.clone()]).is_ok());
        let bad = LaneTarget {
            host: good.host.clone(),
            holding_slug: "BAD SLUG".to_owned(),
        };
        assert_eq!(
            validate_targets(&[bad]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
        assert_eq!(
            validate_targets(&[good.clone(), good]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
        assert_eq!(
            validate_targets(&[]).unwrap_err().code,
            "NATIVE_RECORDS_WORK_REPORT_CONFIG_INVALID"
        );
    }

    #[test]
    fn query_envelope_is_tagged() {
        let candidates: LaneQuery = serde_json::from_str(
            r#"{"kind":"candidates","companyId":"acme"}"#,
        )
        .unwrap();
        assert_eq!(candidates.company_id(), "acme");
        let events: LaneQuery =
            serde_json::from_str(r#"{"kind":"events","companyId":"acme","ids":"[]"}"#).unwrap();
        assert_eq!(events.company_id(), "acme");
        assert!(serde_json::from_str::<LaneQuery>(r#"{"kind":"other"}"#).is_err());
    }
}
