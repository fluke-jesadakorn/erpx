//! Owner-local axum ingress for the native routine-template list read.
//!
//! Composition mounts this router on the private service. A request is routed
//! only to the Postgres pool registered for its exact `(Host, holding_slug)`
//! pair; no TypeScript writer or fallback is involved.

use crate::application::routine_template_read::{
    TemplateRead, TemplateReadHandler, TemplateScope,
};
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

/// Registered Holding target. Both fields must be exact and distinct.
pub struct TemplateReadTarget {
    pub host: String,
    pub holding_slug: String,
}

struct TemplateReadState {
    targets: BTreeMap<(String, String), TemplateReadHandler>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_ROUTINE_READ_INVALID",
        "Use one bounded Operations routine read.",
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

fn configured(
    targets: Vec<(TemplateReadTarget, TemplateReadHandler)>,
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
            "/api/v1/holdings/{holding_slug}/routine-templates",
            get(list_templates),
        )
        .with_state(Arc::new(TemplateReadState { targets: configured })))
}

/// Build the native read router from the owner Postgres pool.
pub fn router(
    pool: DatabasePool,
    targets: Vec<TemplateReadTarget>,
) -> Result<Router, BusinessError> {
    router_with_scope(
        Arc::new(crate::adapters::native_routines_read::PgTemplateRead::new(pool)),
        Arc::new(
            crate::application::routine_template_read::TrustedTemplateScope,
        ),
        targets,
    )
}

/// One owner reader per Holding pool; a request can only reach the pool
/// registered for its exact target.
pub fn router_for_holdings(
    targets: Vec<(TemplateReadTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    configured(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    TemplateReadHandler::with_trusted_scope(Arc::new(
                        crate::adapters::native_routines_read::PgTemplateRead::new(pool),
                    )),
                )
            })
            .collect(),
    )
}

/// Reader/scope-injected variant for composition and tests.
pub fn router_with_scope(
    reader: Arc<dyn TemplateRead>,
    scope: Arc<dyn TemplateScope>,
    targets: Vec<TemplateReadTarget>,
) -> Result<Router, BusinessError> {
    configured(
        targets
            .into_iter()
            .map(|target| {
                (
                    target,
                    TemplateReadHandler::new(reader.clone(), scope.clone()),
                )
            })
            .collect(),
    )
}

async fn list_templates(
    State(state): State<Arc<TemplateReadState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers();
        if request.uri().query().is_some() {
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
        let rows = handler.list(&actor).await?;
        let value = serde_json::to_value(rows).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_ROUTINE_READ_UNAVAILABLE",
                "The native Operations routine read service is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(serde_json::json!({
            "data": value,
            "context": {
                "companyId": actor.company_id,
                "userId": actor.user_id,
                "assignmentId": actor.assignment_id,
                "revision": actor.revision,
                "assignmentVersion": actor.assignment_version,
            }
        }))
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

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let target = || TemplateReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        let handler = TemplateReadHandler::with_trusted_scope(Arc::new(Probe));
        assert!(configured(vec![(target(), handler)]).is_ok());
        let handler = TemplateReadHandler::with_trusted_scope(Arc::new(Probe));
        assert!(configured(vec![(target(), handler.clone()), (target(), handler)]).is_err());
        let handler = TemplateReadHandler::with_trusted_scope(Arc::new(Probe));
        assert!(configured(vec![(TemplateReadTarget {
            host: String::new(),
            holding_slug: "acme".into(),
        }, handler)])
        .is_err());
    }

    use crate::application::routine_template_read::{
        TemplateListFuture, TemplateScopeFragment,
    };
    use crate::domain::routine_template_read::TemplateRow;

    #[derive(Clone)]
    struct Probe;

    impl TemplateRead for Probe {
        fn list<'a>(
            &'a self,
            _company: &'a str,
            _user: &'a str,
            _assignment: &'a str,
            _scope: TemplateScopeFragment,
        ) -> TemplateListFuture<'a, Vec<TemplateRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }
}
