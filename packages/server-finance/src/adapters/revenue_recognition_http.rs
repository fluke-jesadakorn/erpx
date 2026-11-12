//! Native HTTP ingress for Finance revenue-obligation recognition.
//!
//! Transport facts only: registered Host/Holding targets, origin, idempotency,
//! bounded JSON and error mapping. Normal Account session resolution, current
//! Company/branch/department scope and the serializable owner unit are supplied
//! by the composition backend, which must implement
//! `RevenueRecognitionHttpBackend`.
use crate::adapters::revenue_recognition::{RevenueRecognitionInput, RevenueRecognitionResult};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, Json, Path, Request, Router, State, StatusCode, header, post, to_bytes,
};
use erpx_server_runtime::public::http_support::{IntoResponse, Response};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::Serialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type RevenueRecognitionHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
pub struct RevenueRecognitionResponse {
    pub data: RevenueRecognitionResult,
}

pub struct RevenueRecognitionHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub obligation_id: &'a str,
    pub key: &'a CommandKey,
    pub input: RevenueRecognitionInput,
}

/// Composition-owned authentication, scope and owner-unit adapter. The owner
/// router never mints an actor or a Company.
pub trait RevenueRecognitionHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: RevenueRecognitionHttpCommandRequest<'a>,
    ) -> RevenueRecognitionHttpFuture<'a, RevenueRecognitionResponse>;
}

pub struct RevenueRecognitionHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn RevenueRecognitionHttpBackend>,
}

struct RevenueRecognitionHttpState {
    targets: BTreeMap<(String, String), RevenueRecognitionHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Revenue recognition period, date and amount are required.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| BusinessError::invalid("VALIDATION_FAILED", "Use a valid header."))
        })
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        if rest.is_empty()
            || rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@'))
        {
            return false;
        }
        if let Some((host, port)) = rest.rsplit_once(':') {
            return host.ends_with(".erpx.localhost") && port == "3443";
        }
        true
    })
}

pub fn revenue_recognition_router(
    targets: Vec<RevenueRecognitionHttpTarget>,
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
            return Err(BusinessError::invalid(
                "FINANCE_REVENUE_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/revenue-obligations/{obligation_id}/recognize",
            post(recognize),
        )
        .with_state(Arc::new(RevenueRecognitionHttpState {
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

fn bounded<T: Serialize>(body: &T) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response.headers_mut().insert(
            name,
            erpx_server_runtime::public::http_support::HeaderValue::from_static(value),
        );
    }
    response
}

fn target<'a>(
    state: &'a RevenueRecognitionHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a RevenueRecognitionHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn recognize(
    State(state): State<Arc<RevenueRecognitionHttpState>>,
    Path((slug, obligation_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        if unique_header(request.headers(), header::ORIGIN)?
            .is_some_and(|value| value != target.origin)
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_header(
            request.headers(),
            HeaderName::from_static("idempotency-key"),
        )?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
        })
        .and_then(|value| {
            CommandKey::new(value).map_err(|_| {
                BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
            })
        })?;
        if obligation_id.is_empty()
            || obligation_id.encode_utf16().count() > 200
            || obligation_id.chars().any(char::is_control)
        {
            return Err(missing());
        }
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| {
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller recognition request.")
            })?;
        let input: RevenueRecognitionInput =
            serde_json::from_slice(&body).map_err(|_| malformed())?;
        target
            .backend
            .command(RevenueRecognitionHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                obligation_id: &obligation_id,
                key: &key,
                input,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}
