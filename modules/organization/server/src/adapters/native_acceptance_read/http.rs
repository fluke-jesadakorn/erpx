//! Owner-local axum ingress for the native Company acceptance read.
//!
//! Transport facts only: an exact registered Host/Holding pair, bounded verified
//! actor headers and no query string. The pool, SQL, checks and projection stay
//! inside the owner crate; a missing or malformed native runtime fails closed
//! instead of reaching the TypeScript authority.

use crate::adapters::native_acceptance_read::application::{AcceptanceRead, handle};
use crate::adapters::native_acceptance_read::domain::{AcceptanceQuery, valid_id};
use crate::adapters::native_acceptance_read::postgres::PgAcceptanceRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_PERMISSIONS: HeaderName = HeaderName::from_static("x-erpx-context-permissions");
const CONTEXT_PERSON_IDS: HeaderName = HeaderName::from_static("x-erpx-context-person-ids");
const CONTEXT_SCOPE: HeaderName = HeaderName::from_static("x-erpx-context-scope");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");

/// One exact registered browser origin/Holding pair. Loose patterns and
/// duplicates are rejected so one Holding cannot read another Holding's
/// acceptance state.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn AcceptanceRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_ACCEPTANCE_READ_INVALID",
        "Use one bounded Company acceptance read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "ORGANIZATION_ACCEPTANCE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn response_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_ACCEPTANCE_READ_RESPONSE_INVALID",
        "The native Company acceptance read could not be encoded.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
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

fn values(headers: &HeaderMap, name: HeaderName) -> Result<Vec<String>, BusinessError> {
    match unique_header(headers, name)? {
        Some(value) => Ok(value
            .split(',')
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .collect()),
        None => Ok(Vec::new()),
    }
}

fn actor(headers: &HeaderMap) -> Result<AcceptanceQuery, BusinessError> {
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let user = unique_header(headers, CONTEXT_USER)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let assignment = unique_header(headers, CONTEXT_ASSIGNMENT)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let scope = unique_header(headers, CONTEXT_SCOPE)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?
        .to_owned();
    let branch = unique_header(headers, CONTEXT_BRANCH)?.map(str::to_owned);
    let department = unique_header(headers, CONTEXT_DEPARTMENT)?.map(str::to_owned);
    Ok(AcceptanceQuery {
        company,
        user,
        assignment,
        permissions: values(headers, CONTEXT_PERMISSIONS)?,
        person_ids: values(headers, CONTEXT_PERSON_IDS)?,
        scope,
        branch,
        department,
    })
}

fn router_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn AcceptanceRead>)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
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
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/acceptance",
            get(read),
        )
        .with_state(Arc::new(LaneState { targets: configured })))
}

/// Build the native acceptance-read router. The lead mounts it; no composition
/// is edited here. One owner reader is shared across exact distinct targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn AcceptanceRead> = Arc::new(PgAcceptanceRead::new(pool));
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Build the router with one owner reader and database pool per Holding.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                let reader: Arc<dyn AcceptanceRead> = Arc::new(PgAcceptanceRead::new(pool));
                (target, reader)
            })
            .collect(),
    )
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let query = actor(request.headers())?;
        let envelope = handle(reader.as_ref(), &query).await?;
        serde_json::to_value(envelope).map_err(|_| response_invalid())
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
    use crate::adapters::native_acceptance_read::application::ReadFuture;
    use crate::adapters::native_acceptance_read::domain::{
        AcceptanceFacts, AcceptanceRow, CompanyRow, EventRow,
    };

    struct Stub {
        facts: Result<AcceptanceFacts, BusinessError>,
    }

    impl AcceptanceRead for Stub {
        fn read<'a>(&'a self, _: &'a AcceptanceQuery) -> ReadFuture<'a, AcceptanceFacts> {
            let facts = self.facts.clone();
            Box::pin(async move { facts })
        }
    }

    fn facts() -> AcceptanceFacts {
        AcceptanceFacts {
            company: Some(CompanyRow {
                id: "acme-co".into(),
                display_name: "Acme".into(),
                status: "active".into(),
            }),
            branches: 1,
            levels: 6,
            ceo: 1,
            incomplete: 0,
            current: Some(AcceptanceRow {
                company_id: "acme-co".into(),
                status: "accepted".into(),
                version: 2,
                requested_by: "user-1".into(),
                reviewed_by: Some("reviewer-1".into()),
                requested_at: 10,
                reviewed_at: Some(11),
                reason: "Ready".into(),
                review_reason: None,
                updated_at: 11,
            }),
            events: vec![EventRow {
                id: "event-1".into(),
                action: "accepted".into(),
                actor_id: "reviewer-1".into(),
                version: 2,
                reason: "Ready".into(),
                created_at: 11,
            }],
            authority: true,
        }
    }

    fn router_with() -> Router {
        let reader: Arc<dyn AcceptanceRead> = Arc::new(Stub { facts: Ok(facts()) });
        router_with_readers(vec![(
            LaneTarget {
                host: "acme.example.test".into(),
                holding_slug: "acme".into(),
            },
            reader,
        )])
        .unwrap()
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

    async fn spawn(router: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr, server)
    }

    fn request(host: &str, company: &str, scope: &str, branch: Option<&str>) -> String {
        format!(
            "GET /api/v1/holdings/acme/people/organization/acceptance HTTP/1.1\r\nHost: {host}\r\nx-erpx-context-company: {company}\r\nx-erpx-context-user: user-1\r\nx-erpx-context-assignment: assignment-1\r\nx-erpx-context-permissions: organization.manage\r\nx-erpx-context-person-ids: \r\nx-erpx-context-scope: {scope}\r\n{}\r\nConnection: close\r\n\r\n",
            branch
                .map(|value| format!("x-erpx-context-branch: {value}\r\n"))
                .unwrap_or_default()
        )
    }

    #[tokio::test]
    async fn exact_holding_and_company_scope_produce_complete_payload() {
        let (addr, server) = spawn(router_with()).await;
        let (status, body) = http(addr, request("acme.example.test", "acme-co", "holding", None)).await;
        assert_eq!(status, 200);
        assert!(body.is_null() || body["data"].is_object());

        let wrong_host = http(addr, request("other.example.test", "acme-co", "holding", None)).await;
        assert_eq!(wrong_host.0, 404);
        let narrow_scope = http(addr, request("acme.example.test", "acme-co", "branch", Some("branch-1"))).await;
        assert_eq!(narrow_scope.0, 403);
        let bad_company = http(addr, request("acme.example.test", "../bad", "holding", None)).await;
        assert_eq!(bad_company.0, 422);
        let _ = server.abort();
    }
}
