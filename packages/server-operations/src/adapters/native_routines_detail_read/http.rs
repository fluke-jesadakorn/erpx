//! Owner-local axum ingress for the native routine-template detail read.
//!
//! Composition mounts this router on the private service. A request is routed
//! only to the Postgres pool registered for its exact `(Host, holding_slug)`
//! pair; the retained TypeScript `getTemplate` SQL is never consulted.

use crate::application::routine_template_detail_read::TemplateDetailHandler;
use crate::domain::routine_template_read::TemplateActor;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const CONTEXT_ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const CONTEXT_REVISION: HeaderName = HeaderName::from_static("x-erpx-context-revision");
const CONTEXT_ASSIGNMENT_VERSION: HeaderName =
    HeaderName::from_static("x-erpx-context-assignment-version");
const API_TRANSPORT: HeaderName = HeaderName::from_static("x-erpx-api-transport");

/// Registered Holding target. Both fields must be exact and distinct.
pub struct TemplateDetailTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TemplateDetailState {
    targets: BTreeMap<(String, String), TemplateDetailHandler>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn missing_template() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "TEMPLATE_NOT_FOUND",
        "Routine template not found.",
    )
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_ROUTINE_DETAIL_INVALID",
        "Use one bounded Operations routine detail read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_ROUTINE_CONFIG_INVALID",
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
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

/// The gateway injects one trusted public marker after stripping every
/// browser-supplied `x-erpx-*` header. Duplicates remain invalid.
fn public_transport(headers: &HeaderMap) -> Result<bool, BusinessError> {
    Ok(unique_header(headers, API_TRANSPORT)?
        .filter(|value| *value == "public")
        .is_some())
}

fn required(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    unique_header(headers, name)?
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(invalid)
}

fn counter(headers: &HeaderMap, name: HeaderName) -> Result<i64, BusinessError> {
    let value = required(headers, name)?;
    if value.len() > 18 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    value.parse::<i64>().map_err(|_| invalid())
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

/// The public browser route keeps the former Next `{data}` envelope; the
/// private SSR transport still receives the full context-bearing wire.
fn response_body(
    public: bool,
    value: serde_json::Value,
    actor: &TemplateActor,
) -> serde_json::Value {
    if public {
        serde_json::json!({"data": value})
    } else {
        serde_json::json!({
            "data": value,
            "context": {
                "companyId": &actor.company_id,
                "userId": &actor.user_id,
                "assignmentId": &actor.assignment_id,
                "revision": actor.revision,
                "assignmentVersion": actor.assignment_version,
            }
        })
    }
}

fn configured(
    targets: Vec<(TemplateDetailTarget, TemplateDetailHandler)>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, handler) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), handler)
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
            "/api/v1/holdings/{holding_slug}/routine-templates/{id}",
            get(template_detail),
        )
        .with_state(Arc::new(TemplateDetailState { targets: configured })))
}

/// Build the native detail router from the owner Postgres pool.
pub fn router(pool: DatabasePool, targets: Vec<TemplateDetailTarget>) -> Result<Router, BusinessError> {
    router_with_scope(
        Arc::new(crate::adapters::native_routines_detail_read::PgTemplateDetailRead::new(pool)),
        Arc::new(crate::application::routine_template_read::TrustedTemplateScope),
        targets,
    )
}

/// One owner reader per Holding pool; a request can only reach the pool
/// registered for its exact target.
pub fn router_for_holdings(
    targets: Vec<(TemplateDetailTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    configured(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    TemplateDetailHandler::with_trusted_scope(Arc::new(
                        crate::adapters::native_routines_detail_read::PgTemplateDetailRead::new(pool),
                    )),
                )
            })
            .collect(),
    )
}

/// Reader/scope-injected variant for composition and tests.
pub fn router_with_scope(
    reader: Arc<dyn crate::application::routine_template_detail_read::TemplateDetailRead>,
    scope: Arc<dyn crate::application::routine_template_read::TemplateScope>,
    targets: Vec<TemplateDetailTarget>,
) -> Result<Router, BusinessError> {
    configured(
        targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    TemplateDetailHandler::new(reader.clone(), scope.clone()),
                )
            })
            .collect(),
    )
}

async fn template_detail(
    State(state): State<Arc<TemplateDetailState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers();
        let public = public_transport(headers)?;
        if !public && request.uri().query().is_some() {
            return Err(invalid());
        }
        let host = unique_header(headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let handler = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let actor = TemplateActor {
            company_id: required(headers, CONTEXT_COMPANY)?,
            user_id: required(headers, CONTEXT_USER)?,
            assignment_id: required(headers, CONTEXT_ASSIGNMENT)?,
            revision: counter(headers, CONTEXT_REVISION)?,
            assignment_version: counter(headers, CONTEXT_ASSIGNMENT_VERSION)?,
        };
        let row = handler.detail(&actor, &id).await?;
        if public && row.is_none() {
            return Err(missing_template());
        }
        let value = serde_json::to_value(row).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_ROUTINE_READ_UNAVAILABLE",
                "The native Operations routine read service is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(response_body(public, value, &actor))
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
    use crate::application::routine_template_detail_read::{
        TemplateDetailFuture, TemplateDetailRead,
    };
    use crate::application::routine_template_read::TemplateScopeFragment;
    use crate::domain::routine_template_read::TemplateRow;

    #[derive(Clone)]
    struct Probe;

    impl TemplateDetailRead for Probe {
        fn detail<'a>(
            &'a self,
            company: &'a str,
            id: &'a str,
            _scope: TemplateScopeFragment,
        ) -> TemplateDetailFuture<'a, Option<TemplateRow>> {
            let id = id.to_owned();
            let company = company.to_owned();
            Box::pin(async move {
                Ok(Some(TemplateRow {
                    id,
                    title: "Weekly close".into(),
                    description: String::new(),
                    company_id: company,
                    branch_id: None,
                    department_id: None,
                    creator_user_id: "u-1".into(),
                    assignee_user_id: "u-2".into(),
                    project_id: None,
                    start_approvers_json: "[]".into(),
                    completion_approvers_json: "[]".into(),
                    published: 1,
                    version: 2,
                    created_at: 100,
                    updated_at: 200,
                }))
            })
        }
    }

    fn target() -> TemplateDetailTarget {
        TemplateDetailTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    fn handler() -> TemplateDetailHandler {
        TemplateDetailHandler::with_trusted_scope(Arc::new(Probe))
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        assert!(configured(vec![(target(), handler())]).is_ok());
        assert!(configured(vec![(target(), handler()), (target(), handler())]).is_err());
        assert!(configured(vec![(
            TemplateDetailTarget {
                host: String::new(),
                holding_slug: "acme".into(),
            },
            handler(),
        )])
        .is_err());
    }

    #[test]
    fn detail_route_is_registered_with_a_bounded_path() {
        let router = configured(vec![(target(), handler())]).unwrap();
        let _ = router;
        assert!(valid_slug("acme"));
        assert!(!valid_slug("Acme"));
    }

    #[test]
    fn public_marker_must_be_single_and_exact() {
        let mut headers = HeaderMap::new();
        assert!(!public_transport(&headers).expect("absent marker"));
        headers.insert(API_TRANSPORT, HeaderValue::from_static("public"));
        assert!(public_transport(&headers).expect("public marker"));
        headers.append(API_TRANSPORT, HeaderValue::from_static("public"));
        assert_eq!(public_transport(&headers), Err(invalid()));
        let mut headers = HeaderMap::new();
        headers.insert(API_TRANSPORT, HeaderValue::from_static("private"));
        assert!(!public_transport(&headers).expect("other marker"));
    }

    #[test]
    fn public_missing_template_keeps_next_404() {
        let error = missing_template();
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(error.code, "TEMPLATE_NOT_FOUND");
        assert_eq!(error.message, "Routine template not found.");
    }

    #[test]
    fn public_envelope_omits_context() {
        let actor = TemplateActor {
            company_id: "acme:001".into(),
            user_id: "u-1".into(),
            assignment_id: "a-1".into(),
            revision: 7,
            assignment_version: 2,
        };
        let data = serde_json::json!({"id":"t-1"});
        assert_eq!(
            response_body(true, data.clone(), &actor),
            serde_json::json!({"data": data})
        );
        let private = response_body(false, data, &actor);
        assert_eq!(private["context"]["companyId"], "acme:001");
        assert_eq!(private["context"]["revision"], 7);
    }
}
