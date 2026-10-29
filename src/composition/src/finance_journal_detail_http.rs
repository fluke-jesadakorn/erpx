//! Unmounted detail HTTP; one retained read returns owner data + render context.
use crate::{
    business_http::BusinessScopeSlugs, finance_journal_detail::FinanceJournalDetailService,
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
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, RecordId};
use std::{collections::BTreeMap, sync::Arc};
/// Explicit requested workspace path selectors only. Organization/IAM checks
/// their actual current lineage after the retained Account proof.
pub struct FinanceJournalReadScopeHeaders;
impl BusinessScopeSlugs for FinanceJournalReadScopeHeaders {
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
pub struct FinanceJournalReadTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub service: Arc<FinanceJournalDetailService<C>>,
}
struct ReadState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), FinanceJournalReadTarget<C>>,
}
pub fn finance_journal_read_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceJournalReadTarget<C>>,
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
                "FINANCE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{id}",
            get(detail::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{id}/workspace",
            get(workspace::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/workspace",
            get(invoice_workspace::<C>),
        )
        .with_state(Arc::new(ReadState {
            policy,
            scopes,
            targets: configured,
        })))
}
async fn invoice_workspace<C: Clock + Send + Sync + 'static>(
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
        if id.is_empty() || id.len() > 2 * 1024 * 1024 || id.chars().any(char::is_control) {
            return Err(missing().into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        target
            .service
            .read_service_invoice(
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
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FINANCE_READ_UNAVAILABLE","message":"The invoice could not be read."}}))).into_response()};
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
async fn detail<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    read(state, slug, id, headers, false).await
}
async fn workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    read(state, slug, id, headers, true).await
}
async fn read<C: Clock + Send + Sync + 'static>(
    state: Arc<ReadState<C>>,
    slug: String,
    id: String,
    headers: HeaderMap,
    workspace: bool,
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
        let id = RecordId::new(id).map_err(|_| missing())?;
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        if workspace {
            target
                .service
                .read_workspace(host, proof, input, id)
                .await
                .and_then(|reply| serde_json::to_value(reply).map_err(|_| serialization()))
        } else {
            target
                .service
                .read(host, proof, input, id)
                .await
                .and_then(|reply| serde_json::to_value(reply).map_err(|_| serialization()))
        }
    }
    .await;
    let mut response=match result{Ok(data)=>Json(data).into_response(),Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FINANCE_READ_UNAVAILABLE","message":"The journal could not be read."}}))).into_response()};
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
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn serialization() -> PgTransactionError<BusinessError> {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_READ_UNAVAILABLE",
        "The journal could not be encoded.",
    )
    .into()
}
