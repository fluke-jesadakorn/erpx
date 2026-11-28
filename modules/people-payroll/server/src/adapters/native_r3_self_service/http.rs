//! Owner-local axum ingress for `GET .../self-service/summary`. The exact
//! `{data: ...}` envelope, `Cache-Control` and `X-ERPX-Work-Count` header are
//! owned here. The People-owned next-shift row is read with SQLx; the aggregate
//! is a composition port, never reimplemented in TypeScript or stubbed.

use super::application::{NextShiftRead, OwnerFuture, SummaryAggregatePort};
use super::domain::{
    NextShift, SummaryActor, SummaryResult, SummaryTarget, invalid, not_found, unavailable, valid_id,
    valid_slug,
};
use super::postgres::PgNextShiftRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_ENGAGEMENT: HeaderName = HeaderName::from_static("x-erpx-context-engagement");
const WORK_COUNT: HeaderName = HeaderName::from_static("x-erpx-work-count");

/// Everything the owner needs to serve the read. Composition injects the Pg
/// next-shift reader and the cross-owner aggregate.
pub struct SummaryPorts {
    pub shifts: Arc<dyn NextShiftRead>,
    pub aggregate: Arc<dyn SummaryAggregatePort>,
}

struct AppState {
    targets: BTreeMap<(String, String), Arc<SummaryPorts>>,
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    ).into_response()
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique(headers, name)?.filter(|v| valid_id(v)).ok_or_else(invalid)?;
    Ok(value.to_owned())
}

fn actor(headers: &HeaderMap) -> Result<SummaryActor, BusinessError> {
    let engagement = match unique(headers, CONTEXT_ENGAGEMENT)? {
        Some(value) if !valid_id(value) => return Err(invalid()),
        Some(value) => Some(value.to_owned()),
        None => None,
    };
    Ok(SummaryActor {
        company_id: required(headers, CONTEXT_COMPANY)?,
        user_id: required(headers, CONTEXT_USER)?,
        assignment_id: required(headers, CONTEXT_ASSIGNMENT)?,
        engagement_id: engagement,
    })
}

fn configure(
    entries: impl IntoIterator<Item = (SummaryTarget, Arc<SummaryPorts>)>,
) -> Result<BTreeMap<(String, String), Arc<SummaryPorts>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, ports) in entries {
        if target.host.is_empty()
            || target.host.len() > 253
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), ports)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PEOPLE_PAYROLL_SELF_SERVICE_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "PEOPLE_PAYROLL_SELF_SERVICE_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(configured)
}

fn finish(targets: BTreeMap<(String, String), Arc<SummaryPorts>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/self-service/summary",
            get(summary),
        )
        .with_state(Arc::new(AppState { targets }))
}

/// Router for the lead to mount on the private native People Payroll axum
/// service with `PgNextShiftRead` in `SummaryPorts`.
pub fn router(
    targets: Vec<(SummaryTarget, Arc<SummaryPorts>)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(targets)?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(SummaryTarget, Arc<SummaryPorts>)>,
) -> Result<Router, BusinessError> {
    router(targets)
}

async fn summary(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let host = unique(&headers, header::HOST)?
            .filter(|host| !host.is_empty() && host.len() <= 253)
            .ok_or_else(not_found)?;
        let ports = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(not_found)?;
        let who = actor(&headers)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis() as i64)
            .unwrap_or(0);
        let shift = match &who.engagement_id {
            Some(engagement) => {
                ports.shifts.next_shift(&who.company_id, engagement, now).await?
            }
            None => None,
        };
        ports.aggregate.summarize(&who, shift.as_ref(), now).await
    }
    .await;
    match result {
        Ok(SummaryResult { data, work_count }) => {
            let mut response = (StatusCode::OK, Json(json!({"data": data}))).into_response();
            for (name, value) in [
                (header::CACHE_CONTROL, "private, no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ] {
                response.headers_mut().insert(name, HeaderValue::from_static(value));
            }
            let count = work_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            if let Ok(value) = HeaderValue::from_str(&count) {
                response.headers_mut().insert(WORK_COUNT, value);
            }
            response
        }
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    struct ShiftProbe;
    impl NextShiftRead for ShiftProbe {
        fn next_shift<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: i64,
        ) -> OwnerFuture<'a, Option<NextShift>> {
            Box::pin(async { Ok(None) })
        }
    }

    struct AggregateProbe {
        count: Option<i64>,
    }
    impl SummaryAggregatePort for AggregateProbe {
        fn summarize<'a>(
            &'a self,
            _: &'a SummaryActor,
            _: Option<&'a NextShift>,
            _: i64,
        ) -> OwnerFuture<'a, SummaryResult> {
            let count = self.count;
            Box::pin(async move {
                Ok(SummaryResult {
                    data: json!({"companyId":"acme-co","rosterRequired":false}),
                    work_count: count,
                })
            })
        }
    }

    fn ports(count: Option<i64>) -> Arc<SummaryPorts> {
        Arc::new(SummaryPorts {
            shifts: Arc::new(ShiftProbe),
            aggregate: Arc::new(AggregateProbe { count }),
        })
    }

    fn target() -> SummaryTarget {
        SummaryTarget { host: "acme.test".into(), holding_slug: "acme".into() }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(router_with_reader(vec![(target(), ports(Some(2)))]).is_ok());
        assert!(router_with_reader(vec![(target(), ports(None)), (target(), ports(None))]).is_err());
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes).to_string();
        let status = text.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok()).unwrap();
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
        (status, text, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    async fn serve(count: Option<i64>, raw: String) -> (u16, String, Value) {
        let router = router_with_reader(vec![(target(), ports(count))]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let result = http(addr, raw).await;
        server.abort();
        result
    }

    #[tokio::test]
    async fn envelope_headers_and_scope_are_stable() {
        let raw = "GET /api/v1/holdings/acme/self-service/summary HTTP/1.1\r\nHost: acme.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: u-1\r\nx-erpx-context-assignment: a-1\r\nx-erpx-context-engagement: e-1\r\nConnection: close\r\n\r\n".to_owned();
        let (code, text, body) = serve(Some(3), raw).await;
        assert_eq!(code, 200);
        let lower = text.to_ascii_lowercase();
        assert!(lower.contains("cache-control: private, no-store"));
        assert!(lower.contains("x-erpx-work-count: 3"));
        assert_eq!(body["data"]["companyId"], json!("acme-co"));

        let (code, text, _) = serve(None, "GET /api/v1/holdings/acme/self-service/summary HTTP/1.1\r\nHost: acme.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: u-1\r\nx-erpx-context-assignment: a-1\r\nConnection: close\r\n\r\n".to_owned()).await;
        assert_eq!(code, 200);
        assert!(text.to_ascii_lowercase().contains("x-erpx-work-count: unknown"));

        let (code, _, body) = serve(Some(1), "GET /api/v1/holdings/other/self-service/summary HTTP/1.1\r\nHost: acme.test\r\nx-erpx-context-company: acme-co\r\nx-erpx-context-user: u-1\r\nx-erpx-context-assignment: a-1\r\nConnection: close\r\n\r\n".to_owned()).await;
        assert_eq!(code, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[allow(dead_code)]
    fn pg_reader(pool: erpx_platform_runtime_server::public::postgres::DatabasePool) -> Arc<dyn NextShiftRead> {
        Arc::new(PgNextShiftRead::new(pool))
    }
}
