//! Native CRM pipeline read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded pipeline,
//! staff and customer labels and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_crm::public::pipeline::{
    CrmPipelineActor, CrmPipelineCustomerDirectory, CrmPipelineEntitlement,
    CrmPipelineEntitlementPort, CrmPipelinePage, CrmPipelinePeopleDirectory, CrmPipelineQuery,
    PgCrmPipelineRead, crm_pipeline,
};
use erpx_server_crm::public::pipeline_http::{
    CrmPipelineHttpBackend, CrmPipelineHttpFuture, CrmPipelineHttpReadRequest,
    CrmPipelineHttpResponse,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_UNAVAILABLE",
        "The CRM pipeline is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CrmSources;

impl HoldingWorkSourcePreparation for CrmSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

struct CrmPeopleDirectoryImpl;

impl CrmPipelinePeopleDirectory for CrmPeopleDirectoryImpl {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        _user_ids: &'a [String],
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<
        'a,
        BTreeMap<String, String>,
        Self::Error,
    > {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

struct CrmCustomerDirectoryImpl;

impl CrmPipelineCustomerDirectory for CrmCustomerDirectoryImpl {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        _user_ids: &'a [String],
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<
        'a,
        BTreeMap<String, String>,
        Self::Error,
    > {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

struct CrmEntitlementImpl;

impl CrmPipelineEntitlementPort for CrmEntitlementImpl {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error>
    {
        Box::pin(async {
            Ok(CrmPipelineEntitlement {
                enforced: false,
                history_allowed: false,
                new_work: false,
                settle_existing: false,
                revision: String::new(),
            })
        })
    }
}

#[derive(Clone)]
pub struct CrmConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CrmConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn crm_actor(current: &AuthorizedWorkContext) -> Result<CrmPipelineActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(CrmPipelineActor {
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

pub struct CrmService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CrmConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CrmService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CrmConfiguration,
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
            permission: "commerce.read".into(),
            module: Some("crm".into()),
            destination: "crm".into(),
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
        query: CrmPipelineQuery,
    ) -> Result<CrmPipelineHttpResponse, Error>
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
                CrmSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let query = query.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = crm_actor(current)?;
                        let mut repository = PgCrmPipelineRead::new(connection);
                        let mut people = CrmPeopleDirectoryImpl;
                        let mut customers = CrmCustomerDirectoryImpl;
                        let mut entitlement = CrmEntitlementImpl;
                        let now = clock.now();
                        let page: CrmPipelinePage = crm_pipeline(
                            &actor,
                            &query,
                            &mut repository,
                            &mut people,
                            &mut customers,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(page).map_err(|_| unavailable())?;
                        Ok(CrmPipelineHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CrmBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CrmService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CrmBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CrmService<C>>,
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

impl<C: Clock + Send + Sync + 'static> CrmPipelineHttpBackend for CrmBackend<C> {
    fn read<'a>(
        &'a self,
        request: CrmPipelineHttpReadRequest<'a>,
    ) -> CrmPipelineHttpFuture<'a, CrmPipelineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let query = request.query.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), query)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(
    targets: Vec<erpx_server_crm::public::pipeline_http::CrmPipelineHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    erpx_server_crm::public::pipeline_http::crm_pipeline_http_router(targets)
}
