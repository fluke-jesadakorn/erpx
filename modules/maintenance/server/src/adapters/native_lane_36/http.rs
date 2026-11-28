//! Owner-local axum router for the Maintenance lane-36 descriptor read.

use super::application;
use super::domain::{self, DescriptorScope};
use super::postgres::PgDescriptorRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    /// One reader per registered (host, holding_slug). `router_for_holdings`
    /// gives each Holding its own DatabasePool; `router_with_reader` shares a
    /// single injected reader for tests and single-pool deployments.
    readers: BTreeMap<(String, String), Arc<dyn application::DescriptorRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_LANE36_REQUEST_INVALID",
        "Use one unambiguous Maintenance descriptor read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_LANE36_CONFIG_INVALID",
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

fn target_id(query: Option<&str>) -> Result<String, BusinessError> {
    let mut found: Option<String> = None;
    if let Some(query) = query {
        for pair in query.split('&') {
            let Some((name, value)) = pair.split_once('=') else {
                return Err(invalid());
            };
            if name != "id" || found.is_some() || !domain::valid_id(value) {
                return Err(invalid());
            }
            found = Some(value.to_owned());
        }
    }
    found.ok_or_else(invalid)
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

impl LaneState {
    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
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
        let id = target_id(query)?;
        let scope = DescriptorScope::new(domain::TARGET_MAINTENANCE_WORK, &id, company)?;
        application::handle(reader.as_ref(), scope).await
    }
}

/// Insert one reader per exact registered Holding target. Duplicate host/slug
/// pairs and empty configurations are rejected before any state is built.
fn state_with_readers(
    targets: Vec<(LaneTarget, Arc<dyn application::DescriptorRead>)>,
) -> Result<LaneState, BusinessError> {
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
    Ok(LaneState { readers })
}

fn new_router(state: LaneState) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/maintenance/lane-36/attachment-target",
            get(current),
        )
        .with_state(Arc::new(state))
}

/// Build the descriptor router with one injected reader shared by every
/// target. Retained for tests and single-pool deployments.
pub fn router_with_reader(
    reader: Arc<dyn application::DescriptorRead>,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )?;
    Ok(new_router(state))
}

/// Build the descriptor router with a single pool shared by all targets.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgDescriptorRead::new(pool)), targets)
}

/// Build the descriptor router with one distinct `PgDescriptorRead`, and
/// therefore one distinct `DatabasePool`, per registered Holding. Composition
/// pairs each entry of `config.holdings` with its own pool; no cross-Holding
/// pool reuse is possible because the map is keyed by `(host, holding_slug)`.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let state = state_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgDescriptorRead::new(pool)) as Arc<dyn application::DescriptorRead>,
                )
            })
            .collect(),
    )?;
    Ok(new_router(state))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    state.read(request.headers(), &slug, request.uri().query()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fake;

    impl application::DescriptorRead for Fake {
        fn descriptor<'a>(
            &'a self,
            scope: &'a DescriptorScope,
        ) -> application::DescriptorReadFuture<'a, Option<serde_json::Value>> {
            let scope = scope.clone();
            Box::pin(async move {
                Ok(Some(json!({
                    "company_id": scope.company_id,
                    "branch_id": null,
                    "department_id": null,
                    "version": 1,
                })))
            })
        }
    }

    fn state() -> LaneState {
        state_with_readers(vec![(
            LaneTarget {
                host: "acme.example.com".to_owned(),
                holding_slug: "acme".to_owned(),
            },
            Arc::new(Fake) as Arc<dyn application::DescriptorRead>,
        )])
        .unwrap()
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.com"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers
    }

    #[tokio::test]
    async fn exact_target_and_company_are_required() {
        let state = state();
        let response = state.read(&headers(), "acme", Some("id=work-1")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        assert_eq!(
            state.read(&headers(), "other", Some("id=work-1")).await.status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            state.read(&headers(), "acme", None).await.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn rejects_unbounded_target_id() {
        let state = state();
        assert_eq!(
            state
                .read(&headers(), "acme", Some("id=x/y"))
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    /// Distinct injected readers stand in for distinct per-Holding pools:
    /// `router_for_holdings` builds one `PgDescriptorRead` per entry through
    /// the same state map and lookup used by this test.
    struct Marked(&'static str);

    impl application::DescriptorRead for Marked {
        fn descriptor<'a>(
            &'a self,
            _scope: &'a DescriptorScope,
        ) -> application::DescriptorReadFuture<'a, Option<serde_json::Value>> {
            let marker = self.0;
            Box::pin(async move {
                Ok(Some(json!({
                    "company_id": marker,
                    "branch_id": null,
                    "department_id": null,
                    "version": 1,
                })))
            })
        }
    }

    async fn injected_marker(state: &LaneState, headers: &HeaderMap, slug: &str) -> String {
        let response = state.read(headers, slug, Some("id=work-1")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = erpx_platform_runtime_server::public::http_support::body::to_bytes(
            response.into_body(),
            usize::MAX,
        )
        .await
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["data"]["company_id"].as_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn two_holdings_route_to_two_distinct_readers() {
        let state = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.com".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                Arc::new(Marked("acme-pool")) as Arc<dyn application::DescriptorRead>,
            ),
            (
                LaneTarget {
                    host: "north.example.com".to_owned(),
                    holding_slug: "north".to_owned(),
                },
                Arc::new(Marked("north-pool")) as Arc<dyn application::DescriptorRead>,
            ),
        ])
        .unwrap();
        assert_eq!(state.readers.len(), 2);

        let acme = headers();
        let mut north = headers();
        north.insert(
            header::HOST,
            HeaderValue::from_static("north.example.com"),
        );
        assert_eq!(injected_marker(&state, &acme, "acme").await, "acme-pool");
        assert_eq!(injected_marker(&state, &north, "north").await, "north-pool");
        assert_eq!(
            state
                .read(&north, "acme", Some("id=work-1"))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
}
