//! Native staff portal-support response/closure composition.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_customer_authority::public::portal_support_commands::{
    AcknowledgeInput, AcknowledgementReceipt, CloseInput, CustomerAckBackend, CustomerAckRequest,
    CustomerActor, CustomerTarget, PgPortalSupportCommands, RespondInput, StaffBackend,
    StaffCommand, StaffRequest, SupportActor, SupportFuture, SupportReply, SupportTarget,
    portal_support_customer_router, portal_support_staff_router,
};
use erpx_server_customer_authority::public::portal_session::{
    PgPortalEnrollment, PortalCustomerProof, PortalCustomerRequest, PortalSessionEvidence,
    PortalSessionFuture, PortalSessionVerifier, resolve_portal_customer,
};
use erpx_server_identity_access::public::portal_session::{PortalSessionProof,PortalSessionReadRequest,require_current_portal_session,verify_portal_session};
use erpx_server_identity_access::public::{AccountPolicy,DeviceProof,RequestAudience,account_inbound_http::parse_inbound_account_request,effective_authority::{AuthorizedWorkContext,HoldingWorkAuthorizer,HoldingWorkSourcePreparation,WorkAuthorizationRequest},work_context::{HoldingWorkspaceRequest,WorkScopeKind}};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{DatabasePool,PgTransactionError,PgUnitOfWork,PgWorkFuture,TransactionOptions};
use erpx_server_shared::public::{BusinessError,Clock,CompanyId,FailureKind,HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;
type Error=PgTransactionError<BusinessError>;
fn scope()->BusinessError{BusinessError::new(FailureKind::Forbidden,"SALES_SCOPE_REQUIRED","Choose the current authorized Company workspace.")}
fn not_found()->BusinessError{BusinessError::new(FailureKind::NotFound,"NOT_FOUND","Not found.")}
fn unavailable()->BusinessError{BusinessError::new(FailureKind::Unavailable,"SALES_UNAVAILABLE","The Sales service is unavailable.")}
fn business(e:Error)->BusinessError{match e{PgTransactionError::Business(e)=>e,PgTransactionError::Persistence(_)=>unavailable()}}
fn mode()->PackageEnforcementMode{match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT").unwrap_or_else(|_|"disabled".into()).as_str(){"disabled"=>PackageEnforcementMode::Disabled,"shadow"=>PackageEnforcementMode::Shadow,_=>PackageEnforcementMode::Enforced}}
fn production()->bool{std::env::var("NODE_ENV").map(|v|v=="production").unwrap_or(false)}
#[derive(Clone,Copy)]struct Sources;
impl HoldingWorkSourcePreparation for Sources{fn prepare<'c>(&'c self,_:&'c mut PgConnection)->PgWorkFuture<'c,(),BusinessError>{Box::pin(async{Ok(())})}}
#[derive(Clone)]pub struct PortalSupportConfiguration{holding:HoldingId,company:CompanyId}
impl PortalSupportConfiguration{pub fn new(holding:HoldingId,company:CompanyId)->Self{Self{holding,company}}pub fn company(&self)->&CompanyId{&self.company}}
fn actor(c:&AuthorizedWorkContext)->Result<SupportActor,BusinessError>{let s=c.selection();let w=s.workspace();let a=s.assignment();Ok(SupportActor{user_id:c.user_id().as_str().into(),company_id:w.company_id.as_str().into(),branch_id:w.branch_id.as_ref().map(|v|v.as_str().into()),department_id:w.department_id.as_ref().map(|v|v.as_str().into()),assignment_id:a.id.as_str().into(),role:a.role.clone(),session_id:Some(c.session_id().as_str().into()),assignment_version:Some(a.version),permissions:c.command_iam_metadata().permissions.clone()})}
fn context(c:&AuthorizedWorkContext)->Value{let s=c.selection();let w=s.workspace();let a=s.assignment();serde_json::json!({"userId":c.user_id().as_str(),"holdingId":c.holding_id().as_str(),"companyId":w.company_id.as_str(),"branchId":w.branch_id.as_ref().map(|v|v.as_str()),"departmentId":w.department_id.as_ref().map(|v|v.as_str()),"branchSlug":w.branch_slug,"departmentSlug":w.department_slug,"assignmentId":a.id.as_str(),"assignmentVersion":a.version,"revision":s.revision(),"role":a.role,"assignmentScopeKind":match a.scope_kind{WorkScopeKind::Holding=>"holding",WorkScopeKind::Branch=>"branch",WorkScopeKind::Department=>"department"},"modules":c.modules(),"permissions":c.command_iam_metadata().permissions,"expiresAt":c.expires_at()})}
pub struct PortalSupportService<C>{authorizer:Arc<HoldingWorkAuthorizer>,configuration:PortalSupportConfiguration,clock:Arc<C>,mode:PackageEnforcementMode,production:bool}
impl<C:Clock+Send+Sync+'static> PortalSupportService<C>{pub fn new(a:Arc<HoldingWorkAuthorizer>,c:PortalSupportConfiguration,cl:Arc<C>,m:PackageEnforcementMode,p:bool)->Self{Self{authorizer:a,configuration:c,clock:cl,mode:m,production:p}}pub fn from_runtime(a:Arc<HoldingWorkAuthorizer>,c:PortalSupportConfiguration,cl:Arc<C>)->Self{Self::new(a,c,cl,mode(),production())}pub fn company(&self)->&CompanyId{self.configuration.company()}
 pub async fn execute(&self,host:&str,proof:DeviceProof,selection:HoldingWorkspaceRequest,key:String,command:StaffCommand)->Result<SupportReply,Error>{let holding=self.configuration.holding.clone();let company=self.configuration.company.clone();let clock=self.clock.clone();let mode=self.mode;let production=self.production;let expected=company.clone();self.authorizer.execute_with_sources_deferred(host,proof,move||{let mut r=selection;if r.company_id!=expected{return Err(scope())}r.mutation=true;Ok(r)},WorkAuthorizationRequest{permission:"commerce.approve".into(),module:Some("sales".into()),destination:"sales".into(),api:true},Sources,move|connection,current|{let holding=holding.clone();let company=company.clone();let clock=clock.clone();let key=key.clone();let command=command.clone();Box::pin(async move{if current.holding_id()!=&holding||current.selection().workspace().company_id!=company{return Err(scope().into())}current.require_permission("commerce.approve")?;let actor=actor(current)?;let now=clock.now().get();let mut repo=PgPortalSupportCommands::new(connection,mode,production);let receipt=match command{StaffCommand::Respond{request_id,input}=>{let input:RespondInput=input;repo.publish(&actor,&request_id,input,&key,now).await?}StaffCommand::Close{request_id,input}=>{let input:CloseInput=input;repo.close(&actor,&request_id,input,&key,now).await?}};Ok(SupportReply{data:receipt.wire(),context:context(current)})})}).await}}
pub struct PortalSupportBackend<C>{policy:Arc<AccountPolicy>,scopes:Arc<dyn BusinessScopeSlugs>,service:Arc<PortalSupportService<C>>}
impl<C:Clock+Send+Sync+'static> PortalSupportBackend<C>{pub fn new(policy:Arc<AccountPolicy>,scopes:Arc<dyn BusinessScopeSlugs>,service:Arc<PortalSupportService<C>>)->Self{Self{policy,scopes,service}}fn context(&self,h:&HeaderMap)->Result<(DeviceProof,HoldingWorkspaceRequest),BusinessError>{let inbound=parse_inbound_account_request(&self.policy,h)?;if !matches!(inbound.audience(),RequestAudience::Holding{..}){return Err(not_found())}let(branch,department)=self.scopes.slugs(h)?;Ok((inbound.into_proof(),HoldingWorkspaceRequest{company_id:self.service.company().clone(),branch_slug:branch,department_slug:department,expected:None,mutation:false}))}}
impl<C:Clock+Send+Sync+'static> StaffBackend for PortalSupportBackend<C>{fn execute<'a>(&'a self,r:StaffRequest<'a>)->SupportFuture<'a,SupportReply>{Box::pin(async move{let(proof,selection)=self.context(r.headers)?;self.service.execute(r.host,proof,selection,r.key,r.command).await.map_err(business)})}}
pub fn target(host:String,holding_slug:String,origin:String,backend:Arc<dyn StaffBackend>)->SupportTarget{SupportTarget{host,holding_slug,origin,backend}}
pub fn router(targets:Vec<SupportTarget>)->Result<erpx_server_runtime::public::http_support::Router,BusinessError>{portal_support_staff_router(targets)}

/// Verified portal-product session acknowledgement. The Control Authority
/// proves the opaque product session and the Holding resolves the exact
/// enrolled customer; the acknowledgement then commits in one serializable
/// Holding unit and the session is re-proved after deferred effects while the
/// acknowledgement retains its own request/binding/identity image readback.
#[derive(Clone)]
pub struct PortalSupportAcknowledgementConfiguration { holding_id: HoldingId, holding_slug: String, audience: String }
impl PortalSupportAcknowledgementConfiguration {
    pub fn new(holding_id: HoldingId, holding_slug: String, audience: String) -> Self { Self { holding_id, holding_slug, audience } }
    pub fn holding_id(&self) -> &HoldingId { &self.holding_id }
}

struct AckSessionVerifier<'c> { connection: &'c mut PgConnection, proof: Option<PortalSessionProof> }
impl PortalSessionVerifier for AckSessionVerifier<'_> {
    type Error = Error;
    fn verify_session<'a>(&'a mut self, request: &'a PortalCustomerRequest<'_>, now: i64) -> PortalSessionFuture<'a, Option<PortalSessionEvidence>, Self::Error> {
        Box::pin(async move {
            let read = PortalSessionReadRequest { token: request.token, audience: request.audience, environment: request.environment };
            let proof = verify_portal_session(&mut *self.connection, &read, now).await?;
            if let Some(proof) = &proof { self.proof = Some(proof.clone()); }
            Ok(proof.map(|proof| PortalSessionEvidence { session_id: proof.session_id, user_id: proof.user_id, email: proof.email, email_verified: proof.email_verified, authenticated_at: proof.authenticated_at, expires_at: proof.expires_at }))
        })
    }
}

fn portal_session_required() -> BusinessError { BusinessError::new(FailureKind::Unauthenticated,"CUSTOMER_PORTAL_SESSION_REQUIRED","Sign in to the customer portal.") }
fn portal_session_changed() -> BusinessError { BusinessError::new(FailureKind::Unauthenticated,"SESSION_REVOKED","The verified customer sign-in changed.") }
fn portal_unavailable() -> BusinessError { BusinessError::new(FailureKind::Unavailable,"CUSTOMER_PORTAL_SUPPORT_UNAVAILABLE","The customer portal support service is unavailable.") }
fn cookie_token(headers: &HeaderMap) -> Result<String, BusinessError> {
    let raw = headers.get("cookie").and_then(|value| value.to_str().ok()).ok_or_else(portal_session_required)?;
    raw.split(';').filter_map(|part| part.trim().split_once('=')).find(|(name, _)| *name == "__Host-erpx-product").map(|(_, value)| value.to_owned()).filter(|value| !value.is_empty()).ok_or_else(portal_session_required)
}

async fn prove_customer(control: &DatabasePool, holding: &DatabasePool, audience: Option<&str>, token: &str, environment: &str, holding_id: &str, holding_slug: &str, now: i64) -> Result<(PortalCustomerProof, PortalSessionProof), Error> {
    let mut control = control.raw().acquire().await.map_err(|_| PgTransactionError::Business(portal_unavailable()))?;
    let mut holding = holding.raw().acquire().await.map_err(|_| PgTransactionError::Business(portal_unavailable()))?;
    let request = PortalCustomerRequest { token, audience, environment, holding_id, holding_slug };
    let mut verifier = AckSessionVerifier { connection: &mut control, proof: None };
    let mut enrollment = PgPortalEnrollment::new(&mut holding);
    let proof = resolve_portal_customer(&mut verifier, &mut enrollment, &request, now).await?;
    let session = verifier.proof.ok_or_else(|| PgTransactionError::Business(portal_session_required()))?;
    if session.expires_at <= now || proof.holding_id.as_deref() != Some(holding_id) { return Err(PgTransactionError::Business(portal_session_required())) }
    Ok((proof, session))
}

pub struct PortalSupportAcknowledgementService<C> {
    control: DatabasePool,
    holding: DatabasePool,
    configuration: PortalSupportAcknowledgementConfiguration,
    environment: String,
    clock: Arc<C>,
    mode: PackageEnforcementMode,
    production: bool,
}
impl<C: Clock + Send + Sync + 'static> PortalSupportAcknowledgementService<C> {
    pub fn new(control: DatabasePool, holding: DatabasePool, configuration: PortalSupportAcknowledgementConfiguration, environment: String, clock: Arc<C>, mode: PackageEnforcementMode, production: bool) -> Self { Self { control, holding, configuration, environment, clock, mode, production } }
    pub fn from_runtime(control: DatabasePool, holding: DatabasePool, configuration: PortalSupportAcknowledgementConfiguration, environment: String, clock: Arc<C>) -> Self { Self::new(control, holding, configuration, environment, clock, mode(), production()) }

    pub async fn execute(&self, _host: &str, headers: &HeaderMap, request_id: &str, input: AcknowledgeInput, key: &str) -> Result<(AcknowledgementReceipt, Value), Error> {
        let now = self.clock.now().get();
        let token = cookie_token(headers)?;
        let (proof, session) = prove_customer(&self.control, &self.holding, Some(self.configuration.audience.as_str()), &token, &self.environment, self.configuration.holding_id.as_str(), &self.configuration.holding_slug, now).await?;
        let actor = CustomerActor { user_id: proof.user_id, email: proof.email };
        let request = request_id.to_owned();
        let key = key.to_owned();
        let environment = self.environment.clone();
        let audience = self.configuration.audience.clone();
        let token = token.clone();
        let session = session.clone();
        let holding_id = self.configuration.holding_id.as_str().to_owned();
        let control = self.control.clone();
        let mode = self.mode;
        let production = self.production;
        PgUnitOfWork::new(self.holding.clone())
            .run_once(TransactionOptions::default(), move |connection| {
                let actor = actor.clone();
                let request = request.clone();
                let input = input.clone();
                let key = key.clone();
                let environment = environment.clone();
                let audience = audience.clone();
                let token = token.clone();
                let session = session.clone();
                let holding_id = holding_id.clone();
                let control = control.clone();
                Box::pin(async move {
                    let mut repo = PgPortalSupportCommands::new(connection, mode, production);
                    let receipt = repo.acknowledge(&actor, &request, input, &key, now).await?;
                    drop(repo);
                    let mut control = control.raw().acquire().await.map_err(|_| PgTransactionError::Business(portal_unavailable()))?;
                    let read = PortalSessionReadRequest { token: &token, audience: Some(audience.as_str()), environment: &environment };
                    require_current_portal_session(&mut control, &read, &session, now).await.map_err(|error| match error { PgTransactionError::Business(_) => PgTransactionError::Business(portal_session_changed()), PgTransactionError::Persistence(_) => PgTransactionError::Business(portal_unavailable()) })?;
                    let context = serde_json::json!({"userId": actor.user_id, "email": actor.email, "holdingId": holding_id});
                    Ok((receipt, context))
                })
            })
            .await
    }
}

pub struct PortalSupportAcknowledgementBackend<C> { service: Arc<PortalSupportAcknowledgementService<C>> }
impl<C: Clock + Send + Sync + 'static> PortalSupportAcknowledgementBackend<C> {
    pub fn new(service: Arc<PortalSupportAcknowledgementService<C>>) -> Self { Self { service } }
}
impl<C: Clock + Send + Sync + 'static> CustomerAckBackend for PortalSupportAcknowledgementBackend<C> {
    fn execute<'a>(&'a self, r: CustomerAckRequest<'a>) -> SupportFuture<'a, SupportReply> {
        Box::pin(async move {
            let (receipt, context) = self.service.execute(r.host, r.headers, &r.request_id, r.input, &r.key).await.map_err(business)?;
            Ok(SupportReply { data: receipt.wire(), context })
        })
    }
}
pub fn customer_target(host:String,holding_slug:String,origin:String,backend:Arc<dyn CustomerAckBackend>)->CustomerTarget{CustomerTarget{host,holding_slug,origin,backend}}
pub fn customer_router(targets:Vec<CustomerTarget>)->Result<erpx_server_runtime::public::http_support::Router,BusinessError>{portal_support_customer_router(targets)}
