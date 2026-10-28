//! Explicit registered-Holding ingress for the bounded manual journal edit.
use crate::{business_http::BusinessScopeSlugs, finance_journal_edit::FinanceJournalEditService};
use axum::{
    Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_finance::public::journal_create_wire::normalize_manual_journal_edit;
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use std::{collections::BTreeMap, sync::Arc};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub struct FinanceJournalEditTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<FinanceJournalEditService<C>>,
}
struct EditState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), FinanceJournalEditTarget<C>>,
}
pub fn finance_journal_edit_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceJournalEditTarget<C>>,
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
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_EDIT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{journal_id}/edit",
            post(edit::<C>),
        )
        .with_state(Arc::new(EditState {
            policy,
            scopes,
            targets: configured,
        })))
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
async fn edit<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<EditState<C>>>,
    Path((slug, journal_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
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
            .is_some_and(|v| v != inbound.expected_origin())
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
        target
            .service
            .edit(
                journal_id,
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                move || {
                    let bytes = body.as_ref().map_err(Clone::clone)?;
                    let wire = normalize_manual_journal_edit(bytes)?;
                    let key = unique_request_header(&keys, "idempotency-key")?
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "IDEMPOTENCY_KEY_REQUIRED",
                                "Idempotency-Key is required.",
                            )
                        })?;
                    Ok((CommandKey::new(key)?, wire))
                },
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(data) => match serde_json::from_str::<serde_json::Value>(data.as_str()) {
            Ok(value) => (StatusCode::OK, axum::Json(serde_json::json!({"data":value}))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_EDIT_UNAVAILABLE","message":"The journal could not be encoded."}}))).into_response(),
        },
        Err(PgTransactionError::Business(e)) => {
            let status = match e.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, axum::Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()
        }
        Err(PgTransactionError::Persistence(_)) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_EDIT_UNAVAILABLE","message":"The journal could not be edited."}}))).into_response(),
    };
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
