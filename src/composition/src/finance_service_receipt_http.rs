//! Exact registered-Holding original receipt create/review ingress; no legacy fallback.
use crate::{
    finance_journal_create::FinanceJournalCreateConfiguration,
    finance_service_receipt_bank::{execute_service_receipt_bank, normalize_service_receipt_bank},
    finance_service_receipt_confirm::{
        execute_service_receipt_confirm, normalize_service_receipt_confirm,
    },
    finance_service_receipt_create::{
        execute_service_receipt_create, normalize_service_receipt_create,
    },
    finance_service_receipt_review::{
        execute_service_receipt_review, normalize_service_receipt_decision,
    },
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer;
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind};
use std::{collections::BTreeMap, sync::Arc};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub struct FinanceServiceReceiptTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceJournalCreateConfiguration,
    pub clock: Arc<C>,
    pub file_features:
        erpx_server_files::public::payment_proof_binding::PaymentProofBindingFeatures,
}
struct PostState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String,String), FinanceServiceReceiptTarget<C>>,
}
pub fn finance_service_receipt_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceServiceReceiptTarget<C>>,
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
                .insert((target.host.clone(), target.holding_slug.clone(),target.configuration.company.as_str().to_owned()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_RECEIPT_REVIEW_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/finance/sales/receipts/{id}/workspace",get(customer_receipt_workspace::<C>))
        .route("/api/v1/holdings/{holding_slug}/finance/sales/receipts/{id}/runtime",get(customer_receipt_runtime::<C>))
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payments/{id}/workspace",
            get(receipt_workspace::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payments/{id}/runtime",
            get(receipt_runtime::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/bank/statements/import",
            post(receipt_bank::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/payment-runtime",
            get(payment_runtime::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payments",
            post(receipt_create::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payments/{id}/{action}",
            post(receipt_review::<C>),
        )
        .with_state(Arc::new(PostState {
            policy,
            scopes,
            targets: configured,
        })))
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
/// Persisted invoice capability observation for the generic payment create
/// entry. It grants no posting or payment authority; the create command still
/// proves current IAM, purchase and owner facts.
async fn payment_runtime<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?.to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, &headers)?;
        let inbound = parse_inbound_account_request(&state.policy, &headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(&headers, "origin")?.is_some_and(|v| v != inbound.expected_origin()) {
            return Err(BusinessError::new(FailureKind::Forbidden, "ORIGIN_FORBIDDEN", "Use the application origin.").into());
        }
        if id.is_empty() || id.len() > BODY_LIMIT || id.chars().any(char::is_control) {
            return Err(missing().into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let configuration = target.configuration.clone();
        let company = configuration.company.clone();
        target.authorizer
            .execute_with_sources_deferred(
                &host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve_read(company)
                },
                erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest {
                    permission: "finance.read".into(),
                    module: Some("finance".into()),
                    destination: "finance/payments".into(),
                    api: true,
                },
                crate::finance_service_receipt_files::FinancePaymentIntakeSources,
                move |c, current| {
                    let configuration = configuration.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        current.require_permission("module.finance.read")?;
                        if current.holding_id() != &configuration.holding
                            || current.selection().workspace().company_id != configuration.company
                        {
                            return Err(missing().into());
                        }
                        let native: bool = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=$1 AND i.company_id=$2 \
                             AND i.kind IN ('payable','receivable') AND i.status IN ('posted','partially_paid') \
                             AND i.source_type IS DISTINCT FROM 'expense_claim')",
                        )
                        .bind(&id)
                        .bind(configuration.company.as_str())
                        .fetch_one(&mut *c)
                        .await?;
                        Ok(serde_json::json!({"data":{"id":id,"runtime":if native{"rust"}else{"legacy"}}}))
                    })
                },
            )
            .await
    }
    .await;
    match result {
        Ok(reply) => (
            StatusCode::OK,
            [
                (axum::http::header::CACHE_CONTROL, "private, no-store"),
                (axum::http::header::REFERRER_POLICY, "no-referrer"),
                (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            axum::Json(reply),
        )
            .into_response(),
        Err(PgTransactionError::Business(e)) => {
            let status = match e.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, axum::Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()
        }
        Err(PgTransactionError::Persistence(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error":{"code":"FINANCE_PAYMENT_RUNTIME_UNAVAILABLE","message":"The payment runtime could not be observed."}})),
        )
            .into_response(),
    }
}

async fn receipt_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Response {
    receipt_read(state, slug, id, headers, false, false).await
}
async fn receipt_runtime<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: axum::http::HeaderMap,
) -> Response {
    receipt_read(state, slug, id, headers, true, false).await
}
async fn customer_receipt_workspace<C:Clock+Send+Sync+'static>(State(state):State<Arc<PostState<C>>>,Path((slug,id)):Path<(String,String)>,headers:axum::http::HeaderMap)->Response{
 receipt_read(state,slug,id,headers,false,true).await
}
async fn customer_receipt_runtime<C:Clock+Send+Sync+'static>(State(state):State<Arc<PostState<C>>>,Path((slug,id)):Path<(String,String)>,headers:axum::http::HeaderMap)->Response{
 receipt_read(state,slug,id,headers,true,true).await
}
async fn receipt_read<C: Clock + Send + Sync + 'static>(
    state: Arc<PostState<C>>,
    slug: String,
    id: String,
    headers: axum::http::HeaderMap,
    runtime: bool,
    customer: bool,
) -> Response {
    let result=async {
        let host=unique_request_header(&headers,"host")?.ok_or_else(missing)?.to_owned();
        let target=crate::http_scope::selected_company_target(&state.targets,&host,&slug,&headers)?;
        let inbound=parse_inbound_account_request(&state.policy,&headers)?;
        if !matches!(inbound.audience(),RequestAudience::Holding{..}){return Err(missing().into());}
        if unique_request_header(&headers,"origin")?.is_some_and(|v|v!=inbound.expected_origin()) {return Err(BusinessError::new(FailureKind::Forbidden,"ORIGIN_FORBIDDEN","Use the application origin.").into());}
        if id.is_empty()||id.len()>BODY_LIMIT||id.chars().any(char::is_control){return Err(missing().into());}
        let files_mode=if target.file_features.enabled&&!target.file_features.read_only&&!target.file_features.uploads_frozen {"enabled"}else if target.file_features.enabled||target.file_features.read_only {"read-only"}else{"disabled"};
        let scope=state.scopes.slugs(&headers);let(branch,department)=scope.as_ref().cloned().unwrap_or_default();
        let captured=CapturedHoldingMutation::capture(&headers,branch,department);let configuration=target.configuration.clone();let company=configuration.company.clone();let clock=target.clock.clone();
        target.authorizer.execute_with_sources_deferred(&host,inbound.into_proof(),move||{scope?;captured.resolve_read(company)},erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest{permission:if runtime&&!customer{"module.finance.read"}else{"finance.read"}.into(),module:Some("finance".into()),destination:"finance/payments".into(),api:true},crate::finance_service_receipt_files::FinancePaymentIntakeSources,move|c,current|{
            let(configuration,clock,id)=(configuration.clone(),clock.clone(),id.clone());
            Box::pin(async move {
                if customer {
                    let reply=crate::finance_service_receipt_files::read_finance_service_customer_receipt(c,current,&configuration,clock.as_ref(),&id,runtime).await?;
                    if serde_json::to_vec(&reply).map_err(|_|missing())?.len()>BODY_LIMIT{return Err(BusinessError::new(FailureKind::Unavailable,"FINANCE_REPLY_LIMIT","The issued receipt is unavailable.").into());}
                    return Ok(reply);
                }
                if runtime {
                    let data=crate::finance_service_receipt_files::finance_service_receipt_runtime(c,current,&configuration,clock.as_ref(),&id).await?;
                    return Ok(serde_json::json!({"data":data}));
                }
                let(data,source_only,purchase_controls)=crate::finance_service_receipt_files::read_finance_service_receipt_workspace(c,current,&configuration,clock.as_ref(),&id).await?;
                let reply=serde_json::json!({"data":data,"context":crate::finance_journal_detail::rendered(current)?,"filesMode":files_mode,"sourceOnly":source_only,"purchaseControls":purchase_controls});
                let bytes=serde_json::to_vec(&reply).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_REPLY_LIMIT","The receipt workspace is unavailable."))?;
                if bytes.len()>BODY_LIMIT{return Err(BusinessError::new(FailureKind::Unavailable,"FINANCE_REPLY_LIMIT","The receipt workspace is unavailable.").into());}
                Ok(reply)
            })
        }).await
    }.await;
    match result {
        Ok(reply) => (
            StatusCode::OK,
            [
                (axum::http::header::CACHE_CONTROL, "private, no-store"),
                (axum::http::header::REFERRER_POLICY, "no-referrer"),
                (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            axum::Json(reply),
        )
            .into_response(),
        Err(error) => {
            let (status, code, message) = match error {
                PgTransactionError::Business(e) => (
                    match e.kind {
                        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                        FailureKind::Forbidden => StatusCode::FORBIDDEN,
                        FailureKind::NotFound => StatusCode::NOT_FOUND,
                        FailureKind::Conflict => StatusCode::CONFLICT,
                        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                    },
                    e.code.to_string(),
                    e.message.to_string(),
                ),
                PgTransactionError::Persistence(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "FINANCE_RECEIPT_READ_UNAVAILABLE".into(),
                    "The receipt workspace could not be read.".into(),
                ),
            };
            (
                status,
                [
                    (axum::http::header::CACHE_CONTROL, "private, no-store"),
                    (axum::http::header::REFERRER_POLICY, "no-referrer"),
                    (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                ],
                axum::Json(serde_json::json!({"error":{"code":code,"message":message}})),
            )
                .into_response()
        }
    }
}
async fn receipt_bank<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    receipt_command(state, slug, None, Some("bank-import".into()), request).await
}
async fn receipt_create<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    receipt_command(state, slug, None, None, request).await
}
async fn receipt_review<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id, action)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    receipt_command(state, slug, Some(id), Some(action), request).await
}
async fn receipt_command<C: Clock + Send + Sync + 'static>(
    state: Arc<PostState<C>>,
    slug: String,
    id: Option<String>,
    action: Option<String>,
    request: Request,
) -> Response {
    let success = if id.is_none() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let result = async {
        let approve = match action.as_deref() {
            Some("approve") => true,
            Some("release" | "bank-import" | "confirm") | None => false,
            _ => return Err(missing().into()),
        };
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets,&host,&slug,request.headers())?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|v| v != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let mut keys = axum::http::HeaderMap::new();
        for value in request.headers().get_all("idempotency-key") {
            keys.append("idempotency-key", value.clone());
        }
        let proof = inbound.into_proof();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        if action.is_none() {
            return execute_service_receipt_create(
                &target.authorizer,
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                target.configuration.clone(),
                target.clock.clone(),
                move || {
                    let bytes = body.as_ref().map_err(Clone::clone)?;
                    let wire = normalize_service_receipt_create(bytes)?;
                    let key = unique_request_header(&keys, "idempotency-key")?
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "IDEMPOTENCY_KEY_REQUIRED",
                                "Idempotency-Key is required.",
                            )
                        })?;
                    Ok((CommandKey::new(key)?.as_str().to_owned(), wire))
                },
            )
            .await;
        }
        if action.as_deref() == Some("bank-import") {
            return execute_service_receipt_bank(
                &target.authorizer,
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                target.configuration.clone(),
                target.clock.clone(),
                move || {
                    let bytes = body.as_ref().map_err(Clone::clone)?;
                    let wire = normalize_service_receipt_bank(bytes)?;
                    let key = unique_request_header(&keys, "idempotency-key")?
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "IDEMPOTENCY_KEY_REQUIRED",
                                "Idempotency-Key is required.",
                            )
                        })?;
                    Ok((CommandKey::new(key)?.as_str().to_owned(), wire))
                },
            )
            .await;
        }
        let id = id.ok_or_else(missing)?;
        if action.as_deref() == Some("confirm") {
            return execute_service_receipt_confirm(
                &target.authorizer,
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                target.configuration.clone(),
                target.clock.clone(),
                target.file_features.clone(),
                move || {
                    let bytes = body.as_ref().map_err(Clone::clone)?;
                    let wire = normalize_service_receipt_confirm(bytes)?;
                    let key = unique_request_header(&keys, "idempotency-key")?
                        .filter(|v| !v.is_empty())
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "IDEMPOTENCY_KEY_REQUIRED",
                                "Idempotency-Key is required.",
                            )
                        })?;
                    Ok((CommandKey::new(key)?.as_str().to_owned(), id.clone(), wire))
                },
            )
            .await;
        }

        execute_service_receipt_review(
            &target.authorizer,
            &host,
            proof,
            move || {
                scope?;
                captured.resolve()
            },
            target.configuration.clone(),
            target.clock.clone(),
            approve,
            move || {
                let bytes = body.as_ref().map_err(Clone::clone)?;
                let wire = normalize_service_receipt_decision(bytes, approve)?;
                let key = unique_request_header(&keys, "idempotency-key")?
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        BusinessError::invalid(
                            "IDEMPOTENCY_KEY_REQUIRED",
                            "Idempotency-Key is required.",
                        )
                    })?;
                Ok((CommandKey::new(key)?.as_str().to_owned(), id.clone(), wire))
            },
        )
        .await
    }
    .await;
    let mut response = match result {
        Ok(data) => match serde_json::from_str::<serde_json::Value>(data.as_str()) {
            Ok(value) => (success, axum::Json(serde_json::json!({"data":value}))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_RECEIPT_REVIEW_UNAVAILABLE","message":"The receipt decision could not be encoded."}}))).into_response(),
        },
        Err(PgTransactionError::Business(e)) => {
            let status = match e.kind {
                FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                FailureKind::Forbidden => StatusCode::FORBIDDEN,
                FailureKind::NotFound => StatusCode::NOT_FOUND,
                FailureKind::Conflict => StatusCode::CONFLICT,
                FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            };
            (status, axum::Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()
        }
        Err(PgTransactionError::Persistence(_)) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_RECEIPT_REVIEW_UNAVAILABLE","message":"The receipt decision could not be recorded."}}))).into_response(),
    };
    for (name, value) in [
        (axum::http::header::CACHE_CONTROL, "private, no-store"),
        (axum::http::header::REFERRER_POLICY, "no-referrer"),
        (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, axum::http::HeaderValue::from_static(value));
    }
    response
}
