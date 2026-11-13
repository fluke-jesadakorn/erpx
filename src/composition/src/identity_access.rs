//! Native Identity Access assignment HTTP mount. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! assignment workspace and withdrawal facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::iam_roles::{
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewDecisionInput, IamRoleActor,
    PgIamRolesReviews, ReviewDecisionInput, RoleSaveInput, cancel_access_review_campaign,
    decide_access_review_item, decide_review, save_role, start_access_review,
};
use erpx_server_identity_access::public::iam_roles_http::{
    IamRolesReviewsHttpBackend, IamRolesReviewsHttpCommand, IamRolesReviewsHttpFuture,
    IamRolesReviewsHttpRequest, IamRolesReviewsHttpResponse, IamRolesReviewsHttpTarget,
};
use erpx_server_identity_access::public::people_access::{
    PeopleAccessActor, PgPeopleRelationships, RELATIONSHIP_MANAGE_PERMISSION, apply_relationship,
    preview_relationship,
};
use erpx_server_identity_access::public::people_access_http::{
    PeopleRelationshipsHttpBackend, PeopleRelationshipsHttpCommand, PeopleRelationshipsHttpFuture,
    PeopleRelationshipsHttpRequest, PeopleRelationshipsHttpResponse,
};
use erpx_server_identity_access::public::people_grants::{
    ACCESS_APPLY_PERMISSION, DirectInvitationDelivery, PeopleGrantActor, PgPeopleGrants,
    apply_grant_change, direct_invitation_token_hash, preview_grant_change, read_direct_invitation,
    stage_direct_invitation_delivery,
};
use erpx_server_identity_access::public::people_grants_http::{
    PeopleGrantsHttpBackend, PeopleGrantsHttpCommand, PeopleGrantsHttpFuture,
    PeopleGrantsHttpRequest, PeopleGrantsHttpResponse,
};
use erpx_server_identity_access::public::permission_catalog::permission_catalog;
use erpx_server_identity_access::public::user_management::{
    UserManagementActor, UserManagementHttpBackend, UserManagementHttpCommand,
    UserManagementHttpFuture, UserManagementHttpRequest, UserManagementHttpResponse,
    apply_management_review, decide_management_review, management_command, management_read,
    reconcile_management,
};
use erpx_server_identity_access::public::position_gpo::{
    FreshMfa, PgPositionGpo, PgPositionGpoDirectory, PositionGpoHttpBackend,
    PositionGpoHttpCommand, PositionGpoHttpFuture, PositionGpoHttpRequest, PositionGpoHttpResponse,
    ReqwestConnector, position_gpo_overview, preview_position_gpo, preview_unmap_position_gpo,
    publish_position_gpo, retry_position_gpo, retry_position_gpo_config, unmap_position_gpo,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    account_security::MFA_FRESH_MS,
    assignment::{
        AssignmentActor, AssignmentRevocation, PgIdentityAssignments, assignment_workspace,
        revoke_assignment,
    },
    assignment_http::{
        IamAssignmentHttpBackend, IamAssignmentHttpCommand, IamAssignmentHttpFuture,
        IamAssignmentHttpRequest, IamAssignmentHttpResponse, IamAssignmentHttpTarget,
    },
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "IDENTITY_ACCESS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "IDENTITY_ACCESS_UNAVAILABLE",
        "The Identity Access workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct IdentityAccessSources;

impl HoldingWorkSourcePreparation for IdentityAccessSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct IdentityAccessConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl IdentityAccessConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn assignment_actor(current: &AuthorizedWorkContext) -> Result<AssignmentActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = current.command_iam_metadata();
    let scope_kind = match assignment.scope_kind {
        WorkScopeKind::Holding => "holding",
        WorkScopeKind::Branch => "branch",
        WorkScopeKind::Department => "department",
    };
    Ok(AssignmentActor {
        user_id: current.user_id().clone(),
        session_id: current.session_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        scope_kind: scope_kind.to_owned(),
        scope_node_id: assignment.scope_node_id.clone(),
        role: Some(assignment.role.clone()),
        role_revision: Some(iam.role_revision),
        assignment_version: Some(assignment.version),
        identity_version: Some(iam.identity_version),
        authority_revision: Some(selected.revision()),
        permissions: iam.permissions.clone(),
        person_user_ids: iam.person_user_ids.clone(),
        emergency_until: iam.emergency_until,
        expires_at: current.expires_at(),
    })
}

fn iam_role_actor(current: &AuthorizedWorkContext) -> Result<IamRoleActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = current.command_iam_metadata();
    let scope_kind = match assignment.scope_kind {
        WorkScopeKind::Holding => "holding",
        WorkScopeKind::Branch => "branch",
        WorkScopeKind::Department => "department",
    };
    Ok(IamRoleActor {
        user_id: current.user_id().clone(),
        session_id: current.session_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        scope_kind: scope_kind.to_owned(),
        scope_node_id: assignment.scope_node_id.clone(),
        role: Some(assignment.role.clone()),
        role_revision: Some(iam.role_revision),
        assignment_version: Some(assignment.version),
        identity_version: Some(iam.identity_version),
        authority_revision: Some(selected.revision()),
        permissions: iam.permissions.clone(),
        grantable_permissions: iam.grantable_permissions.clone(),
        person_user_ids: iam.person_user_ids.clone(),
        person_id: iam.person_id.clone(),
        platform: false,
        emergency_until: iam.emergency_until,
        expires_at: current.expires_at(),
    })
}

fn management_actor(
    current: &AuthorizedWorkContext,
) -> Result<UserManagementActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = current.command_iam_metadata();
    Ok(UserManagementActor {
        user_id: current.user_id().as_str().to_owned(),
        session_id: current.session_id().as_str().to_owned(),
        holding_id: current.holding_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .to_owned(),
        scope_node_id: assignment.scope_node_id.as_ref().map(|value| value.as_str().to_owned()),
        role: assignment.role.clone(),
        permissions: iam.permissions.clone(),
        grantable_permissions: iam.grantable_permissions.clone(),
        platform: false,
        expires_at: current.expires_at(),
    })
}

fn management_route_access(command: &UserManagementHttpCommand) -> WorkAuthorizationRequest {
    let permission = match command {
        UserManagementHttpCommand::Read => WorkPermissionRequirement::Exact("iam.read".into()),
        UserManagementHttpCommand::DecideReview { .. } => {
            WorkPermissionRequirement::Exact("iam.review".into())
        }
        _ => WorkPermissionRequirement::AnyOf(vec![
            "iam.manage".into(),
            "iam.directory.manage".into(),
            "iam.attributes.manage".into(),
            "people.custom_attributes.write".into(),
            "people.manage".into(),
            "organization.manage".into(),
        ]),
    };
    WorkAuthorizationRequest {
        permission,
        module: Some("iam".into()),
        destination: "iam".into(),
        api: true,
    }
}

fn people_actor(current: &AuthorizedWorkContext) -> PeopleAccessActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    PeopleAccessActor {
        user_id: current.user_id().clone(),
        session_id: current.session_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        assignment_id: assignment.id.clone(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .to_owned(),
        permissions: current.command_iam_metadata().permissions.clone(),
        platform: false,
        expires_at: current.expires_at(),
    }
}

fn grant_actor(current: &AuthorizedWorkContext) -> PeopleGrantActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    PeopleGrantActor {
        user_id: current.user_id().as_str().to_owned(),
        session_id: current.session_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .to_owned(),
        permissions: current.command_iam_metadata().permissions.clone(),
        platform: false,
        expires_at: current.expires_at(),
    }
}

fn delivery_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_PEOPLE_ACCESS_DELIVERY_UNAVAILABLE",
        "The invitation delivery staging is unavailable.",
    )
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

pub struct IdentityAccessService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: IdentityAccessConfiguration,
    clock: Arc<C>,
    control: Option<DatabasePool>,
}

impl<C: Clock + Send + Sync + 'static> IdentityAccessService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: IdentityAccessConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            control: None,
        }
    }

    /// Supply the Control pool for the fresh-MFA fact of a sensitive Directory
    /// command. Without it every write fails closed with `FRESH_MFA_REQUIRED`.
    pub fn with_control_pool(mut self, control: DatabasePool) -> Self {
        self.control = Some(control);
        self
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access_read() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "iam.read".into(),
            module: Some("iam".into()),
            destination: "iam".into(),
            api: true,
        }
    }

    fn access_manage() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "iam.manage".into(),
            module: Some("iam".into()),
            destination: "iam".into(),
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
    ) -> Result<IamAssignmentHttpResponse, Error>
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
                Self::access_read(),
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("iam.read")?;
                        let actor = assignment_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgIdentityAssignments::new(connection);
                        let data = assignment_workspace(&mut repository, &actor, now)
                            .await
                            .map_err(business)?;
                        Ok(IamAssignmentHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn revoke<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        assignment_id: String,
        revocation: AssignmentRevocation,
        key: CommandKey,
    ) -> Result<IamAssignmentHttpResponse, Error>
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
                Self::access_manage(),
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let assignment_id = assignment_id.clone();
                    let revocation = revocation.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("iam.manage")?;
                        let actor = assignment_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgIdentityAssignments::new(connection);
                        let data = revoke_assignment(
                            &mut repository,
                            &actor,
                            &key,
                            &assignment_id,
                            &revocation,
                            now,
                        )
                        .await
                        .map_err(business)?;
                        Ok(IamAssignmentHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn people_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: PeopleRelationshipsHttpCommand,
    ) -> Result<PeopleRelationshipsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: RELATIONSHIP_MANAGE_PERMISSION.into(),
                    module: Some("iam".into()),
                    destination: "iam".into(),
                    api: true,
                },
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission(RELATIONSHIP_MANAGE_PERMISSION)?;
                        let actor = people_actor(current);
                        let now = clock.now().get();
                        let mut repository = PgPeopleRelationships::new(connection);
                        let data = match command {
                            PeopleRelationshipsHttpCommand::Preview { change } => {
                                preview_relationship(&mut repository, &actor, change, now).await?
                            }
                            PeopleRelationshipsHttpCommand::Apply {
                                token,
                                idempotency_key,
                            } => {
                                let key = CommandKey::new(idempotency_key)
                                    .map_err(PgTransactionError::Business)?;
                                apply_relationship(&mut repository, &actor, &key, &token, now)
                                    .await?
                            }
                        };
                        Ok(PeopleRelationshipsHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub fn now(&self) -> i64 {
        self.clock.now().get()
    }

    /// Reviewed People access commands. Returns the committed owner response
    /// plus, for an invitation, the Holding review row the composition root
    /// stages into Control *after* this unit commits.
    pub async fn people_grants_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: PeopleGrantsHttpCommand,
    ) -> Result<(PeopleGrantsHttpResponse, Option<DirectInvitationDelivery>), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: ACCESS_APPLY_PERMISSION.into(),
                    module: Some("iam".into()),
                    destination: "iam".into(),
                    api: true,
                },
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission(ACCESS_APPLY_PERMISSION)?;
                        let actor = grant_actor(current);
                        let now = clock.now().get();
                        let mut repository = PgPeopleGrants::new(connection);
                        let data = match command {
                            PeopleGrantsHttpCommand::Preview { change } => {
                                preview_grant_change(
                                    &mut repository,
                                    &actor,
                                    permission_catalog()?,
                                    change,
                                    now,
                                )
                                .await?
                            }
                            PeopleGrantsHttpCommand::Apply {
                                token,
                                idempotency_key,
                            } => {
                                let key = CommandKey::new(idempotency_key)
                                    .map_err(PgTransactionError::Business)?;
                                apply_grant_change(
                                    &mut repository,
                                    &actor,
                                    permission_catalog()?,
                                    &key,
                                    &token,
                                    now,
                                )
                                .await?
                            }
                        };
                        let delivery = match data["invitationId"].as_str() {
                            Some(invitation_id) => Some(
                                read_direct_invitation(
                                    connection,
                                    holding.as_str(),
                                    company.as_str(),
                                    invitation_id,
                                )
                                .await?,
                            ),
                            None => None,
                        };
                        Ok((
                            PeopleGrantsHttpResponse {
                                data,
                                context: context(current),
                            },
                            delivery,
                        ))
                    })
                },
            )
            .await
    }

    /// Reviewed user-management command family. Read/preview/save, the
    /// independent `policy.grant` decision and the native worker reconcile all
    /// run inside the selected serializable Holding unit.
    pub async fn user_management_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: UserManagementHttpCommand,
    ) -> Result<UserManagementHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let mutation = !matches!(command, UserManagementHttpCommand::Read);
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = mutation;
            Ok(request)
        };
        let access = management_route_access(&command);
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                access,
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = management_actor(current)?;
                        let now = clock.now().get();
                        let data = match command {
                            UserManagementHttpCommand::Read => {
                                current.require_permission("iam.read")?;
                                management_read(connection, &actor, now).await?
                            }
                            UserManagementHttpCommand::Preview { change } => {
                                management_command(
                                    connection, &actor, change, "", true, None, now,
                                )
                                .await?
                            }
                            UserManagementHttpCommand::Apply {
                                change,
                                token,
                                idempotency_key,
                            } => {
                                let key = CommandKey::new(idempotency_key)
                                    .map_err(PgTransactionError::Business)?;
                                management_command(
                                    connection,
                                    &actor,
                                    change,
                                    key.as_str(),
                                    false,
                                    Some(&token),
                                    now,
                                )
                                .await?
                            }
                            UserManagementHttpCommand::ApplyReview {
                                payload,
                                idempotency_key,
                            } => {
                                CommandKey::new(idempotency_key)
                                    .map_err(PgTransactionError::Business)?;
                                apply_management_review(connection, &actor, payload, now).await?
                            }
                            UserManagementHttpCommand::DecideReview {
                                review_id,
                                payload,
                                idempotency_key,
                            } => {
                                CommandKey::new(idempotency_key)
                                    .map_err(PgTransactionError::Business)?;
                                decide_management_review(
                                    connection, &actor, &review_id, payload, now,
                                )
                                .await?
                            }
                            UserManagementHttpCommand::Reconcile => {
                                current.require_permission("iam.manage")?;
                                reconcile_management(connection, company.as_str(), now).await?;
                                serde_json::json!({"companyId": company.as_str(), "reconciled": true})
                            }
                        };
                        Ok(UserManagementHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn iam_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: IamRolesReviewsHttpCommand,
    ) -> Result<IamRolesReviewsHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "IDENTITY_SCOPE_REQUIRED",
                    "Choose the current authorized Company workspace.",
                ));
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: erpx_server_identity_access::public::effective_authority::WorkPermissionRequirement::AnyOf(vec![
                        "iam.manage".into(),
                        "iam.roles.manage".into(),
                        "iam.review".into(),
                    ]),
                    module: Some("iam".into()),
                    destination: "iam".into(),
                    api: true,
                },
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "IDENTITY_SCOPE_REQUIRED",
                                "Choose the current authorized Company workspace.",
                            )
                            .into());
                        }
                        let actor = iam_role_actor(current)?;
                        let catalog = permission_catalog()?;
                        let now = clock.now().get();
                        let mut repository = PgIamRolesReviews::new(connection);
                        let key = match &command {
                            IamRolesReviewsHttpCommand::SaveRole { idempotency_key, .. }
                            | IamRolesReviewsHttpCommand::DecideReview { idempotency_key, .. }
                            | IamRolesReviewsHttpCommand::StartAccessReview { idempotency_key, .. }
                            | IamRolesReviewsHttpCommand::DecideAccessReviewItem { idempotency_key, .. }
                            | IamRolesReviewsHttpCommand::CancelAccessReview { idempotency_key, .. } => {
                                erpx_server_shared::public::CommandKey::new(idempotency_key.clone())
                                    .map_err(PgTransactionError::Business)?
                            }
                        };
                        let data = match command {
                            IamRolesReviewsHttpCommand::SaveRole { input, .. } => {
                                save_role(&mut repository, &actor, &catalog, &key, &input, now)
                                    .await
                            }
                            IamRolesReviewsHttpCommand::DecideReview { review_id, input, .. } => {
                                decide_review(&mut repository, &actor, &catalog, &key, &review_id, &input, now)
                                    .await
                            }
                            IamRolesReviewsHttpCommand::StartAccessReview { input, .. } => {
                                start_access_review(&mut repository, &actor, &key, &input, now).await
                            }
                            IamRolesReviewsHttpCommand::DecideAccessReviewItem { item_id, input, .. } => {
                                decide_access_review_item(&mut repository, &actor, &key, &item_id, &input, now)
                                    .await
                            }
                            IamRolesReviewsHttpCommand::CancelAccessReview { campaign_id, input, .. } => {
                                cancel_access_review_campaign(&mut repository, &actor, &key, &campaign_id, &input, now)
                                    .await
                            }
                        }?;
                        Ok(IamRolesReviewsHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    /// One native Position GPO command. Account session, selected Company,
    /// current Work authority and the serializable Holding transaction stay
    /// here; the owner application and the installed Directory connector own
    /// the mapping rules and effects.
    pub async fn position_gpo_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: PositionGpoHttpCommand,
    ) -> Result<PositionGpoHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let control = self.control.clone();
        let write = matches!(
            &command,
            PositionGpoHttpCommand::Publish { .. }
                | PositionGpoHttpCommand::Unmap { .. }
                | PositionGpoHttpCommand::Retry { .. }
                | PositionGpoHttpCommand::RetryConfig { .. }
        );
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = write;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "iam.read".into(),
                        "iam.directory.apply".into(),
                    ]),
                    module: Some("iam".into()),
                    destination: "technology".into(),
                    api: true,
                },
                IdentityAccessSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let control = control.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = iam_role_actor(current)?;
                        let now = clock.now().get();
                        let fresh = if write {
                            fresh_mfa(
                                control.as_ref(),
                                current.user_id().as_str(),
                                current.session_id().as_str(),
                                now,
                            )
                            .await
                        } else {
                            false
                        };
                        let key = match &command {
                            PositionGpoHttpCommand::Publish {
                                idempotency_key, ..
                            }
                            | PositionGpoHttpCommand::Unmap {
                                idempotency_key, ..
                            }
                            | PositionGpoHttpCommand::Retry {
                                idempotency_key, ..
                            }
                            | PositionGpoHttpCommand::RetryConfig {
                                idempotency_key, ..
                            } => Some(
                                CommandKey::new(idempotency_key.clone())
                                    .map_err(PgTransactionError::Business)?,
                            ),
                            _ => None,
                        };
                        let key = key.as_ref().map(CommandKey::as_str).unwrap_or_default();
                        let profile = std::env::var("ERPX_IAM_SECRET_PROFILES")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_default();
                        let mut directory = PgPositionGpoDirectory::new(profile, ReqwestConnector);
                        let mut repository = PgPositionGpo::new(connection);
                        let data = match command {
                            PositionGpoHttpCommand::Overview => {
                                position_gpo_overview(&mut repository, &actor, now).await
                            }
                            PositionGpoHttpCommand::Preview { input } => preview_position_gpo(
                                &mut repository,
                                &mut directory,
                                &actor,
                                &input,
                                now,
                            )
                            .await
                            .map(|preview| preview.value),
                            PositionGpoHttpCommand::Publish { input, .. } => {
                                publish_position_gpo(
                                    &mut repository,
                                    &mut directory,
                                    &actor,
                                    FreshMfa(fresh),
                                    key,
                                    &input,
                                    now,
                                )
                                .await
                            }
                            PositionGpoHttpCommand::UnmapPreview { input } => {
                                preview_unmap_position_gpo(&mut repository, &actor, &input, now)
                                    .await
                            }
                            PositionGpoHttpCommand::Unmap { input, .. } => {
                                unmap_position_gpo(
                                    &mut repository,
                                    &actor,
                                    FreshMfa(fresh),
                                    key,
                                    &input,
                                    now,
                                )
                                .await
                            }
                            PositionGpoHttpCommand::Retry { input, .. } => {
                                retry_position_gpo(
                                    &mut repository,
                                    &actor,
                                    FreshMfa(fresh),
                                    key,
                                    &input,
                                    now,
                                )
                                .await
                            }
                            PositionGpoHttpCommand::RetryConfig { input, .. } => {
                                retry_position_gpo_config(
                                    &mut repository,
                                    &actor,
                                    FreshMfa(fresh),
                                    key,
                                    &input,
                                    now,
                                )
                                .await
                            }
                        }?;
                        Ok(PositionGpoHttpResponse { data })
                    })
                },
            )
            .await
    }
}

/// Fresh-MFA is a Control-store fact for the same session. A missing Control
/// pool or a failed read is reported as not fresh, never as success.
async fn fresh_mfa(
    control: Option<&DatabasePool>,
    user_id: &str,
    session_id: &str,
    now: i64,
) -> bool {
    let Some(control) = control else {
        return false;
    };
    let Ok(mut connection) = control.raw().acquire().await else {
        return false;
    };
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM iam_mfa_sessions f \
         JOIN iam_mfa m ON m.user_id=f.user_id AND m.enabled=true \
         JOIN session s ON s.id=f.session_id AND s.\"userId\"=f.user_id \
         WHERE f.user_id=$1 AND f.session_id=$2 AND f.verified_at>$3 \
         AND s.\"expiresAt\"::timestamptz>to_timestamp($4::double precision/1000.0))",
    )
    .bind(user_id)
    .bind(session_id)
    .bind(now - MFA_FRESH_MS)
    .bind(now as f64)
    .fetch_one(&mut *connection)
    .await
    .unwrap_or(false)
}

pub struct IdentityAccessBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<IdentityAccessService<C>>,
    control: Option<DatabasePool>,
    holding_database: Option<DatabasePool>,
}

impl<C: Clock + Send + Sync + 'static> IdentityAccessBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<IdentityAccessService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
            control: None,
            holding_database: None,
        }
    }

    /// Supply the committed Control and Holding pools for the post-commit
    /// direct-invitation delivery staging. Without them an invitation commits
    /// the review but fails closed before any Control delivery row is written.
    pub fn with_delivery_databases(mut self, control: DatabasePool, holding: DatabasePool) -> Self {
        self.control = Some(control);
        self.holding_database = Some(holding);
        self
    }

    async fn stage_direct_invitation(
        &self,
        delivery: DirectInvitationDelivery,
    ) -> Result<(), Error> {
        let (Some(control), Some(holding)) = (&self.control, &self.holding_database) else {
            return Err(PgTransactionError::Business(delivery_unavailable()));
        };
        let secret = std::env::var("BETTER_AUTH_SECRET")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| PgTransactionError::Business(delivery_unavailable()))?;
        let token_hash = direct_invitation_token_hash(&secret, &delivery.invitation_id);
        let mut control = control
            .raw()
            .acquire()
            .await
            .map_err(|_| PgTransactionError::Business(delivery_unavailable()))?;
        let mut holding = holding
            .raw()
            .acquire()
            .await
            .map_err(|_| PgTransactionError::Business(delivery_unavailable()))?;
        stage_direct_invitation_delivery(
            &mut control,
            &mut holding,
            &delivery,
            &token_hash,
            self.service.now(),
        )
        .await
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

impl<C: Clock + Send + Sync + 'static> UserManagementHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: UserManagementHttpRequest<'a>,
    ) -> UserManagementHttpFuture<'a, UserManagementHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .user_management_command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> IamRolesReviewsHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: IamRolesReviewsHttpRequest<'a>,
    ) -> IamRolesReviewsHttpFuture<'a, IamRolesReviewsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .iam_command(request.host, proof, move || Ok(selection), request.command)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PeopleRelationshipsHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PeopleRelationshipsHttpRequest<'a>,
    ) -> PeopleRelationshipsHttpFuture<'a, PeopleRelationshipsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .people_command(request.host, proof, move || Ok(selection), request.command)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PeopleGrantsHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PeopleGrantsHttpRequest<'a>,
    ) -> PeopleGrantsHttpFuture<'a, PeopleGrantsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let (reply, delivery) = self
                .service
                .people_grants_command(request.host, proof, move || Ok(selection), request.command)
                .await
                .map_err(business)?;
            // The Holding unit has committed. Staging only queues Control rows:
            // a failure here must never roll back the assignment or invitation.
            if let Some(delivery) = delivery {
                self.stage_direct_invitation(delivery)
                    .await
                    .map_err(business)?;
            }
            Ok(reply)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> IamAssignmentHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: IamAssignmentHttpRequest<'a>,
    ) -> IamAssignmentHttpFuture<'a, IamAssignmentHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                IamAssignmentHttpCommand::Workspace => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                IamAssignmentHttpCommand::Revoke {
                    assignment_id,
                    input,
                    idempotency_key,
                } => {
                    let key = CommandKey::new(idempotency_key)?;
                    self.service
                        .revoke(
                            request.host,
                            proof,
                            move || Ok(selection),
                            assignment_id,
                            input,
                            key,
                        )
                        .await
                        .map_err(business)
                }
            }
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PositionGpoHttpBackend for IdentityAccessBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PositionGpoHttpRequest<'a>,
    ) -> PositionGpoHttpFuture<'a, PositionGpoHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .position_gpo_command(request.host, proof, move || Ok(selection), request.command)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<IamAssignmentHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_identity_access::public::assignment_http::iam_assignment_http_router(targets)
}
