//! Private inbound routes; no bootstrap/main/live mount is included here.
use crate::{
    finance_journal_review::FinanceJournalReviewAdministration,
    legal_queue_administration::{LegalQueueAdministration, LegalQueueRequest},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use erpx_server_finance::public::journal_review_wire::{
    normalize_journal_post, normalize_journal_reverse,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_legal::public::LegalProcessingKind;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
use std::{collections::BTreeMap, sync::Arc};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Trusted bootstrap data, validated against the actual Control binding registry.
/// No Deserialize, request-supplied factory, actor or permission defaults.
pub struct BusinessHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub finance: Option<Arc<FinanceJournalReviewAdministration<C>>>,
    pub legal: Option<Arc<LegalQueueAdministration<C>>>,
}
/// Scope selectors only, never principal/authority facts. The root gateway
/// adapter must supply actual branch/department requested path selectors.
pub trait BusinessScopeSlugs: Send + Sync + 'static {
    fn slugs(&self, headers: &HeaderMap)
    -> Result<(Option<String>, Option<String>), BusinessError>;
}
struct BusinessHttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), BusinessHttpTarget<C>>,
}
pub fn business_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    holdings: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<BusinessHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut registered = BTreeMap::new();
    for target in targets {
        let matching = policy
            .audience(&target.host)
            .and_then(|audience| holdings.expected_for_audience(&audience).ok())
            .is_some_and(|entry| entry.slug == target.holding_slug);
        if !matching
            || target.holding_slug.is_empty()
            || !policy.holdings.iter().any(|h| h.host == target.host)
            || registered
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "BUSINESS_HTTP_CONFIGURATION_INVALID",
                "Configure distinct actual Holding hosts and slugs.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{record_id}/post",
            post(finance_post::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{record_id}/reverse",
            post(finance_reverse::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/documents/{record_id}/ocr",
            post(legal_ocr::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/legal/documents/{record_id}/verify",
            post(legal_verify::<C>),
        )
        .with_state(Arc::new(BusinessHttpState {
            policy,
            scopes,
            targets: registered,
        })))
}
fn target<'a, C>(
    state: &'a BusinessHttpState<C>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a BusinessHttpTarget<C>, BusinessError> {
    let host = unique_request_header(headers, "host")?.ok_or_else(not_found)?;
    state
        .targets
        .get(&(host.into(), slug.into()))
        .ok_or_else(not_found)
}
fn check_origin(headers: &HeaderMap, expected: &str, required: bool) -> Result<(), BusinessError> {
    let origin = unique_request_header(headers, "origin")?;
    if required && origin.is_none() || origin.is_some_and(|value| value != expected) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    Ok(())
}
fn target_origin<'a, C>(
    state: &'a BusinessHttpState<C>,
    target: &BusinessHttpTarget<C>,
) -> Result<&'a str, BusinessError> {
    state
        .policy
        .holdings
        .iter()
        .find(|h| h.host == target.host)
        .map(|h| h.origin.as_str())
        .ok_or_else(not_found)
}
async fn finance_post<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BusinessHttpState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    finance(state, slug, id, request, false).await
}
async fn finance_reverse<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BusinessHttpState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    finance(state, slug, id, request, true).await
}
async fn finance<C: Clock + Send + Sync + 'static>(
    state: Arc<BusinessHttpState<C>>,
    slug: String,
    id: String,
    request: Request,
    reverse: bool,
) -> Response {
    let result = async {
        let target = target(&state, request.headers(), &slug)?;
        let administration = target.finance.clone().ok_or_else(not_found)?;
        check_origin(request.headers(), target_origin(&state, target)?, false)?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found().into());
        }
        let host = inbound.host().to_owned();
        let proof = inbound.into_proof();
        let scopes = state.scopes.slugs(request.headers());
        let (branch, department) = scopes.as_ref().cloned().unwrap_or_default();
        let workspace = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let mut command_headers = HeaderMap::new();
        for value in request.headers().get_all("idempotency-key") {
            command_headers.append("idempotency-key", value.clone());
        }
        // Request.json() does not require a Content-Type. Read a bounded immutable
        // buffer here, but emit any read/schema/key failure INSIDE current authority.
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        let prepared = move || {
            let bytes = body.as_ref().map_err(Clone::clone)?;
            let key = || {
                let value = unique_request_header(&command_headers, "idempotency-key")?
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        BusinessError::invalid(
                            "IDEMPOTENCY_KEY_REQUIRED",
                            "Idempotency-Key is required.",
                        )
                    })?;
                CommandKey::new(value)
            };
            // Validation still precedes the idempotency-key error as in B0.
            if reverse {
                let wire = normalize_journal_reverse(bytes).map_err(|f| f.error)?;
                Ok(wire.into_command(key()?, RecordId::new(&id)?))
            } else {
                let wire = normalize_journal_post(bytes).map_err(|f| f.error)?;
                Ok(wire.into_command(key()?, RecordId::new(&id)?))
            }
        };
        administration
            .review_transport(
                &host,
                proof,
                move || {
                    scopes?;
                    workspace.resolve()
                },
                prepared,
            )
            .await
    }
    .await;
    match result {
        Ok(data) => private(
            (
                if reverse {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                },
                Json(serde_json::json!({"data":data})),
            )
                .into_response(),
        ),
        Err(error) => failure(error),
    }
}
async fn legal_ocr<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BusinessHttpState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    legal(state, slug, id, headers, LegalProcessingKind::Ocr).await
}
async fn legal_verify<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<BusinessHttpState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    legal(state, slug, id, headers, LegalProcessingKind::Verify).await
}
async fn legal<C: Clock + Send + Sync + 'static>(
    state: Arc<BusinessHttpState<C>>,
    slug: String,
    id: String,
    headers: HeaderMap,
    kind: LegalProcessingKind,
) -> Response {
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let administration = target.legal.clone().ok_or_else(not_found)?;
        check_origin(&headers, target_origin(&state, target)?, true)?;
        // Original Legal checks key before WorkContext and never reads this body.
        let key = unique_request_header(&headers, "idempotency-key")?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let key = CommandKey::new(key)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found().into());
        }
        let host = inbound.host().to_owned();
        let proof = inbound.into_proof();
        let scopes = state.scopes.slugs(&headers);
        let (branch, department) = scopes.as_ref().cloned().unwrap_or_default();
        let workspace = CapturedHoldingMutation::capture(&headers, branch, department);
        administration
            .queue_transport(
                &host,
                proof,
                move || {
                    scopes?;
                    workspace.resolve()
                },
                LegalQueueRequest {
                    key,
                    document_id: RecordId::new(id)?,
                    kind,
                },
            )
            .await
    }
    .await;
    match result {
        Ok(data) => private(Json(serde_json::json!({"data":data})).into_response()),
        Err(error) => failure(error),
    }
}
fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn private(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
fn failure(error: PgTransactionError<BusinessError>) -> Response {
    match error{
 PgTransactionError::Persistence(_)=>private((StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"INTERNAL_ERROR","message":"The request could not be completed."}}))).into_response()),
 PgTransactionError::Business(error)=>{
 let status=match error.kind{FailureKind::InvalidInput if error.code=="BODY_TOO_LARGE"=>StatusCode::PAYLOAD_TOO_LARGE,FailureKind::InvalidInput if ["INVALID_COOKIE_HEADER","COOKIE_HEADER_TOO_LARGE","AMBIGUOUS_COOKIE_HEADER","INVALID_SAVED_ACCOUNT_PROOF","INVALID_SESSION_PROOF","AMBIGUOUS_REQUEST_AUTHORITY","INVALID_REQUEST_HEADER"].contains(&error.code)=>StatusCode::BAD_REQUEST,FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};
 if status==StatusCode::NOT_FOUND{private((status,"Not found.").into_response())}else{private((status,Json(serde_json::json!({"error":{"code":error.code,"message":error.message}}))).into_response())}
 }}
}
