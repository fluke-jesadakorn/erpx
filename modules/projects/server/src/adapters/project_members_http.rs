//! Owner-local HTTP ingress for the native Projects member-roster read.
//!
//! Transport facts only: exact Host/Holding target, bounded Company and user
//! context headers, project identity and query rejection. Account/session
//! resolution, selected-context checks and the project read policy are
//! composition responsibilities supplied through the backend trait. The SQL
//! is executed by this crate's sqlx adapter; no TypeScript prepares or executes
//! it and no TS reader fallback exists in this slice.

use crate::domain::project_members::{ProjectMemberRow, ProjectMemberScope};
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

pub type ProjectMemberHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ProjectMemberHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub company: &'a str,
    pub user: &'a str,
    pub project_id: &'a str,
    pub scope: &'a ProjectMemberScope,
    pub headers: &'a HeaderMap,
}

pub trait ProjectMemberHttpBackend: Send + Sync {
    /// Resolve the session and read policy, then run the owner query through
    /// `PgProjectMembers`. Return the complete retained member roster.
    fn read<'a>(
        &'a self,
        request: ProjectMemberHttpRequest<'a>,
    ) -> ProjectMemberHttpFuture<'a, Vec<ProjectMemberRow>>;
}

pub struct ProjectMemberHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn ProjectMemberHttpBackend>,
}

struct ProjectMemberHttpState {
    targets: BTreeMap<(String, String), ProjectMemberHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROJECTS_REQUEST_INVALID",
        "Use one unambiguous Projects member request.",
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

/// `GET /api/v1/holdings/{holding_slug}/projects/{project_id}/members`.
pub fn project_members_router(
    targets: Vec<ProjectMemberHttpTarget>,
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
            "/api/v1/holdings/{holding_slug}/projects/{project_id}/members",
            get(read_members),
        )
        .with_state(Arc::new(ProjectMemberHttpState {
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

fn bounded(members: Vec<ProjectMemberRow>) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"data": members})),
    )
        .into_response();
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

fn unique_header<'a>(
    headers: &'a HeaderMap,
    name: HeaderName,
) -> Result<Option<&'a str>, BusinessError> {
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
    state: &'a ProjectMemberHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a ProjectMemberHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_members(
    State(state): State<Arc<ProjectMemberHttpState>>,
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
        let scope = ProjectMemberScope::new(&company, &project_id).map_err(|_| request_invalid())?;
        target
            .backend
            .read(ProjectMemberHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                company: &company,
                user: &user,
                project_id: scope.project_id.as_str(),
                scope: &scope,
                headers: request.headers(),
            })
            .await
    }
    .await;
    match result {
        Ok(members) => bounded(members),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Empty;
    impl ProjectMemberHttpBackend for Empty {
        fn read<'a>(
            &'a self,
            _: ProjectMemberHttpRequest<'a>,
        ) -> ProjectMemberHttpFuture<'a, Vec<ProjectMemberRow>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    fn target() -> ProjectMemberHttpTarget {
        ProjectMemberHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.jesadakorn.com".into(),
            backend: Arc::new(Empty) as Arc<dyn ProjectMemberHttpBackend>,
        }
    }

    #[test]
    fn duplicate_targets_are_refused() {
        assert_eq!(
            project_members_router(vec![target(), target()])
                .err()
                .unwrap()
                .code,
            "PROJECTS_CONFIGURATION_INVALID"
        );
    }

    #[test]
    fn only_exact_https_origins_are_registered() {
        assert!(valid_origin("https://acme.jesadakorn.com"));
        assert!(valid_origin("https://acme.jesadakorn.com:8443"));
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/path"));
        assert!(!valid_origin("https://acme.jesadakorn.com:0"));
    }

    #[test]
    fn duplicate_company_context_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company"));
        assert_eq!(
            required_context(&headers, CONTEXT_COMPANY)
                .unwrap_err()
                .code,
            "PROJECTS_REQUEST_INVALID"
        );
    }

    #[test]
    fn empty_roster_has_exact_data_envelope() {
        let response = bounded(Vec::new());
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .unwrap()
                .to_str()
                .unwrap(),
            "private, no-store"
        );
    }
}
