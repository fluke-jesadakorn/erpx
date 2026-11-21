//! Native Control preference composition. Identity supplies the actual Account
//! proof; Work Projection alone reads/writes navigation preferences. Preferences
//! are selectors only: every later Work request rechecks Company/grant authority.
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof,
    account_control_scope::{HoldingAccountControlScope, verify_holding_account_control},
    account_holding_bindings::ConfiguredHoldingRegistry,
    work_context::HoldingWorkContextInspector,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_work_projection::public::account_workspace_preferences::{
    AccountCompanyPreference, AccountWorkspacePreference, CurrentAccountWorkspacePreferences,
    lock_account_workspace_preferences, read_account_workspace_preferences,
};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
#[derive(serde::Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct NativeAccountPreferences {
    pub holding_id: String,
    pub holding_slug: String,
    pub holding_name: String,
    pub user_id: String,
    pub session_id: String,
    pub workspace: Option<AccountWorkspacePreference>,
    pub company: Option<AccountCompanyPreference>,
}
fn view(
    account: &HoldingAccountControlScope,
    p: &CurrentAccountWorkspacePreferences,
) -> Result<NativeAccountPreferences, Error> {
    Ok(NativeAccountPreferences {
        holding_id: account.holding_id().as_str().into(),
        holding_slug: account.holding_slug().into(),
        holding_name: account.holding_name().into(),
        user_id: account.user_id().as_str().into(),
        session_id: account.session().id.clone(),
        workspace: p.workspace()?,
        company: p.company()?,
    })
}
#[derive(Clone)]
pub struct NativeAccountPreferenceService<C> {
    pub control: DatabasePool,
    pub inspector: Arc<HoldingWorkContextInspector>,
    pub policy: Arc<AccountPolicy>,
    pub registry: Arc<ConfiguredHoldingRegistry>,
    pub clock: Arc<C>,
}
impl<C: Clock + Send + Sync + 'static> NativeAccountPreferenceService<C> {
    pub async fn read(
        &self,
        host: String,
        proof: DeviceProof,
    ) -> Result<NativeAccountPreferences, Error> {
        let policy = self.policy.clone();
        let registry = self.registry.clone();
        let clock = self.clock.clone();
        PgUnitOfWork::new(self.control.clone())
            .run(TransactionOptions::default(), move |c| {
                let policy = policy.clone();
                let registry = registry.clone();
                let proof = proof.clone();
                let host = host.clone();
                let clock = clock.clone();
                Box::pin(async move {
                    lock_account_workspace_preferences(c, false).await?;
                    let account = verify_holding_account_control(
                        c,
                        &host,
                        proof,
                        (*policy).clone(),
                        registry,
                        clock.now().get(),
                    )
                    .await?;
                    let preferences = read_account_workspace_preferences(
                        c,
                        account.user_id(),
                        account.holding_id(),
                    )
                    .await?;
                    preferences.require_current(c).await?;
                    account.require_current(c, clock.now().get()).await?;
                    view(&account, &preferences)
                })
            })
            .await
    }
    pub async fn select_company(
        &self,
        host: String,
        proof: DeviceProof,
        company: String,
        lineage: Option<(Option<String>, Option<String>, Option<String>)>,
    ) -> Result<NativeAccountPreferences, Error> {
        let policy = self.policy.clone();
        let registry = self.registry.clone();
        let clock = self.clock.clone();
        let inspector = self.inspector.clone();
        PgUnitOfWork::new(self.control.clone())
            .run(TransactionOptions::default(), move |c| {
                let policy = policy.clone();
                let registry = registry.clone();
                let proof = proof.clone();
                let host = host.clone();
                let clock = clock.clone();
                let inspector = inspector.clone();
                let company = company.clone();
                let lineage=lineage.clone();
                Box::pin(async move {
                    lock_account_workspace_preferences(c, true).await?;
                    let account = verify_holding_account_control(
                        c,
                        &host,
                        proof.clone(),
                        (*policy).clone(),
                        registry,
                        clock.now().get(),
                    )
                    .await?;
                    let preferences = read_account_workspace_preferences(
                        c,
                        account.user_id(),
                        account.holding_id(),
                    )
                    .await?;
                    if let Some((branch_slug,department_slug,selected_company))=lineage{
                        let company=selected_company.ok_or_else(||BusinessError::invalid("COMPANY_CONTEXT_REQUIRED","Reload the current Company workspace."))?;
                        let roles=crate::native_work_roles::execute(&inspector,&host,proof.clone(),erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest{company_id:erpx_server_shared::public::CompanyId::new(company)?,branch_slug,department_slug,expected:None,mutation:false},clock.clone(),crate::native_work_roles::WorkRoleAction::Inspect).await?;
                        if roles.options.is_empty(){return Err(BusinessError::new(FailureKind::Forbidden,"WORKSPACE_UNAVAILABLE","That workspace is unavailable.").into());}
                        return Err(BusinessError::new(FailureKind::Conflict,"COMPANY_DERIVED_FROM_LINEAGE","Local workspaces derive Company from their organization lineage.").into());
                    }
                    // Holding grant checks perform no writes. A preference never grants Company
                    // access; subsequent commands independently resolve current Holding authority.
                    let eligible = crate::native_work_roles::companies(
                        &inspector,
                        &host,
                        proof,
                        clock.clone(),
                    )
                    .await?;
                    if eligible.user_id != account.user_id().as_str()
                        || eligible.session_id != account.session().id
                        || eligible.holding_id != account.holding_id().as_str()
                        || !eligible.companies.iter().any(|c| c.id == company)
                    {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "COMPANY_UNAVAILABLE",
                            "That Company is not available in this Holding.",
                        )
                        .into());
                    }
                    account.require_current(c, clock.now().get()).await?;
                    let path = format!("/holdings/{}/home", account.holding_slug());
                    let saved = preferences
                        .remember_workspace(c, &path, Some(&company), clock.now().get())
                        .await?;
                    sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE")
                        .execute(&mut *c)
                        .await?;
                    saved.require_current(c).await?;
                    account.require_current(c, clock.now().get()).await?;
                    view(&account, &saved)
                })
            })
            .await
    }
}

#[derive(Clone)]
struct WorkspaceSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for WorkspaceSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut sqlx::PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_identity_access::public::account_work_presentation::lock_account_work_presentation_sources(c).await
        })
    }
}
impl<C: Clock + Send + Sync + 'static> NativeAccountPreferenceService<C> {
    pub async fn remember_workspace(
        &self,
        host: String,
        proof: DeviceProof,
        path: String,
        requested: erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
    ) -> Result<NativeAccountPreferences, Error> {
        let policy = self.policy.clone();
        let registry = self.registry.clone();
        let clock = self.clock.clone();
        let inspector = self.inspector.clone();
        PgUnitOfWork::new(self.control.clone()).run(TransactionOptions::default(),move|c|{let policy=policy.clone();let registry=registry.clone();let proof=proof.clone();let host=host.clone();let clock=clock.clone();let inspector=inspector.clone();let path=path.clone();let requested=requested.clone();Box::pin(async move{
   lock_account_workspace_preferences(c,true).await?;
   let account=verify_holding_account_control(c,&host,proof.clone(),(*policy).clone(),registry,clock.now().get()).await?;
   let selectors=erpx_server_work_projection::public::workspace_preference::workspace_preference_scope(&path,account.holding_slug())?;
   if selectors.branch_slug!=requested.branch_slug||selectors.department_slug!=requested.department_slug{return Err(BusinessError::new(FailureKind::Conflict,"WORK_CONTEXT_CHANGED","Review the current requested workspace.").into());}
   let preferences=read_account_workspace_preferences(c,account.user_id(),account.holding_id()).await?;
   let observed_clock=clock.clone();
   let (holding,user,session,company,deadline)=inspector.with_account_work(&host,proof,requested,WorkspaceSources,move|h,scope|{let clock=observed_clock.clone();Box::pin(async move{
    let presentation=erpx_server_identity_access::public::account_work_presentation::read_account_work_presentation(h,scope,"home",false,clock.now().get()).await?;
    scope.require_current(h,clock.now().get()).await?;
    Ok((scope.request().holding_id.as_str().to_owned(),scope.request().user_id.as_str().to_owned(),scope.request().session_id.as_str().to_owned(),scope.request().company_id.as_str().to_owned(),presentation.context.expires_at()))
   })}).await?;
   if holding!=account.holding_id().as_str()||user!=account.user_id().as_str()||session!=account.session().id||clock.now().get()>=deadline{return Err(BusinessError::new(FailureKind::Conflict,"WORK_CONTEXT_CHANGED","Review your current selected workspace.").into());}
   account.require_current(c,clock.now().get()).await?;
   let saved=preferences.remember_workspace(c,&path,Some(&company),clock.now().get()).await?;
   sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;
   saved.require_current(c).await?;account.require_current(c,clock.now().get()).await?;
   if clock.now().get()>=deadline{return Err(BusinessError::new(FailureKind::Conflict,"WORK_CONTEXT_CHANGED","Review your current selected workspace.").into());}
   view(&account,&saved)
  })}).await
    }
}
