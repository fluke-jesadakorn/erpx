//! Explicit native Field-origin invoice posting ingress; no legacy fallback.
use crate::{
    finance_journal_create::FinanceJournalCreateConfiguration,
    finance_service_invoice_post::{
        execute_service_invoice_post, read_service_invoice_post_runtime,
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
use erpx_server_finance::public::service_invoice_posting::normalize_service_invoice_post;
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
use erpx_server_workflow::public::commands::WorkflowRequest;
use std::{collections::BTreeMap, sync::Arc};
const BODY_LIMIT: usize = 2 * 1024 * 1024;
pub struct FinanceServiceInvoicePostTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: FinanceJournalCreateConfiguration,
    pub clock: Arc<C>,
}
struct PostState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String, String), FinanceServiceInvoicePostTarget<C>>,
}
pub fn finance_service_invoice_post_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<FinanceServiceInvoicePostTarget<C>>,
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
                        target.configuration.company.as_str().to_owned(),
                    ),
                    target,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "FINANCE_INVOICE_POST_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/journals/{id}/flow",
            get(field_journal_flow::<C>).post(field_journal_flow::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/confirm-no-charge",
            post(invoice_post::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/flow",
            post(invoice_post::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/post",
            post(invoice_post::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/finance/invoices/{id}/post-runtime",
            get(invoice_post::<C>),
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
async fn invoice_post<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async {
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
        if request.method() == axum::http::Method::GET {
            return read_service_invoice_post_runtime(
                &target.authorizer,
                &host,
                inbound.into_proof(),
                move || {
                    scope?;
                    captured.resolve()
                },
                target.configuration.clone(),
                target.clock.clone(),
                id,
            )
            .await;
        }
        let mut keys = axum::http::HeaderMap::new();
        for value in request.headers().get_all("idempotency-key") {
            keys.append("idempotency-key", value.clone());
        }
        let no_charge=request.uri().path().ends_with("/confirm-no-charge");
        let flow=request.uri().path().ends_with("/flow");
        let proof = inbound.into_proof();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map(Arc::new)
            .map_err(|_| BusinessError::invalid("BODY_TOO_LARGE", "Use a smaller request."));
        let prepare=move||{
            let bytes=body.as_ref().map_err(Clone::clone)?;
            let input=if no_charge{erpx_server_finance::public::service_no_charge::normalize_service_no_charge(bytes)?.normalized().clone()}else if flow{crate::finance_service_invoice_flow_command::normalize_service_invoice_flow_assignment(bytes)?}else{normalize_service_invoice_post(bytes)?};
            let key=unique_request_header(&keys,"idempotency-key")?.filter(|v|!v.is_empty()).ok_or_else(||BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required."))?;
            Ok(WorkflowRequest{key:CommandKey::new(key)?.as_str().to_owned(),operation:if no_charge{"finance.service-no-charge.confirm"}else if flow{"flow.assign"}else{"finance.invoice.post"}.into(),record_id:id.clone(),input})
        };
        if no_charge{
            crate::finance_service_no_charge::execute_service_no_charge(&target.authorizer,&host,proof,move||{scope?;captured.resolve()},target.configuration.clone(),target.clock.clone(),prepare).await
        }else if flow{
            crate::finance_service_invoice_flow_command::execute_service_invoice_flow_assignment(&target.authorizer,&host,proof,move||{scope?;captured.resolve()},target.configuration.clone(),target.clock.clone(),prepare).await
        }else{
            execute_service_invoice_post(&target.authorizer,&host,proof,move||{scope?;captured.resolve()},target.configuration.clone(),target.clock.clone(),prepare).await
        }
    }
    .await;
    let mut response = match result {
        Ok(data) => match serde_json::from_str::<serde_json::Value>(data.as_str()) {
            Ok(value) => (StatusCode::OK, axum::Json(serde_json::json!({"data":value}))).into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_INVOICE_POST_UNAVAILABLE","message":"The invoice could not be encoded."}}))).into_response(),
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
        Err(PgTransactionError::Persistence(_)) => (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({"error":{"code":"FINANCE_INVOICE_POST_UNAVAILABLE","message":"The invoice could not be posted."}}))).into_response(),
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

async fn field_journal_flow<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<PostState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    let result:Result<serde_json::Value,PgTransactionError<BusinessError>>=async{
        let host=unique_request_header(request.headers(),"host")?.ok_or_else(missing)?.to_owned();
        let target=crate::http_scope::selected_company_target(&state.targets,&host,&slug,request.headers())?;
        let inbound=parse_inbound_account_request(&state.policy,request.headers())?;
        if !matches!(inbound.audience(),RequestAudience::Holding{..}){return Err(missing().into())}
        if unique_request_header(request.headers(),"origin")?.is_some_and(|v|v!=inbound.expected_origin()){
            return Err(BusinessError::new(FailureKind::Forbidden,"ORIGIN_FORBIDDEN","Use the application origin.").into())
        }
        let scope=state.scopes.slugs(request.headers());let(branch,department)=scope.as_ref().cloned().unwrap_or_default();
        let captured=CapturedHoldingMutation::capture(request.headers(),branch,department);
        let config=target.configuration.clone();let clock=target.clock.clone();let company=config.company.clone();
        if request.method()==axum::http::Method::GET{
            let source=crate::finance_field_journal_flow_command::Sources::new(&config)?;
            return Box::pin(target.authorizer.execute_with_sources_deferred(&host,inbound.into_proof(),move||{scope?;captured.resolve_read(company)},WorkAuthorizationRequest{permission:"finance.read".into(),module:Some("finance".into()),destination:"finance/journals".into(),api:true},source,move|c,current|{
                let(config,clock,id)=(config.clone(),clock.clone(),id.clone());
                Box::pin(async move{
                    let id=erpx_server_shared::public::RecordId::new(id)?;
                    let native=crate::finance_field_journal_flow::field_journal_flow_is_native(c,current,&id).await?;
                    if !native{
                        erpx_server_identity_access::public::selected_command_authority::require_selected_actor_current(c,current,clock.now().get()).await?;
                        return serde_json::to_value(crate::finance_field_journal_flow::FinanceFieldJournalFlowResponse{runtime:crate::finance_field_journal_flow::FinanceFieldJournalFlowRuntime::Legacy,data:None,workspace:None,access:None,context:crate::finance_journal_detail::rendered(current)?}).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_FIELD_JOURNAL_FLOW_UNAVAILABLE","The journal workflow could not be encoded.").into());
                    }
                    let flow=crate::finance_field_journal_flow::read_field_journal_flow(c,current,&id,&config,clock.as_ref()).await?;
                    let output=serde_json::to_value(crate::finance_field_journal_flow::FinanceFieldJournalFlowResponse{runtime:crate::finance_field_journal_flow::FinanceFieldJournalFlowRuntime::Rust,data:Some(flow.view.clone()),workspace:Some(flow.workspace()?),access:Some(flow.access(current)),context:crate::finance_journal_detail::rendered(current)?}).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_FIELD_JOURNAL_FLOW_UNAVAILABLE","The journal workflow could not be encoded."))?;
                    crate::finance_journal_detail::admit_finance_transport_response(&output)?;
                    flow.require_current(c,current,&config,clock.as_ref()).await?;
                    Ok(output)
                })
            })).await;
        }
        let mut keys=axum::http::HeaderMap::new();
        for value in request.headers().get_all("idempotency-key"){keys.append("idempotency-key",value.clone());}
        let proof=inbound.into_proof();
        let bytes=to_bytes(request.into_body(),BODY_LIMIT).await.map(Arc::new).map_err(|_|BusinessError::invalid("BODY_TOO_LARGE","Use a smaller request."));
        let prepare=move||{
            let input=crate::finance_service_invoice_flow_command::normalize_field_journal_flow_assignment(bytes.as_ref().map_err(Clone::clone)?)?;
            let key=unique_request_header(&keys,"idempotency-key")?.filter(|v|!v.is_empty()).ok_or_else(||BusinessError::invalid("IDEMPOTENCY_KEY_REQUIRED","Idempotency-Key is required."))?;
            Ok(WorkflowRequest{key:CommandKey::new(key)?.as_str().to_owned(),operation:"flow.assign".into(),record_id:id.clone(),input})
        };
        let reply=Box::pin(crate::finance_field_journal_flow_command::execute_field_journal_flow_assignment(&target.authorizer,&host,proof,move||{scope?;captured.resolve()},config,clock,prepare)).await?;
        let data:crate::finance_field_journal_flow::FinanceFieldJournalFlowResult=serde_json::from_str(reply.as_str()).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_FIELD_JOURNAL_FLOW_UNAVAILABLE","The journal workflow could not be encoded."))?;
        serde_json::to_value(crate::finance_field_journal_flow::FinanceFieldJournalFlowCommandResponse{data}).map_err(|_|BusinessError::new(FailureKind::Unavailable,"FINANCE_FIELD_JOURNAL_FLOW_UNAVAILABLE","The journal workflow could not be encoded.").into())
    }.await;
    let mut response=match result{
        Ok(value)=>(StatusCode::OK,axum::Json(value)).into_response(),
        Err(PgTransactionError::Business(e))=>{
            let status=match e.kind{FailureKind::InvalidInput=>StatusCode::UNPROCESSABLE_ENTITY,FailureKind::Unauthenticated=>StatusCode::UNAUTHORIZED,FailureKind::Forbidden=>StatusCode::FORBIDDEN,FailureKind::NotFound=>StatusCode::NOT_FOUND,FailureKind::Conflict=>StatusCode::CONFLICT,FailureKind::Unavailable=>StatusCode::SERVICE_UNAVAILABLE};
            (status,axum::Json(serde_json::json!({"error":{"code":e.code,"message":e.message}}))).into_response()
        },
        Err(PgTransactionError::Persistence(_))=>(StatusCode::INTERNAL_SERVER_ERROR,axum::Json(serde_json::json!({"error":{"code":"FINANCE_FIELD_JOURNAL_FLOW_UNAVAILABLE","message":"The journal workflow could not be completed."}}))).into_response(),
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
