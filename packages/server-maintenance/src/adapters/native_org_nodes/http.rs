//! Owner-local axum router for the Maintenance organization-node option read.

use super::application;
use super::domain::{self, NodeScope};
use super::postgres::PgOrgNodeRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const WORKSPACE_BRANCH: HeaderName = HeaderName::from_static("x-erpx-workspace-branch-id");
const WORKSPACE_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-workspace-department-id");

pub struct OrgNodeTarget {
    pub host: String,
    pub holding_slug: String,
}

struct OrgNodeState {
    readers: BTreeMap<(String, String), Arc<dyn application::OrgNodeRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_ORG_NODES_REQUEST_INVALID",
        "Use one unambiguous Maintenance organization-node read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_ORG_NODES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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

fn optional_scope_id(value: Option<&str>) -> Result<Option<&str>, BusinessError> {
    match value {
        Some(value) if domain::valid_id(value) => Ok(Some(value)),
        Some(_) => Err(invalid()),
        None => Ok(None),
    }
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

impl OrgNodeState {
    pub async fn read(&self, headers: &HeaderMap, holding_slug: &str) -> Response {
        match self.read_inner(headers, holding_slug).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
    ) -> Result<serde_json::Value, BusinessError> {
        let host = unique_header(headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .readers
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| domain::valid_id(value))
            .ok_or_else(invalid)?;
        let branch = optional_scope_id(unique_header(headers, WORKSPACE_BRANCH)?)?;
        let department = optional_scope_id(unique_header(headers, WORKSPACE_DEPARTMENT)?)?;
        let scope = NodeScope::new(company, branch, department)?;
        application::handle(reader.as_ref(), scope).await
    }
}

fn state_with_readers(
    targets: Vec<(OrgNodeTarget, Arc<dyn application::OrgNodeRead>)>,
) -> Result<OrgNodeState, BusinessError> {
    let mut readers = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || readers
                .insert((target.host, target.holding_slug), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if readers.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(OrgNodeState { readers })
}

fn new_router(state: OrgNodeState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/maintenance/org-nodes",
            get(current),
        )
        .with_state(Arc::new(state))
}

/// Build the router with one injected reader shared by every target. Retained
/// for tests and single-pool deployments.
pub fn router_with_reader(
    reader: Arc<dyn application::OrgNodeRead>,
    targets: Vec<OrgNodeTarget>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(new_router(state))
}

/// Build the router with a single pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<OrgNodeTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgOrgNodeRead::new(pool)), targets)
}

/// Build the router with one distinct pool per registered Holding. The map is
/// keyed by `(host, holding_slug)`, so no cross-Holding pool reuse is possible.
pub fn router_for_holdings(
    targets: Vec<(OrgNodeTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgOrgNodeRead::new(pool)) as Arc<dyn application::OrgNodeRead>,
                )
            })
            .collect(),
    )?;
    Ok(new_router(state))
}

async fn current(
    State(state): State<Arc<OrgNodeState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    state.read(request.headers(), &slug).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fake;

    impl application::OrgNodeRead for Fake {
        fn nodes<'a>(
            &'a self,
            scope: &'a NodeScope,
        ) -> application::OrgNodeReadFuture<'a, Vec<domain::OrgNode>> {
            let company = scope.company_id.clone();
            let branch = scope.branch_id.clone();
            let department = scope.department_id.clone();
            Box::pin(async move {
                Ok(vec![domain::OrgNode {
                    id: "node-1".into(),
                    kind: "branch".into(),
                    display_name: format!("{company}/{branch:?}/{department:?}"),
                    branch_id: None,
                }])
            })
        }
    }

    fn target() -> OrgNodeTarget {
        OrgNodeTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }
    }

    fn state() -> OrgNodeState {
        state_with_readers(vec![(
            target(),
            Arc::new(Fake) as Arc<dyn application::OrgNodeRead>,
        )])
        .unwrap()
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::HOST,
            HeaderValue::from_static("acme.jesadakorn.com"),
        );
        headers.insert(
            CONTEXT_COMPANY,
            HeaderValue::from_static("company-acme"),
        );
        headers
    }

    #[tokio::test]
    async fn returns_complete_data_envelope_with_security_headers() {
        let response = state().read(&headers(), "acme").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            usize::MAX,
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            json!({"data":[{"id":"node-1","kind":"branch","display_name":"company-acme/None/None","branch_id":null}]})
        );
    }

    #[tokio::test]
    async fn binds_optional_branch_and_department_scope() {
        let mut headers = headers();
        headers.insert(WORKSPACE_BRANCH, HeaderValue::from_static("branch-2"));
        headers.insert(WORKSPACE_DEPARTMENT, HeaderValue::from_static("dept-3"));
        let response = state().read(&headers, "acme").await;
        let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
            response.into_body(),
            usize::MAX,
        )
        .await
        .unwrap();
        let body = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        assert_eq!(
            body["data"][0]["display_name"],
            json!("company-acme/Some(\"branch-2\")/Some(\"dept-3\")")
        );
    }

    #[tokio::test]
    async fn rejects_wrong_host_missing_company_and_unbounded_scope() {
        let state = state();
        let mut wrong = headers();
        wrong.insert(header::HOST, HeaderValue::from_static("other.example"));
        assert_eq!(state.read(&wrong, "acme").await.status(), StatusCode::NOT_FOUND);
        let mut absent = headers();
        absent.remove(CONTEXT_COMPANY);
        assert_eq!(
            state.read(&absent, "acme").await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut unbounded = headers();
        unbounded.insert(WORKSPACE_BRANCH, HeaderValue::from_static("bad/branch"));
        assert_eq!(
            state.read(&unbounded, "acme").await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut duplicate = headers();
        duplicate.append(CONTEXT_COMPANY, HeaderValue::from_static("company-acme"));
        assert_eq!(
            state.read(&duplicate, "acme").await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn configuration_rejects_empty_and_duplicate_targets() {
        assert!(state_with_readers(Vec::new()).is_err());
        assert!(state_with_readers(vec![
            (target(), Arc::new(Fake) as Arc<dyn application::OrgNodeRead>),
            (target(), Arc::new(Fake) as Arc<dyn application::OrgNodeRead>),
        ])
        .is_err());
        assert!(router_with_reader(Arc::new(Fake), Vec::new()).is_err());
    }
}
