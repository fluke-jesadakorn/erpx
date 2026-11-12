//! Native Files attachment metadata read assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! bounded attachment-set and link-status facts.
use crate::business_http::BusinessScopeSlugs;
#[cfg(feature = "field-service")]
use erpx_server_collaboration::public::messaging_native::{
    MessagingActor, read_room, require_member,
};
use erpx_server_files::public::attachment_command::{
    AttachmentWithdrawAuthority, WithdrawAttachmentRequest, WithdrawOwnerAccess, WithdrawTarget,
    normalize_withdrawal, withdraw_attachment, withdraw_owner_access,
};
use erpx_server_files::public::attachment_http::{
    AttachmentCommandHttpWithdrawRequest, AttachmentReadHttpBackend, AttachmentReadHttpFuture,
    AttachmentReadHttpListRequest, AttachmentReadHttpReply, AttachmentReadHttpStatusRequest,
};
use erpx_server_files::public::attachment_read::{
    AttachmentListFilter, AttachmentReader, AttachmentTarget, AttachmentTargetAuthority,
    AttachmentTargetScope, PostgresAttachmentRead, attachment_link_status, list_attachment_sets,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FILES_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_UNAVAILABLE",
        "The Files workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct FilesSources;

impl HoldingWorkSourcePreparation for FilesSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct FilesConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl FilesConfiguration {
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

#[derive(Clone)]
struct CurrentAttachmentAuthority {
    company_id: CompanyId,
    branch_id: Option<String>,
    department_id: Option<String>,
    version: i64,
}

impl AttachmentTargetAuthority for CurrentAttachmentAuthority {
    fn authorize_read<'a>(
        &'a self,
        _: &'a mut PgConnection,
        reader: &'a AttachmentReader,
        _target: &'a AttachmentTarget<'a>,
    ) -> PgWorkFuture<'a, AttachmentTargetScope, BusinessError> {
        let company_id = self.company_id.clone();
        let branch_id = self.branch_id.clone();
        let department_id = self.department_id.clone();
        let version = if self.version > 0 { self.version } else { 1 };
        Box::pin(async move {
            if reader.company_id() != &company_id {
                return Err(scope().into());
            }
            AttachmentTargetScope::new(company_id, branch_id, department_id, version)
                .map_err(Into::into)
        })
    }
}

fn owner_denied() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ATTACHMENT_FORBIDDEN",
        "Your current assignment cannot read this file's record.",
    )
}

fn owner_missing() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The file owner was not found in this Company.",
    )
}

fn owner_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ATTACHMENT_WITHDRAW_AUTHORITY_CHANGED",
        "The selected role or file record changed. Reload and retry.",
    )
}

/// Withdraw-only owner authority. The generic Files scope is not enough: the
/// owning read port must also prove the current actor may read the target, and
/// the same proof is repeated after the Files row lock so a grant revoked in
/// between cannot withdraw a draft.
struct CurrentWithdrawAuthority<'a, C> {
    company_id: CompanyId,
    branch_id: Option<String>,
    department_id: Option<String>,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
}

impl<'a, C: Clock + Send + Sync> CurrentWithdrawAuthority<'a, C> {
    async fn authorize<'b>(
        &'b self,
        c: &'b mut PgConnection,
        target: &'b WithdrawTarget<'b>,
    ) -> Result<(), BusinessError> {
        if target.company_id != self.company_id.as_str() {
            return Err(scope());
        }
        if let (Some(current), Some(set)) = (&self.branch_id, target.branch_id)
            && current != set
        {
            return Err(scope());
        }
        if let (Some(current), Some(set)) = (&self.department_id, target.department_id)
            && current != set
        {
            return Err(scope());
        }
        self.current
            .require_permission("records.read")
            .map_err(|_| owner_denied())?;
        match withdraw_owner_access(target.target_type) {
            WithdrawOwnerAccess::RecordsRead => {}
            WithdrawOwnerAccess::Brand => {
                if !self.current.modules().iter().any(|value| value == "brand") {
                    return Err(owner_denied());
                }
                match target.target_type {
                    "holding_brand" if target.target_id != self.current.holding_id().as_str() => {
                        return Err(owner_missing());
                    }
                    "company_storefront_brand" if target.target_id != self.company_id.as_str() => {
                        return Err(owner_missing());
                    }
                    "branch_brand" => {
                        if let Some(branch) = &self.branch_id
                            && target.target_id != branch
                        {
                            return Err(owner_missing());
                        }
                    }
                    _ => {}
                }
            }
            WithdrawOwnerAccess::Legal => {
                self.current
                    .require_permission("legal.read")
                    .map_err(|_| owner_denied())?;
            }
            WithdrawOwnerAccess::Messaging => {
                #[cfg(feature = "field-service")]
                {
                    if !self
                        .current
                        .modules()
                        .iter()
                        .any(|value| value == "collaboration")
                    {
                        return Err(owner_denied());
                    }
                    let room = read_room(c, &self.company_id, target.target_id)
                        .await
                        .map_err(business)?
                        .ok_or_else(owner_missing)?;
                    let actor = MessagingActor {
                        user_id: self.current.user_id().as_str().to_owned(),
                        display_name: self.current.user_id().as_str().to_owned(),
                        company_id: self.company_id.as_str().to_owned(),
                    };
                    require_member(&actor, &room)?;
                }
                #[cfg(not(feature = "field-service"))]
                {
                    let _ = (c, target);
                    return Err(owner_denied());
                }
            }
            WithdrawOwnerAccess::Portal => {
                if !self.current.has_permission("commerce.read")
                    && !self.current.has_permission("support.read")
                {
                    return Err(owner_denied());
                }
            }
        }
        Ok(())
    }
}

impl<C: Clock + Send + Sync> AttachmentWithdrawAuthority for CurrentWithdrawAuthority<'_, C> {
    fn authorize_withdraw<'c>(
        &'c self,
        c: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.authorize(c, target).await?;
            Ok(())
        })
    }

    fn revalidate_withdraw<'c>(
        &'c self,
        c: &'c mut PgConnection,
        target: &'c WithdrawTarget<'c>,
    ) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            self.authorize(c, target).await?;
            let now = self.clock.now().get();
            if now >= self.current.expires_at() {
                return Err(owner_changed().into());
            }
            require_selected_actor_current(c, self.current, now).await?;
            Ok(())
        })
    }
}

pub struct FilesService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: FilesConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> FilesService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: FilesConfiguration,
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
            module: Some("files".into()),
            destination: "files".into(),
            api: true,
        }
    }

    fn write_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("files".into()),
            destination: "files".into(),
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

    pub async fn list<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        filter: AttachmentListFilter,
        read_only: bool,
    ) -> Result<AttachmentReadHttpReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let filter = filter.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                FilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let filter = filter.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let reader = AttachmentReader::new(
                            current.user_id().as_str().to_owned(),
                            workspace.company_id.clone(),
                            workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            current.expires_at(),
                        )
                        .map_err(PgTransactionError::Business)?;
                        let authority = CurrentAttachmentAuthority {
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            department_id: workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            version: assignment.version,
                        };
                        let mut ports = PostgresAttachmentRead::new(authority);
                        let now = clock.now();
                        let data = list_attachment_sets(
                            connection,
                            &ports,
                            &reader,
                            &filter,
                            read_only,
                            now.get(),
                        )
                        .await?;
                        let data = serde_json::to_value(data)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(AttachmentReadHttpReply {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn status<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        link_id: String,
    ) -> Result<AttachmentReadHttpReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let link_id = link_id.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                FilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let link_id = link_id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let reader = AttachmentReader::new(
                            current.user_id().as_str().to_owned(),
                            workspace.company_id.clone(),
                            workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            current.expires_at(),
                        )
                        .map_err(PgTransactionError::Business)?;
                        let authority = CurrentAttachmentAuthority {
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            department_id: workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            version: assignment.version,
                        };
                        let mut ports = PostgresAttachmentRead::new(authority);
                        let now = clock.now();
                        let data = attachment_link_status(
                            connection,
                            &ports,
                            &reader,
                            &link_id,
                            false,
                            now.get(),
                        )
                        .await?;
                        let data = serde_json::to_value(data)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(AttachmentReadHttpReply {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Owner unit, exact authority, target, command key and reason are all required"
    )]
    pub async fn withdraw<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        link_id: String,
        reason: String,
        command_key: String,
    ) -> Result<AttachmentReadHttpReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let action_company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let request = input()?;
                    if request.company_id != company {
                        return Err(scope());
                    }
                    if !request.mutation {
                        return Err(scope());
                    }
                    Ok(request)
                },
                Self::write_access(),
                FilesSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let action_company = action_company.clone();
                    let clock = clock.clone();
                    let link_id = link_id.clone();
                    let reason = reason.clone();
                    let command_key = command_key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != action_company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        current.require_permission("records.read")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let normalized = normalize_withdrawal(&reason, &command_key)
                            .map_err(PgTransactionError::Business)?;
                        let reader = AttachmentReader::new(
                            current.user_id().as_str().to_owned(),
                            workspace.company_id.clone(),
                            workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            current.expires_at(),
                        )
                        .map_err(PgTransactionError::Business)?;
                        let authority = CurrentWithdrawAuthority {
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            department_id: workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            current,
                            clock: clock.as_ref(),
                        };
                        let now = clock.now();
                        withdraw_attachment(
                            connection,
                            &authority,
                            WithdrawAttachmentRequest {
                                actor_id: current.user_id().as_str(),
                                company_id: workspace.company_id.as_str(),
                                link_id: &link_id,
                                reason: &normalized.reason,
                                command_key: &normalized.command_key,
                                now: now.get(),
                            },
                        )
                        .await?;
                        let read_authority = CurrentAttachmentAuthority {
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace
                                .branch_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            department_id: workspace
                                .department_id
                                .as_ref()
                                .map(|value| value.as_str().to_owned()),
                            version: assignment.version,
                        };
                        let mut ports = PostgresAttachmentRead::new(read_authority);
                        let data = attachment_link_status(
                            connection,
                            &ports,
                            &reader,
                            &link_id,
                            false,
                            now.get(),
                        )
                        .await?;
                        let data = serde_json::to_value(data)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(AttachmentReadHttpReply {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct FilesBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<FilesService<C>>,
}

impl<C: Clock + Send + Sync + 'static> FilesBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<FilesService<C>>,
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

    fn mutation_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let company =
            unique_request_header(headers, "x-erpx-context-company")?.ok_or_else(scope)?;
        if company != self.service.company().as_str() {
            return Err(scope());
        }
        let assignment =
            unique_request_header(headers, "x-erpx-context-assignment")?.ok_or_else(scope)?;
        let revision: i64 = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(scope)?
            .parse()
            .map_err(|_| scope())?;
        let assignment_version: i64 =
            unique_request_header(headers, "x-erpx-context-assignment-version")?
                .ok_or_else(scope)?
                .parse()
                .map_err(|_| scope())?;
        if !(1..=9_007_199_254_740_991).contains(&revision)
            || !(1..=9_007_199_254_740_991).contains(&assignment_version)
        {
            return Err(scope());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(ExpectedWorkSelection {
                    revision,
                    assignment_id: AssignmentId::new(assignment)?,
                    assignment_version,
                }),
                mutation: true,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> AttachmentReadHttpBackend for FilesBackend<C> {
    fn list<'a>(
        &'a self,
        request: AttachmentReadHttpListRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .list(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.filter.clone(),
                    request.read_only,
                )
                .await
                .map_err(business)
        })
    }

    fn status<'a>(
        &'a self,
        request: AttachmentReadHttpStatusRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .status(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.link_id.to_owned(),
                )
                .await
                .map_err(business)
        })
    }

    fn withdraw<'a>(
        &'a self,
        request: AttachmentCommandHttpWithdrawRequest<'a>,
    ) -> AttachmentReadHttpFuture<'a, AttachmentReadHttpReply> {
        Box::pin(async move {
            let (proof, selection) = self.mutation_context(request.headers)?;
            self.service
                .withdraw(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.link_id.to_owned(),
                    request.reason.to_owned(),
                    request.command_key.to_owned(),
                )
                .await
                .map_err(business)
        })
    }
}
