//! Additive private source-admin ingress. No listener, bootstrap allowlist,
//! generic IAM handler, platform route or runtime activation is included.
use crate::{
    business_http::BusinessScopeSlugs,
    directory_source_administration::DirectorySourceAdministration,
    directory_source_wire::{DirectorySourceResponse, parse_approve, parse_preview},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
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
pub const DIRECTORY_SOURCE_BODY_LIMIT: usize = 16 * 1024;
pub struct DirectorySourceHttpTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub administration: Arc<DirectorySourceAdministration<C>>,
}
struct HttpState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), Arc<DirectorySourceAdministration<C>>>,
}
pub fn directory_source_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<DirectorySourceHttpTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut registered = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|a| registry.expected_for_audience(&a).ok())
            .is_some_and(|r| r.slug == target.holding_slug)
            && policy.holdings.iter().any(|h| h.host == target.host);
        if !valid
            || registered
                .insert((target.host, target.holding_slug), target.administration)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "DIRECTORY_SOURCE_HTTP_CONFIGURATION_INVALID",
                "Configure distinct registered Holding source-admin targets.",
            ));
        }
    }
    Ok(Router::new().route("/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/preview",post(preview::<C>))
 .route("/api/v1/holdings/{holding_slug}/iam/connections/{connection_id}/source/reviews/{review_id}/approve",post(approve::<C>))
 .with_state(Arc::new(HttpState{policy,scopes,targets:registered})).layer(axum::middleware::map_response(private_response)))
}
async fn preview<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<C>>>,
    Path((slug, connection)): Path<(String, String)>,
    request: Request,
) -> Response {
    handle(state, slug, connection, None, request).await
}
async fn approve<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<HttpState<C>>>,
    Path((slug, connection, review)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    handle(state, slug, connection, Some(review), request).await
}
fn key(headers: &HeaderMap) -> Result<String, BusinessError> {
    unique_request_header(headers, "idempotency-key")?
        .filter(|k| !k.is_empty() && k.len() <= 256)
        .map(str::to_owned)
        .ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "A valid Idempotency-Key is required.",
            )
        })
}
async fn handle<C: Clock + Send + Sync + 'static>(
    state: Arc<HttpState<C>>,
    slug: String,
    connection: String,
    review: Option<String>,
    request: Request,
) -> Response {
    let result = async {
        if request.uri().authority().is_some()
            || request.uri().scheme().is_some()
            || [
                "forwarded",
                "x-forwarded-host",
                "x-forwarded-proto",
                "x-forwarded-port",
                "x-original-host",
            ]
            .iter()
            .any(|h| request.headers().contains_key(*h))
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "UNTRUSTED_REQUEST_AUTHORITY",
                "Use the registered Holding origin.",
            )
            .into());
        }
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found().into());
        }
        let administration = state
            .targets
            .get(&(inbound.host().into(), slug))
            .cloned()
            .ok_or_else(not_found)?;
        // These are new high-risk endpoints: require one exact registered Origin.
        if unique_request_header(request.headers(), "origin")? != Some(inbound.expected_origin()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the Holding application origin.",
            )
            .into());
        }
        let host = inbound.host().to_owned();
        let proof = inbound.into_proof();
        let scopes = state.scopes.slugs(request.headers());
        let (branch, department) = scopes.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let key = key(request.headers());
        // Buffer only a bounded immutable body. Defer every read/schema/key error
        // until the original Control session, exact MFA and current IAM are proved.
        let bytes = to_bytes(request.into_body(), DIRECTORY_SOURCE_BODY_LIMIT)
            .await
            .map(Arc::new)
            .map_err(|_| {
                BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller Directory source request.")
            });
        let source = connection.clone();
        let output = if let Some(review) = review {
            administration
                .approve_transport(
                    &host,
                    proof,
                    move || {
                        scopes?;
                        captured.resolve()
                    },
                    connection,
                    review.clone(),
                    move || {
                        let bytes = bytes.as_ref().map_err(Clone::clone)?;
                        let key = key.as_ref().map_err(Clone::clone)?;
                        parse_approve(bytes, &source, &review, key)
                    },
                )
                .await?
        } else {
            administration
                .preview_transport(
                    &host,
                    proof,
                    move || {
                        scopes?;
                        captured.resolve()
                    },
                    connection,
                    move || {
                        let bytes = bytes.as_ref().map_err(Clone::clone)?;
                        let key = key.as_ref().map_err(Clone::clone)?;
                        parse_preview(bytes, &source, key)
                    },
                )
                .await?
        };
        DirectorySourceResponse::try_from(output).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "DIRECTORY_SOURCE_RESPONSE_UNAVAILABLE",
                "The source approval result is unavailable.",
            )
            .into()
        })
    }
    .await;
    match result {
        Ok(data) => private(Json(data).into_response()),
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
  PgTransactionError::Persistence(_)=>private((StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"INTERNAL_ERROR","message":"The source request could not be completed."}}))).into_response()),
  PgTransactionError::Business(error)=>{let status=match error.kind{
   FailureKind::InvalidInput if error.code=="BODY_TOO_LARGE"=>StatusCode::PAYLOAD_TOO_LARGE,
   FailureKind::InvalidInput if ["INVALID_COOKIE_HEADER","COOKIE_HEADER_TOO_LARGE","AMBIGUOUS_COOKIE_HEADER","INVALID_SAVED_ACCOUNT_PROOF","INVALID_SESSION_PROOF","AMBIGUOUS_REQUEST_AUTHORITY","INVALID_REQUEST_HEADER"].contains(&error.code)=>StatusCode::BAD_REQUEST,
   FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};
   if status==StatusCode::NOT_FOUND{private((status,"Not found.").into_response())}else{private((status,Json(serde_json::json!({"error":{"code":error.code,"message":error.message}}))).into_response())}
  }
 }
}

async fn private_response(response: Response) -> Response {
    private(response)
}
