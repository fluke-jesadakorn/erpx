//! Native Maintenance read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded workspace,
//! detail, preventive plans and cancellation facts.
use crate::business_http::BusinessScopeSlugs;
use crate::maintenance_parts::{MaintenancePartsSourcePreparation, execute_maintenance_issue_part};
use crate::maintenance_transitions::{
    MaintenanceTransitionsSourcePreparation, execute_maintenance_transition,
};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_maintenance::public::http::{
    MaintenanceHttpBackend, MaintenanceHttpCommandRequest, MaintenanceHttpFuture,
    MaintenanceHttpReadKind, MaintenanceHttpReadRequest, MaintenanceHttpResponse,
};
use erpx_server_maintenance::public::read::{
    read_maintenance_detail, read_maintenance_plan_detail, read_maintenance_plans,
    read_maintenance_workspace,
};
use erpx_server_maintenance::public::{
    work_commands::MaintenanceWorkCommand, work_wire::MaintenanceChangeAction,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_shared::public::{IdentifierSource, RecordId};
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
                        let data =
                            Value::Array(plans.into_iter().map(|plan| plan.value()).collect());
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
                        let data =
                            read_maintenance_plan_detail(connection, current, &id, now).await?;
                        Ok(MaintenanceHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
    /// Native transition/issue-part command. The preparation is selected from
    /// the owner command action before the first Holding snapshot; Account,
    /// current IAM, purchase history and the owner unit stay in one commit.
    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        command: MaintenanceWorkCommand,
    ) -> Result<MaintenanceHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        if selection.company_id != company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let access = WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("maintenance".into()),
            destination: "maintenance".into(),
            api: true,
        };
        let issue_part = matches!(
            command.input().action,
            MaintenanceChangeAction::IssuePart { .. }
        );
        if issue_part {
            self.authorizer
                .execute_with_sources(
                    host,
                    proof,
                    selection,
                    access,
                    MaintenancePartsSourcePreparation,
                    move |connection, current| {
                        run_maintenance_command(
                            connection,
                            current,
                            &holding,
                            &company,
                            clock.clone(),
                            &command,
                        )
                    },
                )
                .await
        } else {
            self.authorizer
                .execute_with_sources(
                    host,
                    proof,
                    selection,
                    access,
                    MaintenanceTransitionsSourcePreparation,
                    move |connection, current| {
                        run_maintenance_command(
                            connection,
                            current,
                            &holding,
                            &company,
                            clock.clone(),
                            &command,
                        )
                    },
                )
                .await
        }
    }
}

fn run_maintenance_command<'c, C: Clock + Send + Sync + 'static>(
    connection: &'c mut PgConnection,
    current: &'c AuthorizedWorkContext,
    holding: &HoldingId,
    company: &CompanyId,
    clock: Arc<C>,
    command: &MaintenanceWorkCommand,
) -> PgWorkFuture<'c, MaintenanceHttpResponse, BusinessError> {
    let holding = holding.clone();
    let company = company.clone();
    let command = command.clone();
    Box::pin(async move {
        if current.holding_id() != &holding || current.selection().workspace().company_id != company
        {
            return Err(scope().into());
        }
        current.require_permission("records.write")?;
        let mut identifiers = MaintenanceIds;
        let outcome = if matches!(
            command.input().action,
            MaintenanceChangeAction::IssuePart { .. }
        ) {
            execute_maintenance_issue_part(
                connection,
                current,
                &command,
                PackageEnforcementMode::Enforced,
                clock.as_ref(),
                clock.as_ref(),
                &mut identifiers,
            )
            .await?
        } else {
            execute_maintenance_transition(
                connection,
                current,
                &command,
                PackageEnforcementMode::Enforced,
                clock.as_ref(),
                clock.as_ref(),
                &mut identifiers,
            )
            .await?
        };
        let data = serde_json::from_str(outcome.result.as_str()).map_err(|_| {
            BusinessError::new(
                FailureKind::Conflict,
                "MAINTENANCE_RESULT_INVALID",
                "Refresh the work order before continuing.",
            )
        })?;
        Ok(MaintenanceHttpResponse {
            data,
            context: context(current),
        })
    })
}

struct MaintenanceIds;
impl IdentifierSource for MaintenanceIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
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

    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let (proof, mut selection) = self.context(headers)?;
        let revision = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(|| BusinessError::invalid("MAINTENANCE_CONTEXT_REQUIRED", "Use the current work revision."))?
            .parse()
            .map_err(|_| BusinessError::invalid("MAINTENANCE_CONTEXT_REQUIRED", "Use the current work revision."))?;
        let assignment = unique_request_header(headers, "x-erpx-context-assignment")?
            .ok_or_else(|| BusinessError::invalid("MAINTENANCE_CONTEXT_REQUIRED", "Choose the current assignment."))?
            .to_owned();
        let assignment_version = unique_request_header(headers, "x-erpx-context-assignment-version")?
            .ok_or_else(|| BusinessError::invalid("MAINTENANCE_CONTEXT_REQUIRED", "Use the current assignment version."))?
            .parse()
            .map_err(|_| BusinessError::invalid("MAINTENANCE_CONTEXT_REQUIRED", "Use the current assignment version."))?;
        selection.expected = Some(ExpectedWorkSelection {
            revision,
            assignment_id: AssignmentId::new(assignment)?,
            assignment_version,
        });
        selection.mutation = true;
        Ok((proof, selection))
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
            let (proof, selection) = self.command_context(request.headers)?;
            let Some(command) = request.command.change_command() else {
                return Err(command_unavailable());
            };
            self.service
                .command(request.host, proof, selection, command)
                .await
                .map_err(business)
        })
    }
}
