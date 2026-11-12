//! Native Projects read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded WBS read.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest,
};
use erpx_server_files::public::project_attachment::{
    PROJECT_ATTACHMENT_PURPOSES, ProjectAttachmentContentVariant, bind_project_attachment_with_current_facts,
    project_attachment_bound_manifest, project_attachment_content, project_attachment_link_ids,
    project_attachment_link_status, project_attachment_links, project_attachment_set_facts,
    project_attachment_sets,
};
use erpx_server_files::public::canonical_original::LocalOriginalStore;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_projects::public::project_commands::{
    PgProjectMilestoneCommands, ProjectCommandActor, SaveMilestoneCommand,
    TransitionMilestoneCommand, save_milestone, transition_milestone,
};
use erpx_server_projects::public::project_commands_http::{
    ProjectCommandHttpFuture, ProjectCommandHttpResponse, ProjectMilestoneCommandHttpBackend,
    ProjectMilestoneSaveHttpRequest, ProjectMilestoneTransitionHttpRequest,
};
use erpx_server_projects::public::project_graph::{
    PgProjectGraphRead, ProjectGraphActor, project_graph,
};
use erpx_server_projects::public::project_graph_http::{
    ProjectGraphHttpBackend, ProjectGraphHttpFuture, ProjectGraphHttpRequest,
    ProjectGraphHttpResponse,
};
use erpx_server_projects::public::project_structure::{
    PgProjectStructureCommands, ProjectStructureActor, ProjectStructureCommand,
    execute_structure_command,
};
use erpx_server_projects::public::project_structure_http::{
    ProjectStructureHttpBackend, ProjectStructureHttpFuture, ProjectStructureHttpRequest,
    ProjectStructureHttpResponse, ProjectStructureHttpTarget,
};
use erpx_server_projects::public::project_actuals::{
    CreateCostAllocationCommand, DecideCostAllocationCommand, DecideWorkLogCommand,
    PgProjectActualsCommands, ProjectActualsActor, SaveWorkLogCommand, create_cost_allocation,
    decide_cost_allocation, decide_work_log, save_work_log,
};
use erpx_server_projects::public::project_actuals_http::{
    ProjectActualsHttpBackend, ProjectActualsHttpFuture, ProjectActualsHttpResponse,
    ProjectActualsHttpTarget, ProjectCostAllocationDecisionHttpRequest,
    ProjectCostAllocationHttpRequest, ProjectWorkLogDecisionHttpRequest, ProjectWorkLogHttpRequest,
};
use erpx_server_people_payroll::public::staff_facts as people_staff_facts;
use erpx_server_projects::public::project_attachment::{
    PROJECT_ATTACHMENT_BIND_OPERATION, PgProjectAttachmentCommandReceipts,
    ProjectAttachmentActor, ProjectAttachmentBindingCommand, ProjectAttachmentBoundLink,
    ProjectAttachmentCommandReceipt, ProjectAttachmentReceiptPlan, attachment_invalid,
    attachment_set_changed, attachment_set_empty, attachment_set_locked,
    attachment_set_not_found, project_attachment_authority_json,
    project_attachment_command_receipt, project_attachment_fingerprint,
    replay_project_attachment_binding,
};
use erpx_server_projects::public::project_attachment_read_http::{
    ProjectAttachmentContentHttpResponse, ProjectAttachmentReadContentHttpRequest,
    ProjectAttachmentReadHttpBackend, ProjectAttachmentReadHttpFuture,
    ProjectAttachmentReadHttpResponse,
    ProjectAttachmentReadHttpTarget, ProjectAttachmentReadLinksHttpRequest,
    ProjectAttachmentReadSetsHttpRequest, ProjectAttachmentReadStatusHttpRequest,
    project_attachment_read_router,
};
use erpx_server_projects::public::project_attachment_http::{
    ProjectAttachmentBindHttpBackend, ProjectAttachmentBindHttpRequest, ProjectAttachmentHttpFuture,
    ProjectAttachmentHttpResponse, ProjectAttachmentHttpTarget, project_attachment_router,
};
use erpx_server_projects::public::project_wbs::PgProjectWbsRead;
use erpx_server_projects::public::project_wbs::{ProjectReadActor, project_wbs};
use erpx_server_projects::public::project_wbs_http::{
    ProjectWbsHttpBackend, ProjectWbsHttpFuture, ProjectWbsHttpRequest, ProjectWbsHttpResponse,
    ProjectWbsHttpTarget, projects_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
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

#[derive(Clone)]
pub enum ProjectAttachmentReadAccess {
    Sets { project_id: String },
    Links { project_id: String, set_id: String },
    Status { project_id: String, link_id: String },
    Content { project_id: String, link_id: String, variant: ProjectAttachmentContentVariant },
}

pub enum ProjectAttachmentReadOutcome {
    Data(ProjectAttachmentReadHttpResponse),
    Content(ProjectAttachmentContentHttpResponse),
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
                        let active_staff =
                            active_staff(connection, current.user_id().as_str(), &company).await?;
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
                        let active_staff =
                            active_staff(connection, current.user_id().as_str(), &company).await?;
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


    pub async fn read_project_attachment<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        access: ProjectAttachmentReadAccess,
        store: Option<Arc<LocalOriginalStore>>,
    ) -> Result<ProjectAttachmentReadOutcome, Error>
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
                input,
                Self::access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let access = access.clone();
                    let store = store.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let actor = ProjectAttachmentActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.clone(),
                            branch_id: current.selection().workspace().branch_id.clone(),
                            department_id: current.selection().workspace().department_id.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                            active_staff: false,
                            wbs_purchased: false,
                        };
                        let response = match access {
                            ProjectAttachmentReadAccess::Sets { project_id } => {
                                let sets = project_attachment_sets(
                                    connection,
                                    &actor,
                                    &project_id,
                                    now,
                                )
                                .await?;
                                ProjectAttachmentReadOutcome::Data(
                                    ProjectAttachmentReadHttpResponse {
                                        data: serde_json::to_value(sets)
                                            .map_err(|_| attachment_invalid())?,
                                        context: context(current),
                                    },
                                )
                            }
                            ProjectAttachmentReadAccess::Links { project_id, set_id } => {
                                let links = project_attachment_links(
                                    connection,
                                    &actor,
                                    &project_id,
                                    &set_id,
                                    now,
                                )
                                .await?;
                                ProjectAttachmentReadOutcome::Data(
                                    ProjectAttachmentReadHttpResponse {
                                        data: serde_json::to_value(links)
                                            .map_err(|_| attachment_invalid())?,
                                        context: context(current),
                                    },
                                )
                            }
                            ProjectAttachmentReadAccess::Status { project_id, link_id } => {
                                let status = project_attachment_link_status(
                                    connection,
                                    &actor,
                                    &project_id,
                                    &link_id,
                                    now,
                                )
                                .await?;
                                ProjectAttachmentReadOutcome::Data(
                                    ProjectAttachmentReadHttpResponse {
                                        data: serde_json::to_value(status)
                                            .map_err(|_| attachment_invalid())?,
                                        context: context(current),
                                    },
                                )
                            }
                            ProjectAttachmentReadAccess::Content {
                                project_id,
                                link_id,
                                variant,
                            } => {
                                let store = store.as_deref().ok_or_else(|| {
                                    PgTransactionError::Business(unavailable())
                                })?;
                                let content = project_attachment_content(
                                    connection,
                                    store,
                                    &actor,
                                    &project_id,
                                    &link_id,
                                    variant,
                                    now,
                                )
                                .await?;
                                ProjectAttachmentReadOutcome::Content(
                                    ProjectAttachmentContentHttpResponse {
                                        media_type: content.media_type,
                                        filename: content.filename,
                                        inline: content.inline,
                                        body: content.body,
                                    },
                                )
                            }
                        };
                        Ok(response)
                    })
                },
            )
            .await
    }

    pub async fn bind_project_attachment<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        set_id: String,
        expected_version: i64,
        key: String,
    ) -> Result<ProjectAttachmentHttpResponse, Error>
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
                self.command_input(input),
                Self::command_access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let set_id = set_id.clone();
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
                        let key = CommandKey::new(key)?;
                        let now = clock.now();
                        let active_staff = people_staff_facts::active_staff(
                            connection,
                            company.as_str(),
                            current.user_id().as_str(),
                        )
                        .await?
                        .is_some();
                        // Retained project attachment writes require the current
                        // `projects.wbs` new-work capability for the `project`
                        // target (`authorizeProjectAttachmentPurchase`).
                        let wbs_purchased = purchase_allowed(
                            connection,
                            &holding,
                            &company,
                            &catalogue,
                            "projects.wbs",
                            CapabilityPurpose::NewWork,
                            now,
                        )
                        .await;
                        let actor = ProjectAttachmentActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.clone(),
                            branch_id: current.selection().workspace().branch_id.clone(),
                            department_id: current.selection().workspace().department_id.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                            active_staff,
                            wbs_purchased,
                        };
                        let facts =
                            project_attachment_set_facts(connection, company.as_str(), &set_id)
                                .await?
                                .ok_or_else(attachment_set_not_found)?;
                        if facts.company_id != company.as_str() {
                            return Err(attachment_set_not_found().into());
                        }
                        if facts.target_type != "project" {
                            return Err(BusinessError::invalid(
                                "ATTACHMENT_TARGET_UNSUPPORTED",
                                "This attachment target has no native Projects policy.",
                            )
                            .into());
                        }
                        if facts.component_kind != "record" {
                            return Err(BusinessError::invalid(
                                "ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED",
                                "Action, line and event evidence must bind inside its domain command.",
                            )
                            .into());
                        }
                        if !PROJECT_ATTACHMENT_PURPOSES.contains(&facts.purpose_key.as_str()) {
                            return Err(attachment_invalid().into());
                        }
                        // The Projects receipt is the replay authority. Check it
                        // before reading the now-bound draft links or stepping the
                        // project version a second time.
                        let stored = {
                            let mut receipts =
                                PgProjectAttachmentCommandReceipts::new(connection);
                            receipts.stored_receipt(key.as_str()).await?
                        };
                        if let Some(receipt) = replay_project_attachment_binding(
                            stored,
                            &actor,
                            &set_id,
                            expected_version,
                        )? {
                            let manifest = project_attachment_bound_manifest(
                                connection,
                                company.as_str(),
                                &set_id,
                                &facts.target_id,
                            )
                            .await?;
                            let data = attachment_response_data(&receipt, &manifest)?;
                            return Ok(ProjectAttachmentHttpResponse {
                                data,
                                context: context(current),
                            });
                        }
                        if facts.expected_target_version != expected_version {
                            return Err(attachment_set_changed().into());
                        }
                        if facts.state != "draft" {
                            return Err(attachment_set_locked().into());
                        }
                        let link_ids =
                            project_attachment_link_ids(connection, company.as_str(), &set_id)
                                .await?;
                        if link_ids.is_empty() {
                            return Err(attachment_set_empty().into());
                        }
                        let command = ProjectAttachmentBindingCommand {
                            target_type: "project".into(),
                            target_id: facts.target_id.clone(),
                            expected_version: facts.expected_target_version,
                            // Retained project-target writes authorize `new-work`
                            // and the exact `projects.wbs` capability.
                            purpose: "new-work".into(),
                            link_ids,
                            caption: None,
                        };
                        let binding = bind_project_attachment_with_current_facts(
                            connection,
                            &actor,
                            &command,
                            now,
                        )
                        .await?;
                        let manifest = project_attachment_bound_manifest(
                            connection,
                            company.as_str(),
                            &set_id,
                            &facts.target_id,
                        )
                        .await?;
                        let bound = manifest
                            .iter()
                            .map(|item| ProjectAttachmentBoundLink {
                                link_id: item.link_id.clone(),
                                source_id: item.source_id.clone(),
                            })
                            .collect::<Vec<_>>();
                        let receipt =
                            project_attachment_command_receipt(&binding, &bound)?;
                        let plan = ProjectAttachmentReceiptPlan {
                            key: key.as_str().to_owned(),
                            operation:
                                PROJECT_ATTACHMENT_BIND_OPERATION
                                    .to_owned(),
                            company_id: company.as_str().to_owned(),
                            actor_id: actor.user_id.clone(),
                            request_fingerprint: project_attachment_fingerprint(
                                &actor,
                                &set_id,
                                expected_version,
                            )?,
                            set_id: set_id.clone(),
                            receipt: receipt.clone(),
                            authority_json: project_attachment_authority_json(&actor)?,
                            now: now.get(),
                        };
                        let mut receipts =
                            PgProjectAttachmentCommandReceipts::new(connection);
                        receipts.record_receipt(&plan).await?;
                        let data = attachment_response_data(&receipt, &manifest)?;
                        Ok(ProjectAttachmentHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}



impl<C: Clock + Send + Sync + 'static> ProjectsService<C> {
    fn command_input<Input>(
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
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn structure_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
        command: ProjectStructureCommand,
        key: String,
    ) -> Result<ProjectStructureHttpResponse, Error>
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
                self.command_input(input),
                Self::command_access(),
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let catalogue = catalogue.clone();
                    let project_id = project_id.clone();
                    let command = command.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let now = clock.now();
                        let staff = staff_window(connection, current.user_id().as_str(), &company).await?;
                        let wbs_new_work = purchase_allowed(
                            connection, &holding, &company, &catalogue, "projects.wbs",
                            CapabilityPurpose::NewWork, now,
                        ).await;
                        let wbs_settle_existing = purchase_allowed(
                            connection, &holding, &company, &catalogue, "projects.wbs",
                            CapabilityPurpose::SettleExisting, now,
                        ).await;
                        let cost_history = purchase_allowed(
                            connection, &holding, &company, &catalogue, "projects.costs",
                            CapabilityPurpose::History, now,
                        ).await;
                        let actor = structure_actor(
                            current, company, staff.0, wbs_new_work, wbs_settle_existing, cost_history,
                        );
                        let mut repository = PgProjectStructureCommands::new(connection);
                        let receipt = execute_structure_command(
                            &actor, &mut repository, &project_id, &command, &key, now,
                        ).await?;
                        Ok(ProjectStructureHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    async fn actuals_actor_at(
        connection: &mut PgConnection,
        current: &AuthorizedWorkContext,
        holding: &HoldingId,
        company: &CompanyId,
        catalogue: &CommercialCatalogue,
        now: erpx_server_shared::public::TimestampMillis,
    ) -> Result<ProjectActualsActor, BusinessError> {
        let staff = staff_window(connection, current.user_id().as_str(), company).await?;
        let time_new = purchase_allowed(
            connection, holding, company, catalogue, "projects.time",
            CapabilityPurpose::NewWork, now,
        ).await;
        let time_settle = purchase_allowed(
            connection, holding, company, catalogue, "projects.time",
            CapabilityPurpose::SettleExisting, now,
        ).await;
        let costs_new = purchase_allowed(
            connection, holding, company, catalogue, "projects.costs",
            CapabilityPurpose::NewWork, now,
        ).await;
        let costs_settle = purchase_allowed(
            connection, holding, company, catalogue, "projects.costs",
            CapabilityPurpose::SettleExisting, now,
        ).await;
        let cost_history = purchase_allowed(
            connection, holding, company, catalogue, "projects.costs",
            CapabilityPurpose::History, now,
        ).await;
        Ok(actuals_actor(
            current,
            company.clone(),
            staff,
            (time_new, time_settle, costs_new, costs_settle, cost_history),
        ))
    }

    pub async fn save_work_log<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
        command: SaveWorkLogCommand,
        key: String,
    ) -> Result<ProjectActualsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let catalogue = self.catalogue.clone();
        self.actuals_command(host, proof, input, "records.write", move |connection, current, holding, company, now| {
            let project_id = project_id.clone();
            let command = command.clone();
            let key = key.clone();
            let catalogue = catalogue.clone();
            Box::pin(async move {
                let actor = Self::actuals_actor_at(connection, current, holding, company, &catalogue, now).await?;
                let mut repository = PgProjectActualsCommands::new(connection);
                let receipt = save_work_log(&actor, &mut repository, &project_id, &command, &key, now).await?;
                Ok(receipt)
            })
        })
        .await
    }

    pub async fn decide_work_log<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        project_id: String,
        log_id: String,
        command: DecideWorkLogCommand,
        key: String,
    ) -> Result<ProjectActualsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let catalogue = self.catalogue.clone();
        self.actuals_command(host, proof, input, "records.write", move |connection, current, holding, company, now| {
            let project_id = project_id.clone();
            let log_id = log_id.clone();
            let command = command.clone();
            let key = key.clone();
            let catalogue = catalogue.clone();
            Box::pin(async move {
                let actor = Self::actuals_actor_at(connection, current, holding, company, &catalogue, now).await?;
                let mut repository = PgProjectActualsCommands::new(connection);
                let receipt = decide_work_log(&actor, &mut repository, &project_id, &log_id, &command, &key, now).await?;
                Ok(receipt)
            })
        })
        .await
    }

    pub async fn create_cost<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: CreateCostAllocationCommand,
        key: String,
    ) -> Result<ProjectActualsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let catalogue = self.catalogue.clone();
        self.actuals_command(host, proof, input, "finance.write", move |connection, current, holding, company, now| {
            let command = command.clone();
            let key = key.clone();
            let catalogue = catalogue.clone();
            Box::pin(async move {
                let actor = Self::actuals_actor_at(connection, current, holding, company, &catalogue, now).await?;
                let mut repository = PgProjectActualsCommands::new(connection);
                let receipt = create_cost_allocation(&actor, &mut repository, &command, &key, now).await?;
                Ok(receipt)
            })
        })
        .await
    }

    pub async fn decide_cost<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        allocation_id: String,
        command: DecideCostAllocationCommand,
        key: String,
    ) -> Result<ProjectActualsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let catalogue = self.catalogue.clone();
        self.actuals_command(host, proof, input, "finance.approve", move |connection, current, holding, company, now| {
            let allocation_id = allocation_id.clone();
            let command = command.clone();
            let key = key.clone();
            let catalogue = catalogue.clone();
            Box::pin(async move {
                let actor = Self::actuals_actor_at(connection, current, holding, company, &catalogue, now).await?;
                let mut repository = PgProjectActualsCommands::new(connection);
                let receipt = decide_cost_allocation(&actor, &mut repository, &allocation_id, &command, &key, now).await?;
                Ok(receipt)
            })
        })
        .await
    }

    async fn actuals_command<Input, Action>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        permission: &'static str,
        action: Action,
    ) -> Result<ProjectActualsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Action: for<'a, 'c> Fn(
                &'a mut PgConnection,
                &'a AuthorizedWorkContext,
                &'a HoldingId,
                &'a CompanyId,
                erpx_server_shared::public::TimestampMillis,
            ) -> PgWorkFuture<'a, erpx_server_projects::public::project_actuals::ActualCommandReceipt, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.command_input(input),
                WorkAuthorizationRequest {
                    permission: permission.into(),
                    module: None,
                    destination: "projects".into(),
                    api: true,
                },
                ProjectsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let now = clock.now();
                    let action = action.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission(permission)?;
                        let receipt = action(connection, current, &holding, &company, now).await?;
                        Ok(ProjectActualsHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

async fn staff_window(
    connection: &mut PgConnection,
    user_id: &str,
    company: &CompanyId,
) -> Result<(bool, String, Option<String>), BusinessError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT e.start_date,e.end_date FROM hr_people p JOIN hr_engagements e          ON e.person_id=p.id AND e.company_id=p.company_id          WHERE p.user_id=$1 AND p.company_id=$2 AND p.status='active'          AND p.merged_into_id IS NULL AND e.status='active'          ORDER BY e.start_date DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(company.as_str())
    .fetch_optional(connection)
    .await
    .map_err(|_| unavailable())?;
    Ok(match row {
        Some((start, end)) => (true, start, end),
        None => (false, String::new(), None),
    })
}

async fn purchase_allowed(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    catalogue: &CommercialCatalogue,
    capability: &str,
    purpose: CapabilityPurpose,
    now: erpx_server_shared::public::TimestampMillis,
) -> bool {
    let mut reader =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, catalogue);
    let request = PurchaseRequest {
        capabilities: vec![capability.to_owned()],
        purpose,
        mode: PackageEnforcementMode::Enforced,
    };
    match reader.inspect(&request, now).await {
        Ok(inspection) => inspection.require_allowed().is_ok(),
        Err(_) => false,
    }
}

/// One response image for both the fresh bind and an exact replay. The retained
/// document client consumes `setId`/`version`/`manifest`; the Projects receipt
/// fields stay present so callers can verify the owner-owned command result.
fn attachment_response_data(
    receipt: &ProjectAttachmentCommandReceipt,
    manifest: &[erpx_server_files::public::project_attachment::ProjectAttachmentManifestItem],
) -> Result<Value, BusinessError> {
    let mut value = serde_json::to_value(receipt)
        .map_err(|_| BusinessError::new(FailureKind::Unavailable, "PROJECTS_UNAVAILABLE", "The Projects workspace is unavailable."))?;
    let Value::Object(ref mut object) = value else {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PROJECTS_UNAVAILABLE",
            "The Projects workspace is unavailable.",
        ));
    };
    object.insert(
        "setId".into(),
        Value::String(receipt.attachment_set_id.clone()),
    );
    object.insert(
        "manifest".into(),
        serde_json::to_value(manifest).map_err(|_| {
            BusinessError::new(
                FailureKind::Unavailable,
                "PROJECTS_UNAVAILABLE",
                "The Projects workspace is unavailable.",
            )
        })?,
    );
    Ok(value)
}

fn structure_actor(
    current: &AuthorizedWorkContext,
    company: CompanyId,
    active_staff: bool,
    wbs_new_work: bool,
    wbs_settle_existing: bool,
    cost_history: bool,
) -> ProjectStructureActor {
    ProjectStructureActor {
        command: ProjectCommandActor {
            user_id: current.user_id().as_str().to_owned(),
            company_id: company,
            branch_id: current.selection().workspace().branch_id.clone(),
            department_id: current.selection().workspace().department_id.clone(),
            assignment_id: current.selection().assignment().id.as_str().to_owned(),
            permissions: current.command_iam_metadata().permissions.clone(),
            modules: current.modules().to_vec(),
            expires_at: current.expires_at(),
            active_staff,
            wbs_new_work,
            wbs_settle_existing,
        },
        cost_history,
    }
}

fn actuals_actor(
    current: &AuthorizedWorkContext,
    company: CompanyId,
    staff: (bool, String, Option<String>),
    flags: (bool, bool, bool, bool, bool),
) -> ProjectActualsActor {
    ProjectActualsActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: company,
        branch_id: current.selection().workspace().branch_id.clone(),
        department_id: current.selection().workspace().department_id.clone(),
        assignment_id: current.selection().assignment().id.as_str().to_owned(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
        active_staff: staff.0,
        staff_start_date: staff.1,
        staff_end_date: staff.2,
        time_new_work: flags.0,
        time_settle_existing: flags.1,
        costs_new_work: flags.2,
        costs_settle_existing: flags.3,
        cost_history: flags.4,
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

pub struct ProjectAttachmentReadBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ProjectsService<C>>,
    store: Option<Arc<LocalOriginalStore>>,
}

impl<C: Clock + Send + Sync + 'static> ProjectAttachmentReadBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ProjectsService<C>>,
        store: Option<Arc<LocalOriginalStore>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
            store,
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

impl<C: Clock + Send + Sync + 'static> ProjectAttachmentReadHttpBackend
    for ProjectAttachmentReadBackend<C>
{
    fn sets<'a>(
        &'a self,
        request: ProjectAttachmentReadSetsHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match self
                .service
                .read_project_attachment(
                    request.host,
                    proof,
                    move || Ok(selection),
                    ProjectAttachmentReadAccess::Sets {
                        project_id: request.project_id.to_owned(),
                    },
                    self.store.clone(),
                )
                .await
                .map_err(business)?
            {
                ProjectAttachmentReadOutcome::Data(response) => Ok(response),
                ProjectAttachmentReadOutcome::Content(_) => Err(attachment_invalid()),
            }
        })
    }

    fn links<'a>(
        &'a self,
        request: ProjectAttachmentReadLinksHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match self
                .service
                .read_project_attachment(
                    request.host,
                    proof,
                    move || Ok(selection),
                    ProjectAttachmentReadAccess::Links {
                        project_id: request.project_id.to_owned(),
                        set_id: request.set_id.to_owned(),
                    },
                    self.store.clone(),
                )
                .await
                .map_err(business)?
            {
                ProjectAttachmentReadOutcome::Data(response) => Ok(response),
                ProjectAttachmentReadOutcome::Content(_) => Err(attachment_invalid()),
            }
        })
    }

    fn status<'a>(
        &'a self,
        request: ProjectAttachmentReadStatusHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match self
                .service
                .read_project_attachment(
                    request.host,
                    proof,
                    move || Ok(selection),
                    ProjectAttachmentReadAccess::Status {
                        project_id: request.project_id.to_owned(),
                        link_id: request.link_id.to_owned(),
                    },
                    self.store.clone(),
                )
                .await
                .map_err(business)?
            {
                ProjectAttachmentReadOutcome::Data(response) => Ok(response),
                ProjectAttachmentReadOutcome::Content(_) => Err(attachment_invalid()),
            }
        })
    }

    fn content<'a>(
        &'a self,
        request: ProjectAttachmentReadContentHttpRequest<'a>,
    ) -> ProjectAttachmentReadHttpFuture<'a, ProjectAttachmentContentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let variant = ProjectAttachmentContentVariant::parse(request.variant)?;
            match self
                .service
                .read_project_attachment(
                    request.host,
                    proof,
                    move || Ok(selection),
                    ProjectAttachmentReadAccess::Content {
                        project_id: request.project_id.to_owned(),
                        link_id: request.link_id.to_owned(),
                        variant,
                    },
                    self.store.clone(),
                )
                .await
                .map_err(business)?
            {
                ProjectAttachmentReadOutcome::Content(response) => Ok(response),
                ProjectAttachmentReadOutcome::Data(_) => Err(attachment_invalid()),
            }
        })
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
                .wbs(
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


impl<C: Clock + Send + Sync + 'static> ProjectStructureHttpBackend for ProjectsBackend<C> {
    fn execute<'a>(
        &'a self,
        request: ProjectStructureHttpRequest<'a>,
    ) -> ProjectStructureHttpFuture<'a, ProjectStructureHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .structure_command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> ProjectActualsHttpBackend for ProjectsBackend<C> {
    fn save_work_log<'a>(
        &'a self,
        request: ProjectWorkLogHttpRequest<'a>,
    ) -> ProjectActualsHttpFuture<'a, ProjectActualsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .save_work_log(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }

    fn decide_work_log<'a>(
        &'a self,
        request: ProjectWorkLogDecisionHttpRequest<'a>,
    ) -> ProjectActualsHttpFuture<'a, ProjectActualsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .decide_work_log(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.project_id.to_owned(),
                    request.log_id.to_owned(),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }

    fn create_cost<'a>(
        &'a self,
        request: ProjectCostAllocationHttpRequest<'a>,
    ) -> ProjectActualsHttpFuture<'a, ProjectActualsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .create_cost(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.input.clone(),
                    request.key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }

    fn decide_cost<'a>(
        &'a self,
        request: ProjectCostAllocationDecisionHttpRequest<'a>,
    ) -> ProjectActualsHttpFuture<'a, ProjectActualsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .decide_cost(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.allocation_id.to_owned(),
                    request.input.clone(),
                    request.key.to_owned(),
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

impl<C: Clock + Send + Sync + 'static> ProjectAttachmentBindHttpBackend for ProjectsBackend<C> {
    fn bind<'a>(
        &'a self,
        request: ProjectAttachmentBindHttpRequest<'a>,
    ) -> ProjectAttachmentHttpFuture<'a, ProjectAttachmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .bind_project_attachment(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.set_id.to_owned(),
                    request.expected_version,
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

#[allow(dead_code)]
fn _project_attachment_router(
    targets: Vec<ProjectAttachmentHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    project_attachment_router(targets)
}
