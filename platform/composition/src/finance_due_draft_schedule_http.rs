//! Explicit native Finance due-draft schedule/decide ingress; no legacy fallback.
use crate::{
    finance_due_draft_schedule_command::{FinanceDueDraftConfiguration, execute_due_draft_decide, execute_due_draft_schedule},
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_identity_access_server::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    effective_authority::HoldingWorkAuthorizer,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, Clock, CommandKey, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const BODY_LIMIT: usize = 2 * 1024 * 1024;

pub struct FinanceDueDraftScheduleTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceDueDraftConfiguration,
    pub clock: Arc<C>,
}

struct DueDraftState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), FinanceDueDraftScheduleTarget<C>>,
}

pub fn finance_due_draft_schedule_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceDueDraftScheduleTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|h| h.slug == target.holding_slug);
        if !valid
            || configured
                .insert(
                    (
                        target.host.clone(),
                        target.holding_slug.clone(),
                        target.configuration.company.as_str().to_owned(),
                    ),
                    target,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_DUE_DRAFT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/due-drafts",
            post(schedule::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/due-drafts/{id}/retry",
            post(decide_retry::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/due-drafts/{id}/cancel",
            post(decide_cancel::<C>),
        )
        .with_state(Arc::new(DueDraftState {
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

fn failure(error: PgTransactionError<BusinessError>) -> Response {
    match error {
        PgTransactionError::Business(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
        PgTransactionError::Persistence(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":{"code":"FINANCE_DUE_DRAFT_UNAVAILABLE","message":"The Finance draft could not be scheduled."}})),
        )
            .into_response(),
    }
}

fn private(mut response: Response) -> Response {
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

fn command_key(headers: &axum::http::HeaderMap) -> Result<CommandKey, BusinessError> {
    let key = unique_request_header(headers, "idempotency-key")?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required."))?;
    CommandKey::new(key.to_owned())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleBody {
    kind: String,
    source_id: String,
    run_on: String,
    input: serde_json::Value,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBody {
    expected_version: i64,
    reason: String,
}

async fn schedule<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<DueDraftState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, request.headers())?;
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
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        let target_config = target.configuration.clone();
        let target_clock = target.clock.clone();
        execute_due_draft_schedule(
            &target.authorizer,
            &host,
            proof,
            move || {
                scope?;
                captured.resolve()
            },
            target_config,
            target_clock,
            move || {
                let bytes = body.as_ref().map_err(Clone::clone)?;
                let parsed: ScheduleBody = serde_json::from_slice(bytes).map_err(|_| {
                    BusinessError::invalid("VALIDATION_FAILED", "Invalid draft schedule.")
                })?;
                if !parsed.input.is_object() {
                    return Err(BusinessError::invalid(
                        "VALIDATION_FAILED",
                        "Invalid draft schedule.",
                    ));
                }
                let key = command_key(&keys)?;
                Ok((
                    key,
                    parsed.kind,
                    parsed.source_id,
                    parsed.run_on,
                    parsed.input.to_string(),
                    parsed.reason,
                ))
            },
        )
        .await
    }
    .await;
    match result {
        Ok(value) => {
            let value: serde_json::Value =
                serde_json::from_str(value.as_str()).unwrap_or(serde_json::Value::Null);
            private((StatusCode::CREATED, Json(serde_json::json!({"data": value}))).into_response())
        }
        Err(error) => private(failure(error)),
    }
}

async fn decide<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<DueDraftState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    action: &'static str,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, request.headers())?;
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
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        let target_config = target.configuration.clone();
        let target_clock = target.clock.clone();
        execute_due_draft_decide(
            &target.authorizer,
            &host,
            proof,
            move || {
                scope?;
                captured.resolve()
            },
            target_config,
            target_clock,
            move || {
                let bytes = body.as_ref().map_err(Clone::clone)?;
                let parsed: DecisionBody = serde_json::from_slice(bytes).map_err(|_| {
                    BusinessError::invalid("VALIDATION_FAILED", "Invalid draft decision.")
                })?;
                let key = command_key(&keys)?;
                Ok((key, id.clone(), action.to_owned(), parsed.expected_version, parsed.reason))
            },
        )
        .await
    }
    .await;
    match result {
        Ok(value) => {
            let value: serde_json::Value =
                serde_json::from_str(value.as_str()).unwrap_or(serde_json::Value::Null);
            private((StatusCode::OK, Json(serde_json::json!({"data": value}))).into_response())
        }
        Err(error) => private(failure(error)),
    }
}

async fn decide_retry<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<DueDraftState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    decide::<C>(State(state), Path((slug, id)), "retry", request).await
}

async fn decide_cancel<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<DueDraftState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    decide::<C>(State(state), Path((slug, id)), "cancel", request).await
}
