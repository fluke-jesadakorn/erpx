//! Private HTTP ingress for native Inventory transport commands. The Account
//! cookie and rendered work-context CAS headers are verified again inside the
//! command's own serializable transaction; this layer supplies no authority.
use crate::http_scope::{BusinessScopeSlugs, selected_company_target};
use crate::inventory_transit_commands::InventoryTransitCommand;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_identity_access::public::{
    AccountPolicy,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_inventory::public::transit::{
    TransitCancelInput, TransitDispatchInput, TransitLossDecisionInput, TransitLossInput,
    TransitReceiveInput,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct InventoryTransitHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<InventoryTransitCommand<C>>,
}

#[derive(Clone)]
struct InventoryTransitHttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<InventoryTransitCommand<C>>>,
}

pub fn inventory_transit_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryTransitHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut registered = BTreeMap::new();
    for target in targets {
        let key = (
            target.host.clone(),
            target.holding_slug.clone(),
            target.command.company.as_str().to_owned(),
        );
        if registered.insert(key, target.command).is_some() {
            return Err(BusinessError::invalid(
                "INVENTORY_TRANSIT_HTTP_CONFIGURATION_INVALID",
                "Configure distinct Inventory Holding targets.",
            ));
        }
    }
    if registered.is_empty() {
        return Err(BusinessError::invalid(
            "INVENTORY_TRANSIT_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Inventory Holding target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/stock-transit/{id}/{action}",
            post(post_transit::<C>),
        )
        .with_state(Arc::new(InventoryTransitHttpState {
            policy,
            scopes,
            targets: registered,
        })))
}

async fn post_transit<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<InventoryTransitHttpState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let host = headers
            .get("host")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(not_found)?;
        let target = selected_company_target(&state.targets, host, &holding_slug, &headers)?;
        if let Some(origin) = unique_request_header(&headers, "origin")?
            && origin != format!("https://{host}")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let key = unique_request_header(&headers, "idempotency-key")?.ok_or_else(|| {
            BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
        })?;
        if key.is_empty() || key.len() > 256 {
            return Err(BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Idempotency-Key is required.",
            ));
        }
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let (branch, department) = state.scopes.slugs(&headers)?;
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let text = std::str::from_utf8(&body).map_err(|_| {
            BusinessError::invalid("INVALID_JSON", "Provide a valid transport body.")
        })?;
        let wire = CommandJson::parse(text)?;
        let value: Value = serde_json::from_str(text).map_err(|_| {
            BusinessError::invalid("INVALID_JSON", "Provide a valid transport body.")
        })?;
        let key = CommandKey::new(key)?;
        let proof = inbound.into_proof();
        let invalid = || {
            BusinessError::invalid(
                "INVENTORY_TRANSIT_INPUT_INVALID",
                "Provide a valid transport command.",
            )
        };
        let outcome = match action.as_str() {
            "dispatch" => {
                let input: TransitDispatchInput =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                let id = id.clone();
                target
                    .dispatch(
                        host,
                        proof,
                        move || captured.resolve(),
                        id,
                        input,
                        wire,
                        key,
                    )
                    .await
            }
            "receive" => {
                let input: TransitReceiveInput =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                let id = id.clone();
                target
                    .receive(
                        host,
                        proof,
                        move || captured.resolve(),
                        id,
                        input,
                        wire,
                        key,
                    )
                    .await
            }
            "loss" => {
                let input: TransitLossInput =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                let id = id.clone();
                target
                    .loss(
                        host,
                        proof,
                        move || captured.resolve(),
                        id,
                        input,
                        wire,
                        key,
                    )
                    .await
            }
            "decide-loss" => {
                let input: TransitLossDecisionInput =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                let id = id.clone();
                target
                    .decide_loss(
                        host,
                        proof,
                        move || captured.resolve(),
                        id,
                        input,
                        wire,
                        key,
                    )
                    .await
            }
            "cancel" => {
                let input: TransitCancelInput =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                let id = id.clone();
                target
                    .cancel(
                        host,
                        proof,
                        move || captured.resolve(),
                        id,
                        input,
                        wire,
                        key,
                    )
                    .await
            }
            _ => {
                return Err(not_found());
            }
        };
        let outcome = outcome.map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_TRANSIT_UNAVAILABLE",
                "The transport command is unavailable.",
            ),
        })?;
        let data: Value = serde_json::from_str(outcome.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_TRANSIT_UNAVAILABLE",
                "The transport command is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(Json(json!({"data": data})))
    }
    .await;
    match result {
        Ok(response) => response.into_response(),
        Err(error) => business_error(error),
    }
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn business_error(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(json!({"error": {"code": error.code, "message": error.message}})),
    )
        .into_response()
}
