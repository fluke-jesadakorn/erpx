//! Native Projects read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded WBS read.
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
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest,
};
use erpx_server_projects::public::project_commands::{
    ProjectCommandActor, PgProjectMilestoneCommands, SaveMilestoneCommand, TransitionMilestoneCommand,
    save_milestone, transition_milestone,
};
use erpx_server_projects::public::project_commands_http::{
    ProjectCommandHttpResponse, ProjectMilestoneCommandHttpBackend, ProjectMilestoneSaveHttpRequest,
    ProjectMilestoneTransitionHttpRequest, ProjectCommandHttpFuture,
};
use erpx_server_projects::public::project_graph::{ProjectGraphActor, project_graph, PgProjectGraphRead};
use erpx_server_projects::public::project_graph_http::{
    ProjectGraphHttpBackend, ProjectGraphHttpFuture, ProjectGraphHttpRequest, ProjectGraphHttpResponse,
};
use erpx_server_projects::public::project_wbs::{ProjectReadActor, project_wbs};
use erpx_server_projects::public::project_wbs::PgProjectWbsRead;
use erpx_server_projects::public::project_wbs_http::{
    ProjectWbsHttpBackend, ProjectWbsHttpFuture, ProjectWbsHttpRequest, ProjectWbsHttpResponse,
    ProjectWbsHttpTarget, projects_router,
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
        "PROJECTS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROJECTS_UNAVAILABLE",
        "The Projects workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

async fn active_staff(
    connection: &mut PgConnection,
    user_id: &str,
    company: &CompanyId,
) -> Result<bool, BusinessError> {
    let value: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM hr_people p JOIN hr_engagements e          ON e.person_id=p.id AND e.company_id=p.company_id          WHERE p.user_id=$1 AND p.company_id=$2 AND p.status='active'            AND p.merged_into_id IS NULL AND e.status='active'            AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')            AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))",
    )
    .bind(user_id)
    .bind(company.as_str())
    .fetch_one(connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(value)
}

#[derive(Clone, Copy)]
struct ProjectsSources;

impl HoldingWorkSourcePreparation for ProjectsSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct ProjectsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ProjectsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn project_read_actor(current: &AuthorizedWorkContext) -> Result<ProjectReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(ProjectReadActor {
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

pub struct ProjectsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ProjectsConfiguration,
    clock: Arc<C>,
    catalogue: Arc<CommercialCatalogue>,
}

impl<C: Clock + Send + Sync + 'static> ProjectsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ProjectsConfiguration,
        clock: Arc<C>,
        catalogue: Arc<CommercialCatalogue>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            catalogue,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.read".into(),
            module: Some("projects".into()),
            destination: "projects".into(),
            api: true,
        }
    }

    fn command_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("projects".into()),
            destination: "projects".into(),
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

    pub async fn wbs<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
    ) -> Result<ProjectWbsHttpResponse, Error>
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
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let project_id = project_id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = project_read_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgProjectWbsRead::new(connection);
                        let data = project_wbs(&actor, &mut repository, &project_id, now).await?;
                        Ok(ProjectWbsHttpResponse {
                            data: serde_json::to_value(data).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn graph<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
    ) -> Result<ProjectGraphHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let project_id = project_id.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor_read = project_read_actor(current)?;
                        let now = clock.now();
                        let can_read_cost_history = if current.has_permission("finance.read") {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let request = PurchaseRequest {
                                capabilities: vec!["projects.costs".into()],
                                purpose: CapabilityPurpose::History,
                                mode: PackageEnforcementMode::Enforced,
                            };
                            match reader.inspect(&request, now).await {
                                Ok(inspection) => inspection.require_allowed().is_ok(),
                                Err(_) => false,
                            }
                        } else {
                            false
                        };
                        let actor = ProjectGraphActor {
                            read: actor_read,
                            can_read_cost_history,
                        };
                        let mut repository = PgProjectGraphRead::new(connection);
                        let data = project_graph(&actor, &mut repository, &project_id, now).await?;
                        Ok(ProjectGraphHttpResponse {
                            data: serde_json::to_value(data).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn save_milestone<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
        milestone_id: Option<String>,
        command: SaveMilestoneCommand,
        key: String,
    ) -> Result<ProjectCommandHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::command_access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let project_id = project_id.clone();
                    let milestone_id = milestone_id.clone();
                    let command = command.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let now = clock.now();
                        let active_staff = active_staff(connection, current.user_id().as_str(), &company).await?;
                        let mut reader = PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        );
                        let request = PurchaseRequest {
                            capabilities: vec!["projects.wbs".into()],
                            purpose: CapabilityPurpose::NewWork,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let wbs_new_work = match reader.inspect(&request, now).await {
                            Ok(inspection) => inspection.require_allowed().is_ok(),
                            Err(_) => false,
                        };
                        let actor = ProjectCommandActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.clone(),
                            branch_id: current.selection().workspace().branch_id.clone(),
                            department_id: current.selection().workspace().department_id.clone(),
                            assignment_id: current.selection().assignment().id.as_str().to_owned(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                            active_staff,
                            wbs_new_work,
                            wbs_settle_existing: false,
                        };
                        let mut repository = PgProjectMilestoneCommands::new(connection);
                        let receipt = save_milestone(
                            &actor,
                            &mut repository,
                            &project_id,
                            milestone_id.as_deref(),
                            &command,
                            &key,
                            now,
                        )
                        .await?;
                        Ok(ProjectCommandHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn transition_milestone<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
        milestone_id: String,
        command: TransitionMilestoneCommand,
        key: String,
    ) -> Result<ProjectCommandHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let catalogue = self.catalogue.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::command_access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let project_id = project_id.clone();
                    let milestone_id = milestone_id.clone();
                    let command = command.clone();
                    let key = key.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let now = clock.now();
                        let active_staff = active_staff(connection, current.user_id().as_str(), &company).await?;
                        let mut reader = PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        );
                        let new_request = PurchaseRequest {
                            capabilities: vec!["projects.wbs".into()],
                            purpose: CapabilityPurpose::NewWork,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let settle_request = PurchaseRequest {
                            capabilities: vec!["projects.wbs".into()],
                            purpose: CapabilityPurpose::SettleExisting,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let wbs_new_work = match reader.inspect(&new_request, now).await {
                            Ok(inspection) => inspection.require_allowed().is_ok(),
                            Err(_) => false,
                        };
                        let wbs_settle_existing = match reader.inspect(&settle_request, now).await {
                            Ok(inspection) => inspection.require_allowed().is_ok(),
                            Err(_) => false,
                        };
                        let actor = ProjectCommandActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.clone(),
                            branch_id: current.selection().workspace().branch_id.clone(),
                            department_id: current.selection().workspace().department_id.clone(),
                            assignment_id: current.selection().assignment().id.as_str().to_owned(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                            active_staff,
                            wbs_new_work,
                            wbs_settle_existing,
                        };
                        let mut repository = PgProjectMilestoneCommands::new(connection);
                        let receipt = transition_milestone(
                            &actor,
                            &mut repository,
                            &project_id,
                            &milestone_id,
                            &command,
                            &key,
                            now,
                        )
                        .await?;
                        Ok(ProjectCommandHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

}

pub struct ProjectsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ProjectsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ProjectsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ProjectsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> ProjectWbsHttpBackend for ProjectsBackend<C> {
    fn read<'a>(
        &'a self,
        request: ProjectWbsHttpRequest<'a>,
    ) -> ProjectWbsHttpFuture<'a, ProjectWbsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .wbs(request.host, proof, move || Ok(selection), request.project_id.to_owned())
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> ProjectGraphHttpBackend for ProjectsBackend<C> {
    fn read<'a>(
        &'a self,
        request: ProjectGraphHttpRequest<'a>,
    ) -> ProjectGraphHttpFuture<'a, ProjectGraphHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .graph(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> ProjectMilestoneCommandHttpBackend for ProjectsBackend<C> {
    fn save<'a>(
        &'a self,
        request: ProjectMilestoneSaveHttpRequest<'a>,
    ) -> ProjectCommandHttpFuture<'a, ProjectCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .save_milestone(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                    request.milestone_id.map(str::to_owned),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }

    fn transition<'a>(
        &'a self,
        request: ProjectMilestoneTransitionHttpRequest<'a>,
    ) -> ProjectCommandHttpFuture<'a, ProjectCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .transition_milestone(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                    request.milestone_id.to_owned(),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<ProjectWbsHttpTarget>) -> Result<axum::Router, BusinessError> {
    projects_router(targets)
}
