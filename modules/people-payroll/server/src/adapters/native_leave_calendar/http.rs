//! Owner-local axum ingress for the native leave-calendar month read. Host and
//! Holding targets, bounded context headers, one bounded `month` query value,
//! payload order and error mapping are owned here; the PostgreSQL execution
//! lives in `postgres`. No TypeScript SQL or business fallback exists for this
//! operation.

use super::application::{LeaveCalendarQuery, LeaveCalendarRead, leave_calendar_month};
use super::domain::{LeaveCalendarMonth, invalid, missing, valid_id, valid_month};
use super::postgres::{PgLeaveCalendarAuthority, PgLeaveCalendarRead};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const CONTEXT_ASSIGNMENT_VERSION: HeaderName =
    HeaderName::from_static("x-erpx-context-assignment-version");

/// Context supplied by the trusted session bridge, never by browser headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveCalendarAuthorityContext {
    pub host: String,
    pub holding_slug: String,
    pub company_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub revision: i64,
    pub assignment_version: i64,
}

/// Composition must check current IAM authority and the `people.workforce`
/// purchased history capability before permitting the private calendar read.
pub trait LeaveCalendarAuthority: Send + Sync {
    fn authorize<'a>(
        &'a self,
        context: &'a LeaveCalendarAuthorityContext,
    ) -> super::application::CalendarFuture<'a, ()>;
}

struct UnconfiguredAuthority;
impl LeaveCalendarAuthority for UnconfiguredAuthority {
    fn authorize<'a>(
        &'a self,
        _: &'a LeaveCalendarAuthorityContext,
    ) -> super::application::CalendarFuture<'a, ()> {
        Box::pin(async { Err(super::domain::unavailable()) })
    }
}

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's calendar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveCalendarTarget {
    pub host: String,
    pub holding_slug: String,
}

struct CalendarState {
    targets: BTreeMap<(String, String), Arc<dyn LeaveCalendarRead>>,
    authority: Arc<dyn LeaveCalendarAuthority>,
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

#[derive(serde::Serialize)]
struct CalendarResponse {
    data: LeaveCalendarMonth,
}

/// The retired route set only `Cache-Control: private, no-store`; the two
/// additional security headers match the sibling native owner routers.
fn bounded(body: CalendarResponse) -> Response {
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

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LEAVE_CALENDAR_CONFIG_INVALID",
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

/// The retired route read exactly one query key. A duplicate key fails closed;
/// an absent or empty value is the retired `LEAVE_MONTH_REQUIRED` 422.
fn month_value(query: Option<&str>) -> Result<Option<String>, BusinessError> {
    let mut found = None;
    for pair in query
        .unwrap_or("")
        .split('&')
        .filter(|part| !part.is_empty())
    {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == "month" {
            if found.replace(value.to_owned()).is_some() {
                return Err(invalid());
            }
        }
    }
    Ok(found)
}

fn configure(
    entries: impl IntoIterator<Item = (LeaveCalendarTarget, Arc<dyn LeaveCalendarRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn LeaveCalendarRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn finish(
    targets: BTreeMap<(String, String), Arc<dyn LeaveCalendarRead>>,
    authority: Arc<dyn LeaveCalendarAuthority>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/leave/calendar",
            get(calendar),
        )
        .with_state(Arc::new(CalendarState { targets, authority }))
}

/// Native router with one Pg owner reader per registered Holding pool. The lead
/// mounts this on the private native People Payroll axum service.
pub fn router_for_holdings(
    targets: Vec<(LeaveCalendarTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(
        configure(targets.into_iter().map(|(target, pool)| {
            (
                target,
                Arc::new(PgLeaveCalendarRead::new(pool)) as Arc<dyn LeaveCalendarRead>,
            )
        }))?,
        Arc::new(UnconfiguredAuthority),
    ))
}

/// Composition supplies a fail-closed current-authority and purchase checker.
pub fn router_for_holdings_with_authority(
    targets: Vec<(LeaveCalendarTarget, DatabasePool)>,
    authority: Arc<dyn LeaveCalendarAuthority>,
    pg_authority: Arc<dyn PgLeaveCalendarAuthority>,
) -> Result<Router, BusinessError> {
    Ok(finish(
        configure(targets.into_iter().map(|(target, pool)| {
            (
                target,
                Arc::new(PgLeaveCalendarRead::with_authority(
                    pool,
                    pg_authority.clone(),
                )) as Arc<dyn LeaveCalendarRead>,
            )
        }))?,
        authority,
    ))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    reader: Arc<dyn LeaveCalendarRead>,
    targets: Vec<LeaveCalendarTarget>,
) -> Result<Router, BusinessError> {
    router_with_reader_and_authority(reader, targets, Arc::new(UnconfiguredAuthority))
}

/// Injectable reader and authority for tests and composed owner adapters.
pub fn router_with_reader_and_authority(
    reader: Arc<dyn LeaveCalendarRead>,
    targets: Vec<LeaveCalendarTarget>,
    authority: Arc<dyn LeaveCalendarAuthority>,
) -> Result<Router, BusinessError> {
    Ok(finish(
        configure(targets.into_iter().map(|target| (target, reader.clone())))?,
        authority,
    ))
}

async fn calendar(
    State(state): State<Arc<CalendarState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let reader = state
            .targets
            .get(&(host.clone(), slug.clone()))
            .ok_or_else(missing)?
            .clone();

        let raw_month = month_value(request.uri().query())?
            .filter(|value| !value.is_empty())
            .ok_or_else(super::domain::month_required)?;
        if !valid_month(&raw_month) {
            return Err(super::domain::month_invalid());
        }

        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .map(str::to_owned)
            .ok_or_else(invalid)?;
        let user = unique_header(&headers, CONTEXT_USER)?
            .filter(|value| valid_id(value))
            .map(str::to_owned)
            .ok_or_else(invalid)?;

        let assignment = unique_header(&headers, CONTEXT_ASSIGNMENT)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let number = |name| -> Result<i64, BusinessError> {
            unique_header(&headers, name)?
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|value| (1..=9_007_199_254_740_991).contains(value))
                .ok_or_else(invalid)
        };
        let authority = LeaveCalendarAuthorityContext {
            host,
            holding_slug: slug,
            company_id: company.clone(),
            user_id: user.clone(),
            assignment_id: assignment.to_owned(),
            revision: number(CONTEXT_REVISION)?,
            assignment_version: number(CONTEXT_ASSIGNMENT_VERSION)?,
        };
        state.authority.authorize(&authority).await?;
        let mut query = LeaveCalendarQuery::try_new(&company, &user, &raw_month)?;
        query.authority = Some(authority);
        let month = leave_calendar_month(reader.as_ref(), &query).await?;
        Ok::<_, BusinessError>(CalendarResponse { data: month })
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
    use crate::adapters::native_leave_calendar::application::CalendarFuture;
    use crate::adapters::native_leave_calendar::domain::{
        fixture_row, month_required, unavailable,
    };
    use erpx_platform_runtime_server::public::http_support::serde_json::json;
    use sha2::Digest;
    use std::sync::Mutex;

    struct Allow;
    impl LeaveCalendarAuthority for Allow {
        fn authorize<'a>(
            &'a self,
            _: &'a LeaveCalendarAuthorityContext,
        ) -> super::super::application::CalendarFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }
    }

    struct Probe {
        facts: super::super::domain::LeaveCalendarFacts,
        fail: bool,
        seen: Arc<Mutex<Vec<LeaveCalendarQuery>>>,
    }

    impl LeaveCalendarRead for Probe {
        fn read<'a>(
            &'a self,
            query: &'a LeaveCalendarQuery,
        ) -> CalendarFuture<'a, super::super::domain::LeaveCalendarFacts> {
            let query = query.clone();
            let facts = self.facts.clone();
            let fail = self.fail;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                if fail {
                    return Err(unavailable());
                }
                Ok(facts)
            })
        }
    }

    fn target(host: &str, slug: &str) -> LeaveCalendarTarget {
        LeaveCalendarTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn LeaveCalendarRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), vec![target("acme.test", "acme")]).is_ok());
        assert!(
            router_with_reader(
                reader.clone(),
                vec![target("acme.test", "acme"), target("acme.test", "acme")],
            )
            .is_err()
        );
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader, vec![target("acme.test", "Acme")]).is_err());
    }

    struct Deny(Arc<Mutex<Vec<LeaveCalendarAuthorityContext>>>);
    impl LeaveCalendarAuthority for Deny {
        fn authorize<'a>(
            &'a self,
            context: &'a LeaveCalendarAuthorityContext,
        ) -> super::super::application::CalendarFuture<'a, ()> {
            self.0.lock().unwrap().push(context.clone());
            Box::pin(async {
                Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "PEOPLE_ACCESS_CHANGED",
                    "Access changed.",
                ))
            })
        }
    }

    #[tokio::test]
    async fn denied_authority_receives_scope_before_any_calendar_read() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let checks = Arc::new(Mutex::new(Vec::new()));
        let reader: Arc<dyn LeaveCalendarRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader_and_authority(
            reader,
            vec![target("acme.test", "acme")],
            Arc::new(Deny(checks.clone())),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ =
                erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let request = "GET /api/v1/holdings/acme/leave/calendar?month=2026-11 HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: usr-1\r\nx-erpx-context-assignment: asg-1\r\nx-erpx-context-revision: 7\r\nx-erpx-context-assignment-version: 2\r\n\r\n";
        let denied = http(addr, request.into()).await;
        assert_eq!(denied.0, 403);
        assert_eq!(denied.2["error"]["code"], "PEOPLE_ACCESS_CHANGED");
        assert!(seen.lock().unwrap().is_empty());
        assert_eq!(
            checks.lock().unwrap().as_slice(),
            &[LeaveCalendarAuthorityContext {
                host: "acme.test".into(),
                holding_slug: "acme".into(),
                company_id: "acme-co".into(),
                user_id: "usr-1".into(),
                assignment_id: "asg-1".into(),
                revision: 7,
                assignment_version: 2,
            }]
        );
        for (name, value) in [
            ("revision", "0"),
            ("revision", "-1"),
            ("revision", "9007199254740992"),
            ("assignment-version", "0"),
            ("assignment-version", "9007199254740992"),
        ] {
            let original = if name == "revision" {
                "x-erpx-context-revision: 7"
            } else {
                "x-erpx-context-assignment-version: 2"
            };
            let replacement = format!("x-erpx-context-{name}: {value}");
            assert_eq!(
                http(addr, request.replace(original, &replacement)).await.0,
                422
            );
        }
        assert_eq!(checks.lock().unwrap().len(), 1);
        assert!(seen.lock().unwrap().is_empty());
        let max = request.replace(
            "x-erpx-context-revision: 7",
            "x-erpx-context-revision: 9007199254740991",
        );
        assert_eq!(http(addr, max).await.0, 403);
        assert_eq!(checks.lock().unwrap().len(), 2);
        server.abort();
    }

    #[tokio::test]
    async fn unconfigured_authority_never_reads_calendar() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader: Arc<dyn LeaveCalendarRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader(reader, vec![target("acme.test", "acme")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ =
                erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let request = "GET /api/v1/holdings/acme/leave/calendar?month=2026-11 HTTP/1.1\r\nHost: acme.test\r\nConnection: close\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: usr-1\r\nx-erpx-context-assignment: asg-1\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment-version: 1\r\n\r\n";
        assert_eq!(http(addr, request.into()).await.0, 503);
        assert!(seen.lock().unwrap().is_empty());
        server.abort();
    }

    #[test]
    fn payload_matches_the_retired_typescript_output() {
        let body = serde_json::to_string(&CalendarResponse {
            data: super::super::domain::LeaveCalendarMonth {
                month: "2026-11".into(),
                requests: vec![fixture_row()],
                holidays: vec![],
            },
        })
        .unwrap();
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex, "1cf77f5f4865735fa19d0d453df5ac28ad22ed0c14cd901d6bd06e7f4a674d38",
            "update this oracle in Rust if the retired response shape changes"
        );
    }

    async fn http(
        addr: std::net::SocketAddr,
        raw: String,
    ) -> (u16, String, serde_json::Value, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let json = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
        (status, head.to_owned(), json, body.to_owned())
    }

    #[tokio::test]
    async fn router_scopes_the_holding_and_binds_the_month_and_context() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader: Arc<dyn LeaveCalendarRead> = Arc::new(Probe {
            facts: super::super::domain::LeaveCalendarFacts {
                requests: vec![fixture_row()],
                holidays: vec![super::super::domain::HolidayRow {
                    holiday_date: "2026-11-25".into(),
                    name: "Holiday".into(),
                }],
            },
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader_and_authority(
            reader,
            vec![target("acme.test", "acme")],
            Arc::new(Allow),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ =
                erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, query: &str, company: &str, user: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/leave/calendar{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
                 x-erpx-context-company: {company}\r\nx-erpx-context-user: {user}\r\nx-erpx-context-assignment: asg-1\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment-version: 1\r\n\r\n"
            )
        };
        let ok = http(
            addr,
            get("acme.test", "acme", "?month=2026-11", "acme-co", "usr-1"),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert!(
            ok.1.to_ascii_lowercase()
                .contains("cache-control: private, no-store")
        );
        assert_eq!(ok.2["data"]["month"], json!("2026-11"));
        assert_eq!(ok.2["data"]["requests"][0]["id"], json!("leave-1"));
        assert_eq!(
            ok.2["data"]["holidays"][0]["holiday_date"],
            json!("2026-11-25")
        );
        let bound = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(bound.company_id, "acme-co");
        assert_eq!(bound.user_id, "usr-1");
        assert_eq!(bound.month, "2026-11");
        assert_eq!(bound.start, "2026-11-01");
        assert_eq!(bound.end, "2026-11-30");
        assert!(ok.3.starts_with("{\"data\":{\"month\":\"2026-11\","));

        assert_eq!(
            http(addr, get("acme.test", "acme", "", "acme-co", "usr-1"))
                .await
                .0,
            422
        );
        assert_eq!(
            http(
                addr,
                get("acme.test", "acme", "?month=2026-13", "acme-co", "usr-1")
            )
            .await
            .2["error"]["code"],
            json!("VALIDATION_FAILED")
        );
        assert_eq!(
            http(
                addr,
                get("other.test", "acme", "?month=2026-11", "acme-co", "usr-1")
            )
            .await
            .0,
            404
        );
        assert_eq!(
            http(
                addr,
                get("acme.test", "other", "?month=2026-11", "acme-co", "usr-1")
            )
            .await
            .0,
            404
        );
        assert_eq!(
            http(
                addr,
                get(
                    "acme.test",
                    "acme",
                    "?month=2026-11",
                    "bad company",
                    "usr-1"
                )
            )
            .await
            .0,
            422
        );
        server.abort();

        let failing: Arc<dyn LeaveCalendarRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: true,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let router = router_with_reader_and_authority(
            failing,
            vec![target("fail.test", "fail")],
            Arc::new(Allow),
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ =
                erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let failed = http(
            addr,
            "GET /api/v1/holdings/fail/leave/calendar?month=2026-11 HTTP/1.1\r\nHost: fail.test\r\nConnection: close\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: usr-1\r\nx-erpx-context-assignment: asg-1\r\nx-erpx-context-revision: 1\r\nx-erpx-context-assignment-version: 1\r\n\r\n".into(),
        )
        .await;
        assert_eq!(failed.0, 503);
        assert_eq!(
            failed.2["error"]["code"],
            "PEOPLE_PAYROLL_LEAVE_CALENDAR_READ_UNAVAILABLE"
        );
        server.abort();

        // The missing-month code is pinned to the retired ApiError, not the
        // generic invalid-input owner code.
        assert_eq!(month_required().code, "LEAVE_MONTH_REQUIRED");
    }
}
