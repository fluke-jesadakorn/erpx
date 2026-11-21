//! Field's selected Company customer prerequisite. Finance owns its rows;
//! Workflow owns receipts/audit/outbox, and Commercial owns both purchases.
use erpx_server_commercial::public::purchases::{
    self, CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseInspection, PurchaseRequest,
};
use erpx_server_finance::public::{
    ServiceCustomerInput,
    field_service_customer::{
        CurrentServiceCustomerReferences, PreparedServiceCustomer, RetainedServiceCustomerCreation,
        ServiceCustomerCurrency, ServiceCustomerReference, lock_service_customer_sources,
        prepare_service_customer, read_service_customer_references,
    },
};
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam,
    effective_authority::{AuthorizedWorkContext, HoldingWorkSourcePreparation},
    selected_command_authority::require_selected_actor_current,
    work_context::WorkScopeKind,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, IdentifierSource, RecordId};
use erpx_server_workflow::public::{
    commands::{
        CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
        PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
        validate_key,
    },
    record_origin::{
        RawRecordOriginRequest, lock_record_origin_sources,
        read_raw_record_origin_result_with_limit, verify_raw_record_origin,
    },
    service_parts_posting::{lock_service_posting_receipts, read_service_prerequisite_receipt},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
const OP: &str = "finance.party.create";
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_CUSTOMER_SOURCE_CHANGED",
        "The original Service customer, references or receipt changed.",
    )
}
#[derive(Clone, Copy)]
pub struct ServiceCustomerSources;
impl HoldingWorkSourcePreparation for ServiceCustomerSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            purchases::lock_commercial_authority_sources(c).await?;
            lock_service_posting_receipts(c).await?;
            lock_record_origin_sources(c).await?;
            lock_service_customer_sources(c, true).await?;
            Ok(())
        })
    }
}
async fn authority(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &impl Clock,
    write: bool,
) -> Result<(), Error> {
    require_selected_actor_current(c, current, clock.now().get())
        .await
        .map_err(|error| match error {
            Error::Business(e)
                if matches!(e.code, "WORK_ROLE_UNAVAILABLE" | "WORK_CONTEXT_CHANGED") =>
            {
                Error::Business(BusinessError::new(
                    FailureKind::Forbidden,
                    "QUEUED_AUTHORITY_CHANGED",
                    "The original selected Service authority changed.",
                ))
            }
            other => other,
        })?;
    let s = current.selection();
    let w = s.workspace();
    if w.branch_id.is_some()
        || w.department_id.is_some()
        || s.assignment().scope_kind != WorkScopeKind::Holding
        || !current.modules().iter().any(|m| m == "field_service")
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SERVICE_CUSTOMER_REQUIRED",
            "Use a current Company Service and Finance assignment.",
        )
        .into());
    }
    current.require_permission("records.read")?;
    current.require_permission("finance.read")?;
    if write {
        current.require_permission("finance.write")?;
    }
    Ok(())
}
fn actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let s = current.selection();
    let a = s.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: s.workspace().company_id.clone(),
        branch_id: CommandField::Null,
        department_id: CommandField::Null,
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|n| n.as_str().into()),
    })
}
struct CustomerPurchases {
    catalogue: CommercialCatalogue,
    history: PurchaseInspection,
    new_work: PurchaseInspection,
}
impl CustomerPurchases {
    async fn capture(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        clock: &impl Clock,
    ) -> Result<Self, Error> {
        let catalogue = purchases::current_package_catalogue()?;
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &catalogue,
        );
        let request = |purpose| PurchaseRequest {
            capabilities: vec![
                "service.agreements".into(),
                "finance.service-invoice".into(),
            ],
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let history = reader
            .inspect(&request(CapabilityPurpose::History), clock.now())
            .await?;
        let new_work = reader
            .inspect(&request(CapabilityPurpose::NewWork), clock.now())
            .await?;
        Ok(Self {
            catalogue,
            history,
            new_work,
        })
    }
    async fn require_current(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        clock: &impl Clock,
        purpose: CapabilityPurpose,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &self.catalogue,
        );
        let h = reader
            .revalidate(&self.history.witness, clock.now())
            .await?;
        let n = reader
            .revalidate(&self.new_work.witness, clock.now())
            .await?;
        if h.revision != self.history.revision || n.revision != self.new_work.revision {
            return Err(changed().into());
        }
        match purpose {
            CapabilityPurpose::History => {
                h.require_allowed()?;
            }
            CapabilityPurpose::NewWork => {
                n.require_allowed()?;
            }
            _ => return Err(changed().into()),
        };
        Ok(())
    }
}
fn context_key(
    current: &AuthorizedWorkContext,
    refs: &CurrentServiceCustomerReferences,
    p: &CustomerPurchases,
) -> Result<String, BusinessError> {
    let image = CommandJson::from_serializable(&(
        actor(current)?.receipt_authority()?.as_str(),
        project_command_iam(current)?,
        refs.context_image(),
        &p.history.revision,
        &p.new_work.revision,
    ))?;
    Ok(hex::encode(Sha256::digest(image.as_str().as_bytes())))
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceCustomerPreparation {
    pub company_id: String,
    pub available: bool,
    pub context_key: Option<String>,
    pub can_create: bool,
    pub currencies: Vec<ServiceCustomerCurrency>,
    pub customers: Vec<ServiceCustomerReference>,
    #[cfg_attr(
        feature = "contracts",
        ts(type = "\"authority-required\" | \"purchase-required\" | null")
    )]
    pub unavailable_reason: Option<&'static str>,
}
fn unavailable(company: &str, reason: &'static str) -> ServiceCustomerPreparation {
    ServiceCustomerPreparation {
        company_id: company.into(),
        available: false,
        context_key: None,
        can_create: false,
        currencies: vec![],
        customers: vec![],
        unavailable_reason: Some(reason),
    }
}
pub async fn read_service_customer_preparation(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &impl Clock,
) -> Result<ServiceCustomerPreparation, Error> {
    let company = &current.selection().workspace().company_id;
    if current.selection().workspace().branch_id.is_some()
        || current.selection().workspace().department_id.is_some()
        || !current.modules().iter().any(|m| m == "field_service")
        || !current.has_permission("finance.read")
    {
        return Ok(unavailable(company.as_str(), "authority-required"));
    }
    authority(c, current, clock, false).await?;
    let p = CustomerPurchases::capture(c, current, clock).await?;
    if p.history.require_allowed().is_err() {
        authority(c, current, clock, false).await?;
        return Ok(unavailable(company.as_str(), "purchase-required"));
    }
    let refs = read_service_customer_references(c, company).await?;
    let can_create =
        current.has_permission("finance.write") && p.new_work.require_allowed().is_ok();
    if can_create {
        authority(c, current, clock, true).await?;
    }
    let view = ServiceCustomerPreparation {
        company_id: company.as_str().into(),
        available: true,
        context_key: Some(context_key(current, &refs, &p)?),
        can_create,
        currencies: refs.currencies()?,
        customers: refs.customers()?,
        unavailable_reason: None,
    };
    refs.require_current(c).await?;
    authority(c, current, clock, false).await?;
    p.require_current(c, current, clock, CapabilityPurpose::History)
        .await?;
    Ok(view)
}
struct CustomerAuthority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    purchases: CustomerPurchases,
    purpose: CapabilityPurpose,
}
impl<C: Clock + Sync> PgCommandAuthority for CustomerAuthority<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        authority(c, self.current, self.clock, true).await?;
        Ok(actor(self.current)?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        authority(c, self.current, self.clock, true).await?;
        self.purchases
            .require_current(c, self.current, self.clock, self.purpose)
            .await
    }
}
fn stored_result(
    result: &erpx_server_finance::public::ServiceCustomerResult,
    key: &str,
    company: &str,
) -> Result<CommandJson, BusinessError> {
    let public = CommandJson::from_serializable(result)?;
    let origin = CommandJson::from_serializable(
        &serde_json::json!({"kind":"native-service-customer.v1","companyId":company,"commandId":key}),
    )?;
    CommandJson::parse(&format!(
        "{},\"_serviceCustomerPreparation\":{}}}",
        &public.as_str()[..public.as_str().len() - 1],
        origin.as_str()
    ))
}
struct CustomerEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    input: &'a ServiceCustomerInput,
    purchases: &'a CustomerPurchases,
    prepared: Option<PreparedServiceCustomer>,
    retained: Option<RetainedServiceCustomerCreation>,
}
impl<C: Clock + Sync> PgOwnerCommandEffects for CustomerEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let refs =
            read_service_customer_references(c, &self.current.selection().workspace().company_id)
                .await?;
        if self.input.context_key != context_key(self.current, &refs, self.purchases)? {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_CUSTOMER_CONTEXT_CHANGED",
                "Review the current Service customer references before submitting.",
            )
            .into());
        }
        let plan = prepare_service_customer(
            refs,
            request.record_id.clone(),
            self.input.input.clone(),
            self.clock.now().get(),
        )?;
        let result = stored_result(
            &plan.result(),
            &request.key,
            self.current.selection().workspace().company_id.as_str(),
        )?;
        self.prepared = Some(plan);
        Ok(WorkflowOutput {
            version: 1,
            result,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained = Some(self.prepared.take().ok_or_else(changed)?.persist(c).await?);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let retained = self.retained.as_ref().ok_or_else(changed)?;
        retained.require_current(c).await?;
        sqlx::raw_sql("SET CONSTRAINTS finance_book_projection IMMEDIATE; SET CONSTRAINTS finance_book_projection DEFERRED; SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await.map_err(|error| {
            if error.as_database_error().is_some_and(|db| db.code().as_deref()==Some("23514") && db.message()=="PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED") { Error::Business(changed()) } else { error.into() }
        })?;
        retained.require_current(c).await
    }
}
struct CustomerIds;
impl IdentifierSource for CustomerIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID record identity")
    }
}
pub struct ServiceCustomerOutcome {
    pub result: CommandJson,
    pub confirmation: CommandJson,
    pub replayed: bool,
}
pub async fn execute_service_customer(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    clock: &(impl Clock + Sync),
    key: &str,
    input: &ServiceCustomerInput,
    confirmation: bool,
) -> Result<ServiceCustomerOutcome, Error> {
    validate_key(key)?;
    authority(c, current, clock, true).await?;
    let receipt = read_service_prerequisite_receipt(c, key).await?;
    let company = &current.selection().workspace().company_id;
    if receipt.facts().is_some_and(|r| {
        r.operation != OP
            || r.actor_id != current.user_id().as_str()
            || r.company_id.as_deref() != Some(company.as_str())
    }) {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "IDEMPOTENCY_CONFLICT",
            "This key belongs to a different original request.",
        )
        .into());
    }
    let replay = receipt.facts().is_some();
    if confirmation && !replay {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SERVICE_CUSTOMER_RECEIPT_NOT_FOUND",
            "Retry the original request.",
        )
        .into());
    }
    let id = receipt
        .facts()
        .map(|r| r.record_id.clone())
        .unwrap_or_else(|| format!("party-{}", uuid::Uuid::new_v4()));
    let purchases = CustomerPurchases::capture(c, current, clock).await?;
    let purpose = if replay {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::NewWork
    };
    purchases
        .require_current(c, current, clock, purpose)
        .await?;
    let effects_purchases = CustomerPurchases::capture(c, current, clock).await?;
    let mut auth = CustomerAuthority {
        current,
        clock,
        purchases,
        purpose,
    };
    let mut effects = CustomerEffects {
        current,
        clock,
        input,
        purchases: &effects_purchases,
        prepared: None,
        retained: None,
    };
    let request = WorkflowRequest {
        key: key.into(),
        operation: OP.into(),
        record_id: id.clone(),
        input: CommandJson::from_serializable(input)?,
    };
    let outcome = PgCommandLedger::new(company.clone(), c, &mut auth, clock, &mut CustomerIds)
        .execute(&request, &mut effects)
        .await?;
    let identity = RawRecordOriginRequest {
        command_key: key.into(),
        operation: OP.into(),
        record_id: id.clone(),
        creator_user_id: current.user_id().as_str().into(),
        company_id: company.clone(),
    };
    let (origin, result) = read_raw_record_origin_result_with_limit(c, identity, 2 * 1024 * 1024)
        .await?
        .ok_or_else(changed)?;
    let expected = erpx_server_finance::public::ServiceCustomerResult {
        id: id.clone(),
        party: input.input.clone(),
        credit_limit_minor: input.input.credit_minor().map_err(|_| changed())?,
    };
    if result.as_str() != stored_result(&expected, key, company.as_str())?.as_str()
        || result != outcome.result
    {
        return Err(changed().into());
    }
    let refs = read_service_customer_references(c, company).await?;
    let party = refs
        .customer(&id)?
        .filter(|p| p.kind == "customer" && matches!(p.status.as_str(), "active" | "blocked"))
        .ok_or_else(changed)?;
    let confirmation = CommandJson::from_serializable(&ServiceCustomerConfirmation {
        command_id: key.into(),
        company_id: company.as_str().into(),
        id,
        kind: "customer".into(),
        status: party.status,
        code: party.code,
        name: party.name,
        currency: party.currency,
    })?;
    refs.require_current(c).await?;
    verify_raw_record_origin(c, &origin).await?;
    auth.purchases
        .require_current(c, current, clock, purpose)
        .await?;
    authority(c, current, clock, true).await?;
    Ok(ServiceCustomerOutcome {
        result: CommandJson::from_serializable(&expected)?,
        confirmation,
        replayed: outcome.replayed,
    })
}

use crate::{
    field_service_reservation_read::{FieldServiceRenderedContext, context, selected_read_context},
    service_parts_posting_setup::{ServicePostingEnvironment, ServicePostingTarget},
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        HoldingWorkAuthorizer, SelectedContextRead, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use std::sync::Arc;
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceCustomerReadResponse {
    pub data: ServiceCustomerPreparation,
    pub context: FieldServiceRenderedContext,
}
pub struct ServiceCustomerCommandResponse {
    pub data: CommandJson,
    pub context: FieldServiceRenderedContext,
}
struct CustomerReadIngress {
    selected: SelectedContextRead,
    holding: erpx_server_shared::public::HoldingId,
    company: erpx_server_shared::public::CompanyId,
}
enum CustomerReadDenial {
    View(Box<ServiceCustomerReadResponse>),
    Ingress(BusinessError),
}
fn read_denied(
    input: &CustomerReadIngress,
    error: BusinessError,
) -> Result<CustomerReadDenial, BusinessError> {
    if error.code != "PERMISSION_REQUIRED" {
        return Err(error);
    }
    if input.selected.holding_id() != &input.holding
        || input.selected.selection().workspace().company_id != input.company
    {
        return Err(changed());
    }
    Ok(CustomerReadDenial::View(Box::new(
        ServiceCustomerReadResponse {
            data: unavailable(input.company.as_str(), "authority-required"),
            context: selected_read_context(&input.selected)?,
        },
    )))
}
pub async fn read_selected_service_customer<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
) -> Result<ServiceCustomerReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    if environment != ServicePostingEnvironment::Development {
        return Err(changed().into());
    }
    let selected_holding = holding.clone();
    let selected_company = company.clone();
    let request_company = company.clone();
    let callbacks=SelectedTransportCallbacks::<_,CustomerReadIngress,CustomerReadDenial,_>::new::<ServiceCustomerReadResponse>(move|selected:&SelectedWorkIngress<'_>|->SelectedIngressFuture<CustomerReadIngress,CustomerReadDenial>{let result=selected.read_context().and_then(|selected|{let company=selected_company.selected(&selected_holding,&selected)?;Ok(CustomerReadIngress{selected,holding:selected_holding.clone(),company})}).map_err(CustomerReadDenial::Ingress);Box::pin(async move{result})},read_denied,move|c,current,input:CustomerReadIngress|{let clock=clock.clone();let company=company.clone();Box::pin(async move{company.current(&input.holding,current)?;if current.selection().workspace().company_id!=input.company{return Err(changed().into());}let response=ServiceCustomerReadResponse{data:read_service_customer_preparation(c,current,clock.as_ref()).await?,context:context(current)?};serde_json::to_writer(&mut crate::field_service_reservation_read::ReplyBudget(0),&response).map_err(|_|changed())?;Ok(Ok(response))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request_company.request(request()?)?;
                r.mutation = false;
                Ok(r)
            },
            (
                WorkAuthorizationRequest {
                    permission: "records.read".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServiceCustomerSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(CustomerReadDenial::View(v)) => Ok(*v),
        Err(CustomerReadDenial::Ingress(e)) => Err(e.into()),
    }
}
pub struct ServiceCustomerTransportCommand {
    pub key: Result<String, BusinessError>,
    pub body: axum::body::Body,
    pub confirmation: bool,
}
pub async fn execute_selected_service_customer<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    request: Input,
    target: ServicePostingTarget<C>,
    command: ServiceCustomerTransportCommand,
) -> Result<ServiceCustomerCommandResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let ServicePostingTarget {
        holding,
        company,
        clock,
        environment,
    } = target;
    if environment != ServicePostingEnvironment::Development {
        return Err(changed().into());
    }
    let ServiceCustomerTransportCommand {
        key,
        body,
        confirmation,
    } = command;
    let request_company = company.clone();
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cached = Arc::new(tokio::sync::OnceCell::new());
    let callbacks=SelectedTransportCallbacks::<_,(String,ServiceCustomerInput),BusinessError,_>::new::<ServiceCustomerCommandResponse>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,ServiceCustomerInput),BusinessError>{let key=key.clone();let body=body.clone();let cached=cached.clone();Box::pin(async move{let key=key?;validate_key(&key)?;let bytes=cached.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(||BusinessError::invalid("SERVICE_CUSTOMER_INPUT_INVALID","Use a bounded customer input."))?;let text=String::from_utf8_lossy(bytes);let text=text.strip_prefix('\u{feff}').unwrap_or(&text);let json=CommandJson::parse(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let input=ServiceCustomerInput::parse(json.as_str().as_bytes()).map_err(|_|BusinessError::invalid("VALIDATION_FAILED","Use a valid Service customer input."))?;Ok((key,input))})},|_,e|Err(e),move|c,current,(key,input):(String,ServiceCustomerInput)|{let clock=clock.clone();let holding=holding.clone();let company=company.clone();Box::pin(async move{company.current(&holding,current)?;let rendered=context(current)?;let outcome=execute_service_customer(c,current,clock.as_ref(),&key,&input,confirmation).await?;let data=if confirmation{outcome.confirmation}else{outcome.result};if data.as_str().len().saturating_add(serde_json::to_vec(&rendered).map_err(|_|changed())?.len()).saturating_add(20)>2*1024*1024{return Err(changed().into());}Ok(Ok(ServiceCustomerCommandResponse{data,context:rendered}))})});
    match authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut r = request_company.request(request()?)?;
                r.mutation = true;
                Ok(r)
            },
            (
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: None,
                    destination: "services/field-service".into(),
                    api: true,
                },
                ServiceCustomerSources,
            ),
            callbacks,
        )
        .await?
    {
        Ok(v) => Ok(v),
        Err(e) => Err(e.into()),
    }
}

/// Public command contracts exclude the private native queue origin stored in
/// the immutable Workflow receipt.
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ServiceCustomerConfirmation {
    pub command_id: String,
    pub company_id: String,
    pub id: String,
    #[cfg_attr(feature = "contracts", ts(type = "\"customer\""))]
    pub kind: String,
    #[cfg_attr(feature = "contracts", ts(type = "\"active\" | \"blocked\""))]
    pub status: String,
    pub code: String,
    pub name: String,
    pub currency: String,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceCustomerCreateResponse {
    pub data: erpx_server_finance::public::ServiceCustomerResult,
    pub context: FieldServiceRenderedContext,
}
#[derive(Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServiceCustomerConfirmationResponse {
    pub data: ServiceCustomerConfirmation,
    pub context: FieldServiceRenderedContext,
}
