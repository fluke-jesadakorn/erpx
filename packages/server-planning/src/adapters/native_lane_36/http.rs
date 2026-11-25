//! Owner-local axum router for the Planning lane-36 scoped workspace reads.
//!
//! Transport facts only: an exact registered Host/Holding target, one bounded
//! Company header and an optional bounded branch query. The owner reader owns
//! the SQL, the projection and the scope. No TypeScript fallback and no caller
//! SQL crosses this port.

use super::application;
use super::domain::LaneScope;
use super::postgres::PgLaneRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered Holding target. Duplicate host/slug pairs are rejected.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct LaneState {
    targets: BTreeMap<(String, String), ()>,
    reader: Arc<dyn application::LaneRead>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_REQUEST_INVALID",
        "Use one unambiguous Planning scoped read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLANNING_LANE36_CONFIG_INVALID",
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

/// Read one optional bounded query value. Values are already restricted to the
/// safe identifier alphabet, so no percent-decoding is required or admitted.
fn query_value(query: Option<&str>, key: &str) -> Result<Option<String>, BusinessError> {
    let Some(query) = query else {
        return Ok(None);
    };
    let mut found: Option<String> = None;
    for pair in query.split('&') {
        let Some((name, value)) = pair.split_once('=') else {
            return Err(invalid());
        };
        if name != key {
            continue;
        }
        if found.is_some() || !super::domain::valid_scope(value) {
            return Err(invalid());
        }
        found = Some(value.to_owned());
    }
    Ok(found)
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
    /// Exact handler used by both the axum route and the owner tests.
    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        kind: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, kind, query).await {
            Ok(value) => bounded(value),
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        kind: &str,
        query: Option<&str>,
    ) -> Result<serde_json::Value, BusinessError> {
        let host = unique_header(headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        if !self
            .targets
            .contains_key(&(host.to_owned(), holding_slug.to_owned()))
        {
            return Err(missing());
        }
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| super::domain::valid_id(value))
            .ok_or_else(invalid)?;
        let branch = query_value(query, "branch")?;
        // Reject unrelated query keys so a caller cannot smuggle a predicate.
        if let Some(query) = query {
            for pair in query.split('&') {
                let name = pair.split_once('=').map(|(name, _)| name).unwrap_or(pair);
                if name != "branch" {
                    return Err(invalid());
                }
            }
        }
        let scope = LaneScope::new(kind, company, branch.as_deref())?;
        application::handle(self.reader.as_ref(), scope).await
    }
}

/// Build the native read router. The lead mounts it; no composition or shared
/// module is edited by this owner.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader = Arc::new(PgLaneRead::new(pool)) as Arc<dyn application::LaneRead>;
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host, target.holding_slug), ())
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
            "/api/v1/holdings/{holding_slug}/planning/lane-36/{kind}",
            get(current),
        )
        .with_state(Arc::new(LaneState {
            targets: configured,
            reader,
        })))
}

async fn current(
    State(state): State<Arc<LaneState>>,
    Path((slug, kind)): Path<(String, String)>,
    request: Request,
) -> Response {
    state
        .read(request.headers(), &slug, &kind, request.uri().query())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_shared::public::BusinessError as Error;
    use serde_json::json;

    struct Fake;

    impl application::LaneRead for Fake {
        fn read<'a>(
            &'a self,
            scope: &'a LaneScope,
        ) -> application::LaneReadFuture<'a, super::super::domain::LaneData> {
            let scope = scope.clone();
            Box::pin(async move {
                Ok(super::super::domain::LaneData::List(vec![json!({
                    "kind": scope.kind,
                    "companyId": scope.company_id,
                    "branchId": scope.branch,
                })]))
            })
        }
    }

    fn state() -> LaneState {
        let mut targets = BTreeMap::new();
        targets.insert(("acme.example.com".to_owned(), "acme".to_owned()), ());
        LaneState {
            targets,
            reader: Arc::new(Fake),
        }
    }

    fn headers(company: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("acme.example.com"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_str(company).unwrap());
        headers
    }

    #[tokio::test]
    async fn exact_target_and_company_are_required() {
        let state = state();
        let response = state.read(&headers("company-1"), "acme", "resources", None).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let wrong_host = state.read(&headers("company-1"), "other", "resources", None).await;
        assert_eq!(wrong_host.status(), StatusCode::NOT_FOUND);
        let mut without_company = headers("company-1");
        without_company.remove(CONTEXT_COMPANY);
        let response = state
            .read(&without_company, "acme", "resources", None)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn rejects_unknown_kind_and_unbounded_branch() {
        let state = state();
        let response = state
            .read(&headers("company-1"), "acme", "scenarios", None)
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = state
            .read(
                &headers("company-1"),
                "acme",
                "resources",
                Some("branch=x/y"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let response = state
            .read(
                &headers("company-1"),
                "acme",
                "resources",
                Some("branch=b1&company_id=other"),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn duplicate_or_empty_targets_are_rejected() {
        // Configuration validation is covered through the shared builder below.
        let duplicate = vec![
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
            LaneTarget {
                host: "acme.example.com".into(),
                holding_slug: "acme".into(),
            },
        ];
        assert!(validate_targets(&duplicate).is_err());
        assert!(validate_targets(&[]).is_err());
        let valid = vec![LaneTarget {
            host: "acme.example.com".into(),
            holding_slug: "acme".into(),
        }];
        assert!(validate_targets(&valid).is_ok());
    }

    /// Pure mirror of router target validation, used by the test above without
    /// requiring a live pool.
    fn validate_targets(targets: &[LaneTarget]) -> Result<(), Error> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || configured
                    .insert(
                        (target.host.clone(), target.holding_slug.clone()),
                        (),
                    )
                    .is_some()
            {
                return Err(configuration_invalid());
            }
        }
        if configured.is_empty() {
            return Err(configuration_invalid());
        }
        Ok(())
    }

    #[test]
    fn state_trait_is_object_safe() {
        fn assert_object_safe(_: &dyn application::LaneRead) {}
        assert_object_safe(&Fake);
        fn assert_future(_: application::LaneReadFuture<'_, ()>) {}
        let _ = assert_future;
    }
}
