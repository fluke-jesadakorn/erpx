//! Unmounted detail HTTP; one retained read returns owner data + render context.
use crate::{
    field_service_reservation_read::FieldServiceReservationReadService,
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Json, Router,
    extract::Request,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
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
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use std::{collections::BTreeMap, sync::Arc};
/// Explicit requested workspace path selectors only. Organization/IAM checks
/// their actual current lineage after the retained Account proof.
pub struct FieldServiceReadScopeHeaders;
impl BusinessScopeSlugs for FieldServiceReadScopeHeaders {
    fn slugs(
        &self,
        headers: &HeaderMap,
    ) -> Result<(Option<String>, Option<String>), BusinessError> {
        let slug = |key| -> Result<Option<String>, BusinessError> {
            unique_request_header(headers, key)?
                .map(|v| {
                    if v.is_empty()
                        || v.len() > 128
                        || !v.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_'
                        })
                    {
                        return Err(BusinessError::invalid(
                            "INVALID_WORKSPACE_SELECTOR",
                            "Use an exact workspace slug.",
                        ));
                    }
                    Ok(v.to_owned())
                })
                .transpose()
        };
        Ok((
            slug("x-erpx-workspace-branch")?,
            slug("x-erpx-workspace-department")?,
        ))
    }
}
pub struct FieldServiceReservationReadTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<FieldServiceReservationReadService<C>>,
}
struct ReadState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), FieldServiceReservationReadTarget<C>>,
}
pub fn field_service_reservation_read_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FieldServiceReservationReadTarget<C>>,
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
                "FIELD_SERVICE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/field-service/{order_id}/reservations/{reservation_id}",get(read::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/{order_id}/issues/{key}",get(read_issue::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/visits/{key}",get(read_visit::<C>))
        .route("/api/v1/holdings/{holding_slug}/attachment-sets/{set_id}/bind",post(attachment_bind::<C>))
        .route("/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/status",get(attachment_status::<C>))
        .route("/api/v1/holdings/{holding_slug}/attachment-links/{link_id}/withdraw",post(attachment_withdraw::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/workspace",get(read_workspace::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup",get(read_parts_posting::<C>).post(initialize_parts_posting::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/existing",post(confirm_parts_posting::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/prerequisites",get(read_parts_prerequisites::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/stock",get(read_stock_setup::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock",get(read_initial_count_selection::<C>).post(create_initial_count::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}",get(read_initial_count_record::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/flow",get(read_initial_count_flow::<C>).post(assign_initial_count::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/{action}",post(act_initial_count::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count/{action}",post(act_generic_initial_count::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/initial-physical-stock/{id}/stock-count",get(read_initial_count_generic_detail::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/masters/products",post(create_stock_product::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/masters/warehouses",post(create_stock_warehouse::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/charts",post(create_parts_chart::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/accounts",post(create_parts_account::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/periods",post(create_parts_period::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/parts-setup/rates",post(confirm_parts_rate::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/work",get(read_work::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/orders/{id}/detail",get(read_detail::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/orders/{id}/source-record",get(read_source_record::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/billings/{key}",get(read_billing::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/agreements/reviews/{key}",get(read_agreement_review::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/agreements/creates/{key}",get(read_agreement_create::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/requests/{key}",get(read_request::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/orders/{key}",get(read_created_order::<C>))
        .with_state(Arc::new(ReadState {policy,scopes,targets:configured})))
}
async fn read_stock_setup<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let registered_company = target.service.registered_setup_companies();
        target
            .service
            .stock_setup(host, inbound.into_proof(), move || {
                scope?;
                if registered_company { captured.resolve_rendered_read() } else { captured.resolve_read(company) }
            })
            .await
    }
    .await;
    posting_response(result)
}
async fn create_stock_product<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_stock_master(s, p, r, "products").await
}
async fn create_stock_warehouse<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_stock_master(s, p, r, "warehouses").await
}
async fn write_stock_master<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
    kind: &'static str,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(&headers, "idempotency-key").and_then(|v| {
            v.map(str::to_owned).ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
            })
        });
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        target
            .service
            .stock_command(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve()
                },
                crate::service_stock_setup::ServiceStockTransportCommand { key, body, kind },
            )
            .await
    }
    .await;
    match result {
        Ok(value) => {
            let rendered = match serde_json::to_string(&value.context) {
                Ok(v) => v,
                Err(_) => {
                    return posting_response::<serde_json::Value>(Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "SERVICE_POSTING_UNAVAILABLE",
                        "The Service parts setup is unavailable.",
                    )
                    .into()));
                }
            };
            let body = format!(
                "{{\"data\":{},\"context\":{rendered}}}",
                value.data.as_str()
            );
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response.headers_mut().insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            );
            response.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            response
        }
        Err(error) => posting_response::<serde_json::Value>(Err(error)),
    }
}
async fn read_parts_prerequisites<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let registered_company = target.service.registered_setup_companies();
        target
            .service
            .parts_prerequisites(host, inbound.into_proof(), move || {
                scope?;
                if registered_company { captured.resolve_rendered_read() } else { captured.resolve_read(company) }
            })
            .await
    }
    .await;
    posting_response(result)
}
async fn create_parts_chart<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_parts_prerequisite(
        s,
        p,
        r,
        erpx_server_finance::public::service_parts_prerequisites::ServicePrerequisiteKind::Chart,
    )
    .await
}
async fn create_parts_account<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_parts_prerequisite(
        s,
        p,
        r,
        erpx_server_finance::public::service_parts_prerequisites::ServicePrerequisiteKind::Account,
    )
    .await
}
async fn create_parts_period<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_parts_prerequisite(
        s,
        p,
        r,
        erpx_server_finance::public::service_parts_prerequisites::ServicePrerequisiteKind::Period,
    )
    .await
}
async fn confirm_parts_rate<C: Clock + Send + Sync + 'static>(
    s: State<Arc<ReadState<C>>>,
    p: Path<String>,
    r: Request,
) -> Response {
    write_parts_prerequisite(
        s,
        p,
        r,
        erpx_server_finance::public::service_parts_prerequisites::ServicePrerequisiteKind::Rate,
    )
    .await
}
async fn write_parts_prerequisite<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
    kind: erpx_server_finance::public::service_parts_prerequisites::ServicePrerequisiteKind,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(&headers, "idempotency-key").and_then(|v| {
            v.map(str::to_owned).ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
            })
        });
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        target
            .service
            .parts_prerequisite_command(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve()
                },
                crate::service_parts_prerequisite_setup::ServicePrerequisiteTransportCommand {
                    key,
                    body,
                    kind,
                },
            )
            .await
    }
    .await;
    match result {
        Ok(value) => {
            let rendered = match serde_json::to_string(&value.context) {
                Ok(v) => v,
                Err(_) => {
                    return posting_response::<serde_json::Value>(Err(BusinessError::new(
                        FailureKind::Unavailable,
                        "SERVICE_POSTING_UNAVAILABLE",
                        "The Service parts setup is unavailable.",
                    )
                    .into()));
                }
            };
            let body = format!(
                "{{\"data\":{},\"context\":{rendered}}}",
                value.data.as_str()
            );
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            response.headers_mut().insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            );
            response.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            response
        }
        Err(error) => posting_response::<serde_json::Value>(Err(error)),
    }
}
fn posting_response<T: serde::Serialize>(
    result: Result<T, PgTransactionError<BusinessError>>,
) -> Response {
    let mut response=match result {Ok(value)=>Json(value).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"SERVICE_POSTING_UNAVAILABLE","message":"The Service parts setup is unavailable."}}))).into_response()};
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
async fn read_parts_posting<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let registered_company = target.service.registered_setup_companies();
        target
            .service
            .parts_posting_setup(host, inbound.into_proof(), move || {
                scope?;
                if registered_company { captured.resolve_rendered_read() } else { captured.resolve_read(company) }
            })
            .await
    }
    .await;
    posting_response(result)
}
async fn initialize_parts_posting<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    path: Path<String>,
    request: Request,
) -> Response {
    write_parts_posting(
        state,
        path,
        request,
        erpx_server_commerce::public::service_parts_posting::PostingMode::Initialize,
    )
    .await
}
async fn confirm_parts_posting<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    path: Path<String>,
    request: Request,
) -> Response {
    write_parts_posting(
        state,
        path,
        request,
        erpx_server_commerce::public::service_parts_posting::PostingMode::ConfirmExisting,
    )
    .await
}
async fn write_parts_posting<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
    mode: erpx_server_commerce::public::service_parts_posting::PostingMode,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(&headers, "idempotency-key").and_then(|v| {
            v.map(str::to_owned).ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
            })
        });
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        target
            .service
            .parts_posting_command(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve()
                },
                crate::service_parts_posting_setup::ServicePostingTransportCommand {
                    key,
                    body,
                    mode,
                },
            )
            .await
    }
    .await;
    posting_response(result)
}
async fn read<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, order, reservation)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if order.is_empty()
            || order.chars().any(char::is_control)
            || reservation.is_empty()
            || reservation.chars().any(char::is_control)
            || order.len().saturating_add(reservation.len()) > 2 * 1024 * 1024
        {
            return Err(missing().into());
        }
        target
            .service
            .read(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                order,
                reservation,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The reservation could not be read."}}))).into_response()};
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

async fn read_issue<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, order, key)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if order.is_empty()
            || order.chars().any(char::is_control)
            || key.is_empty()
            || key.chars().any(char::is_control)
            || order.len().saturating_add(key.len()) > 2 * 1024 * 1024
        {
            return Err(missing().into());
        }
        target
            .service
            .read_issue(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                order,
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The issue could not be read."}}))).into_response()};
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

async fn read_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_workspace(host, inbound.into_proof(), move || {
                scope?;
                captured.resolve_read(company)
            })
            .await
    }
    .await;
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Service workspace could not be read."}}))).into_response()};
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
async fn read_work<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_work(host, inbound.into_proof(), move || {
                scope?;
                captured.resolve_read(company)
            })
            .await
    }
    .await;
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Service workspace could not be read."}}))).into_response()};
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
#[derive(serde::Deserialize)]
struct SourceRecordQuery {
    version: Option<String>,
}
async fn read_source_record<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<SourceRecordQuery>,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        if id.is_empty() || id.encode_utf16().count() > 180 || id.contains('\0') {
            return Err(BusinessError::invalid(
                "SOURCE_RECORD_INVALID",
                "Provide a valid source record.",
            )
            .into());
        }
        let version = query
            .version
            .map(|v| {
                v.parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite() && *n > 0.0 && n.fract() == 0.0)
                    .ok_or_else(|| {
                        BusinessError::invalid(
                            "SOURCE_RECORD_INVALID",
                            "Provide a valid source version.",
                        )
                    })
            })
            .transpose()?;
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_source_record(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                id,
                version,
            )
            .await
    }
    .await;
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Service source record could not be read."}}))).into_response()};
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
async fn read_detail<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        if id.is_empty() || id.len() > 2 * 1024 * 1024 || id.contains('\0') {
            return Err(missing().into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_detail(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                id,
            )
            .await
    }
    .await;
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Service detail could not be read."}}))).into_response()};
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
async fn read_visit<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 2 * 1024 * 1024 {
            return Err(missing().into());
        }
        target
            .service
            .read_visit(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The visit could not be read."}}))).into_response()};
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

async fn read_billing<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 2 * 1024 * 1024 {
            return Err(missing().into());
        }
        target
            .service
            .read_billing(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The billing release could not be read."}}))).into_response()};
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

async fn read_agreement_review<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 2 * 1024 * 1024 {
            return Err(missing().into());
        }
        target
            .service
            .read_agreement_review(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Agreement review could not be read."}}))).into_response()};
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

async fn read_agreement_create<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 2 * 1024 * 1024 {
            return Err(missing().into());
        }
        target
            .service
            .read_agreement_create(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The Agreement create evidence could not be read."}}))).into_response()};
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

async fn read_request<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 180 {
            return Err(missing().into());
        }
        target
            .service
            .read_request(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The request could not be read."}}))).into_response()};
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

async fn read_created_order<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if key.is_empty() || key.chars().any(char::is_control) || key.len() > 180 {
            return Err(missing().into());
        }
        target
            .service
            .read_created_order(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                key,
            )
            .await
    }
    .await;
    let mut response=match result {Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FIELD_SERVICE_READ_UNAVAILABLE","message":"The order could not be read."}}))).into_response()};
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

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn attachment_response(
    result: Result<
        crate::field_attachment::FieldAttachmentStatusResponse,
        PgTransactionError<BusinessError>,
    >,
) -> Response {
    let mut response=match result {Ok(value)=>Json(value).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"ATTACHMENT_UNAVAILABLE","message":"The file link is unavailable."}}))).into_response()};
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
async fn attachment_status<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        if id.is_empty() || id.len() > 2 * 1024 * 1024 || id.contains('\0') {
            return Err(missing().into());
        }
        target
            .service
            .attachment_status(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                id,
            )
            .await
    }
    .await;
    attachment_response(result)
}
async fn attachment_withdraw<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(&headers, "idempotency-key")?
            .filter(|v| {
                (8..=180).contains(&v.len())
                    && v.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-')
                    })
            })
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "COMMAND_KEY_REQUIRED",
                    "A valid Idempotency-Key is required.",
                )
            })?
            .to_owned();
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured =
            CapturedHoldingMutation::capture(&headers, branch.clone(), department.clone());
        let captured_read = CapturedHoldingMutation::capture(&headers, branch, department);
        let scope_read = state.scopes.slugs(&headers);
        let company = target.service.company_id().clone();
        let company_read = company.clone();
        if id.is_empty() || id.len() > 2 * 1024 * 1024 || id.contains('\0') {
            return Err(missing().into());
        }
        let proof = inbound.into_proof();
        target
            .service
            .attachment_withdraw(
                host,
                proof.clone(),
                move || {
                    scope?;
                    captured.resolve()
                },
                id.clone(),
                key,
                body,
            )
            .await?;
        target
            .service
            .attachment_status(
                host,
                proof,
                move || {
                    scope_read?;
                    captured_read.resolve_read(company_read)
                },
                id,
            )
            .await
    }
    .await;
    attachment_response(result)
}

async fn attachment_bind<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(&headers, "idempotency-key")?
            .filter(|v| {
                (8..=180).contains(&v.len())
                    && v.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'.' | b'-')
                    })
            })
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::InvalidInput,
                    "COMMAND_KEY_REQUIRED",
                    "A valid Idempotency-Key is required.",
                )
            })?
            .to_owned();
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        if id.is_empty() || id.len() > 2 * 1024 * 1024 || id.contains('\0') {
            return Err(missing().into());
        }
        target
            .service
            .attachment_bind(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve()
                },
                id,
                key,
                body,
            )
            .await
    }
    .await;
    let mut response=match result{Ok(v)=>Json(v).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"ATTACHMENT_UNAVAILABLE","message":"The file set is unavailable."}}))).into_response()};
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

// Count routes retain the same Account capture and trusted target boundaries as
// other Field reads. Their service remains disabled without an explicit opt-in.
async fn read_initial_count_selection<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> Response {
    read_initial_count(
        state,
        slug,
        headers,
        crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute::Selection {
            query,
        },
    )
    .await
}
async fn read_initial_count_record<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> Response {
    read_initial_count(
        state,
        slug,
        headers,
        crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute::Record {
            id,
            query,
        },
    )
    .await
}
async fn read_initial_count_generic_detail<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>, Path((slug,id)): Path<(String,String)>,
    axum::extract::RawQuery(query): axum::extract::RawQuery, headers: HeaderMap,
) -> Response {
    read_initial_count(state,slug,headers,crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute::GenericDetail{id,query}).await
}
async fn read_initial_count_flow<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    axum::extract::RawQuery(query): axum::extract::RawQuery,
    headers: HeaderMap,
) -> Response {
    read_initial_count(
        state,
        slug,
        headers,
        crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute::Flow {
            id,
            query,
        },
    )
    .await
}
async fn read_initial_count<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    slug: String,
    headers: HeaderMap,
    route: crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute,
) -> Response {
    use crate::service_initial_physical_stock_transport::ServiceInitialCountReadRoute;
    if matches!(&route, ServiceInitialCountReadRoute::Record {id,..} | ServiceInitialCountReadRoute::Flow {id,..} | ServiceInitialCountReadRoute::GenericDetail {id,..} if id == "masters") {
        return initial_count_response::<serde_json::Value>(Err(missing().into()), false);
    }
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = state
            .targets
            .get(&(host.into(), slug))
            .ok_or_else(missing)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);

        target
            .service
            .initial_count_read(
                host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_rendered_read()
                },
                route,
            )
            .await
    }
    .await;
    initial_count_response(result, false)
}
async fn create_initial_count<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    write_initial_count(
        state,
        slug,
        request,
        crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute::Create,
    )
    .await
}
async fn assign_initial_count<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    write_initial_count(
        state,
        slug,
        request,
        crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute::Assign {
            id,
        },
    )
    .await
}
async fn act_initial_count<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path((slug, id, action)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    use crate::service_initial_physical_stock::CountAction;
    if action == "update" {
        return write_initial_count(
            state, slug, request,
            crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute::Update { id },
        ).await;
    }
    let action = match action.as_str() {
        "submit" => CountAction::Submit,
        "approve" => CountAction::Approve,
        "post" => CountAction::Post,
        "reject" => CountAction::Reject,
        "cancel" => CountAction::Cancel,
        _ => return initial_count_response::<serde_json::Value>(Err(missing().into()), false),
    };
    write_initial_count(
        state,
        slug,
        request,
        crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute::Action {
            id,
            action,
        },
    )
    .await
}
async fn act_generic_initial_count<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    Path((slug, id, action)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    use crate::service_initial_physical_stock::CountAction;
    use crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute;
    let route = if action == "flow" {
        ServiceInitialCountCommandRoute::GenericAssign { id }
    } else {
        let action = match action.as_str() {
            "submit" => CountAction::Submit,
            "approve" => CountAction::Approve,
            "post" => CountAction::Post,
            "reject" => CountAction::Reject,
            "cancel" => CountAction::Cancel,
            _ => return initial_count_response::<serde_json::Value>(Err(missing().into()), false),
        };
        ServiceInitialCountCommandRoute::GenericAction { id, action }
    };
    write_initial_count(state, slug, request, route).await
}
async fn write_initial_count<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    slug: String,
    request: Request,
    route: crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute,
) -> Response {
    use crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute;
    if matches!(&route, ServiceInitialCountCommandRoute::Action {id,..} | ServiceInitialCountCommandRoute::Assign {id} | ServiceInitialCountCommandRoute::Update {id} | ServiceInitialCountCommandRoute::GenericAction {id,..} | ServiceInitialCountCommandRoute::GenericAssign {id} if id == "masters") {
        return initial_count_response::<serde_json::Value>(Err(missing().into()), false);
    }
    let created = matches!(
        &route,
        crate::service_initial_physical_stock_transport::ServiceInitialCountCommandRoute::Create
    );
    let (parts, body) = request.into_parts();
    let headers = parts.headers;
    let result=async {
        let host=unique_request_header(&headers,"host")?.ok_or_else(missing)?;
        let target=state.targets.get(&(host.into(),slug)).ok_or_else(missing)?;
        let inbound=parse_inbound_account_request(&state.policy,&headers)?;
        if !matches!(inbound.audience(),RequestAudience::Holding{..}){return Err(missing().into());}
        if unique_request_header(&headers,"origin")?.is_some_and(|v|v!=inbound.expected_origin()){return Err(BusinessError::new(FailureKind::Forbidden,"ORIGIN_FORBIDDEN","Use the application origin.").into());}
        let key=unique_request_header(&headers,"idempotency-key").and_then(|v|v.map(str::to_owned).ok_or_else(||BusinessError::invalid("COMMAND_KEY_REQUIRED","Idempotency-Key is required.")));
        let scope=state.scopes.slugs(&headers);let(branch,department)=scope.as_ref().cloned().unwrap_or_default();let captured=CapturedHoldingMutation::capture(&headers,branch,department);
        target.service.initial_count_command(host,inbound.into_proof(),move||{scope?;captured.resolve()},crate::service_initial_physical_stock_transport::ServiceInitialCountTransportCommand{key,body,route}).await
    }.await;
    initial_count_response(result, created)
}
fn initial_count_response<T: serde::Serialize>(
    result: Result<T, PgTransactionError<BusinessError>>,
    created: bool,
) -> Response {
    let result = match result {
        Err(PgTransactionError::Persistence(_)) => Err(BusinessError::new(
            FailureKind::Unavailable,
            "SERVICE_INITIAL_PHYSICAL_STOCK_UNAVAILABLE",
            "The Service initial physical stock is unavailable.",
        )
        .into()),
        other => other,
    };
    let mut response = posting_response(result);
    if created && response.status() == StatusCode::OK {
        *response.status_mut() = StatusCode::CREATED;
    }
    response
}
