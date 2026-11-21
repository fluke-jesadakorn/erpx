use super::account_http::{failure, json, one_header, presented, private};
use crate::application::recovery::EmergencyAccess;
use crate::domain::account_auth::RequestAudience;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::sync::Arc;

/// Reviewed temporary Account access. Mounted beside the native Account
/// security router; the body never selects the actor.
pub fn recovery_router(service: Arc<EmergencyAccess>) -> Router {
    Router::new()
        .route("/api/v1/account/iam/emergency/active", get(active))
        .route("/api/v1/account/iam/emergency/activate", post(activate))
        .route("/api/v1/account/iam/emergency/revoke", post(revoke))
        .layer(axum::middleware::from_fn(
            |request: Request, next: axum::middleware::Next| async move {
                let mut response = private(next.run(request).await, None);
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    axum::http::HeaderValue::from_static("private, no-store"),
                );
                response.headers_mut().insert(
                    header::REFERRER_POLICY,
                    axum::http::HeaderValue::from_static("no-referrer"),
                );
                response
            },
        ))
        .with_state(service)
}

fn failed(error: BusinessError) -> Response {
    let status = match error.code {
        "IDEMPOTENCY_CONFLICT" => Some(axum::http::StatusCode::CONFLICT),
        _ if error.kind == FailureKind::InvalidInput => {
            Some(axum::http::StatusCode::UNPROCESSABLE_ENTITY)
        }
        _ => None,
    };
    let mut response = failure(error);
    if let Some(status) = status {
        *response.status_mut() = status;
    }
    response
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

/// Resolves the Account-only proof and rejects a foreign Origin on writes.
fn proof(
    service: &EmergencyAccess,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<crate::application::account_ports::DeviceProof, BusinessError> {
    let host = one_header(headers, "host")?.ok_or_else(not_found)?;
    let origin = one_header(headers, "origin")?;
    if mutation
        && (origin.is_none() || origin.is_some_and(|value| value != format!("https://{host}")))
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the ERPX Account application to submit this request.",
        ));
    }
    service.proof(&host, &presented(headers, &RequestAudience::Account)?)
}

fn data(value: Value) -> Response {
    Json(json!({ "data": value })).into_response()
}

async fn active(State(service): State<Arc<EmergencyAccess>>, request: Request) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let proof = proof(&service, &headers, false)?;
        service.active(proof).await.map(data)
    }
    .await;
    result.unwrap_or_else(failed)
}

async fn activate(State(service): State<Arc<EmergencyAccess>>, request: Request) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let proof = proof(&service, &headers, true)?;
        let key = one_header(&headers, "idempotency-key")?.unwrap_or_default();
        let body: Value = json(request).await?;
        service.activate(proof, &body, &key).await.map(data)
    }
    .await;
    result.unwrap_or_else(failed)
}

async fn revoke(State(service): State<Arc<EmergencyAccess>>, request: Request) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let proof = proof(&service, &headers, true)?;
        let key = one_header(&headers, "idempotency-key")?.unwrap_or_default();
        let body: Value = json(request).await?;
        service.revoke(proof, &body, &key).await.map(data)
    }
    .await;
    result.unwrap_or_else(failed)
}
