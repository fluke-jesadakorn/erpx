//! Owner-local HTTP ingress for the bounded Organization access projection.
//!
//! Transport facts only: registered Host/Holding targets, exact origin,
//! bounded read and business-error mapping. The normal Account session,
//! current Holding authority and serializable owner unit are supplied by the
//! composition backend through `OrganizationAccessHttpBackend`. This module
//! never mints a session, actor or Holding scope.
use erpx_server_runtime::public::http_support::{
    get, header, HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{json, Value};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type OrganizationAccessHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, BusinessError>> + Send + 'a>>;

pub struct OrganizationAccessHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

/// Composition-owned Account session, current IAM authority and Company unit.
pub trait OrganizationAccessHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: OrganizationAccessHttpRequest<'a>,
    ) -> OrganizationAccessHttpFuture<'a>;
}

pub struct OrganizationAccessHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn OrganizationAccessHttpBackend>,
}

pub struct OrganizationAccessHttpState {
    targets: BTreeMap<(String, String), OrganizationAccessHttpTarget>,
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Provide a complete organization access request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
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
                (1..=5).contains(&port.len()) && port.bytes().all(|byte| byte.is_ascii_digit())
            })
    })
}

fn secured(mut response: Response) -> Response {
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

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

impl OrganizationAccessHttpState {
    pub fn new(
        targets: Vec<OrganizationAccessHttpTarget>,
    ) -> Result<OrganizationAccessHttpState, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.host.len() > 253
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "ORGANIZATION_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<(&'a OrganizationAccessHttpTarget, &'a str), BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok((target, host))
    }

    pub async fn read(&self, headers: &HeaderMap, holding_slug: &str) -> Response {
        let result = async {
            let (target, host) = self.target(headers, holding_slug)?;
            let value = target
                .backend
                .read(OrganizationAccessHttpRequest {
                    host,
                    holding_slug: &target.holding_slug,
                    headers,
                })
                .await?;
            Ok::<_, BusinessError>(json_response(StatusCode::OK, json!({"data": value})))
        }
        .await;
        match result {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }
}

/// Router is mounted by the composition root after it implements the backend
/// with the normal Account session, current IAM authority and owner unit.
pub fn organization_access_router(
    targets: Vec<OrganizationAccessHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/access",
            get(read_handler),
        )
        .with_state(Arc::new(OrganizationAccessHttpState::new(targets)?)))
}

async fn read_handler(
    State(state): State<Arc<OrganizationAccessHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    state.read(request.headers(), &holding_slug).await
}
