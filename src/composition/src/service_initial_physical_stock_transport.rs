//! Unmounted selected-Account transport for the Service initial Count.
//! The caller supplies trusted environment and Files policy; request data grants nothing.
use crate::field_service_reservation_read::{context, selected_read_context};
use crate::service_initial_physical_stock::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, SelectedContextRead, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource,
};
use serde::Serialize;
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use one exact warehouse, product and count date selection.",
    )
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service Count source changed.",
    )
}

#[derive(Clone)]
pub struct ServiceInitialCountTarget<C> {
    pub holding: HoldingId,
    pub company: CompanyId,
    pub clock: Arc<C>,
    pub environment: ServiceInitialPhysicalStockEnvironment,
    pub files_features: CountBindingFeatures,
}
#[derive(Clone)]
pub enum ServiceInitialCountReadRoute {
    Selection { query: Option<String> },
    Record { id: String, query: Option<String> },
    Flow { id: String, query: Option<String> },
}
pub enum ServiceInitialCountCommandRoute {
    Create,
    GenericAction { id: String, action: CountAction },
    GenericAssign { id: String },
    Update { id: String },
    Action { id: String, action: CountAction },
    Assign { id: String },
}
pub struct ServiceInitialCountTransportCommand {
    pub key: Result<String, BusinessError>,
    pub body: axum::body::Body,
    pub route: ServiceInitialCountCommandRoute,
}

use crate::service_initial_physical_stock_contracts::*;

fn component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            let hex = |x: u8| (x as char).to_digit(16);
            if let (Some(a), Some(z)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((a * 16 + z) as u8);
                i += 3;
            } else {
                out.push(b[i]);
                i += 1;
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn selection(query: Option<&str>) -> Result<InitialCountSelection, BusinessError> {
    if query.is_some_and(|s| s.len() > 2 * 1024 * 1024) {
        return Err(invalid());
    }
    let mut value = InitialCountSelection::default();
    for p in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
        let (k, v) = p.split_once('=').unwrap_or((p, ""));
        let k = component(k);
        let v = component(v);
        let dst = match k.as_str() {
            "warehouseId" => &mut value.warehouse_id,
            "productId" => &mut value.product_id,
            "countDate" => &mut value.count_date,
            _ => return Err(invalid()),
        };
        if dst.is_some()
            || v.is_empty()
            || (k == "countDate" && !crate::service_initial_physical_stock_command::date(&v))
        {
            return Err(invalid());
        }
        *dst = Some(v);
    }
    Ok(value)
}
fn no_query(query: &Option<String>) -> Result<(), BusinessError> {
    if query.as_deref().is_some_and(|v| !v.is_empty()) {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn scope(
    current: &erpx_server_identity_access::public::effective_authority::AuthorizedWorkContext,
    holding: &HoldingId,
    company: &CompanyId,
) -> Result<(), BusinessError> {
    if current.holding_id() != holding || &current.selection().workspace().company_id != company {
        Err(BusinessError::new(
            FailureKind::Forbidden,
            "SERVICE_INITIAL_PHYSICAL_STOCK_SCOPE_REQUIRED",
            "Use the current Company.",
        ))
    } else {
        Ok(())
    }
}
fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "inventory.read".into(),
        module: None,
        destination: "services/field-service".into(),
        api: true,
    }
}
fn bounded<T: Serialize>(v: &T) -> Result<(), BusinessError> {
    serde_json::to_writer(
        &mut crate::field_service_reservation_read::ReplyBudget(0),
        v,
    )
    .map_err(|_| changed())
}

fn checked<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, BusinessError> {
    fn safe(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Number(n) => n
                .as_i64()
                .is_some_and(|n| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n)),
            serde_json::Value::Array(a) => a.iter().all(safe),
            serde_json::Value::Object(o) => o.values().all(safe),
            _ => true,
        }
    }
    if !safe(&value) {
        return Err(changed());
    }
    serde_json::from_value(value).map_err(|_| changed())
}

struct CountReadIngress {
    route: ServiceInitialCountReadRoute,
    selected: SelectedContextRead,
    holding: HoldingId,
    company: CompanyId,
}
enum CountReadDenial {
    View(Box<ServiceInitialCountReadResponse>),
    Ingress(BusinessError),
}
fn read_denied(
    input: &CountReadIngress,
    error: BusinessError,
) -> Result<CountReadDenial, BusinessError> {
    if error.code != "PERMISSION_REQUIRED"
        || matches!(input.route, ServiceInitialCountReadRoute::Flow { .. })
    {
        return Err(error);
    }
    if input.selected.holding_id() != &input.holding
        || input.selected.selection().workspace().company_id != input.company
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "SERVICE_INITIAL_PHYSICAL_STOCK_SCOPE_REQUIRED",
            "Use the current Company.",
        ));
    }
    Ok(CountReadDenial::View(Box::new(
        ServiceInitialCountReadResponse {
            context: selected_read_context(&input.selected)?,
            data: ServiceInitialCountReadData::Workspace(CountWorkspace {
                company_id: input.company.as_str().into(),
                available: false,
                currency: None,
                document_currency: None,
                context_key: None,
                unavailable_reason: Some(CountUnavailableReason::StockAuthorityRequired),
                can_prepare: false,
                can_create_warehouse: false,
                can_create_product: false,
                selection: CountSelection {
                    warehouse_id: None,
                    product_id: None,
                    count_date: None,
                },
                readiness: CountReadiness {
                    schema_available: false,
                    posting_configured: false,
                    variance_account_available: false,
                    posting_period_available: false,
                },
                warehouses: vec![],
                branches: vec![],
                products: vec![],
                record: None,
            }),
        },
    )))
}
pub async fn read_selected_service_initial_count<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServiceInitialCountTarget<C>,
    route: ServiceInitialCountReadRoute,
) -> Result<ServiceInitialCountReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServiceInitialCountTarget {
        holding,
        company,
        clock,
        environment,
        ..
    } = target;
    let sources = environment.sources();
    let expected_holding = holding.clone();
    let expected_company = company.clone();
    let callbacks=SelectedTransportCallbacks::<_,CountReadIngress,CountReadDenial,_>::new::<ServiceInitialCountReadResponse>(move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<CountReadIngress,CountReadDenial>{let route=route.clone();let holding=expected_holding.clone();let company=expected_company.clone();let selected=selected.read_context().map_err(CountReadDenial::Ingress);Box::pin(async move{let selected=selected?;match &route {ServiceInitialCountReadRoute::Selection{query}=>{selection(query.as_deref()).map_err(CountReadDenial::Ingress)?;},ServiceInitialCountReadRoute::Record{id,query}|ServiceInitialCountReadRoute::Flow{id,query}=>{no_query(query).map_err(CountReadDenial::Ingress)?;if id.is_empty(){return Err(CountReadDenial::Ingress(invalid()));}}};Ok(CountReadIngress{route,selected,holding,company})})},read_denied,move|c,current,input:CountReadIngress|{let clock=clock.clone();let env=environment.clone();let holding=holding.clone();let company=company.clone();Box::pin(async move{scope(current,&holding,&company)?;let data=match input.route {ServiceInitialCountReadRoute::Selection{query}=>{let value=read_initial_count_selection(c,current,clock.as_ref(),selection(query.as_deref())?,&env).await?;ServiceInitialCountReadData::Workspace(checked(serde_json::to_value(value).map_err(|_|changed())?)?)},ServiceInitialCountReadRoute::Record{id,..}=>{let value=read_initial_count_workspace_record(c,current,clock.as_ref(),&id,&env).await?;ServiceInitialCountReadData::Workspace(checked(serde_json::to_value(value).map_err(|_|changed())?)?)},ServiceInitialCountReadRoute::Flow{id,..}=>ServiceInitialCountReadData::Flow(checked(read_service_initial_count_flow(c,current,clock.as_ref(),&id,&env).await?)?)};let response=ServiceInitialCountReadResponse{data,context:context(current)?};bounded(&response)?;Ok(Ok(response))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request()?;
                r.mutation = false;
                Ok(r)
            },
            (access(), sources),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(CountReadDenial::View(v)) => Ok(*v),
        Err(CountReadDenial::Ingress(e)) => Err(e.into()),
    }
}

struct CountIds;
impl IdentifierSource for CountIds {
    fn next(&mut self) -> erpx_server_shared::public::RecordId {
        erpx_server_shared::public::RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("UUID identity")
    }
}
#[derive(Clone)]
enum ParsedCommand {
    Create(InitialCountCreate),
    GenericAssign(String, erpx_server_workflow::public::commands::CommandJson),
    Update(String, String),
    Action(String, CountAction, InitialCountActionInput),
    Assign(String, erpx_server_workflow::public::commands::CommandJson),
}
pub async fn execute_selected_service_initial_count<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServiceInitialCountTarget<C>,
    command: ServiceInitialCountTransportCommand,
) -> Result<ServiceInitialCountCommandResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServiceInitialCountTarget {
        holding,
        company,
        clock,
        environment,
        files_features,
    } = target;
    let sources = environment.sources();
    let ServiceInitialCountTransportCommand { key, body, route } = command;
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cached = Arc::new(tokio::sync::OnceCell::new());
    let route = Arc::new(route);
    let callbacks=SelectedTransportCallbacks::<_,(String,ParsedCommand),BusinessError,_>::new::<ServiceInitialCountCommandResponse>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,ParsedCommand),BusinessError>{let body=body.clone();let cached=cached.clone();let key=key.clone();let route=route.clone();Box::pin(async move{let key=key?;if key.is_empty(){return Err(BusinessError::invalid("COMMAND_KEY_REQUIRED","Idempotency-Key is required."));}let bytes=cached.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|b|b.to_vec())}).await.as_ref().ok_or_else(invalid)?;let raw=String::from_utf8_lossy(bytes);let raw=raw.strip_prefix('\u{feff}').unwrap_or(&raw);erpx_server_workflow::public::commands::CommandJson::parse(raw).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let parsed=match route.as_ref(){ServiceInitialCountCommandRoute::Create=>ParsedCommand::Create(normalize_initial_count_create(raw)?),ServiceInitialCountCommandRoute::GenericAction{id,action}=>{if id.is_empty(){return Err(invalid());}ParsedCommand::Action(id.clone(),*action,normalize_generic_initial_count_action(raw)?)},ServiceInitialCountCommandRoute::GenericAssign{id}=>{if id.is_empty(){return Err(invalid());}ParsedCommand::GenericAssign(id.clone(),normalize_generic_stock_count_assignment(raw)?)},ServiceInitialCountCommandRoute::Update{id}=>{if id.is_empty(){return Err(invalid());}crate::service_initial_physical_stock_update::normalize_service_count_update(raw)?;ParsedCommand::Update(id.clone(),raw.to_owned())},ServiceInitialCountCommandRoute::Action{id,action}=>{if id.is_empty(){return Err(invalid());}ParsedCommand::Action(id.clone(),*action,normalize_initial_count_action(raw)?)},ServiceInitialCountCommandRoute::Assign{id}=>{if id.is_empty(){return Err(invalid());}ParsedCommand::Assign(id.clone(),normalize_stock_count_assignment(raw)?)} };Ok((key,parsed))})},|_,e|Err(e),move|c,current,(key,parsed):(String,ParsedCommand)|{let files_features=files_features.clone();let clock=clock.clone();let env=environment.clone();let holding=holding.clone();let company=company.clone();Box::pin(async move{scope(current,&holding,&company)?;let data=match parsed {ParsedCommand::Create(input)=>{let value=create_service_initial_count(c,current,clock.as_ref(),&mut CountIds,&key,&input,&env).await?;ServiceInitialCountCommandData::Create(checked(value)?)},ParsedCommand::Update(id,raw)=>{let value=crate::service_initial_physical_stock_update::execute_service_initial_count_update(c,current,clock.as_ref(),&mut CountIds,crate::service_initial_physical_stock_update::ServiceCountUpdateCommand{key:&key,id:&id,raw:&raw},&env).await?;ServiceInitialCountCommandData::Create(checked(value)?)},ParsedCommand::Action(id,action,input)=>{let value=execute_service_count_action(c,current,clock.as_ref(),&mut CountIds,ServiceCountActionCommand{key:&key,id:&id,action,input:&input,files_features},&env).await?;ServiceInitialCountCommandData::Action(checked(value)?)},ParsedCommand::GenericAssign(id,input)=>{let value=assign_generic_service_count_flow(c,current,clock.as_ref(),&mut CountIds,ServiceCountAssignmentCommand{key:&key,id:&id,input:&input},&env).await?;ServiceInitialCountCommandData::Assign(checked(value)?)},ParsedCommand::Assign(id,input)=>{let value=assign_service_count_flow(c,current,clock.as_ref(),&mut CountIds,ServiceCountAssignmentCommand{key:&key,id:&id,input:&input},&env).await?;ServiceInitialCountCommandData::Assign(checked(value)?)}};let response=ServiceInitialCountCommandResponse{data,context:context(current)?};bounded(&response)?;Ok(Ok(response))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request()?;
                r.mutation = true;
                Ok(r)
            },
            (access(), sources),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_and_projection_are_bounded_and_exact() {
        assert!(selection(Some("warehouseId=w&warehouseId=x")).is_err());
        assert!(selection(Some("warehouseId=w&extra=x")).is_err());
        assert!(selection(Some("countDate=2026-02-30")).is_err());
        assert_eq!(
            selection(Some("productId=a%2Bb"))
                .unwrap()
                .product_id
                .as_deref(),
            Some("a+b")
        );
        assert!(
            checked::<CountAssignmentResult>(serde_json::json!({"id":"x","version":1.5})).is_err()
        );
        assert!(
            checked::<CountAssignmentResult>(
                serde_json::json!({"id":"x","version":9007199254740992u64})
            )
            .is_err()
        );
    }
}
