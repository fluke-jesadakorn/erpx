//! Explicit registered-Holding ingress for the native Brand identity
//! workspace and commands. One retained Account proof and one owner unit.
use crate::{
    brand_identity::{BrandIdentityCommandResponse, BrandIdentityService},
    business_http::BusinessScopeSlugs,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub struct BrandIdentityTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<BrandIdentityService<C>>,
}

struct BrandState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), BrandIdentityTarget<C>>,
}

pub fn brand_identity_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<BrandIdentityTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| holding.slug == target.holding_slug);
        if !valid
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "BRAND_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/brand",
            get(read::<C>).post(command::<C>),
        )
        .with_state(Arc::new(BrandState {
            policy,
            scopes,
            targets: configured,
        })))
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

fn error_response(error: PgTransactionError<BusinessError>) -> Response {
    match error {
        PgTransactionError::Business(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
        PgTransactionError::Persistence(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":{"code":"BRAND_UNAVAILABLE","message":"The Brand identity could not be read or changed."}})),
        )
            .into_response(),
    }
}

fn bounded<T: serde::Serialize>(body: &T) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (axum::http::header::CACHE_CONTROL, "private, no-store"),
        (axum::http::header::REFERRER_POLICY, "no-referrer"),
        (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, axum::http::HeaderValue::from_static(value));
    }
    response
}

async fn read<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BrandState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host.clone(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .workspace(&host, inbound.into_proof(), move || {
                captured.resolve_read(company)
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn command<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BrandState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host.clone(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let mut keys = axum::http::HeaderMap::new();
        for value in request.headers().get_all("idempotency-key") {
            keys.append("idempotency-key", value.clone());
        }
        let proof = inbound.into_proof();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| {
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Brand command.")
            })?;
        let input = serde_json::from_slice::<Value>(&body).map_err(|_| {
            BusinessError::invalid("BRAND_COMMAND_REQUIRED", "Choose a Brand identity command.")
        })?;
        let action = input
            .get("action")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BusinessError::invalid("BRAND_COMMAND_REQUIRED", "Choose a Brand identity command.")
            })?
            .to_owned();
        let draft = input
            .get("id")
            .and_then(Value::as_str)
            .map(|value| RecordId::new(value.to_owned()))
            .transpose()?;
        let key = unique_request_header(&keys, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let prepare_value = input.clone();
        let prepare_key = CommandKey::new(key.to_owned())?;
        let prepare = move || Ok((prepare_key.clone(), prepare_value.clone()));
        let reply: BrandIdentityCommandResponse = match action.as_str() {
            "save" => {
                target
                    .service
                    .save(&host, proof, move || captured.resolve(), prepare)
                    .await?
            }
            "publish" => {
                let draft = draft.ok_or_else(|| {
                    BusinessError::invalid(
                        "BRAND_COMMAND_REQUIRED",
                        "Choose the current Brand draft.",
                    )
                })?;
                target
                    .service
                    .publish(
                        &host,
                        proof,
                        move || captured.resolve(),
                        prepare,
                        draft.as_str().to_owned(),
                    )
                    .await?
            }
            "cancel" => {
                let draft = draft.ok_or_else(|| {
                    BusinessError::invalid(
                        "BRAND_COMMAND_REQUIRED",
                        "Choose the current Brand draft.",
                    )
                })?;
                target
                    .service
                    .cancel(
                        &host,
                        proof,
                        move || captured.resolve(),
                        prepare,
                        draft.as_str().to_owned(),
                    )
                    .await?
            }
            "rollback" => {
                target
                    .service
                    .rollback(&host, proof, move || captured.resolve(), prepare)
                    .await?
            }
            "category-lock" => {
                target
                    .service
                    .change_lock(&host, proof, move || captured.resolve(), prepare)
                    .await?
            }
            _ => {
                return Err(BusinessError::invalid(
                    "BRAND_COMMAND_REQUIRED",
                    "Choose a Brand identity command.",
                )
                .into());
            }
        };
        Ok::<_, PgTransactionError<BusinessError>>(reply)
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}
