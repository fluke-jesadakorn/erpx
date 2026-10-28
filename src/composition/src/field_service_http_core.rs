//! Shared HTTP envelope and deferred body, with no authority or owner factory.
use crate::field_service_http_wire::{SharedPostWire, SharedPostWireFailure, classify_shared_post};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_field_service::public::parts::field_service_first_wire_message;
use erpx_server_identity_access::public::account_inbound_http::unique_request_header;
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::{Mutex, OnceCell};

pub(crate) struct TrustedTarget {
    pub host: String,
    pub slug: String,
    pub origin: String,
}
pub(crate) struct PendingIngress {
    slot: Mutex<Option<Body>>,
    cache: OnceCell<Result<Arc<[u8]>, ()>>,
}
pub(crate) enum WireError {
    InvalidJson,
    Validation(String),
}
impl PendingIngress {
    fn new(body: Body) -> Self {
        Self {
            slot: Mutex::new(Some(body)),
            cache: OnceCell::new(),
        }
    }
    /// The executor invokes this only after its own admission; bytes/fault are reused on retry.
    pub(crate) async fn decode(&self) -> Result<(Arc<[u8]>, SharedPostWire), WireError> {
        let bytes = self
            .cache
            .get_or_init(|| async {
                let body = self.slot.lock().await.take().ok_or(())?;
                to_bytes(body, usize::MAX)
                    .await
                    .map(|v| Arc::<[u8]>::from(v.to_vec()))
                    .map_err(|_| ())
            })
            .await
            .as_ref()
            .map_err(|_| WireError::InvalidJson)?
            .clone();
        let wire = match classify_shared_post(&bytes) {
            Ok(wire) => wire,
            Err(SharedPostWireFailure::InvalidJson) => return Err(WireError::InvalidJson),
            Err(SharedPostWireFailure::Schema(e)) => {
                return Err(match field_service_first_wire_message(&bytes, &e) {
                    Some(message) => WireError::Validation(message),
                    None => WireError::InvalidJson,
                });
            }
        };
        Ok((bytes, wire))
    }
}
pub(crate) enum PostResponse {
    Success(CommandJson),
    Error {
        status: StatusCode,
        code: String,
        message: String,
    },
}
impl PostResponse {
    pub(crate) fn error(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Error {
            status,
            code: code.into(),
            message: message.into(),
        }
    }
    pub(crate) fn business(e: BusinessError) -> Self {
        let status = match e.kind {
            FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
            FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
            FailureKind::Forbidden => StatusCode::FORBIDDEN,
            FailureKind::NotFound => StatusCode::NOT_FOUND,
            FailureKind::Conflict => StatusCode::CONFLICT,
            FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        };
        Self::error(status, e.code, e.message)
    }
    pub(crate) fn internal() -> Self {
        Self::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "The request could not be completed.",
        )
    }
    pub(crate) fn wire(error: WireError) -> Self {
        match error {
            WireError::InvalidJson => Self::internal(),
            WireError::Validation(message) => Self::error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_FAILED",
                message,
            ),
        }
    }
    fn response(self) -> Response {
        match self {
            Self::Success(data) => {
                let mut response = (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    Body::from(format!("{{\"data\":{}}}", data.as_str())),
                )
                    .into_response();
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-store"),
                );
                response
            }
            Self::Error {
                status,
                code,
                message,
            } => (
                status,
                axum::Json(serde_json::json!({"error":{"code":code,"message":message}})),
            )
                .into_response(),
        }
    }
}
/// Data envelope only. Implementations own their admission; this is not an actor.
pub(crate) trait PostExecutor: Send + Sync + 'static {
    fn execute<'a>(
        &'a self,
        host: String,
        slug: String,
        headers: HeaderMap,
        key: String,
        ingress: Arc<PendingIngress>,
    ) -> Pin<Box<dyn Future<Output = PostResponse> + Send + 'a>>;
}
struct CoreState<E> {
    targets: BTreeMap<(String, String), String>,
    executor: E,
}
pub(crate) fn router_for_executor<E: PostExecutor>(
    targets: Vec<TrustedTarget>,
    executor: E,
) -> Router {
    let targets = targets
        .into_iter()
        .map(|t| ((t.host, t.slug), t.origin))
        .collect();
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/field-service",
            post(handler::<E>),
        )
        .with_state(Arc::new(CoreState { targets, executor }))
}
pub(crate) fn joined(headers: &HeaderMap, name: &'static str) -> Option<String> {
    let values = headers
        .get_all(name)
        .iter()
        .map(|v| {
            v.as_bytes()
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>()
                .trim_matches([' ', '\t'])
                .to_owned()
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.join(", "))
    }
}
async fn handler<E: PostExecutor>(
    State(state): State<Arc<CoreState<E>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    if let Some(origin) = joined(&headers, "origin").filter(|value| !value.is_empty()) {
        let host = match unique_request_header(&headers, "host") {
            Ok(Some(host)) => host,
            Ok(None) => {
                return PostResponse::error(StatusCode::NOT_FOUND, "NOT_FOUND", "Not found.")
                    .response();
            }
            Err(e) => return PostResponse::business(e).response(),
        };
        let Some(expected) = state.targets.get(&(host.to_owned(), slug.clone())) else {
            return PostResponse::error(StatusCode::NOT_FOUND, "NOT_FOUND", "Not found.")
                .response();
        };
        if &origin != expected {
            return PostResponse::error(
                StatusCode::FORBIDDEN,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .response();
        }
    }
    let key = match joined(&headers, "idempotency-key") {
        Some(key) if !key.is_empty() => key,
        _ => {
            return PostResponse::error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required.",
            )
            .response();
        }
    };
    let host = match unique_request_header(&headers, "host") {
        Ok(Some(host)) => host.to_owned(),
        Ok(None) => {
            return PostResponse::error(StatusCode::NOT_FOUND, "NOT_FOUND", "Not found.")
                .response();
        }
        Err(e) => return PostResponse::business(e).response(),
    };
    if !state.targets.contains_key(&(host.clone(), slug.clone())) {
        return PostResponse::error(StatusCode::NOT_FOUND, "NOT_FOUND", "Not found.").response();
    }
    state
        .executor
        .execute(
            host,
            slug,
            headers,
            key,
            Arc::new(PendingIngress::new(body)),
        )
        .await
        .response()
}
