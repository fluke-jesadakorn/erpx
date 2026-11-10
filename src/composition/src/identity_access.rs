//! Native Identity Access assignment HTTP mount. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! assignment workspace and withdrawal facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
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
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_identity_access::public::iam_roles::{
    AccessReviewCampaignInput, AccessReviewCancelInput, AccessReviewDecisionInput, IamRoleActor,
    PgIamRolesReviews, ReviewDecisionInput, RoleSaveInput, cancel_access_review_campaign,
    decide_access_review_item, decide_review, save_role, start_access_review,
};
use erpx_server_identity_access::public::iam_roles_http::{
    IamRolesReviewsHttpBackend, IamRolesReviewsHttpCommand, IamRolesReviewsHttpFuture,
    IamRolesReviewsHttpRequest, IamRolesReviewsHttpResponse, IamRolesReviewsHttpTarget,
};
use erpx_server_identity_access::public::permission_catalog::permission_catalog;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
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
        }
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
}

pub struct IdentityAccessBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<IdentityAccessService<C>>,
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

#[allow(dead_code)]
fn _router(targets: Vec<IamAssignmentHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_identity_access::public::assignment_http::iam_assignment_http_router(targets)
}
