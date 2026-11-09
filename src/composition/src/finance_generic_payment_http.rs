//! Dedicated generic controlled-payment create ingress. This path is mounted
//! for ordinary Finance read targets and does not require the Field receipt
//! configuration; the shared owner command still decides Field vs generic
//! source from the persisted invoice, never from the request label.
use crate::{
    finance_journal_create::FinanceJournalCreateConfiguration,
    finance_service_receipt_create::{
        ServiceReceiptCreateWire, execute_service_receipt_create, normalize_service_receipt_create,
    },
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    effective_authority::HoldingWorkAuthorizer,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use std::{collections::BTreeMap, sync::Arc};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub struct GenericPaymentTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceJournalCreateConfiguration,
    pub clock: Arc<C>,
}
struct PostState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn crate::business_http::BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), GenericPaymentTarget<C>>,
}
pub fn finance_generic_payment_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn crate::business_http::BusinessScopeSlugs>,
    targets: Vec<GenericPaymentTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| {
                holding.slug == target.holding_slug
                    && holding.holding_id == target.configuration.holding
            });
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
                "FINANCE_GENERIC_PAYMENT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payments/generic",
            post(create::<C>),
        )
        .with_state(Arc::new(PostState {
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
fn response(error: PgTransactionError<BusinessError>) -> Response {
    match error {
        PgTransactionError::Business(error) => (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
        PgTransactionError::Persistence(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":{"code":"NATIVE_GENERIC_PAYMENT_UNAVAILABLE","message":"The controlled payment could not be prepared."}})),
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
async fn create<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let (target, target_selection) = crate::http_scope::deferred_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let config = target.configuration.clone();
        let clock = target.clock.clone();
        execute_service_receipt_create(
            &target.authorizer,
            &host,
            proof,
            move || {
                scope?;
                target_selection?;
                captured.resolve()
            },
            config,
            clock,
            move || {
                let bytes = body.as_ref().map_err(Clone::clone)?;
                let wire: ServiceReceiptCreateWire = normalize_service_receipt_create(bytes)?;
                let key = unique_request_header(&keys, "idempotency-key")?
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        BusinessError::invalid(
                            "IDEMPOTENCY_KEY_REQUIRED",
                            "Idempotency-Key is required.",
                        )
                    })?;
                Ok((CommandKey::new(key)?.as_str().to_owned(), wire))
            },
        )
        .await
    }
    .await;
    match result {
        Ok(value) => {
            let value: serde_json::Value =
                serde_json::from_str(value.as_str()).unwrap_or(serde_json::Value::Null);
            private(
                (
                    StatusCode::CREATED,
                    Json(serde_json::json!({"data": value})),
                )
                    .into_response(),
            )
        }
        Err(error) => private(response(error)),
    }
}
