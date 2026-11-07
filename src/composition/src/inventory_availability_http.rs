//! Private Inventory availability read HTTP. The Account/Holding cookie and the
//! rendered work-context CAS are verified again inside the owner read; this
//! layer supplies only the requested workspace path selectors.
use crate::{
    http_scope::{BusinessScopeSlugs, selected_company_target},
    inventory_availability_read::InventoryAvailabilityReadService,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_inbound_http::{CapturedHoldingMutation, parse_inbound_account_request, unique_request_header},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

pub struct InventoryAvailabilityHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<InventoryAvailabilityReadService<C>>,
}

struct ReadState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), InventoryAvailabilityHttpTarget<C>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn inventory_availability_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryAvailabilityHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let key = (
            target.host.clone(),
            target.holding_slug.clone(),
            target.service.company_id().as_str().to_owned(),
        );
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || configured.insert(key, target).is_some()
        {
            return Err(BusinessError::invalid(
                "INVENTORY_AVAILABILITY_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/inventory/availability",
            get(read::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inventory/movements",
            get(read_movements::<C>),
        )
        .with_state(Arc::new(ReadState {
            policy,
            scopes,
            targets: configured,
        })))
}

#[derive(serde::Deserialize)]
struct MovementQuery {
    cursor: Option<String>,
}

async fn read_movements<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    Query(query): Query<MovementQuery>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        if query
            .cursor
            .as_deref()
            .is_some_and(|value| value.is_empty() || value.len() > 400 || value.chars().any(char::is_control))
        {
            return Err(BusinessError::invalid(
                "INVENTORY_MOVEMENT_CURSOR_INVALID",
                "Use the cursor from the current movement page.",
            )
            .into());
        }
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = selected_company_target(&state.targets, host, &slug, &headers)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let (branch, department) = state.scopes.slugs(&headers)?.clone();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_movements(
                host,
                inbound.into_proof(),
                move || captured.resolve_read(company),
                query.cursor,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(data) => Json(data).into_response(),
        Err(PgTransactionError::Business(error)) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            let body = Json(serde_json::json!({
                "error": {"code": error.code, "message": error.message}
            }))
            .into_response();
            let mut response = Response::new(body.into_body());
            *response.status_mut() = status;
            response
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

async fn read<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = selected_company_target(&state.targets, host, &slug, &headers)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let (branch, department) = state.scopes.slugs(&headers)?.clone();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read(
                host,
                inbound.into_proof(),
                move || captured.resolve_read(company),
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(data) => Json(data).into_response(),
        Err(PgTransactionError::Business(error)) => {
            let status = match error.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            let body = Json(serde_json::json!({
                "error": {"code": error.code, "message": error.message}
            }))
            .into_response();
            let mut response = Response::new(body.into_body());
            *response.status_mut() = status;
            response
        }
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}
