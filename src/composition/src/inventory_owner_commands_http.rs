//! Private HTTP ingress for native Inventory transfer, landed-cost and
//! reorder-policy commands. The Account cookie and rendered work-context CAS
//! headers are verified again inside each command's own serializable
//! transaction; this layer supplies no authority.
use crate::http_scope::{BusinessScopeSlugs, selected_company_target};
use crate::inventory_landed_cost_commands::InventoryLandedCostCommand;
use crate::inventory_reorder_commands::InventoryReorderCommand;
use crate::inventory_transfer_commands::InventoryTransferCommand;
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
use erpx_server_inventory::public::landed_costs::{
    LandedCostAction, LandedCostActionInput, LandedCostCreateInput,
};
use erpx_server_inventory::public::reorder_policies::{
    ReorderAction, ReorderActionInput, ReorderSaveInput,
};
use erpx_server_inventory::public::transfers::{
    TransferAction, TransferActionInput, TransferSaveInput,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct InventoryTransferHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<InventoryTransferCommand<C>>,
}

#[derive(Clone)]
pub struct InventoryLandedCostHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<InventoryLandedCostCommand<C>>,
}

#[derive(Clone)]
pub struct InventoryReorderHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<InventoryReorderCommand<C>>,
}

#[derive(Clone)]
struct TransferState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<InventoryTransferCommand<C>>>,
}

#[derive(Clone)]
struct LandedCostState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<InventoryLandedCostCommand<C>>>,
}

#[derive(Clone)]
struct ReorderState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<InventoryReorderCommand<C>>>,
}

fn keyed<C>(
    targets: Vec<(String, String, Arc<C>)>,
    company: impl Fn(&C) -> &erpx_server_shared::public::CompanyId,
) -> Result<BTreeMap<(String, String, String), Arc<C>>, BusinessError> {
    let mut registered = BTreeMap::new();
    for (host, holding_slug, command) in targets {
        let key = (host, holding_slug, company(&command).as_str().to_owned());
        if registered.insert(key, command).is_some() {
            return Err(BusinessError::invalid(
                "INVENTORY_OWNER_HTTP_CONFIGURATION_INVALID",
                "Configure distinct Inventory Holding targets.",
            ));
        }
    }
    if registered.is_empty() {
        return Err(BusinessError::invalid(
            "INVENTORY_OWNER_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Inventory Holding target.",
        ));
    }
    Ok(registered)
}

pub fn inventory_transfer_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryTransferHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let registered = keyed(
        targets
            .into_iter()
            .map(|target| (target.host, target.holding_slug, target.command))
            .collect(),
        |command| &command.company,
    )?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/stock-transfers",
            post(post_transfer_save::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/stock-transfers/{id}",
            post(post_transfer_save_id::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/stock-transfers/{id}/{action}",
            post(post_transfer_transition::<C>),
        )
        .with_state(Arc::new(TransferState {
            policy,
            scopes,
            targets: registered,
        })))
}

pub fn inventory_landed_cost_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryLandedCostHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let registered = keyed(
        targets
            .into_iter()
            .map(|target| (target.host, target.holding_slug, target.command))
            .collect(),
        |command| &command.company,
    )?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/landed-costs",
            post(post_landed_cost_create::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/landed-costs/{id}/{action}",
            post(post_landed_cost_transition::<C>),
        )
        .with_state(Arc::new(LandedCostState {
            policy,
            scopes,
            targets: registered,
        })))
}

pub fn inventory_reorder_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryReorderHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let registered = keyed(
        targets
            .into_iter()
            .map(|target| (target.host, target.holding_slug, target.command))
            .collect(),
        |command| &command.company,
    )?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/reorder-policies",
            post(post_reorder_save::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/reorder-policies/{id}/{action}",
            post(post_reorder_transition::<C>),
        )
        .with_state(Arc::new(ReorderState {
            policy,
            scopes,
            targets: registered,
        })))
}

struct Common {
    host: String,
    key: CommandKey,
    proof: erpx_server_identity_access::public::DeviceProof,
    captured: CapturedHoldingMutation,
    wire: CommandJson,
    value: Value,
}

fn common(
    policy: &AccountPolicy,
    scopes: &Arc<dyn BusinessScopeSlugs>,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Common, BusinessError> {
    let host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(not_found)?;
    if let Some(origin) = unique_request_header(headers, "origin")?
        && origin != format!("https://{host}")
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let key = unique_request_header(headers, "idempotency-key")?.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
    })?;
    if key.is_empty() || key.len() > 256 {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Idempotency-Key is required.",
        ));
    }
    let inbound = parse_inbound_account_request(policy, headers)?;
    let (branch, department) = scopes.slugs(headers)?;
    let captured = CapturedHoldingMutation::capture(headers, branch, department);
    let text = std::str::from_utf8(body).map_err(|_| {
        BusinessError::invalid("INVALID_JSON", "Provide a valid command body.")
    })?;
    let wire = CommandJson::parse(text)?;
    let value: Value = serde_json::from_str(text).map_err(|_| {
        BusinessError::invalid("INVALID_JSON", "Provide a valid command body.")
    })?;
    Ok(Common {
        host: host.to_owned(),
        key: CommandKey::new(key)?,
        proof: inbound.into_proof(),
        captured,
        wire,
        value,
    })
}

fn outcome(result: Result<CommandJson, PgTransactionError<BusinessError>>) -> Result<Value, BusinessError> {
    let outcome = result.map_err(|error| match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "INVENTORY_COMMAND_UNAVAILABLE",
            "The inventory command is unavailable.",
        ),
    })?;
    serde_json::from_str(outcome.as_str()).map_err(|_| {
        BusinessError::new(
            FailureKind::Unavailable,
            "INVENTORY_COMMAND_UNAVAILABLE",
            "The inventory command is unavailable.",
        )
    })
}

fn respond(result: Result<Value, BusinessError>, status: StatusCode) -> Response {
    match result {
        Ok(data) => (status, Json(json!({"data": data}))).into_response(),
        Err(error) => business_error(error),
    }
}

async fn post_transfer_save<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<TransferState<C>>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let input: TransferSaveInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("TRANSFER_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .save(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    None,
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::CREATED)
}

async fn post_transfer_save_id<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<TransferState<C>>>,
    Path((holding_slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let input: TransferSaveInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("TRANSFER_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .save(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    Some(id),
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::OK)
}

async fn post_transfer_transition<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<TransferState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let action = TransferAction::parse(&action)
            .ok_or_else(|| invalid("TRANSFER_ACTION_INVALID"))?;
        let input: TransferActionInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("TRANSFER_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .transition(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    id,
                    action,
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::OK)
}

async fn post_landed_cost_create<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<LandedCostState<C>>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let input: LandedCostCreateInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("LANDED_COST_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .create(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::CREATED)
}

async fn post_landed_cost_transition<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<LandedCostState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let action = LandedCostAction::parse(&action)
            .ok_or_else(|| invalid("LANDED_COST_ACTION_INVALID"))?;
        let input: LandedCostActionInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("LANDED_COST_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .transition(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    id,
                    action,
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::OK)
}

async fn post_reorder_save<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReorderState<C>>>,
    Path(holding_slug): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let input: ReorderSaveInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("REORDER_INPUT_INVALID"))?;
        let captured = common.captured;
        outcome(
            target
                .save(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::CREATED)
}

async fn post_reorder_transition<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReorderState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let target = selected_company_target(&state.targets, common_host(&headers)?, &holding_slug, &headers)?;
        let common = common(&state.policy, &state.scopes, &headers, &body)?;
        let action = ReorderAction::parse(&action)
            .ok_or_else(|| invalid("REORDER_ACTION_INVALID"))?;
        let input: ReorderActionInput = serde_json::from_value(common.value.clone())
            .map_err(|_| invalid("REORDER_INPUT_INVALID"))?;
        let input = ReorderActionInput { action, ..input };
        let captured = common.captured;
        outcome(
            target
                .transition(
                    &common.host,
                    common.proof,
                    move || captured.resolve(),
                    id,
                    input,
                    common.wire,
                    common.key,
                )
                .await,
        )
    }
    .await;
    respond(result, StatusCode::OK)
}

fn common_host(headers: &HeaderMap) -> Result<&str, BusinessError> {
    headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(not_found)
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Provide a valid inventory command.")
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
