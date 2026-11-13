//! Private HTTP ingress for the native Manufacturing order material/QC commands.
use crate::http_scope::{BusinessScopeSlugs, selected_company_target};
use crate::manufacturing_order_commands::{
    ManufacturingOrderAction, ManufacturingOrderCommand, ManufacturingOrderInput,
};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_identity_access::public::{
    AccountPolicy, account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone)]
pub struct ManufacturingOrderHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub command: Arc<ManufacturingOrderCommand<C>>,
}

#[derive(Clone)]
struct ManufacturingOrderHttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), Arc<ManufacturingOrderCommand<C>>>,
}

pub fn manufacturing_order_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<ManufacturingOrderHttpTarget<C>>,
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
                "MANUFACTURING_ORDER_HTTP_CONFIGURATION_INVALID",
                "Configure distinct Manufacturing Holding targets.",
            ));
        }
    }
    if registered.is_empty() {
        return Err(BusinessError::invalid(
            "MANUFACTURING_ORDER_HTTP_CONFIGURATION_INVALID",
            "Configure at least one Manufacturing Holding target.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/manufacturing/orders/{id}/{action}",
            post(change_order::<C>),
        )
        .with_state(Arc::new(ManufacturingOrderHttpState {
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
    let text = std::str::from_utf8(body).map_err(|_| {
        BusinessError::invalid("INVALID_JSON", "Provide a valid production command body.")
    })?;
    let wire = CommandJson::parse(text)?;
    let value = serde_json::from_str(text).map_err(|_| {
        BusinessError::invalid("INVALID_JSON", "Provide a valid production command body.")
    })?;
    Ok(RequestFacts {
        key: CommandKey::new(key)?,
        wire,
        value,
    })
}

async fn change_order<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ManufacturingOrderHttpState<C>>>,
    Path((holding_slug, id, action)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let result = async {
        let action = ManufacturingOrderAction::parse(&action).ok_or_else(not_found)?;
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
        let input: ManufacturingOrderInput =
            serde_json::from_value(facts.value).map_err(|_| {
                BusinessError::invalid(
                    "MANUFACTURING_ORDER_INPUT_INVALID",
                    "Provide a valid production command.",
                )
            })?;
        let outcome = target
            .change(
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
                    "MANUFACTURING_ORDER_UNAVAILABLE",
                    "The production command is unavailable.",
                ),
            })?;
        let data: Value = serde_json::from_str(outcome.result.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "MANUFACTURING_ORDER_UNAVAILABLE",
                "The production command is unavailable.",
            )
        })?;
        Ok::<_, BusinessError>(Json(json!({"data": data, "context": outcome.context})).into_response())
    }
    .await;
    match result {
        Ok(response) => response,
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
