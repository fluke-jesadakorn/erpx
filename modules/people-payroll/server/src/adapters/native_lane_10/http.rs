//! Owner-local axum ingress for the active leave-policy read. Host/Holding
//! targets, Company scope, bounded request shape and error mapping are owned
//! here; PostgreSQL execution lives in `postgres`. No TS SQL fallback remains.

use super::application::{LeavePolicyRead, project, project_history, validate, validate_history};
use super::domain::{
    ActiveLeavePolicies, LeavePolicyHistory, configuration_invalid, invalid, valid_id, valid_slug,
};
use super::postgres::PgLeavePolicyRead;
use crate::adapters::native_lane_9::LaneTarget;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

struct RouteState {
    targets: BTreeMap<(String, String), Arc<dyn LeavePolicyRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn configure_readers(
    entries: impl IntoIterator<Item = (LaneTarget, Arc<dyn LeavePolicyRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn LeavePolicyRead>>, BusinessError> {
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
    configured: BTreeMap<(String, String), Arc<dyn LeavePolicyRead>>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/active-leave-policies",
            get(active_leave_policies),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/leave-policy-history",
            get(leave_policy_history),
        )
        .with_state(Arc::new(RouteState { targets: configured })))
}

/// Public router contract for a single pool. The lead mounts it.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgLeavePolicyRead::new(pool)), targets)
}

/// Per-Holding constructor: one Pg reader per registered `(LaneTarget, pool)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgLeavePolicyRead::new(pool)) as Arc<dyn LeavePolicyRead>,
        )
    }))?)
}

pub fn router_with_reader(
    reader: Arc<dyn LeavePolicyRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

pub fn router_with_readers(
    entries: Vec<(LaneTarget, Arc<dyn LeavePolicyRead>)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(entries)?)
}

async fn active_leave_policies(
    State(state): State<Arc<RouteState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
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
            .ok_or_else(invalid)?;
        let query = ActiveLeavePolicies::new(company);
        validate(&query)?;
        let rows = reader.active(&query.company).await?;
        project(rows)
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn leave_policy_history(
    State(state): State<Arc<RouteState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
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
            .ok_or_else(invalid)?;
        let query = LeavePolicyHistory::new(company);
        validate_history(&query)?;
        let rows = reader.history(&query.company).await?;
        project_history(rows)
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
    use super::super::application::LeavePolicyFuture;
    use super::super::domain::{LeavePolicyHistoryRow, LeavePolicySetting, unavailable};
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    struct Probe {
        rows: Vec<LeavePolicySetting>,
        history_rows: Vec<LeavePolicyHistoryRow>,
        fail: bool,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl LeavePolicyRead for Probe {
        fn active<'a>(&'a self, company: &'a str) -> LeavePolicyFuture<'a, Vec<LeavePolicySetting>> {
            let seen = self.seen.clone();
            let rows = self.rows.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push(company.to_owned());
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }

        fn history<'a>(
            &'a self,
            company: &'a str,
        ) -> LeavePolicyFuture<'a, Vec<LeavePolicyHistoryRow>> {
            let seen = self.seen.clone();
            let rows = self.history_rows.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push(company.to_owned());
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }
    }

    fn rows() -> Vec<LeavePolicySetting> {
        vec![LeavePolicySetting {
            name: "standard".into(),
            version: 3,
            status: "active".into(),
        }]
    }

    fn history_rows() -> Vec<LeavePolicyHistoryRow> {
        vec![LeavePolicyHistoryRow {
            title: "standard".into(),
            version: 3,
            actor: "u1".into(),
            at: 1_700_000_000,
            kind: "Leave policy".into(),
        }]
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn LeavePolicyRead> = Arc::new(Probe {
            rows: rows(),
            history_rows: history_rows(),
            fail: false,
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let target = || LaneTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![target(), target()]).is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(
            reader,
            vec![LaneTarget {
                holding_slug: "Bad_Slug".into(),
                ..target()
            }]
        )
        .is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn router_scopes_company_and_maps_errors() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe: Arc<dyn LeavePolicyRead> = Arc::new(Probe {
            rows: rows(),
            history_rows: history_rows(),
            fail: false,
            seen: seen.clone(),
        });
        let failing: Arc<dyn LeavePolicyRead> = Arc::new(Probe {
            rows: vec![],
            history_rows: vec![],
            fail: true,
            seen: seen.clone(),
        });
        let router = router_with_reader(
            probe,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .unwrap();
        let failing_router = router_with_reader(
            failing,
            vec![LaneTarget {
                host: "fail.example.test".into(),
                holding_slug: "fail".into(),
            }],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |path: &str, company: &str| {
            format!(
                "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\
                 x-erpx-context-company: {company}\r\n\r\n"
            )
        };
        let path = "/api/v1/holdings/acme/people-payroll/active-leave-policies";
        let (status, body) = http(addr, get(path, "comp_1")).await;
        assert_eq!(status, 200);
        assert_eq!(body, json!([{"name":"standard","status":"active","version":3}]));
        let (status, _) = http(
            addr,
            get("/api/v1/holdings/other/people-payroll/active-leave-policies", "comp_1"),
        )
        .await;
        assert_eq!(status, 404);
        let (status, body) = http(addr, get(path, "bad id")).await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE10_REQUEST_INVALID");
        assert_eq!(seen.lock().unwrap().last().cloned(), Some("comp_1".to_owned()));
        server.abort();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, failing_router).await;
        });
        let raw = "GET /api/v1/holdings/fail/people-payroll/active-leave-policies HTTP/1.1\r\nHost: fail.example.test\r\nConnection: close\r\nx-erpx-context-company: comp_1\r\n\r\n".to_owned();
        let (status, body) = http(addr, raw).await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE10_READ_UNAVAILABLE");
        server.abort();
    }

    #[tokio::test]
    async fn history_route_scopes_company_and_returns_typed_rows() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe: Arc<dyn LeavePolicyRead> = Arc::new(Probe {
            rows: rows(),
            history_rows: history_rows(),
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader(
            probe,
            vec![LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            }],
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |path: &str, company: &str| {
            format!(
                "GET {path} HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\
                 x-erpx-context-company: {company}\r\n\r\n"
            )
        };
        let path = "/api/v1/holdings/acme/people-payroll/leave-policy-history";
        let (status, body) = http(addr, get(path, "comp_1")).await;
        assert_eq!(status, 200);
        assert_eq!(
            body,
            json!([{
                "title":"standard","version":3,"actor":"u1","at":1700000000,"kind":"Leave policy"
            }])
        );
        let (status, _) = http(
            addr,
            get("/api/v1/holdings/other/people-payroll/leave-policy-history", "comp_1"),
        )
        .await;
        assert_eq!(status, 404);
        let (status, body) = http(addr, get(path, "bad id")).await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE10_REQUEST_INVALID");
        assert_eq!(seen.lock().unwrap().last().cloned(), Some("comp_1".to_owned()));
        server.abort();
    }
}
