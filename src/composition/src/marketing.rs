//! Native Marketing campaign/attribution assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! campaign ledger, attribution summary and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_marketing::public::campaign_attribution::{
    MarketingCampaignActor, MarketingCampaignEntitlement, MarketingCampaignEntitlementPort,
    MarketingCampaignFuture,
};
use erpx_server_marketing::public::campaign_attribution_http::{
    MarketingCampaignHttpBackend, MarketingCampaignHttpFuture, MarketingCampaignHttpReadRequest,
    MarketingCampaignHttpResponse,
};
use erpx_server_marketing::public::lead_candidate::{
    MarketingLeadActor, PgMarketingLead, asset_profiles, lead_detail,
};
use erpx_server_marketing::public::lead_candidate_http::{
    MarketingLeadHttpBackend, MarketingLeadHttpFuture, MarketingLeadHttpReadRequest,
    MarketingLeadHttpResponse,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MARKETING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MARKETING_UNAVAILABLE",
        "The Marketing workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct MarketingSources;

impl HoldingWorkSourcePreparation for MarketingSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await.map_err(business)?;
            Ok(())
        })
    }
}

struct MarketingEntitlementImpl;

impl MarketingCampaignEntitlementPort for MarketingEntitlementImpl {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> MarketingCampaignFuture<'a, MarketingCampaignEntitlement, Self::Error> {
        // The selected composition has not yet bound trusted Commercial
        // enforcement and purchase facts. Do not treat their absence as the
        // disabled mode: that could expose data while enforcement is active.
        Box::pin(async { Err(unavailable()) })
    }
}

#[derive(Clone)]
pub struct MarketingConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl MarketingConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn marketing_actor(current: &AuthorizedWorkContext) -> Result<MarketingCampaignActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(MarketingCampaignActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

pub struct MarketingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: MarketingConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MarketingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: MarketingConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.read".into(),
            module: Some("marketing".into()),
            destination: "marketing".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<MarketingCampaignHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                MarketingSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        // No trusted Commerce mode or same-unit purchase
                        // witness has been bound to this composition yet.
                        Err(unavailable().into())
                    })
                },
            )
            .await
    }

    pub async fn lead_read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        candidate: Option<String>,
    ) -> Result<MarketingLeadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                MarketingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let candidate = candidate.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let (witness, revision) = {
                            let catalogue = current_package_catalogue()?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let inspection = purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["sales.marketing".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?;
                            inspection.require_allowed()?;
                            (inspection.witness.clone(), inspection.revision.clone())
                        };
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let actor = MarketingLeadActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: workspace.company_id.as_str().to_owned(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: Some(assignment.role.clone()),
                            authority_revision: Some(selected.revision()),
                            assignment_version: Some(assignment.version),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        let mut repository = PgMarketingLead;
                        let data = match candidate.as_deref() {
                            Some(id) => {
                                let detail = lead_detail(&actor, id, &mut repository, connection, now)
                                    .await
                                    .map_err(business)?;
                                serde_json::json!({
                                    "candidate": detail.candidate,
                                    "history": detail.history,
                                    "purchase": {
                                        "newWork": false,
                                        "settleExisting": false,
                                        "history": true,
                                        "revision": revision,
                                    }
                                })
                            }
                            None => {
                                let assets =
                                    asset_profiles(&actor, &mut repository, connection, now)
                                        .await
                                        .map_err(business)?;
                                serde_json::to_value(assets).map_err(|_| unavailable())?
                            }
                        };
                        let catalogue = current_package_catalogue()?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        Ok(MarketingLeadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

}

pub struct MarketingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MarketingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MarketingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MarketingService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingCampaignHttpBackend for MarketingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MarketingCampaignHttpReadRequest<'a>,
    ) -> MarketingCampaignHttpFuture<'a, MarketingCampaignHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> MarketingLeadHttpBackend for MarketingBackend<C> {
    fn read<'a>(
        &'a self,
        request: MarketingLeadHttpReadRequest<'a>,
    ) -> MarketingLeadHttpFuture<'a, MarketingLeadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .lead_read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.candidate_id.map(str::to_owned),
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<erpx_server_marketing::public::campaign_attribution_http::MarketingCampaignHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_marketing::public::campaign_attribution_http::marketing_campaign_http_router(targets)
}
