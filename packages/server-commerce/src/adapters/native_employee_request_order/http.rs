//! Owner-local axum ingress for the native employee-request order read.
//!
//! Exact Holding targets, unique Host/Company/Record headers, bounded
//! identities, no-store responses and explicit error mapping. The handler
//! never constructs SQL: it calls the application port, whose adapter executes
//! the owner statement.

use super::application::{CommerceEmployeeOrderRead, read_order};
use super::domain::{project, valid_id};
use super::postgres::PgCommerceEmployeeOrderRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_RECORD: HeaderName = HeaderName::from_static("x-erpx-context-record");

/// Exact registered Holding target; composition supplies these explicitly.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

type Reader = Arc<dyn CommerceEmployeeOrderRead>;

struct LaneState {
    targets: BTreeMap<(String, String), Reader>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_EMPLOYEE_ORDER_READ_REQUEST_INVALID",
        "Use one bounded Commerce employee-request order read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_EMPLOYEE_ORDER_READ_CONFIG_INVALID",
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

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
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

fn state_with_readers(entries: Vec<(LaneTarget, Reader)>) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || target.host.len() > 253
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
    Ok(Arc::new(LaneState { targets: configured }))
}

/// Mountable router. Composition supplies one pool and exact targets; the
/// owner owns the reader, its SQL and its response projection.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Per-Holding mount: each target carries its own pool, so a request can only
/// read its own Holding and never falls back to another target.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            (target, Arc::new(PgCommerceEmployeeOrderRead::new(pool)) as Reader)
        })
        .collect();
    Ok(mount(state_with_readers(readers)?))
}

/// Reader-injected constructor used by the owner's own tests.
pub fn router_with_reader(reader: Reader, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    Ok(mount(state_with_readers(
        targets.into_iter().map(|target| (target, reader.clone())).collect(),
    )?))
}

fn mount(state: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/employee-request-order",
            get(read),
        )
        .with_state(state)
}

/// Pure request processing shared by the router and the unit tests.
async fn process(
    headers: &HeaderMap,
    slug: &str,
    lane: &LaneState,
) -> Result<serde_json::Value, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|host| !host.is_empty() && host.len() <= 253)
        .ok_or_else(missing)?;
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let order_id = unique_header(headers, CONTEXT_RECORD)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    let reader = lane
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)?;
    let order = read_order(reader.as_ref(), company, order_id).await?;
    Ok(project(order))
}

async fn read(
    State(lane): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = process(request.headers(), &slug, &lane).await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::application::EmployeeOrderFuture;
    use super::super::domain::EmployeeOrder;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Probe {
        seen: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl CommerceEmployeeOrderRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            order_id: &'a str,
        ) -> EmployeeOrderFuture<'a, Option<EmployeeOrder>> {
            let seen = self.seen.clone();
            let company = company.to_owned();
            let order_id = order_id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, order_id));
                Ok(Some(EmployeeOrder {
                    kind: "purchase".to_owned(),
                    company_id: "co-1".to_owned(),
                    branch_id: Some("br-1".to_owned()),
                    department_id: None,
                    state: "approved".to_owned(),
                    version: 3,
                }))
            })
        }
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget {
            host: host.to_owned(),
            holding_slug: slug.to_owned(),
        }
    }

    fn headers(host: &str, company: &str, record: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).expect("host"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_str(company).expect("company"));
        headers.insert(CONTEXT_RECORD, HeaderValue::from_str(record).expect("record"));
        headers
    }

    #[tokio::test]
    async fn complete_payload_and_company_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Probe { seen: seen.clone() };
        let lane = state_with_readers(vec![(
            target("acme.example.test", "acme"),
            Arc::new(probe) as Reader,
        )])
        .expect("state");
        let value = process(
            &headers("acme.example.test", "co-1", "o-1"),
            "acme",
            &lane,
        )
        .await
        .expect("read");
        assert_eq!(value["data"]["kind"], "purchase");
        assert_eq!(value["data"]["company_id"], "co-1");
        assert_eq!(value["data"]["branch_id"], "br-1");
        assert!(value["data"]["department_id"].is_null());
        assert_eq!(value["data"]["state"], "approved");
        assert_eq!(value["data"]["version"], 3);
        assert_eq!(value["data"].as_object().expect("row").len(), 6);
        assert_eq!(
            *seen.lock().expect("lock"),
            vec![("co-1".to_owned(), "o-1".to_owned())]
        );
    }

    #[tokio::test]
    async fn wrong_host_duplicate_headers_and_bad_scope_fail_closed() {
        let lane = state_with_readers(vec![(
            target("acme.example.test", "acme"),
            Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) }) as Reader,
        )])
        .expect("state");
        assert_eq!(
            process(&headers("attacker.example.test", "co-1", "o-1"), "acme", &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
        assert_eq!(
            process(&headers("acme.example.test", "bad company!", "o-1"), "acme", &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
        assert_eq!(
            process(&headers("acme.example.test", "co-1", "bad id!"), "acme", &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
        let mut duplicate = headers("acme.example.test", "co-1", "o-1");
        duplicate.append(CONTEXT_COMPANY, HeaderValue::from_static("co-2"));
        assert_eq!(
            process(&duplicate, "acme", &lane).await.unwrap_err().kind,
            FailureKind::InvalidInput
        );
    }

    #[test]
    fn router_requires_exact_distinct_targets() {
        let reader: Reader = Arc::new(Probe { seen: Arc::new(Mutex::new(Vec::new())) });
        assert!(router_with_reader(reader.clone(), vec![target("acme.example.test", "acme")]).is_ok());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(
            reader.clone(),
            vec![target("acme.example.test", "acme"), target("acme.example.test", "acme")],
        )
        .is_err());
        assert!(router_with_reader(reader, vec![target("", "acme")]).is_err());
    }
}
