//! Owner-local HTTP ingress for the native Projects handoff-view read.
//!
//! Transport facts only: exact Host/Holding target, bounded Company and user
//! context headers, project identity and query rejection. Account/session
//! resolution, purchase admission, the cost capability and the same-person
//! predicate are composition responsibilities supplied through the backend
//! trait. No TypeScript prepares or executes the owner SQL.

use crate::application::project_handoffs::HandoffViewActor;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

pub type HandoffViewHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct HandoffViewHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub company: &'a str,
    pub user: &'a str,
    pub project_id: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct HandoffViewHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl HandoffViewHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait HandoffViewHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        actor: &'a HandoffViewActor,
        request: HandoffViewHttpRequest<'a>,
    ) -> HandoffViewHttpFuture<'a, HandoffViewHttpResponse>;
}

pub struct HandoffViewHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub actor: HandoffViewActor,
    pub backend: Arc<dyn HandoffViewHttpBackend>,
}

struct HandoffViewHttpState {
    targets: BTreeMap<(String, String), HandoffViewHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Projects handoff request.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

pub fn handoff_views_router(
    targets: Vec<HandoffViewHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/handoffs",
            get(read_views),
        )
        .with_state(Arc::new(HandoffViewHttpState {
            targets: configured,
        })))
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn required_context(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    let value = unique_header(headers, name)?
        .filter(|value| RecordId::new(*value).is_ok())
        .ok_or_else(request_invalid)?;
    Ok(value.to_owned())
}

fn target<'a>(
    state: &'a HandoffViewHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a HandoffViewHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_views(
    State(state): State<Arc<HandoffViewHttpState>>,
    Path((slug, project_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        let company = required_context(request.headers(), CONTEXT_COMPANY)?;
        let user = required_context(request.headers(), CONTEXT_USER)?;
        let project_id = RecordId::new(project_id).map_err(|_| request_invalid())?;
        target
            .backend
            .read(
                &target.actor,
                HandoffViewHttpRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    company: &company,
                    user: &user,
                    project_id: project_id.as_str(),
                    headers: request.headers(),
                },
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_targets_are_refused() {
        let target = || HandoffViewHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            actor: HandoffViewActor {
                can_read_costs: false,
                same_person: Arc::new(|_| false),
            },
            backend: Arc::new(Empty) as Arc<dyn HandoffViewHttpBackend>,
        };
        assert_eq!(
            handoff_views_router(vec![target(), target()])
                .err()
                .unwrap()
                .code,
            "PROJECTS_CONFIGURATION_INVALID"
        );
    }

    struct Empty;
    impl HandoffViewHttpBackend for Empty {
        fn read<'a>(
            &'a self,
            _: &'a HandoffViewActor,
            _: HandoffViewHttpRequest<'a>,
        ) -> HandoffViewHttpFuture<'a, HandoffViewHttpResponse> {
            Box::pin(async {
                Ok(HandoffViewHttpResponse {
                    data: serde_json::json!([]),
                    context: serde_json::json!({}),
                })
            })
        }
    }
}
