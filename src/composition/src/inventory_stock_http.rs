//! Private Inventory stock-register and receipt-source HTTP. The Account/Holding
//! cookie and rendered work-context CAS are verified again inside the owner read;
//! this layer supplies only the requested workspace path selectors.
use crate::{
    http_scope::{BusinessScopeSlugs, selected_company_target},
    inventory_stock_read::InventoryStockRegisterService,
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

pub struct InventoryStockHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<InventoryStockRegisterService<C>>,
}

struct ReadState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), InventoryStockHttpTarget<C>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn inventory_stock_http_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<InventoryStockHttpTarget<C>>,
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
                "INVENTORY_STOCK_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/inventory/stock-register",
            get(read_register::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inventory/receipts/{order_id}",
            get(read_receipt::<C>),
        )
        .with_state(Arc::new(ReadState {
            policy,
            scopes,
            targets: configured,
        })))
}

fn scope<C>(
    state: &ReadState<C>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(String, InventoryStockHttpTarget<C>, CapturedHoldingMutation), BusinessError> {
    let host = unique_request_header(headers, "host")?.ok_or_else(missing)?;
    let target = selected_company_target(&state.targets, host, slug, headers)?;
    let inbound = parse_inbound_account_request(&state.policy, headers)?;
    if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
        return Err(missing());
    }
    if unique_request_header(headers, "origin")?
        .is_some_and(|value| value != inbound.expected_origin())
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let (branch, department) = state.scopes.slugs(headers)?.clone();
    let captured = CapturedHoldingMutation::capture(headers, branch, department);
    Ok((host.to_owned(), selected_target_clone(target), captured))
}

fn selected_target_clone<C>(target: &InventoryStockHttpTarget<C>) -> InventoryStockHttpTarget<C> {
    InventoryStockHttpTarget {
        host: target.host.clone(),
        holding_slug: target.holding_slug.clone(),
        service: target.service.clone(),
    }
}

fn error_response(error: PgTransactionError<BusinessError>) -> Response {
    match error {
        PgTransactionError::Business(error) => {
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
        PgTransactionError::Persistence(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn security_headers(mut response: Response) -> Response {
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

async fn read_register<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let (host, target, captured) = scope(&state, &headers, &slug)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let company = target.service.company_id().clone();
        target
            .service
            .read_register(host.as_str(), inbound.into_proof(), move || {
                captured.resolve_read(company)
            })
            .await
    }
    .await;
    security_headers(match result {
        Ok(data) => Json(data).into_response(),
        Err(error) => error_response(error),
    })
}

async fn read_receipt<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, order_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        if order_id.is_empty() || order_id.len() > 200 || order_id.chars().any(char::is_control) {
            return Err(PgTransactionError::Business(BusinessError::invalid(
                "INVENTORY_RECEIPT_SELECTION_INVALID",
                "Choose a current purchase order.",
            )));
        }
        let (host, target, captured) = scope(&state, &headers, &slug)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        let company = target.service.company_id().clone();
        target
            .service
            .read_receipt(
                host.as_str(),
                inbound.into_proof(),
                move || captured.resolve_read(company),
                order_id,
            )
            .await
    }
    .await;
    security_headers(match result {
        Ok(data) => Json(data).into_response(),
        Err(error) => error_response(error),
    })
}
