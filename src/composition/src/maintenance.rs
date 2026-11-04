//! Native Maintenance read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded workspace,
//! detail, preventive plans and cancellation facts.
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
use erpx_server_maintenance::public::http::{
    MaintenanceHttpBackend, MaintenanceHttpCommand, MaintenanceHttpCommandRequest,
    MaintenanceHttpFuture, MaintenanceHttpReadKind, MaintenanceHttpReadRequest,
    MaintenanceHttpResponse, MaintenanceHttpTarget,
};
use erpx_server_maintenance::public::read::{
    read_maintenance_detail, read_maintenance_plan_detail, read_maintenance_plans,
    read_maintenance_workspace,
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
        "MAINTENANCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_UNAVAILABLE",
        "The Maintenance workspace is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MAINTENANCE_COMMAND_UNAVAILABLE",
        "Native Maintenance commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct MaintenanceSources;

impl HoldingWorkSourcePreparation for MaintenanceSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct MaintenanceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl MaintenanceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
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

pub struct MaintenanceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: MaintenanceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> MaintenanceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: MaintenanceConfiguration,
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
            module: Some("maintenance".into()),
            destination: "maintenance".into(),
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
    ) -> Result<MaintenanceHttpResponse, Error>
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
                MaintenanceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let data = read_maintenance_workspace(connection, current, now).await?;
                        Ok(MaintenanceHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<MaintenanceHttpResponse, Error>
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
                MaintenanceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let data = read_maintenance_detail(connection, current, &id, now).await?;
                        Ok(MaintenanceHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn plans<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<MaintenanceHttpResponse, Error>
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
                MaintenanceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let plans = read_maintenance_plans(connection, current, now).await?;
                        let data = Value::Array(plans.into_iter().map(|plan| plan.value()).collect());
                        Ok(MaintenanceHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn plan_detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<MaintenanceHttpResponse, Error>
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
                MaintenanceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let data = read_maintenance_plan_detail(connection, current, &id, now).await?;
                        Ok(MaintenanceHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct MaintenanceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<MaintenanceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> MaintenanceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<MaintenanceService<C>>,
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

impl<C: Clock + Send + Sync + 'static> MaintenanceHttpBackend for MaintenanceBackend<C> {
    fn read<'a>(
        &'a self,
        request: MaintenanceHttpReadRequest<'a>,
    ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.kind {
                MaintenanceHttpReadKind::Workspace => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                MaintenanceHttpReadKind::Detail(id) => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), id)
                    .await
                    .map_err(business),
                MaintenanceHttpReadKind::Plans => self
                    .service
                    .plans(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                MaintenanceHttpReadKind::PlanDetail(id) => self
                    .service
                    .plan_detail(request.host, proof, move || Ok(selection), id)
                    .await
                    .map_err(business),
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: MaintenanceHttpCommandRequest<'a>,
    ) -> MaintenanceHttpFuture<'a, MaintenanceHttpResponse> {
        Box::pin(async move {
            let (_proof, _selection) = self.context(request.headers)?;
            let _ = request.command;
            Err(command_unavailable())
        })
    }
}
