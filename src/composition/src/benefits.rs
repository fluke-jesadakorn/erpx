//! Native Benefits enrollment workspace HTTP assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! Benefits workspace, People worker/name and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_benefits::public::workspace::{
    BenefitsActor, BenefitsEntitlement, BenefitsEntitlementPort, BenefitsFuture, BenefitsPeoplePort,
    BenefitsWorker, PgBenefitsWorkspaceRead, benefits_workspace,
};
use erpx_server_benefits::public::workspace_http::{
    BenefitsWorkspaceHttpBackend, BenefitsWorkspaceHttpFuture, BenefitsWorkspaceHttpReadRequest,
    BenefitsWorkspaceHttpResponse,
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
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BENEFITS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BENEFITS_UNAVAILABLE",
        "The Benefits workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct BenefitsSources;

impl HoldingWorkSourcePreparation for BenefitsSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

struct BenefitsPeoplePortImpl;

impl BenefitsPeoplePort for BenefitsPeoplePortImpl {
    type Error = BusinessError;

    fn worker<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        _user_id: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitsWorker>, Self::Error> {
        Box::pin(async { Ok(None) })
    }

    fn employee_names<'a>(
        &'a mut self,
        _company: &'a CompanyId,
        person_ids: &'a [String],
    ) -> BenefitsFuture<'a, Vec<(String, String)>, Self::Error> {
        Box::pin(async {
            Ok(person_ids
                .iter()
                .map(|id| (id.clone(), "Unknown".to_owned()))
                .collect())
        })
    }
}

struct BenefitsEntitlementPortImpl;

impl BenefitsEntitlementPort for BenefitsEntitlementPortImpl {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> BenefitsFuture<'a, BenefitsEntitlement, Self::Error> {
        Box::pin(async {
            Ok(BenefitsEntitlement {
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
pub struct BenefitsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl BenefitsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn benefits_actor(current: &AuthorizedWorkContext) -> Result<BenefitsActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(BenefitsActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        work_context: "0".repeat(64),
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

pub struct BenefitsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: BenefitsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> BenefitsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: BenefitsConfiguration,
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
            module: Some("benefits".into()),
            destination: "benefits".into(),
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

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        personal: bool,
    ) -> Result<BenefitsWorkspaceHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let personal = personal;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                BenefitsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let personal = personal;
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = benefits_actor(current)?;
                        let mut repository = PgBenefitsWorkspaceRead::new(connection);
                        let mut people = BenefitsPeoplePortImpl;
                        let mut entitlement = BenefitsEntitlementPortImpl;
                        let now = clock.now();
                        let workspace = benefits_workspace(
                            &actor,
                            personal,
                            &mut repository,
                            &mut people,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(workspace).map_err(|_| unavailable())?;
                        Ok(BenefitsWorkspaceHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct BenefitsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<BenefitsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> BenefitsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<BenefitsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> BenefitsWorkspaceHttpBackend for BenefitsBackend<C> {
    fn read<'a>(
        &'a self,
        request: BenefitsWorkspaceHttpReadRequest<'a>,
    ) -> BenefitsWorkspaceHttpFuture<'a, BenefitsWorkspaceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let personal = request.personal;
            self.service
                .workspace(request.host, proof, move || Ok(selection), personal)
                .await
                .map_err(business)
        })
    }
}
