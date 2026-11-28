//! Owner-local axum ingress for the native leave-summary read. Host and Holding
//! targets, bounded context headers, payload order and error mapping are owned
//! here; the PostgreSQL execution lives in `postgres`. No TypeScript SQL or
//! business fallback exists for this operation.

use super::application::{LeaveSummaryQuery, LeaveSummaryRead, leave_summary};
use super::domain::{LeaveSummary, invalid, missing, valid_id};
use super::postgres::PgLeaveSummaryRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaveSummaryTarget {
    pub host: String,
    pub holding_slug: String,
}

struct SummaryState {
    targets: BTreeMap<(String, String), Arc<dyn LeaveSummaryRead>>,
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
struct SummaryResponse {
    data: LeaveSummary,
}

/// The retired route set `Cache-Control: private, no-store`; the two additional
/// security headers match the sibling native owner routers.
fn bounded(body: SummaryResponse) -> Response {
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
        "PEOPLE_PAYROLL_LEAVE_SUMMARY_CONFIG_INVALID",
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

fn now_ms() -> Result<i64, BusinessError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .map_err(|_| invalid())
}

fn configure(
    entries: impl IntoIterator<Item = (LeaveSummaryTarget, Arc<dyn LeaveSummaryRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn LeaveSummaryRead>>, BusinessError> {
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

fn finish(targets: BTreeMap<(String, String), Arc<dyn LeaveSummaryRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/leave/summary",
            get(summary),
        )
        .with_state(Arc::new(SummaryState { targets }))
}

/// Native router with one Pg owner reader per registered Holding pool. The lead
/// mounts this on the private native People Payroll axum service.
pub fn router_for_holdings(
    targets: Vec<(LeaveSummaryTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgLeaveSummaryRead::new(pool)) as Arc<dyn LeaveSummaryRead>,
        )
    }))?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    reader: Arc<dyn LeaveSummaryRead>,
    targets: Vec<LeaveSummaryTarget>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

async fn summary(
    State(state): State<Arc<SummaryState>>,
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
            .get(&(host, slug))
            .ok_or_else(missing)?
            .clone();

        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .map(str::to_owned)
            .ok_or_else(invalid)?;
        let user = unique_header(&headers, CONTEXT_USER)?
            .filter(|value| valid_id(value))
            .map(str::to_owned)
            .ok_or_else(invalid)?;

        let query = LeaveSummaryQuery::try_new(&company, &user, now_ms()?)?;
        let data = leave_summary(reader.as_ref(), &query).await?;
        Ok::<_, BusinessError>(SummaryResponse { data })
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
    use crate::adapters::native_r3_leave_summary::application::SummaryFuture;
    use crate::adapters::native_r3_leave_summary::domain::{
        fixture_row, fixture_type, Balance, BalanceView, HolidayRow, LeaveSummaryFacts, PolicyFact,
        unavailable,
    };
    use erpx_platform_runtime_server::public::http_support::serde_json::json;
    use std::sync::Mutex;

    struct Probe {
        facts: LeaveSummaryFacts,
        fail: bool,
        seen: Arc<Mutex<Vec<LeaveSummaryQuery>>>,
    }

    impl LeaveSummaryRead for Probe {
        fn read<'a>(
            &'a self,
            query: &'a LeaveSummaryQuery,
        ) -> SummaryFuture<'a, LeaveSummaryFacts> {
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

    fn target(host: &str, slug: &str) -> LeaveSummaryTarget {
        LeaveSummaryTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    fn facts() -> LeaveSummaryFacts {
        LeaveSummaryFacts {
            staff_linked: true,
            policy: Some(PolicyFact {
                assignment_id: "assignment-1".into(),
                policy_version_id: "policy-1".into(),
                policy_name: "Standard".into(),
                policy_version: 1,
                timezone: "Asia/Bangkok".into(),
                holiday_calendar_id: Some("calendar-1".into()),
            }),
            types: vec![fixture_type()],
            balances: vec![Balance {
                leave_type_id: "type-1".into(),
                available: 8.0,
                reserved: 0.0,
                used: 2.0,
                accrued: 0.0,
            }],
            requests: vec![fixture_row()],
            holidays: vec![HolidayRow {
                holiday_date: "2099-11-25".into(),
                name: "Holiday".into(),
            }],
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn LeaveSummaryRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), vec![target("acme.test", "acme")]).is_ok());
        assert!(router_with_reader(
            reader.clone(),
            vec![target("acme.test", "acme"), target("acme.test", "acme")],
        )
        .is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader, vec![target("acme.test", "Acme")]).is_err());
    }

    #[test]
    fn payload_matches_the_retired_typescript_shape() {
        let body = serde_json::to_string(&SummaryResponse {
            data: LeaveSummary {
                as_of: 1_763_000_000_000,
                setup_required: false,
                setup_message: None,
                policy: None,
                balances: vec![BalanceView {
                    leave_type: fixture_type(),
                    available: 8.0,
                    reserved: 0.0,
                    used: 2.0,
                    accrued: 0.0,
                }],
                upcoming: vec![fixture_row()],
                pending: vec![],
                history: vec![fixture_row()],
                holidays: vec![],
            },
        })
        .unwrap();
        assert!(body.starts_with(r#"{"data":{"asOf":1763000000000,"setupRequired":false,"#));
        assert!(body.contains(r#""balances":[{"type":{"id":"type-1","#));
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value, String) {
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
    async fn router_scopes_the_holding_and_binds_context() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader: Arc<dyn LeaveSummaryRead> = Arc::new(Probe {
            facts: facts(),
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader(reader, vec![target("acme.test", "acme")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, company: &str, user: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/leave/summary HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
                 x-erpx-context-company: {company}\r\nx-erpx-context-user: {user}\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme", "acme-co", "usr-1")).await;
        assert_eq!(ok.0, 200);
        assert!(ok.1.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert_eq!(ok.2["data"]["policy"]["name"], json!("Standard"));
        assert_eq!(ok.2["data"]["balances"][0]["available"], json!(8.0));
        assert_eq!(ok.2["data"]["holidays"][0]["holiday_date"], json!("2099-11-25"));
        let bound = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(bound.company_id, "acme-co");
        assert_eq!(bound.user_id, "usr-1");
        assert!(ok.3.starts_with("{\"data\":{\"asOf\":"));

        assert_eq!(
            http(addr, get("other.test", "acme", "acme-co", "usr-1")).await.0,
            404
        );
        assert_eq!(
            http(addr, get("acme.test", "other", "acme-co", "usr-1")).await.0,
            404
        );
        assert_eq!(
            http(addr, get("acme.test", "acme", "bad company", "usr-1")).await.0,
            422
        );
        server.abort();

        let failing: Arc<dyn LeaveSummaryRead> = Arc::new(Probe {
            facts: Default::default(),
            fail: true,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let router = router_with_reader(failing, vec![target("fail.test", "fail")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let failed = http(
            addr,
            "GET /api/v1/holdings/fail/leave/summary HTTP/1.1\r\nHost: fail.test\r\nConnection: close\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: usr-1\r\n\r\n".into(),
        )
        .await;
        assert_eq!(failed.0, 503);
        assert_eq!(
            failed.2["error"]["code"],
            "PEOPLE_PAYROLL_LEAVE_SUMMARY_READ_UNAVAILABLE"
        );
        server.abort();
    }
}
