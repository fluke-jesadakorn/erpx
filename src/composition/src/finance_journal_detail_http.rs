//! Unmounted detail HTTP; one retained read returns owner data + render context.
use crate::{
    finance_journal_detail::{
        FinanceJournalDetailService, FinanceSalesDefaultsResultResponse,
        FinanceSalesDefaultsWorkspaceResponse,
    },
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, State, RawQuery, Request},
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
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, RecordId};
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
    targets: BTreeMap<(String, String,String), FinanceJournalReadTarget<C>>,
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
                .insert((target.host.clone(), target.holding_slug.clone(),target.service.company_id().as_str().to_owned()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    let field_work_routes=Router::new();
    #[cfg(feature = "finance-field-write")]
    let field_work_routes=field_work_routes.route("/api/v1/holdings/{holding_slug}/finance/field-work",get(financial_work::<C>));
    Ok(Router::new().merge(field_work_routes)
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals",
            get(list::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/invoices",
            get(sales_invoice_list::<C>).post(create_sales_invoice::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/defaults",
            get(sales_defaults_workspace::<C>).post(prepare_sales_defaults::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/defaults/{id}/decide",
            post(decide_sales_defaults::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/receipts/{id}",
            get(sales_receipt::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/receipts/{id}/export",
            get(export_sales_receipt::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/receipts/{id}/issue",
            post(issue_sales_receipt::<C>),
        )
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
#[cfg(feature = "finance-field-write")]
async fn financial_work<C:Clock+Send+Sync+'static>(State(state):State<Arc<ReadState<C>>>,Path(slug):Path<String>,headers:HeaderMap,RawQuery(query):RawQuery)->Response{
    let result=async{
        if query.is_some(){return Err(BusinessError::invalid("VALIDATION_FAILED","This Work query accepts only the captured workspace context.").into());}
        let host=unique_request_header(&headers,"host")?.ok_or_else(missing)?;
        let target=crate::http_scope::selected_company_target(&state.targets,host,&slug,&headers)?;
        let inbound=parse_inbound_account_request(&state.policy,&headers)?;
        if !matches!(inbound.audience(),RequestAudience::Holding{..}){return Err(missing().into());}
        if unique_request_header(&headers,"origin")?.is_some_and(|v|v!=inbound.expected_origin()){return Err(BusinessError::new(FailureKind::Forbidden,"ORIGIN_FORBIDDEN","Use the application origin.").into());}
        let scope=state.scopes.slugs(&headers);let(branch,department)=scope.as_ref().cloned().unwrap_or_default();
        let captured=CapturedHoldingMutation::capture(&headers,branch,department);let company=target.service.company_id().clone();
        target.service.read_field_work(host,inbound.into_proof(),move||{scope?;captured.resolve_read(company)}).await
    }.await;
    let mut response=match result{
        Ok(reply)=>Json(reply).into_response(),
        Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},
        Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FINANCE_WORK_UNAVAILABLE","message":"The financial Work contribution could not be read."}}))).into_response(),
    };
    for(name,value)in[(header::CACHE_CONTROL,"private, no-store"),(header::REFERRER_POLICY,"no-referrer"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")]{response.headers_mut().insert(name,HeaderValue::from_static(value));}
    response
}
async fn invoice_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets,host,&slug,&headers)?;
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
        let payment_evidence=match query.as_deref(){None=>false,Some(raw)=>{
            let values=url::form_urlencoded::parse(raw.as_bytes()).collect::<Vec<_>>();
            if values.len()!=1||values[0].0!="purpose"||values[0].1!="payment-evidence"{return Err(BusinessError::invalid("VALIDATION_FAILED","Invalid invoice read purpose.").into());}true
        }};
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
                payment_evidence,
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
async fn sales_receipt<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .sales_receipt(host, proof, input, id)
            .await
    }
    .await;
    match result {
        Ok(Some(reply)) => {
            let mut response = Json(reply).into_response();
            no_store(&mut response);
            response
        }
        Ok(None) => {
            let mut response = business_error(BusinessError::new(
                FailureKind::NotFound,
                "RECEIPT_NOT_FOUND",
                "Receipt not found in this workspace.",
            ));
            no_store(&mut response);
            response
        }
        Err(PgTransactionError::Business(error)) => {
            let mut response = business_error(error);
            no_store(&mut response);
            response
        }
        Err(_) => {
            let mut response = StatusCode::SERVICE_UNAVAILABLE.into_response();
            no_store(&mut response);
            response
        }
    }
}

async fn export_sales_receipt<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .export_sales_receipt(host, proof, input, id)
            .await
    }
    .await;
    match result {
        Ok(Some(reply)) => {
            let mut response = Response::new(reply.data.content.into());
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            response.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'"),
            );
            if let Ok(value) = HeaderValue::from_str(&reply.data.sha256) {
                response
                    .headers_mut()
                    .insert("x-document-sha256", value);
            }
            no_store(&mut response);
            response
        }
        Ok(None) => {
            let mut response = business_error(BusinessError::new(
                FailureKind::NotFound,
                "RECEIPT_NOT_FOUND",
                "Receipt not found in this workspace.",
            ));
            no_store(&mut response);
            response
        }
        Err(PgTransactionError::Business(error)) => {
            let mut response = business_error(error);
            no_store(&mut response);
            response
        }
        Err(_) => {
            let mut response = StatusCode::SERVICE_UNAVAILABLE.into_response();
            no_store(&mut response);
            response
        }
    }
}

async fn issue_sales_receipt<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, &headers)?;
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
        let key = unique_request_header(&headers, "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let command = CommandKey::new(key.to_owned())?;
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let proof = inbound.into_proof();
        target
            .service
            .issue_sales_receipt(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                id,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::CREATED, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn sales_defaults_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .sales_defaults_workspace(host, proof, input)
            .await
    }
    .await;
    read_response(result)
}

async fn prepare_sales_defaults<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, request.headers())?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let command = CommandKey::new(key.to_owned())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body = to_bytes(request.into_body(), 128 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::sales_defaults::SalesDefaultsPrepareInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid("SALES_DEFAULTS_INVALID", "Use valid sales defaults.")
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .prepare_sales_defaults(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                input,
            )
            .await
    }
    .await;
    write_response(result, StatusCode::CREATED)
}

async fn decide_sales_defaults<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, request.headers())?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let command = CommandKey::new(key.to_owned())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body = to_bytes(request.into_body(), 128 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::sales_defaults::SalesDefaultsDecisionInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid("SALES_DEFAULTS_DECISION_INVALID", "Review the sales defaults.")
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .decide_sales_defaults(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                id,
                input,
            )
            .await
    }
    .await;
    write_response(result, StatusCode::OK)
}

fn read_response(
    result: Result<FinanceSalesDefaultsWorkspaceResponse, PgTransactionError<BusinessError>>,
) -> Response {
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

fn write_response(
    result: Result<FinanceSalesDefaultsResultResponse, PgTransactionError<BusinessError>>,
    status: StatusCode,
) -> Response {
    let mut response = match result {
        Ok(reply) => (status, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
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
    let body = Json(serde_json::json!({
        "error": {"code": error.code, "message": error.message}
    }))
    .into_response();
    let mut response = Response::new(body.into_body());
    *response.status_mut() = status;
    response
}

fn no_store(response: &mut Response) {
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
}

async fn create_sales_invoice<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(&state.targets, &host, &slug, request.headers())?;
        let inbound = parse_inbound_account_request(&state.policy, request.headers())?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(missing().into());
        }
        if unique_request_header(request.headers(), "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            )
            .into());
        }
        let key = unique_request_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
            })?;
        let command = CommandKey::new(key.to_owned())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body = to_bytes(request.into_body(), 128 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::sales_invoice_create::SalesInvoiceCreateInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid("INVALID_SALES_INVOICE", "Use a valid sales invoice request.")
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .create_sales_invoice(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                input,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::CREATED, Json(reply)).into_response(),
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

async fn sales_invoice_list<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let mut tab = "invoices".to_owned();
        let mut q = String::new();
        let mut status = String::new();
        let mut start_date = None;
        let mut end_date = None;
        let mut currency = None;
        let mut queue = None;
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        for (key, value) in url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes()) {
            if value.len() > 2000 || value.chars().any(char::is_control) || !seen.insert(key.to_string()) {
                return Err(BusinessError::invalid(
                    "FINANCE_SALES_QUERY_INVALID",
                    "Use the sales invoice list filters.",
                )
                .into());
            }
            match key.as_ref() {
                "tab" if !value.is_empty() => tab = value.into_owned(),
                "q" => q = value.into_owned(),
                "status" => status = value.into_owned(),
                "startDate" if !value.is_empty() => start_date = Some(value.into_owned()),
                "endDate" if !value.is_empty() => end_date = Some(value.into_owned()),
                "currency" if !value.is_empty() => currency = Some(value.into_owned()),
                "queue" if !value.is_empty() => queue = Some(value.into_owned()),
                "cursor" if !value.is_empty() => cursor = Some(value.into_owned()),
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_SALES_QUERY_INVALID",
                        "Use the sales invoice list filters.",
                    )
                    .into())
                }
            }
        }
        let query = erpx_server_finance::public::sales_invoice_list::SalesInvoiceListQuery {
            tab,
            q,
            status,
            start_date,
            end_date,
            cursor,
            currency,
            queue,
        };
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .sales_invoice_list(host, proof, input, query)
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
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

async fn list<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let mut chart = None;
        let mut status = "all".to_owned();
        let mut cursor = None;
        for (key, value) in url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes()) {
            if value.len() > 400 || value.chars().any(char::is_control) {
                return Err(BusinessError::invalid(
                    "FINANCE_JOURNAL_QUERY_INVALID",
                    "Use the journal list filters.",
                )
                .into());
            }
            match key.as_ref() {
                "chart" if chart.is_none() && !value.is_empty() => chart = Some(value.into_owned()),
                "status" if !value.is_empty() => status = value.into_owned(),
                "cursor" if cursor.is_none() && !value.is_empty() => cursor = Some(value.into_owned()),
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_JOURNAL_QUERY_INVALID",
                        "Use the journal list filters.",
                    )
                    .into())
                }
            }
        }
        if status.len() > 40 {
            return Err(BusinessError::invalid(
                "FINANCE_JOURNAL_QUERY_INVALID",
                "Use the journal list filters.",
            )
            .into());
        }
        let scope = state.scopes.slugs(&headers);
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(&headers, branch, department);
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .list(host, proof, input, chart, status, cursor)
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
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

async fn read<C: Clock + Send + Sync + 'static>(
    state: Arc<ReadState<C>>,
    slug: String,
    id: String,
    headers: HeaderMap,
    workspace: bool,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target = crate::http_scope::selected_company_target(&state.targets,host,&slug,&headers)?;
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
