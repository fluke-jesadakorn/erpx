//! Unmounted detail HTTP; one retained read returns owner data + render context.
use crate::{
    finance_journal_detail::{
        FinanceChartAccountsResponse, FinanceJournalDetailService,
        FinanceSalesDefaultsResultResponse, FinanceSalesDefaultsWorkspaceResponse,
        FinanceSetupWorkspaceResponse,
    },
    http_scope::BusinessScopeSlugs,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, RawQuery, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    work_context::HoldingWorkspaceRequest,
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
    targets: BTreeMap<(String, String, String), FinanceJournalReadTarget<C>>,
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
                .insert(
                    (
                        target.host.clone(),
                        target.holding_slug.clone(),
                        target.service.company_id().as_str().to_owned(),
                    ),
                    target,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    let field_work_routes = Router::new();
    #[cfg(feature = "finance-field-write")]
    let field_work_routes = field_work_routes.route(
        "/api/v1/holdings/{holding_slug}/finance/field-work",
        get(financial_work::<C>),
    );
    Ok(Router::new()
        .merge(field_work_routes)
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals",
            get(list::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/sales/invoices",
            get(sales_invoice_list::<C>).post(create_sales_invoice::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payables",
            post(create_supplier_bill::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payables/{id}",
            get(supplier_bill_detail::<C>).post(edit_supplier_bill::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payables/{id}/post",
            post(post_supplier_bill::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/payables/{id}/void",
            post(void_supplier_bill::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-batches",
            post(create_supplier_batch::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-batches/{id}/approve",
            post(approve_supplier_batch::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-batches/{id}/cancel",
            post(cancel_supplier_batch::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-batches/{id}/export",
            get(recover_supplier_batch_export::<C>).post(export_supplier_batch::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-instructions/{id}/result",
            post(supplier_instruction_result::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/supplier-instructions/{id}/confirm",
            post(confirm_supplier_instruction::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods/{id}/workspace",
            get(period_close_workspace::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods/{id}/begin-close",
            post(begin_period_close::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods/{id}/cancel-close",
            post(cancel_period_close::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods/{id}/close",
            post(close_period::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods/{id}/reopen",
            post(reopen_period::<C>),
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
            "/api/v1/holdings/{holding_slug}/finance/reports",
            get(finance_report::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/opening-balances/preview",
            post(opening_balance_preview::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/opening-balances/import",
            post(import_opening_balance::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/charts/{id}/accounts",
            get(finance_chart_accounts::<C>).post(create_account::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/charts",
            post(create_chart::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/charts/import",
            post(import_accounts::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/charts/{id}/accounts/{account_id}/status",
            post(change_account_status::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/periods",
            post(create_period::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices",
            get(finance_invoices_page::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/aging",
            get(finance_invoice_aging::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}",
            post(edit_sales_invoice::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/void",
            post(void_sales_invoice::<C>),
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
            "/api/v1/holdings/{holding_slug}/finance/journals/{id}/field-history",
            get(field_history::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{id}/field-history/workspace",
            get(field_history_workspace::<C>),
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
async fn financial_work<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let result = async {
        if query.is_some() {
            return Err(BusinessError::invalid(
                "VALIDATION_FAILED",
                "This Work query accepts only the captured workspace context.",
            )
            .into());
        }
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
            .read_field_work(host, inbound.into_proof(), move || {
                scope?;
                captured.resolve_read(company)
            })
            .await
    }
    .await;
    let mut response=match result{
        Ok(reply)=>Json(reply).into_response(),
        Err(PgTransactionError::Business(e))=>{let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};(status,Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()},
        Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,Json(serde_json::json!({"error":{"code":"FINANCE_WORK_UNAVAILABLE","message":"The financial Work contribution could not be read."}}))).into_response(),
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
async fn invoice_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let payment_evidence = match query.as_deref() {
            None => false,
            Some(raw) => {
                let values = url::form_urlencoded::parse(raw.as_bytes()).collect::<Vec<_>>();
                if values.len() != 1
                    || values[0].0 != "purpose"
                    || values[0].1 != "payment-evidence"
                {
                    return Err(BusinessError::invalid(
                        "VALIDATION_FAILED",
                        "Invalid invoice read purpose.",
                    )
                    .into());
                }
                true
            }
        };
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
    read(state, slug, id, headers, false, false).await
}
async fn workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    read(state, slug, id, headers, true, false).await
}
async fn field_history<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    read(state, slug, id, headers, false, true).await
}
async fn field_history_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    read(state, slug, id, headers, true, true).await
}
async fn finance_invoices_page<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let mut q = String::new();
        let mut kind = String::new();
        let mut status = String::new();
        let mut queue = String::new();
        let mut chart_id = None;
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        for (key, value) in url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes()) {
            if value.len() > 2000
                || value.chars().any(char::is_control)
                || !seen.insert(key.to_string())
            {
                return Err(BusinessError::invalid(
                    "FINANCE_INVOICE_QUERY_INVALID",
                    "Use the invoice list filters.",
                )
                .into());
            }
            match key.as_ref() {
                "q" => q = value.into_owned(),
                "kind" if !value.is_empty() => kind = value.into_owned(),
                "status" if !value.is_empty() => status = value.into_owned(),
                "queue" if !value.is_empty() => queue = value.into_owned(),
                "chartId" if !value.is_empty() => chart_id = Some(value.into_owned()),
                "cursor" if !value.is_empty() => cursor = Some(value.into_owned()),
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_INVOICE_QUERY_INVALID",
                        "Use the invoice list filters.",
                    )
                    .into());
                }
            }
        }
        let query = erpx_server_finance::public::finance_invoice_read::InvoicePageQuery {
            q,
            kind,
            status,
            queue,
            chart_id,
            cursor,
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
            .finance_invoices_page(host, proof, input, query)
            .await
    }
    .await;
    read_json_response(result)
}

async fn finance_invoice_aging<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let mut kind = String::new();
        let mut as_of = String::new();
        let mut chart_id = None;
        let mut seen = std::collections::BTreeSet::new();
        for (key, value) in url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes()) {
            if value.len() > 2000
                || value.chars().any(char::is_control)
                || !seen.insert(key.to_string())
            {
                return Err(BusinessError::invalid(
                    "FINANCE_INVOICE_AGING_QUERY_INVALID",
                    "Use a valid aging kind, cutoff date and ledger.",
                )
                .into());
            }
            match key.as_ref() {
                "kind" if !value.is_empty() => kind = value.into_owned(),
                "asOf" if !value.is_empty() => as_of = value.into_owned(),
                "chartId" if !value.is_empty() => chart_id = Some(value.into_owned()),
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_INVOICE_AGING_QUERY_INVALID",
                        "Use a valid aging kind, cutoff date and ledger.",
                    )
                    .into());
                }
            }
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
            .finance_invoice_aging(host, proof, input, kind, as_of, chart_id)
            .await
    }
    .await;
    read_json_response(result)
}

fn read_json_response<T: serde::Serialize>(
    result: Result<T, PgTransactionError<BusinessError>>,
) -> Response {
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

fn setup_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let key = unique_request_header(headers, "idempotency-key")?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
        })?;
    CommandKey::new(key.to_owned())
}

async fn setup_body<T: serde::de::DeserializeOwned>(request: Request) -> Result<T, BusinessError> {
    let body = to_bytes(request.into_body(), 256 * 1024)
        .await
        .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
    serde_json::from_slice(&body)
        .map_err(|_| BusinessError::invalid("INVALID_SETUP_REQUEST", "Use a valid setup request."))
}

fn setup_target<'a, C>(
    state: &'a Arc<ReadState<C>>,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(String, &'a FinanceJournalReadTarget<C>), BusinessError> {
    let host = unique_request_header(headers, "host")?
        .ok_or_else(missing)?
        .to_owned();
    let target = crate::http_scope::selected_company_target(&state.targets, &host, slug, headers)?;
    Ok((host, target))
}

async fn opening_balance_preview<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_opening::OpeningInput =
            setup_body(request).await?;
        let company = target.service.company_id().clone();
        let proof = inbound.into_proof();
        let input = move || {
            scope?;
            captured.resolve_read(company)
        };
        target
            .service
            .opening_balance_preview(&host, proof, input, body)
            .await
    }
    .await;
    write_json_response(result, StatusCode::OK)
}

async fn import_opening_balance<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_opening::OpeningImportInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .import_opening_balance(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::CREATED)
}

async fn finance_report<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let mut chart_id = None;
        let mut period_id = None;
        let mut start_date = None;
        let mut end_date = None;
        let mut seen = std::collections::BTreeSet::new();
        for (key, value) in url::form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes()) {
            if value.len() > 2000
                || value.chars().any(char::is_control)
                || !seen.insert(key.to_string())
            {
                return Err(BusinessError::invalid(
                    "FINANCE_REPORT_QUERY_INVALID",
                    "Use a valid ledger, period and date range.",
                )
                .into());
            }
            match key.as_ref() {
                "chart" if !value.is_empty() => chart_id = Some(value.into_owned()),
                "periodId" if !value.is_empty() => period_id = Some(value.into_owned()),
                "startDate" if !value.is_empty() => start_date = Some(value.into_owned()),
                "endDate" if !value.is_empty() => end_date = Some(value.into_owned()),
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_REPORT_QUERY_INVALID",
                        "Use a valid ledger, period and date range.",
                    )
                    .into());
                }
            }
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
            .finance_report(
                host, proof, input, chart_id, period_id, start_date, end_date,
            )
            .await
    }
    .await;
    read_json_response(result)
}

async fn finance_chart_accounts<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, chart_id)): Path<(String, String)>,
    headers: HeaderMap,
    RawQuery(raw): RawQuery,
) -> Response {
    let setup_view = raw.as_deref() == Some("view=setup");
    let result = async {
        let (host, target) = setup_target(&state, &headers, &slug)?;
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
        if setup_view {
            return target
                .service
                .finance_setup_workspace(&host, proof, input)
                .await
                .map(SetupReadReply::Workspace);
        }
        target
            .service
            .finance_chart_accounts(&host, proof, input, chart_id)
            .await
            .map(SetupReadReply::ChartAccounts)
    }
    .await;
    read_json_response(result)
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum SetupReadReply {
    Workspace(FinanceSetupWorkspaceResponse),
    ChartAccounts(FinanceChartAccountsResponse),
}

async fn create_chart<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_setup::CreateChartInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .create_chart(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::CREATED)
}

async fn import_accounts<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_setup::ImportAccountsInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .import_accounts(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::CREATED)
}

async fn create_account<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, chart_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_setup::CreateAccountInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .create_account(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                chart_id,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::CREATED)
}

async fn change_account_status<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, chart_id, account_id)): Path<(String, String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_setup::ChangeAccountStatusInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .change_account_status(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                chart_id,
                account_id,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::OK)
}

async fn create_period<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let (host, target) = setup_target(&state, request.headers(), &slug)?;
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
        let command = setup_key(request.headers())?;
        let scope = state.scopes.slugs(request.headers());
        let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
        let captured = CapturedHoldingMutation::capture(request.headers(), branch, department);
        let body: erpx_server_finance::public::finance_setup::CreatePeriodInput =
            setup_body(request).await?;
        let proof = inbound.into_proof();
        target
            .service
            .create_period(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                command,
                body,
            )
            .await
    }
    .await;
    write_json_response(result, StatusCode::CREATED)
}

fn write_json_response<T: serde::Serialize>(
    result: Result<T, PgTransactionError<BusinessError>>,
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

fn not_owned() -> Response {
    business_error(BusinessError::new(
        FailureKind::NotFound,
        "NATIVE_INVOICE_NOT_OWNED",
        "This invoice belongs to another Finance workflow.",
    ))
}

async fn edit_sales_invoice<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let input: erpx_server_finance::public::sales_invoice_edit::SalesInvoiceEditInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SALES_INVOICE",
                    "Use a valid sales invoice request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .edit_sales_invoice(
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
    let mut response = match result {
        Ok(Some(reply)) => (StatusCode::OK, Json(reply)).into_response(),
        Ok(None) => not_owned(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn void_sales_invoice<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let input: erpx_server_finance::public::sales_invoice_edit::SalesInvoiceVoidInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SALES_INVOICE",
                    "Use a valid sales invoice request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .void_sales_invoice(
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
    let mut response = match result {
        Ok(Some(reply)) => (StatusCode::OK, Json(reply)).into_response(),
        Ok(None) => not_owned(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn sales_receipt<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        target.service.sales_receipt(host, proof, input, id).await
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
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
                response.headers_mut().insert("x-document-sha256", value);
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
        let target =
            crate::http_scope::selected_company_target(&state.targets, &host, &slug, &headers)?;
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
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
                BusinessError::invalid(
                    "SALES_DEFAULTS_DECISION_INVALID",
                    "Review the sales defaults.",
                )
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
    (
        status,
        Json(serde_json::json!({
            "error": {"code": error.code, "message": error.message}
        })),
    )
        .into_response()
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
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
                BusinessError::invalid(
                    "INVALID_SALES_INVOICE",
                    "Use a valid sales invoice request.",
                )
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

async fn create_supplier_bill<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let input: erpx_server_finance::public::supplier_bill::SupplierBillCreateInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_BILL",
                    "Use a valid supplier bill request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .create_supplier_bill(
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
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

struct SupplierBatchIngress {
    proof: DeviceProof,
    command: CommandKey,
    scope: Result<(Option<String>, Option<String>), BusinessError>,
    captured: CapturedHoldingMutation,
}

fn supplier_batch_ingress<C: Clock + Send + Sync + 'static>(
    state: &ReadState<C>,
    headers: &HeaderMap,
) -> Result<SupplierBatchIngress, PgTransactionError<BusinessError>> {
    let inbound = parse_inbound_account_request(&state.policy, headers)?;
    if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
        return Err(missing().into());
    }
    if unique_request_header(headers, "origin")?.is_some_and(|value| value != inbound.expected_origin())
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )
        .into());
    }
    let key = unique_request_header(headers, "idempotency-key")?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED", "Idempotency-Key is required.")
        })?;
    let command = CommandKey::new(key.to_owned())?;
    let scope = state.scopes.slugs(headers);
    let (branch, department) = scope.as_ref().cloned().unwrap_or_default();
    let captured = CapturedHoldingMutation::capture(headers, branch, department);
    Ok(SupplierBatchIngress {
        proof: inbound.into_proof(),
        command,
        scope,
        captured,
    })
}

fn supplier_batch_parts(
    ingress: SupplierBatchIngress,
) -> (
    DeviceProof,
    CommandKey,
    impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
) {
    let SupplierBatchIngress {
        proof,
        command,
        scope,
        captured,
    } = ingress;
    (proof, command, move || {
        scope?;
        captured.resolve()
    })
}

async fn supplier_batch_body(request: Request) -> Result<axum::body::Bytes, PgTransactionError<BusinessError>> {
    to_bytes(request.into_body(), 128 * 1024)
        .await
        .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request.").into())
}

fn supplier_batch_invalid() -> BusinessError {
    BusinessError::invalid(
        "INVALID_SUPPLIER_BATCH",
        "Use a valid supplier batch request.",
    )
}

async fn create_supplier_batch<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
        let ingress = supplier_batch_ingress(&state, request.headers())?;
        let body = supplier_batch_body(request).await?;
        let input: erpx_server_finance::public::supplier_batch::SupplierBatchCreateInput =
            serde_json::from_slice(&body).map_err(|_| supplier_batch_invalid())?;
        let (proof, command, run) = supplier_batch_parts(ingress);
        target
            .service
            .create_supplier_batch(&host, proof, run, command, input)
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

async fn decide_supplier_batch<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
    decision: &'static str,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
        let ingress = supplier_batch_ingress(&state, request.headers())?;
        let body = supplier_batch_body(request).await?;
        let input: erpx_server_finance::public::supplier_batch::SupplierBatchDecisionInput =
            serde_json::from_slice(&body).map_err(|_| supplier_batch_invalid())?;
        let (proof, command, run) = supplier_batch_parts(ingress);
        target
            .service
            .decide_supplier_batch(
                &host,
                proof,
                run,
                command,
                id,
                decision,
                input,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn approve_supplier_batch<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    path: Path<(String, String)>,
    request: Request,
) -> Response {
    decide_supplier_batch(state, path, request, "approve").await
}

async fn cancel_supplier_batch<C: Clock + Send + Sync + 'static>(
    state: State<Arc<ReadState<C>>>,
    path: Path<(String, String)>,
    request: Request,
) -> Response {
    decide_supplier_batch(state, path, request, "cancel").await
}

async fn export_supplier_batch<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
        let ingress = supplier_batch_ingress(&state, request.headers())?;
        let body = supplier_batch_body(request).await?;
        let input: erpx_server_finance::public::supplier_batch::SupplierBatchExportInput =
            serde_json::from_slice(&body).map_err(|_| supplier_batch_invalid())?;
        let (proof, command, run) = supplier_batch_parts(ingress);
        target
            .service
            .export_supplier_batch(&host, proof, run, command, id, input)
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn recover_supplier_batch_export<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            &headers,
        )?;
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
        let run = move || -> Result<HoldingWorkspaceRequest, BusinessError> {
            scope?;
            captured.resolve()
        };
        target
            .service
            .recover_supplier_batch_export(&host, inbound.into_proof(), run, id)
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn supplier_instruction_result<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
        let ingress = supplier_batch_ingress(&state, request.headers())?;
        let body = supplier_batch_body(request).await?;
        let input: erpx_server_finance::public::supplier_batch::SupplierInstructionResultInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_RESULT",
                    "Use a valid supplier bank result.",
                )
            })?;
        let (proof, command, run) = supplier_batch_parts(ingress);
        target
            .service
            .record_supplier_instruction_result(
                &host,
                proof,
                run,
                command,
                id,
                input,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn confirm_supplier_instruction<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
        let ingress = supplier_batch_ingress(&state, request.headers())?;
        let body = supplier_batch_body(request).await?;
        let input: erpx_server_finance::public::supplier_batch::SupplierInstructionConfirmInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_CONFIRMATION",
                    "Use a valid supplier bank confirmation.",
                )
            })?;
        let (proof, command, run) = supplier_batch_parts(ingress);
        target
            .service
            .confirm_supplier_instruction(
                &host,
                proof,
                run,
                command,
                id,
                input,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn edit_supplier_bill<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let input: erpx_server_finance::public::supplier_bill::SupplierBillEditInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_BILL",
                    "Use a valid supplier bill request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .edit_supplier_bill(
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
    let mut response = match result {
        Ok(Some(reply)) => (StatusCode::OK, Json(reply)).into_response(),
        Ok(None) => not_owned(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn post_supplier_bill<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let input: erpx_server_finance::public::supplier_bill::SupplierBillPostInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_BILL_POST",
                    "Use a valid supplier bill posting request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .post_supplier_bill(
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
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn void_supplier_bill<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let body = to_bytes(request.into_body(), 16 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::supplier_bill::SupplierBillVoidInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_SUPPLIER_BILL",
                    "Use a valid supplier bill void request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .void_supplier_bill(
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
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn supplier_bill_detail<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target =
            crate::http_scope::selected_company_target(&state.targets, &host, &slug, &headers)?;
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
        let proof = inbound.into_proof();
        target
            .service
            .supplier_bill_detail(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                id,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn period_close_workspace<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target =
            crate::http_scope::selected_company_target(&state.targets, &host, &slug, &headers)?;
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
        let proof = inbound.into_proof();
        target
            .service
            .period_close_workspace(
                &host,
                proof,
                move || {
                    scope?;
                    captured.resolve()
                },
                id,
            )
            .await
    }
    .await;
    let mut response = match result {
        Ok(reply) => Json(reply).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn begin_period_close<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let body = to_bytes(request.into_body(), 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        if !body.is_empty() {
            return Err(BusinessError::invalid(
                "INVALID_PERIOD_CLOSE",
                "Use an empty begin-close request.",
            )
            .into());
        }
        let proof = inbound.into_proof();
        target
            .service
            .begin_period_close(
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
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn cancel_period_close<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let body = to_bytes(request.into_body(), 16 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::period_close::PeriodCloseCancelInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid(
                    "INVALID_PERIOD_CLOSE",
                    "Use a valid close cancellation request.",
                )
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .cancel_period_close(
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
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn close_period<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let body = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::period_close::PeriodCloseProofInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid("INVALID_PERIOD_CLOSE", "Use valid close evidence.")
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .close_period(
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
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
    response
}

async fn reopen_period<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<ReadState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_request_header(request.headers(), "host")?
            .ok_or_else(missing)?
            .to_owned();
        let target = crate::http_scope::selected_company_target(
            &state.targets,
            &host,
            &slug,
            request.headers(),
        )?;
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
        let body = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."))?;
        let input: erpx_server_finance::public::period_close::PeriodCloseProofInput =
            serde_json::from_slice(&body).map_err(|_| {
                BusinessError::invalid("INVALID_PERIOD_CLOSE", "Use valid reopen evidence.")
            })?;
        let proof = inbound.into_proof();
        target
            .service
            .reopen_period(
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
    let mut response = match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(PgTransactionError::Business(error)) => business_error(error),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    no_store(&mut response);
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
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
            if value.len() > 2000
                || value.chars().any(char::is_control)
                || !seen.insert(key.to_string())
            {
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
                    .into());
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
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
                "cursor" if cursor.is_none() && !value.is_empty() => {
                    cursor = Some(value.into_owned())
                }
                _ => {
                    return Err(BusinessError::invalid(
                        "FINANCE_JOURNAL_QUERY_INVALID",
                        "Use the journal list filters.",
                    )
                    .into());
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
    field_history: bool,
) -> Response {
    let result = async {
        let host = unique_request_header(&headers, "host")?.ok_or_else(missing)?;
        let target =
            crate::http_scope::selected_company_target(&state.targets, host, &slug, &headers)?;
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
        if field_history {
            if workspace {
                target
                    .service
                    .read_field_history_workspace(host, proof, input, id)
                    .await
                    .and_then(|reply| serde_json::to_value(reply).map_err(|_| serialization()))
            } else {
                target
                    .service
                    .read_field_history(host, proof, input, id)
                    .await
                    .and_then(|reply| serde_json::to_value(reply).map_err(|_| serialization()))
            }
        } else if workspace {
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


/// Composition backend for the native revenue-obligation recognition command.
pub struct RevenueRecognitionBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<FinanceJournalDetailService<C>>,
}

impl<C: Clock + Send + Sync + 'static> RevenueRecognitionBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<FinanceJournalDetailService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
        match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "FINANCE_REVENUE_UNAVAILABLE",
                "The Finance revenue recognition service is unavailable.",
            ),
        }
    }
}

impl<C: Clock + Send + Sync + 'static> erpx_server_finance::public::revenue_recognition::RevenueRecognitionHttpBackend
    for RevenueRecognitionBackend<C>
{
    fn command<'a>(
        &'a self,
        request: erpx_server_finance::public::revenue_recognition::RevenueRecognitionHttpCommandRequest<'a>,
    ) -> erpx_server_finance::public::revenue_recognition::RevenueRecognitionHttpFuture<
        'a,
        erpx_server_finance::public::revenue_recognition::RevenueRecognitionResponse,
    > {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, request.headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(missing());
            }
            let (branch, department) = self.scopes.slugs(request.headers)?;
            let selection = erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest {
                company_id: self.service.company_id().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: true,
            };
            let data = self
                .service
                .recognize_revenue(
                    request.host,
                    inbound.into_proof(),
                    move || Ok(selection),
                    request.key.clone(),
                    request.obligation_id.to_owned(),
                    request.input,
                )
                .await
                .map_err(Self::business)?;
            Ok(erpx_server_finance::public::revenue_recognition::RevenueRecognitionResponse { data })
        })
    }
}
