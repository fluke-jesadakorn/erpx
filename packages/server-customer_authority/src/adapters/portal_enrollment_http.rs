//! Native public Customer Portal enrollment HTTP ingress.
//!
//! Transport facts only: one bounded Account-origin path, one strict JSON body
//! and business-error mapping. The verified session, the exact Holding and the
//! owner enrollment unit remain composition responsibilities behind
//! [`PortalEnrollmentHttpBackend`].
use crate::domain::portal_enrollment::EnrollmentReservation;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

const BODY_LIMIT: usize = 16 * 1024;
const TOKEN_LIMIT: usize = 2048;

pub type PortalEnrollmentHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub enum PortalEnrollmentHttpCommand {
    Reserve {
        token: String,
    },
    Release {
        reservation: EnrollmentReservation,
    },
    Accept {
        token: String,
        /// Exact opaque Account/product session token from the registered cookie.
        session: String,
    },
}

pub struct PortalEnrollmentHttpRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub command: PortalEnrollmentHttpCommand,
}

pub trait PortalEnrollmentHttpBackend: Send + Sync {
    fn execute<'a>(
        &'a self,
        request: PortalEnrollmentHttpRequest<'a>,
    ) -> PortalEnrollmentHttpFuture<'a, Value>;
}

pub struct PortalEnrollmentHttpTarget {
    /// Exact Account origin host that serves the public enrollment page.
    pub host: String,
    pub backend: Arc<dyn PortalEnrollmentHttpBackend>,
}

struct PortalEnrollmentHttpState {
    targets: std::collections::BTreeMap<String, Arc<dyn PortalEnrollmentHttpBackend>>,
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_PORTAL_ENROLLMENT_REQUEST_INVALID",
        "Provide a complete Customer Portal enrollment request.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn bounded(body: Value) -> Response {
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

fn target<'a>(
    state: &'a PortalEnrollmentHttpState,
    headers: &HeaderMap,
) -> Result<(&'a Arc<dyn PortalEnrollmentHttpBackend>, String), BusinessError> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(host)
        .map(|backend| (backend, host.to_owned()))
        .ok_or_else(missing)
}

fn bounded_token(value: Option<&Value>) -> Result<String, BusinessError> {
    let token = value
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty() && token.len() <= TOKEN_LIMIT)
        .ok_or_else(malformed)?;
    Ok(token.to_owned())
}

/// Exact opaque session token; never a caller-supplied user identity. The
/// enrollment page runs on the Account origin, so its Account session cookie is
/// the primary source and a product session is accepted as the SSO variant.
fn session_cookie(headers: &HeaderMap) -> Result<String, BusinessError> {
    let raw = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unauthenticated,
                "CUSTOMER_PORTAL_SESSION_REQUIRED",
                "Sign in again to accept this invitation.",
            )
        })?;
    raw.split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(name, _)| *name == "erpx-account.session_token" || *name == "__Host-erpx-product")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty() && value.len() <= 512)
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unauthenticated,
                "CUSTOMER_PORTAL_SESSION_REQUIRED",
                "Sign in again to accept this invitation.",
            )
        })
}

async fn body(request: Request) -> Result<Value, BusinessError> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(malformed)?;
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed());
    }
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| malformed())?;
    if bytes.is_empty() {
        return Err(malformed());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if !value.is_object() {
        return Err(malformed());
    }
    Ok(value)
}

#[allow(dead_code)]
fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<String>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map(str::to_owned).map_err(|_| malformed()))
        .transpose()
}

pub fn portal_enrollment_http_router(
    targets: Vec<PortalEnrollmentHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = std::collections::BTreeMap::new();
    for target in targets {
        if target.host.is_empty() || target.host.len() > 253 {
            return Err(BusinessError::invalid(
                "CUSTOMER_PORTAL_ENROLLMENT_CONFIGURATION_INVALID",
                "Use one exact Account origin.",
            ));
        }
        if configured.insert(target.host, target.backend).is_some() {
            return Err(BusinessError::invalid(
                "CUSTOMER_PORTAL_ENROLLMENT_CONFIGURATION_INVALID",
                "Use one exact Account origin.",
            ));
        }
    }
    Ok(Router::new()
        .route("/api/customer-portal/enroll/reserve", post(reserve))
        .route("/api/customer-portal/enroll/release", post(release))
        .route("/api/customer-portal/enroll/accept", post(accept))
        .with_state(Arc::new(PortalEnrollmentHttpState {
            targets: configured,
        })))
}

async fn dispatch(
    state: Arc<PortalEnrollmentHttpState>,
    request: Request,
    command: impl FnOnce(&Value, &HeaderMap) -> Result<PortalEnrollmentHttpCommand, BusinessError>,
) -> Response {
    let result = async move {
        let (backend, host) = target(&state, request.headers())?;
        let headers = request.headers().clone();
        let body = body(request).await?;
        let command = command(&body, &headers)?;
        backend
            .execute(PortalEnrollmentHttpRequest {
                host: &host,
                headers: &headers,
                command,
            })
            .await
            .map(|data| serde_json::json!({"data": data}))
    }
    .await;
    match result {
        Ok(reply) => bounded(reply),
        Err(error) => error_response(error),
    }
}

async fn reserve(
    State(state): State<Arc<PortalEnrollmentHttpState>>,
    request: Request,
) -> Response {
    dispatch(state, request, |body, _| {
        Ok(PortalEnrollmentHttpCommand::Reserve {
            token: bounded_token(body.get("token"))?,
        })
    })
    .await
}

async fn release(
    State(state): State<Arc<PortalEnrollmentHttpState>>,
    request: Request,
) -> Response {
    dispatch(state, request, |body, _| {
        let reservation = serde_json::from_value::<EnrollmentReservation>(
            body.get("reservation").cloned().unwrap_or(Value::Null),
        )
        .map_err(|_| malformed())?;
        reservation.validate()?;
        Ok(PortalEnrollmentHttpCommand::Release { reservation })
    })
    .await
}

async fn accept(State(state): State<Arc<PortalEnrollmentHttpState>>, request: Request) -> Response {
    dispatch(state, request, |body, headers| {
        Ok(PortalEnrollmentHttpCommand::Accept {
            token: bounded_token(body.get("token"))?,
            session: session_cookie(headers)?,
        })
    })
    .await
}
