//! Unmounted detail HTTP; one retained read returns owner data + render context.
use crate::{
    business_http::BusinessScopeSlugs,
    field_service_reservation_read::FieldServiceReservationReadService,
};
use axum::{
    Json, Router,
    extract::Request,
    extract::{Path, State},
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
        .route("/api/v1/holdings/{holding_slug}/field-service/work",get(read_work::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/orders/{id}/detail",get(read_detail::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/billings/{key}",get(read_billing::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/agreements/reviews/{key}",get(read_agreement_review::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/agreements/creates/{key}",get(read_agreement_create::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/requests/{key}",get(read_request::<C>))
        .route("/api/v1/holdings/{holding_slug}/field-service/orders/{key}",get(read_created_order::<C>))
        .with_state(Arc::new(ReadState {policy,scopes,targets:configured})))
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
