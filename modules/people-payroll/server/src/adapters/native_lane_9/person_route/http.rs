//! Owner-local axum ingress for the work-review person-route read. Host/Holding
//! targets, Company scope, bounded request shape and error mapping are owned
//! here; PostgreSQL execution lives in `postgres`. No TS SQL fallback remains.

use super::application::PersonRouteRead;
use super::domain::{RoutePerson, invalid, valid_id, valid_slug, valid_users};
use super::postgres::PgPersonRouteRead;
use crate::adapters::native_lane_9::LaneTarget;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const BODY_LIMIT: usize = 16 * 1024;

struct RouteState {
    targets: BTreeMap<(String, String), Arc<dyn PersonRouteRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_LANE9_ROUTE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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
    entries: impl IntoIterator<Item = (LaneTarget, Arc<dyn PersonRouteRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn PersonRouteRead>>, BusinessError> {
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
    configured: BTreeMap<(String, String), Arc<dyn PersonRouteRead>>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/route-people",
            post(route_people),
        )
        .with_state(Arc::new(RouteState { targets: configured })))
}

/// Public router contract for a single pool. The lead mounts it.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPersonRouteRead::new(pool)), targets)
}

/// Per-Holding constructor: one Pg reader per registered `(LaneTarget, pool)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgPersonRouteRead::new(pool)) as Arc<dyn PersonRouteRead>,
        )
    }))?)
}

pub fn router_with_reader(
    reader: Arc<dyn PersonRouteRead>,
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
    entries: Vec<(LaneTarget, Arc<dyn PersonRouteRead>)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(entries)?)
}

async fn route_people(
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
        let bytes = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| invalid())?;
        let body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let users = body
            .get("users")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(invalid)?
            .iter()
            .map(|item| item.as_str().map(str::to_owned).ok_or_else(invalid))
            .collect::<Result<Vec<_>, _>>()?;
        if !valid_users(&users) {
            return Err(invalid());
        }
        let rows = reader.route(company, &users).await?;
        let results = rows
            .into_iter()
            .map(|row: RoutePerson| {
                serde_json::json!({"user_id":row.user_id,"legal_name":row.legal_name})
            })
            .collect::<Vec<_>>();
        Ok::<_, BusinessError>(serde_json::json!({"results":results}))
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
    use super::super::domain::unavailable;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    struct Probe {
        fail: bool,
        seen: Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>,
    }

    impl PersonRouteRead for Probe {
        fn route<'a>(
            &'a self,
            company: &'a str,
            users: &'a [String],
        ) -> super::super::application::RouteFuture<'a, Vec<RoutePerson>> {
            let seen = self.seen.clone();
            let fail = self.fail;
            Box::pin(async move {
                seen.lock().unwrap().push((company.to_owned(), users.to_vec()));
                if fail {
                    return Err(unavailable());
                }
                Ok(users
                    .iter()
                    .map(|user| RoutePerson {
                        user_id: user.clone(),
                        legal_name: format!("Name {user}"),
                    })
                    .collect())
            })
        }
    }

    #[test]
    fn target_configuration_requires_distinct_exact_holdings() {
        let reader: Arc<dyn PersonRouteRead> = Arc::new(Probe {
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
    async fn router_scopes_company_users_and_maps_errors() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let probe: Arc<dyn PersonRouteRead> = Arc::new(Probe {
            fail: false,
            seen: seen.clone(),
        });
        let failing: Arc<dyn PersonRouteRead> = Arc::new(Probe {
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
        let post = |path: &str, host: &str, company: &str, body: serde_json::Value| {
            let body = body.to_string();
            format!(
                "POST {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
                 content-type: application/json\r\ncontent-length: {}\r\n\
                 x-erpx-context-company: {company}\r\n\r\n{body}",
                body.len()
            )
        };
        let path = "/api/v1/holdings/acme/people-payroll/route-people";
        let (status, body) = http(
            addr,
            post(path, "acme.example.test", "acme-co", json!({"users":["user-1","user-2"]})),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["results"][0]["user_id"], json!("user-1"));
        assert_eq!(body["results"][1]["legal_name"], json!("Name user-2"));
        let (status, _) = http(
            addr,
            post(
                "/api/v1/holdings/other/people-payroll/route-people",
                "acme.example.test",
                "acme-co",
                json!({"users":["user-1"]}),
            ),
        )
        .await;
        assert_eq!(status, 404);
        let (status, body) = http(
            addr,
            post(path, "acme.example.test", "bad id", json!({"users":["user-1"]})),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_ROUTE_REQUEST_INVALID");
        let (status, body) = http(
            addr,
            post(path, "acme.example.test", "acme-co", json!({"users":[]})),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_ROUTE_REQUEST_INVALID");
        assert_eq!(
            seen.lock().unwrap().last().cloned(),
            Some(("acme-co".to_owned(), vec!["user-1".to_owned(), "user-2".to_owned()]))
        );
        server.abort();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, failing_router).await;
        });
        let (status, body) = http(
            addr,
            post(
                "/api/v1/holdings/fail/people-payroll/route-people",
                "fail.example.test",
                "acme-co",
                json!({"users":["user-1"]}),
            ),
        )
        .await;
        assert_eq!(status, 503);
        assert_eq!(body["error"]["code"], "PEOPLE_PAYROLL_LANE9_ROUTE_READ_UNAVAILABLE");
        server.abort();
    }

    #[tokio::test]
    async fn per_holding_readers_are_distinct() {
        let seen_a = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_b = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader_a: Arc<dyn PersonRouteRead> = Arc::new(Probe {
            fail: false,
            seen: seen_a.clone(),
        });
        let reader_b: Arc<dyn PersonRouteRead> = Arc::new(Probe {
            fail: false,
            seen: seen_b.clone(),
        });
        let router = router_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                },
                reader_a,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                },
                reader_b,
            ),
        ])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let body = json!({"users":["user-9"]}).to_string();
        let (status, body) = http(
            addr,
            format!(
                "POST /api/v1/holdings/northstar/people-payroll/route-people HTTP/1.1\r\n\
                 Host: northstar.example.test\r\nConnection: close\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nx-erpx-context-company: acme-co\r\n\r\n{}",
                body.len(),
                body
            ),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(body["results"][0]["user_id"], json!("user-9"));
        assert!(seen_a.lock().unwrap().is_empty());
        assert_eq!(seen_b.lock().unwrap().len(), 1);
        server.abort();
    }
}
