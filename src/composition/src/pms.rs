//! Native PMS room-board and front-desk HTTP assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the PMS
//! workspace, staff and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_pms::public::front_desk::{
    PgPmsFrontDeskRead, PmsFrontDeskSelection, lock_front_desk_sources, pms_front_desk,
};
use erpx_server_pms::public::pms_http::{
    PmsHttpBackend, PmsHttpFrontDeskRequest, PmsHttpFuture, PmsHttpReadRequest, PmsHttpResponse,
};
use erpx_server_pms::public::room_board::{
    PgPmsBoardRead, PmsBoardActor, PmsBoardEntitlement, PmsBoardEntitlementPort, PmsBoardSelection,
    PmsStaff, PmsStaffDirectory, pms_room_board,
};
use erpx_server_pms::public::room_command::{
    PgPmsRoomCommandRepository, PmsRoomAssigneeScopePort, PmsRoomCommandActor,
    PmsRoomCommandHttpBackend, PmsRoomCommandHttpCommandRequest, PmsRoomCommandHttpContext,
    PmsRoomCommandHttpFuture, PmsRoomCommandHttpResponse, PmsRoomCommandInput,
    lock_room_command_sources, pms_room_command,
};
use erpx_server_pms::public::stay_room::{
    PgPmsStayRoomRepository, PmsStayRoomActor, PmsStayRoomCommand, PmsStayRoomEntitlement,
    PmsStayRoomEntitlementPort, PmsStayRoomHttpBackend, PmsStayRoomHttpCommandRequest,
    PmsStayRoomHttpContext, PmsStayRoomHttpFuture, PmsStayRoomHttpResponse, lock_stay_room_sources,
    pms_stay_room,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PMS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_UNAVAILABLE",
        "The PMS workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PmsSources;

impl HoldingWorkSourcePreparation for PmsSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { lock_front_desk_sources(c).await })
    }
}

#[derive(Clone, Copy)]
struct PmsRoomCommandSources;

impl HoldingWorkSourcePreparation for PmsRoomCommandSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { lock_room_command_sources(c).await })
    }
}

/// Assignment-scope fact resolved by composition in the same serializable
/// snapshot as the PMS unit. The retained `readAssignments2` is a read-only
/// Identity fact; the PMS adapter never claims Identity authority.
struct PmsRoomAssigneeScope(bool);

impl PmsRoomAssigneeScopePort for PmsRoomAssigneeScope {
    type Error = BusinessError;
    fn assignee_in_scope<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: &'a str,
        _assignee_id: &'a str,
        _now: i64,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, bool, Self::Error> {
        let allowed = self.0;
        Box::pin(async move { Ok(allowed) })
    }
}

#[derive(Clone, Copy)]
struct PmsStayRoomSources;

impl HoldingWorkSourcePreparation for PmsStayRoomSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move { lock_stay_room_sources(c).await })
    }
}

/// Commercial `pms.front-desk` settlement entitlement. The current runtime has
/// no enforced package document for PMS, so this preserves the retained
/// disabled contract instead of inventing a grant. A configured enforcement
/// mode must replace it through the Commercial capability-decision port.
struct PmsStayRoomEntitlementImpl;

impl PmsStayRoomEntitlementPort for PmsStayRoomEntitlementImpl {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, PmsStayRoomEntitlement, Self::Error>
    {
        Box::pin(async {
            Ok(PmsStayRoomEntitlement {
                enforced: false,
                settle_existing_allowed: true,
            })
        })
    }
}

struct PmsStaffDirectoryImpl;

impl PmsStaffDirectory for PmsStaffDirectoryImpl {
    type Error = BusinessError;
    fn staff<'a>(
        &'a mut self,
        _company_id: &'a str,
        _branch_id: Option<&'a str>,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, Vec<PmsStaff>, Self::Error> {
        Box::pin(async { Ok(Vec::new()) })
    }
}

struct PmsEntitlementImpl;

impl PmsBoardEntitlementPort for PmsEntitlementImpl {
    type Error = BusinessError;
    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_pms::public::room_board::PmsBoardFuture<'a, PmsBoardEntitlement, Self::Error>
    {
        Box::pin(async {
            Ok(PmsBoardEntitlement {
                enforced: false,
                history_allowed: false,
            })
        })
    }
}

#[derive(Clone)]
pub struct PmsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PmsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn pms_actor(current: &AuthorizedWorkContext) -> Result<PmsBoardActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(PmsBoardActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
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

pub struct PmsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PmsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PmsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PmsConfiguration,
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
            permission: "pms.read".into(),
            module: Some("pms".into()),
            destination: "pms".into(),
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
        selection: PmsBoardSelection,
    ) -> Result<PmsHttpResponse, Error>
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
                PmsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let selection = selection.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pms.read")?;
                        let actor = pms_actor(current)?;
                        let mut repository = PgPmsBoardRead::new(connection);
                        let mut staff = PmsStaffDirectoryImpl;
                        let mut entitlement = PmsEntitlementImpl;
                        let now = clock.now().get();
                        let data = pms_room_board(
                            &actor,
                            &selection,
                            &mut repository,
                            &mut staff,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        Ok(PmsHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn front_desk<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        selection: PmsFrontDeskSelection,
    ) -> Result<PmsHttpResponse, Error>
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
                PmsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let selection = selection.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("pms.read")?;
                        let actor = pms_actor(current)?;
                        let mut repository = PgPmsFrontDeskRead::new(connection);
                        let mut entitlement = PmsEntitlementImpl;
                        let now = clock.now().get();
                        let data = pms_front_desk(
                            &actor,
                            &selection,
                            &mut repository,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        Ok(PmsHttpResponse {
                            data: data.value(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
    /// Apply one native `stay.assign`/`stay.move` command. Context selectors
    /// supplied by transport are compared with the retained Account selection;
    /// they never become authority. The commercial entitlement is the retained
    /// disabled contract until a package enforcement document exists.
    pub async fn stay_room(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        selected_context: PmsStayRoomHttpContext,
        key: CommandKey,
        command: PmsStayRoomCommand,
    ) -> Result<PmsStayRoomHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "pms.frontdesk".into(),
                    module: Some("pms".into()),
                    destination: "pms".into(),
                    api: true,
                },
                PmsStayRoomSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let selected_context = selected_context.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(Error::from(scope()));
                        }
                        current.require_permission("pms.frontdesk")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        if selected_context.company_id != workspace.company_id.as_str()
                            || selected_context.branch_slug.as_deref()
                                != workspace.branch_slug.as_deref()
                            || selected_context.department_slug.as_deref()
                                != workspace.department_slug.as_deref()
                            || selected_context.assignment_id != assignment.id.as_str()
                            || selected_context.revision != selected.revision()
                            || selected_context.assignment_version != assignment.version
                        {
                            return Err(Error::from(BusinessError::new(
                                FailureKind::Conflict,
                                "WORK_CONTEXT_CHANGED",
                                "Your selected work context changed. Review the current workspace and role.",
                            )));
                        }
                        let actor = PmsStayRoomActor {
                            board: pms_actor(current)?,
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: assignment.role.clone(),
                            revision: selected.revision(),
                            assignment_version: assignment.version,
                        };
                        let mut repository = PgPmsStayRoomRepository::new(connection);
                        let mut entitlement = PmsStayRoomEntitlementImpl;
                        let now = clock.now().get();
                        let result = pms_stay_room(
                            &actor,
                            &key,
                            &command,
                            &mut repository,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        Ok(PmsStayRoomHttpResponse {
                            data: serde_json::to_value(result)
                                .map_err(|_| Error::from(unavailable()))?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
    /// Apply one native room/housekeeping command. Permission and People
    /// assignment scope follow the retained PMS operation table; the context
    /// selectors are compared with the retained Account selection and never
    /// become authority.
    pub async fn room_command(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        selected_context: PmsRoomCommandHttpContext,
        key: CommandKey,
        command: erpx_server_pms::public::room_command::PmsRoomCommand,
    ) -> Result<PmsRoomCommandHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let permission = command.operation().permission();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: permission.into(),
                    module: Some("pms".into()),
                    destination: "pms".into(),
                    api: true,
                },
                PmsRoomCommandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let selected_context = selected_context.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(Error::from(scope()));
                        }
                        current.require_permission(permission)?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        if selected_context.company_id != workspace.company_id.as_str()
                            || selected_context.branch_slug.as_deref()
                                != workspace.branch_slug.as_deref()
                            || selected_context.department_slug.as_deref()
                                != workspace.department_slug.as_deref()
                            || selected_context.assignment_id != assignment.id.as_str()
                            || selected_context.revision != selected.revision()
                            || selected_context.assignment_version != assignment.version
                        {
                            return Err(Error::from(BusinessError::new(
                                FailureKind::Conflict,
                                "WORK_CONTEXT_CHANGED",
                                "Your selected work context changed. Review the current workspace and role.",
                            )));
                        }
                        let board = pms_actor(current)?;
                        let now = clock.now().get();
                        // Retained `readAssignments2`: the selected owner must be
                        // a current assignment for this company/branch. The read
                        // runs before the PMS repository borrows the connection
                        // and stays inside the same serializable snapshot.
                        let assignee_allowed = match &command.input {
                            PmsRoomCommandInput::TaskAssign { assignee_id, .. } => {
                                let branch = board.branch_id.as_deref().unwrap_or("");
                                sqlx::query_scalar::<_, i32>(
                                    "SELECT 1 FROM assignments \
                                     WHERE user_id=$1 AND company_id=$2 AND revoked_at IS NULL \
                                     AND valid_from<=$3 AND valid_until>$3 \
                                     AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=$4))",
                                )
                                .bind(assignee_id)
                                .bind(&board.company_id)
                                .bind(now)
                                .bind(branch)
                                .fetch_optional(&mut *connection)
                                .await?
                                .is_some()
                            }
                            _ => true,
                        };
                        let actor = PmsRoomCommandActor {
                            board,
                            assignment_id: assignment.id.as_str().to_owned(),
                            role: assignment.role.clone(),
                            revision: selected.revision(),
                            assignment_version: assignment.version,
                        };
                        let mut repository = PgPmsRoomCommandRepository::new(connection);
                        let mut scope = PmsRoomAssigneeScope(assignee_allowed);
                        let result = pms_room_command(
                            &actor,
                            &key,
                            &command,
                            &mut repository,
                            &mut scope,
                            now,
                        )
                        .await?;
                        Ok(PmsRoomCommandHttpResponse {
                            data: serde_json::to_value(result)
                                .map_err(|_| Error::from(unavailable()))?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PmsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PmsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PmsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PmsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> PmsStayRoomHttpBackend for PmsBackend<C> {
    fn command<'a>(
        &'a self,
        request: PmsStayRoomHttpCommandRequest<'a>,
    ) -> PmsStayRoomHttpFuture<'a, PmsStayRoomHttpResponse> {
        Box::pin(async move {
            let (proof, mut selection) = self.context(request.headers)?;
            selection.mutation = true;
            selection.expected = Some(ExpectedWorkSelection {
                revision: request.context.revision,
                assignment_id: AssignmentId::new(request.context.assignment_id.clone())
                    .map_err(|_| {
                        BusinessError::new(
                            FailureKind::Conflict,
                            "WORK_CONTEXT_CHANGED",
                            "Your selected work context changed. Review the current workspace and role.",
                        )
                    })?,
                assignment_version: request.context.assignment_version,
            });
            self.service
                .stay_room(
                    request.host,
                    proof,
                    selection,
                    request.context,
                    request.key,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PmsRoomCommandHttpBackend for PmsBackend<C> {
    fn command<'a>(
        &'a self,
        request: PmsRoomCommandHttpCommandRequest<'a>,
    ) -> PmsRoomCommandHttpFuture<'a, PmsRoomCommandHttpResponse> {
        Box::pin(async move {
            let (proof, mut selection) = self.context(request.headers)?;
            selection.mutation = true;
            selection.expected = Some(ExpectedWorkSelection {
                revision: request.context.revision,
                assignment_id: AssignmentId::new(request.context.assignment_id.clone())
                    .map_err(|_| {
                        BusinessError::new(
                            FailureKind::Conflict,
                            "WORK_CONTEXT_CHANGED",
                            "Your selected work context changed. Review the current workspace and role.",
                        )
                    })?,
                assignment_version: request.context.assignment_version,
            });
            self.service
                .room_command(
                    request.host,
                    proof,
                    selection,
                    request.context,
                    request.key,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PmsHttpBackend for PmsBackend<C> {
    fn read<'a>(&'a self, request: PmsHttpReadRequest<'a>) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let board_selection = request.selection.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), board_selection)
                .await
                .map_err(business)
        })
    }

    fn front_desk<'a>(
        &'a self,
        request: PmsHttpFrontDeskRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let desk_selection = request.selection.clone();
            self.service
                .front_desk(request.host, proof, move || Ok(selection), desk_selection)
                .await
                .map_err(business)
        })
    }
}
