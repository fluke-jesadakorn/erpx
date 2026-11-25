//! Owner-local axum ingress for lane 6's calendar-availability leave-day read.
//!
//! Transport facts only: an exact registered Host/Holding target, a bounded
//! Company header and one bounded JSON read request. The Postgres reader owns
//! the SQL and the projection; this module maps failures to explicit statuses
//! and adds private, no-store headers. There is no fallback path.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::application::CalendarAvailabilityRead;
use super::domain::{LeaveDayQuery, valid_id, validate};
use super::postgres::PgCalendarAvailabilityRead;

const BODY_LIMIT: usize = 8 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub const READ_PATH: &str =
    "/api/v1/holdings/{holding_slug}/people-payroll/lane6/calendar-availability-leave";

/// Exact registered Holding target. No wildcard or caller-supplied host is
/// accepted; the lead mounts this router with the same targets as the owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn CalendarAvailabilityRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE6_REQUEST_INVALID",
        "Use one bounded calendar availability request.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE6_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
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

/// Build the native read router. The lead mounts it alongside the other
/// People/Payroll owner routers; no new TypeScript SQL or business rule is
/// introduced here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn CalendarAvailabilityRead> = Arc::new(PgCalendarAvailabilityRead::new(pool));
    router_with_reader(reader, targets)
}

/// Testable construction seam: composition and unit tests inject the reader;
/// both paths use the exact same transport checks.
pub fn router_with_reader(
    reader: Arc<dyn CalendarAvailabilityRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 512
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
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
        .route(READ_PATH, post(read))
        .with_state(Arc::new(LaneState {
            targets: configured,
        })))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let query: LeaveDayQuery = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        validate(&company, &query)?;
        let leave_days = reader.leave_days(&company, &query).await?;
        Ok::<_, BusinessError>(serde_json::json!({
            "leaveDays": leave_days
                .into_iter()
                .map(|day| serde_json::json!({"userId": day.user_id, "leaveDate": day.leave_date}))
                .collect::<Vec<_>>()
        }))
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
    use crate::adapters::native_lane_6::application::LeaveDayFuture;
    use crate::adapters::native_lane_6::domain::LeaveDay;

    struct FakeReader {
        rows: Vec<LeaveDay>,
        fail: bool,
    }

    impl CalendarAvailabilityRead for FakeReader {
        fn leave_days<'a>(
            &'a self,
            company: &'a str,
            query: &'a LeaveDayQuery,
        ) -> LeaveDayFuture<'a, Vec<LeaveDay>> {
            let rows = self.rows.clone();
            let company = company.to_owned();
            let query = query.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE",
                        "unavailable",
                    ));
                }
                // The fake reader records that it received a bounded scope.
                assert!(company.len() <= 200);
                assert!(!query.user_ids.is_empty());
                Ok(rows)
            })
        }
    }

    fn reader() -> Arc<dyn CalendarAvailabilityRead> {
        Arc::new(FakeReader {
            rows: vec![LeaveDay {
                user_id: "u1".into(),
                leave_date: "2026-03-02".into(),
            }],
            fail: false,
        })
    }

    #[test]
    fn rejects_duplicate_or_invalid_targets() {
        let target = LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        assert!(router_with_reader(reader(), vec![target.clone()]).is_ok());
        assert!(router_with_reader(reader(), vec![target.clone(), target]).is_err());
        assert!(router_with_reader(reader(), vec![]).is_err());
        assert!(
            router_with_reader(
                reader(),
                vec![LaneTarget {
                    host: "h".into(),
                    holding_slug: "BAD SLUG".into()
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn error_mapping_preserves_failure_kinds() {
        assert_eq!(
            status(FailureKind::InvalidInput),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(FailureKind::Unavailable),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(status(FailureKind::NotFound), StatusCode::NOT_FOUND);
    }

    #[test]
    fn response_envelope_is_json_and_never_cached() {
        let response = bounded(serde_json::json!({"leaveDays":[]}));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            response.headers().get(header::REFERRER_POLICY).unwrap(),
            "no-referrer"
        );
    }
}
