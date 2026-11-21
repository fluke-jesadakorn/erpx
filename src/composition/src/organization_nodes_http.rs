//! Explicit registered-Holding ingress for the native Organization node,
//! business-unit, sharing and lifecycle workspace. One retained Account proof
//! and one owner unit.
use crate::{
    business_http::BusinessScopeSlugs,
    organization_nodes::{
        OrganizationCommandResponse, OrganizationNodeService, OrganizationPreviewResponse,
        OrganizationWorkspaceResponse,
    },
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub struct OrganizationNodesTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<OrganizationNodeService<C>>,
}

struct OrganizationState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), OrganizationNodesTarget<C>>,
}

pub fn organization_nodes_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<OrganizationNodesTarget<C>>,
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
                "ORGANIZATION_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization",
            get(read::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/changes",
            post(save::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/changes/preview",
            post(preview::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/changes/{change_id}/review",
            post(review::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/people/organization/changes/{change_id}/cancel",
            post(cancel::<C>),
        )
        .with_state(Arc::new(OrganizationState {
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
            Json(serde_json::json!({"error":{"code":"ORGANIZATION_UNAVAILABLE","message":"The organization workspace could not be read or changed."}})),
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

async fn target<'a, C>(
    state: &'a OrganizationState<C>,
    host: &str,
    slug: &str,
) -> Result<&'a OrganizationNodesTarget<C>, BusinessError> {
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn query(request: &Request) -> BTreeMap<String, String> {
    request
        .uri()
        .query()
        .map(|value| {
            url::form_urlencoded::parse(value.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

async fn read<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<OrganizationState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = target(&state, &host, &slug).await?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let company = target.service.company_id().clone();
        let query = query(&request);
        target
            .service
            .workspace(
                &host,
                inbound.into_proof(),
                move || captured.resolve_read(company),
                query,
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn save<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<OrganizationState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = target(&state, &host, &slug).await?;
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
        let key = command_key(&request, "idempotency-key")?;
        let body = json_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .save(&host, proof, move || captured.resolve(), {
                let body = body.clone();
                move || Ok((key.clone(), body.clone()))
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn preview<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<OrganizationState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = target(&state, &host, &slug).await?;
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
        let body = json_body(request).await?;
        let proof = inbound.into_proof();
        let company = target.service.company_id().clone();
        let prepare_value = body.clone();
        target
            .service
            .preview(
                &host,
                proof,
                move || captured.resolve_read(company),
                move || {
                    Ok((
                        CommandKey::new("preview-command-key")
                            .expect("the fixed preview key is valid"),
                        prepare_value.clone(),
                    ))
                },
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn review<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<OrganizationState<C>>>,
    Path((slug, change_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = target(&state, &host, &slug).await?;
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
        let key = command_key(&request, "idempotency-key")?;
        let body = json_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .review(
                &host,
                proof,
                move || captured.resolve(),
                {
                    let body = body.clone();
                    move || Ok((key.clone(), body.clone()))
                },
                change_id,
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

async fn cancel<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<OrganizationState<C>>>,
    Path((slug, change_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = target(&state, &host, &slug).await?;
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
        let key = command_key(&request, "idempotency-key")?;
        let body = json_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .cancel(
                &host,
                proof,
                move || captured.resolve(),
                {
                    let body = body.clone();
                    move || Ok((key.clone(), body.clone()))
                },
                change_id,
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(&reply),
        Err(error) => error_response(error),
    }
}

fn command_key(request: &Request, name: &'static str) -> Result<CommandKey, BusinessError> {
    let value = unique_request_header(request.headers(), name)?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::InvalidInput,
                "IDEMPOTENCY_KEY_REQUIRED",
                "Idempotency-Key is required.",
            )
        })?
        .to_owned();
    CommandKey::new(value)
}

async fn json_body(request: Request) -> Result<Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Request body must be valid JSON."))
}

/// Explicitly named so an integrator can assert the exact response types
/// without naming private state.
pub type OrganizationWorkspaceReply = OrganizationWorkspaceResponse;
pub type OrganizationCommandReply = OrganizationCommandResponse;
pub type OrganizationPreviewReply = OrganizationPreviewResponse;
