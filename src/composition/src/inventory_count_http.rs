//! Private HTTP ingress for the native generic stock-count save command.
use crate::http_scope::{BusinessScopeSlugs, selected_company_target};
use crate::inventory_count_commands::{CountTransitionInput, InventoryCountCommand};
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
use erpx_server_inventory::public::counts::CountSaveInput;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct InventoryCountHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<InventoryCountCommand<C>>,
}

#[derive(Clone)]
struct InventoryCountHttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<InventoryCountCommand<C>>>,
}

pub fn inventory_count_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryCountHttpTarget<C>>,
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
                "INVENTORY_COUNT_HTTP_CONFIGURATION_INVALID",
                "Configure distinct Inventory Holding targets.",
            ));
        }
    }
    if registered.is_empty() {
        return Err(BusinessError::invalid(
            "INVENTORY_COUNT_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Inventory Holding target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/stock-counts",
            post(create_count::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/stock-counts/{id}",
            post(update_count::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/stock-counts/{id}/{action}",
            post(transition_count::<C>),
        )
        .with_state(Arc::new(InventoryCountHttpState {
            policy,
            scopes,
            targets: registered,
        })))
}

struct RequestFacts {
    key: CommandKey,
    wire: CommandJson,
    value: Value,
}

fn request_facts(headers: &HeaderMap, body: &Bytes) -> Result<RequestFacts, BusinessError> {
    let key = unique_request_header(headers, "idempotency-key")?.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
    })?;
    if key.is_empty() || key.len() > 256 {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Idempotency-Key is required.",
        ));
    }
    let text = std::str::from_utf8(body)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Provide a valid Count body."))?;
    let wire = CommandJson::parse(text)?;
    let value = serde_json::from_str(text)
        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Provide a valid Count body."))?;
    Ok(RequestFacts {
        key: CommandKey::new(key)?,
        wire,
        value,
    })
}

async fn create_count<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<InventoryCountHttpState<C>>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    run(state, holding_slug, None, headers, body, true).await
}

async fn update_count<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<InventoryCountHttpState<C>>>,
    Path((holding_slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    run(state, holding_slug, Some(id), headers, body, false).await
}

async fn transition_count<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<InventoryCountHttpState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    run_transition(state, holding_slug, id, action, headers, body).await
}

async fn run_transition<C: Clock + Send + Sync + 'static>(
    state: Arc<InventoryCountHttpState<C>>,
    holding_slug: String,
    id: String,
    action: String,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let action = match action.as_str() {
            "submit" => erpx_server_inventory::public::service_initial_physical_count::CountAction::Submit,
            "approve" => erpx_server_inventory::public::service_initial_physical_count::CountAction::Approve,
            "post" => erpx_server_inventory::public::service_initial_physical_count::CountAction::Post,
            "reject" => erpx_server_inventory::public::service_initial_physical_count::CountAction::Reject,
            "cancel" => erpx_server_inventory::public::service_initial_physical_count::CountAction::Cancel,
            _ => return Err(not_found()),
        };
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
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let (branch, department) = state.scopes.slugs(&headers)?;
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let facts = request_facts(&headers, &body)?;
        let input: CountTransitionInput = serde_json::from_value(facts.value).map_err(|_| {
            BusinessError::invalid(
                "INVENTORY_COUNT_INPUT_INVALID",
                "Provide a valid stock Count command.",
            )
        })?;
        let outcome = target
            .transition(
                host,
                inbound.into_proof(),
                move || captured.resolve(),
                id,
                action,
                input,
                facts.wire,
                facts.key,
            )
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => BusinessError::new(
                    FailureKind::Unavailable,
                    "INVENTORY_COUNT_UNAVAILABLE",
                    "The stock Count command is unavailable.",
                ),
            })?;
        let data: Value = serde_json::from_str(outcome.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_COUNT_UNAVAILABLE",
                "The stock Count command is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(Json(json!({"data": data})).into_response())
    }
    .await;
    match result {
        Ok(response) => response.into_response(),
        Err(error) => business_error(error),
    }
}

async fn run<C: Clock + Send + Sync + 'static>(
    state: Arc<InventoryCountHttpState<C>>,
    holding_slug: String,
    id: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    created: bool,
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
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let (branch, department) = state.scopes.slugs(&headers)?;
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let facts = request_facts(&headers, &body)?;
        let input: CountSaveInput = serde_json::from_value(facts.value).map_err(|_| {
            BusinessError::invalid(
                "INVENTORY_COUNT_INPUT_INVALID",
                "Provide a valid stock Count command.",
            )
        })?;
        let outcome = target
            .save(
                host,
                inbound.into_proof(),
                move || captured.resolve(),
                id,
                input,
                facts.wire,
                facts.key,
            )
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => BusinessError::new(
                    FailureKind::Unavailable,
                    "INVENTORY_COUNT_UNAVAILABLE",
                    "The stock Count command is unavailable.",
                ),
            })?;
        let data: Value = serde_json::from_str(outcome.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "INVENTORY_COUNT_UNAVAILABLE",
                "The stock Count command is unavailable.",
            )
        })?;
        let mut response = Json(json!({"data": data})).into_response();
        if created {
            *response.status_mut() = StatusCode::CREATED;
        }
        Ok::<_, BusinessError>(response)
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
