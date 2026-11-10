use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::release_reviewers::PgSamePersonReader;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_legal::public::legal_records::{
    LegalCommandReply, LegalDetailReply, LegalExportReply, LegalHoldNotice, LegalHoldNotifier,
    LegalHttpBackend, LegalHttpCommandRequest, LegalHttpFuture, LegalHttpRead, LegalHttpReadReply,
    LegalHttpReadRequest, LegalIdentity, LegalRecordsActor, LegalRegisterQuery, LegalRegisterReply,
    LegalRenderedContext, PgLegalFuture, PgLegalRecords, legal_command, legal_detail, legal_export,
    legal_register,
};
use erpx_server_legal::public::auxiliary::{
    LegalAuxiliaryActor, LegalAuxiliaryHttpBackend, LegalAuxiliaryHttpFuture,
    LegalAuxiliaryHttpRequest, LegalAuxiliaryHttpTarget, LegalAuxiliaryRead,
    PgLegalAuxiliaryReads, read_auxiliary,
};
use erpx_server_legal::public::trust_http::{
    LegalTrustCommandReply, LegalTrustHttpBackend, LegalTrustHttpCommandRequest,
    LegalTrustHttpFuture, LegalTrustHttpTarget, LegalTrustIdentityBridge,
    authorize_trust_actor, execute_legal_trust_command_with_owner_bridge, parse_trust_command,
};
use erpx_server_legal::public::auxiliary::{
    LegalRetentionActorFuture, LegalRetentionHttpBackend, LegalRetentionHttpCommandRequest,
    LegalRetentionHttpFuture, LegalRetentionHttpSessionRequest, LegalRetentionHttpTarget,
    execute_retention, legal_retention_business_error,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_legal::public::LegalTrustActor;
use erpx_server_legal::public::RustTrustMaterialInspector;
use erpx_server_legal::public::LegalTrustReceipt;
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, UserId,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEGAL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_UNAVAILABLE",
        "The Legal service is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEGAL_COMMAND_UNAVAILABLE",
        "Native Legal commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct LegalSources;

impl HoldingWorkSourcePreparation for LegalSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct LegalConfiguration {
    holding: HoldingId,
    holding_slug: String,
    company: CompanyId,
}

impl LegalConfiguration {
    pub fn new(holding: HoldingId, holding_slug: String, company: CompanyId) -> Self {
        Self {
            holding,
            holding_slug,
            company,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn legal_actor(
    current: &AuthorizedWorkContext,
    holding_slug: &str,
) -> Result<LegalRecordsActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(LegalRecordsActor {
        holding_id: current.holding_id().clone(),
        holding_slug: holding_slug.to_owned(),
        company_id: workspace.company_id.clone(),
        user_id: current.user_id().clone(),
        assignment_id: selected.assignment().id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        role: selected.assignment().role.clone(),
        permissions: current
            .command_iam_metadata()
            .permissions
            .iter()
            .cloned()
            .collect(),
        actor_json: {
            let workspace = selected.workspace();
            let assignment = selected.assignment();
            let scope_kind = if workspace.department_id.is_some() {
                "department"
            } else if workspace.branch_id.is_some() {
                "branch"
            } else {
                "holding"
            };
            serde_json::json!({
                "userId": current.user_id().as_str(),
                "name": "",
                "email": "",
                "assignmentId": assignment.id.as_str(),
                "role": assignment.role,
                "companyId": workspace.company_id.as_str(),
                "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
                "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
                "holdingId": current.holding_id().as_str(),
                "holdingSlug": holding_slug,
                "holdingName": "",
                "scope": if scope_kind == "department" {
                    serde_json::json!({"kind":"department","branchSlug":workspace.branch_slug,"departmentSlug":workspace.department_slug})
                } else if scope_kind == "branch" {
                    serde_json::json!({"kind":"branch","branchSlug":workspace.branch_slug})
                } else {
                    serde_json::json!({"kind":"holding"})
                },
                "assignmentScope": scope_kind,
                "inherited": false,
                "modules": current.modules(),
                "assignments": [],
                "workspaces": [],
                "iam": {"permissions": current.command_iam_metadata().permissions},
                "authority": {"revision": selected.revision(), "assignmentVersion": assignment.version, "sessionId": current.session_id().as_str()},
                "expiresAt": current.expires_at()
            })
            .to_string()
        },
    })
}

fn rendered_context(current: &AuthorizedWorkContext) -> LegalRenderedContext {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    LegalRenderedContext {
        user_id: current.user_id().as_str().to_owned(),
        holding_id: current.holding_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        assignment_version: assignment.version,
        revision: selected.revision(),
        role: assignment.role.clone(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    }
}

struct LegalSamePerson;
impl LegalIdentity for LegalSamePerson {
    fn same_person<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        left: &'a str,
        right: &'a str,
        company: &'a CompanyId,
    ) -> PgLegalFuture<'a, bool> {
        Box::pin(async move {
            let left = UserId::new(left.to_owned()).map_err(PgTransactionError::Business)?;
            let right = UserId::new(right.to_owned()).map_err(PgTransactionError::Business)?;
            let mut reader = PgSamePersonReader::new(connection);
            reader.same_person(company, &left, &right).await
        })
    }
}

struct LegalHoldNotifierImpl;
impl LegalHoldNotifier for LegalHoldNotifierImpl {
    fn notify<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        notices: &'a [LegalHoldNotice],
    ) -> PgLegalFuture<'a, ()> {
        Box::pin(async move {
            let facts = notices
                .iter()
                .map(
                    |notice| erpx_server_collaboration::public::legal_hold_notifications::LegalHoldNotificationFacts {
                        hold_id: notice.hold_id.clone(),
                        company_id: notice.company_id.clone(),
                        item_id: notice.item_id.clone(),
                        item_version: notice.item_version,
                        user_id: notice.user_id.clone(),
                        notified_at: notice.notified_at,
                        message: "A preservation notice requires acknowledgement".to_owned(),
                    },
                )
                .collect::<Vec<_>>();
            erpx_server_collaboration::public::legal_hold_notifications::stage_legal_hold_notifications(
                connection,
                &facts,
            )
            .await
        })
    }
}

pub struct LegalService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LegalConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> LegalService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: LegalConfiguration,
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
            permission: "legal.read".into(),
            module: Some("legal".into()),
            destination: "legal".into(),
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

    pub async fn register<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: LegalRegisterQuery,
    ) -> Result<LegalHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        let _clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let holding_slug = holding_slug.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.read")?;
                        let actor = legal_actor(current, &holding_slug)?;
                        let mut ports = PgLegalRecords::new(connection, None, None);
                        let data = legal_register(&mut ports, &actor, query).await?;
                        Ok(LegalHttpReadReply::Register(Box::new(LegalRegisterReply {
                            data,
                            context: rendered_context(current),
                        })))
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
    ) -> Result<LegalHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let holding_slug = holding_slug.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.read")?;
                        let actor = legal_actor(current, &holding_slug)?;
                        let mut ports = PgLegalRecords::new(connection, None, None);
                        let data = legal_detail(&mut ports, &actor, &id).await?;
                        Ok(LegalHttpReadReply::Detail(Box::new(LegalDetailReply {
                            data,
                            context: rendered_context(current),
                        })))
                    })
                },
            )
            .await
    }

    pub async fn export<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
        format: Option<String>,
    ) -> Result<LegalHttpReadReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let holding_slug = holding_slug.clone();
                    let id = id.clone();
                    let format = format.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.export")?;
                        let actor = legal_actor(current, &holding_slug)?;
                        let mut ports = PgLegalRecords::new(connection, None, None);
                        let data = legal_export(&mut ports, &actor, &id, format.as_deref()).await?;
                        Ok(LegalHttpReadReply::Export(Box::new(LegalExportReply {
                            data,
                            context: rendered_context(current),
                        })))
                    })
                },
            )
            .await
    }

    pub async fn command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        body: Value,
        key: String,
        request_hash: String,
    ) -> Result<LegalCommandReply, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let holding_slug = self.configuration.holding_slug.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let holding_slug = holding_slug.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let request_hash = request_hash.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.read")?;
                        let actor = legal_actor(current, &holding_slug)?;
                        let command_key =
                            CommandKey::new(key).map_err(PgTransactionError::Business)?;
                        let mut ports = PgLegalRecords::new(
                            connection,
                            Some(Arc::new(LegalSamePerson)),
                            Some(Arc::new(LegalHoldNotifierImpl)),
                        );
                        let data =
                            legal_command(&mut ports, &actor, &command_key, &request_hash, &body)
                                .await?;
                        Ok(LegalCommandReply {
                            data,
                            context: rendered_context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn auxiliary<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        read: LegalAuxiliaryRead,
    ) -> Result<Value, Error>
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
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let read = read.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let actor = LegalAuxiliaryActor {
                            user_id: current.user_id().clone(),
                            assignment_id: selected.assignment().id.clone(),
                            holding_id: current.holding_id().clone(),
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            role: selected.assignment().role.clone(),
                            permissions: current
                                .command_iam_metadata()
                                .permissions
                                .clone(),
                        };
                        let mut ports = PgLegalAuxiliaryReads::new(connection);
                        let data = read_auxiliary(&mut ports, &actor, read, clock.as_ref())
                            .await
                            .map_err(|error| match error {
                                erpx_server_legal::public::auxiliary::LegalAuxiliaryError::Business(error) => Error::Business(error),
                                erpx_server_legal::public::auxiliary::LegalAuxiliaryError::Persistence(error) => Error::Persistence(error),
                            })?;
                        Ok(data)
                    })
                },
            )
            .await
    }
}



struct LegalTrustIdentity {
    company: CompanyId,
    branch: Option<erpx_server_shared::public::RecordId>,
    department: Option<erpx_server_shared::public::RecordId>,
}

impl LegalTrustIdentity {
    async fn allowed(
        connection: &mut PgConnection,
        company: &CompanyId,
        user: &UserId,
        branch: Option<&erpx_server_shared::public::RecordId>,
        department: Option<&erpx_server_shared::public::RecordId>,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM assignments a \
             WHERE a.company_id=$1 AND a.user_id=$2 AND a.revoked_at IS NULL \
             AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
             AND iam_role_allows(a.role,a.company_id,'legal.manage') \
             AND ((a.scope_kind='holding' AND $3::text IS NULL AND $4::text IS NULL) \
               OR (a.scope_kind='branch' AND a.scope_node_id=$3) \
               OR (a.scope_kind='department' AND a.scope_node_id=$4)))",
        )
        .bind(company.as_str())
        .bind(user.as_str())
        .bind(branch.map(|value| value.as_str()))
        .bind(department.map(|value| value.as_str()))
        .fetch_one(&mut *connection)
        .await
        .map_err(PgTransactionError::from)?;
        Ok(allowed)
    }
}

impl LegalTrustIdentityBridge for LegalTrustIdentity {
    async fn authorize_manage(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
    ) -> Result<String, PgTransactionError<BusinessError>> {
        if !LegalTrustIdentity::allowed(
            connection,
            &self.company,
            &actor.user_id,
            actor.branch_id.as_ref(),
            actor.department_id.as_ref(),
        )
        .await?
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "LEGAL_TRUST_PERMISSION_REQUIRED",
                "Your selected role does not grant legal trust-material management.",
            )
            .into());
        }
        Ok("legal.manage".to_owned())
    }

    async fn revalidate_authority(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        _witness: &str,
    ) -> Result<(), PgTransactionError<BusinessError>> {
        if LegalTrustIdentity::allowed(
            connection,
            &self.company,
            &actor.user_id,
            actor.branch_id.as_ref(),
            actor.department_id.as_ref(),
        )
        .await?
        {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Conflict,
                "LEGAL_TRUST_AUTHORITY_CHANGED",
                "Your legal authority changed. Reload and retry.",
            )
            .into())
        }
    }

    async fn same_person(
        &mut self,
        connection: &mut PgConnection,
        actor: &LegalTrustActor,
        other: &UserId,
    ) -> Result<bool, PgTransactionError<BusinessError>> {
        let mut reader = PgSamePersonReader::new(connection);
        reader
            .same_person(&self.company, &actor.user_id, other)
            .await
    }
}

impl<C: Clock + Send + Sync + 'static> LegalService<C> {
    pub async fn trust_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: CommandKey,
        command: erpx_server_legal::public::LegalTrustCommand,
    ) -> Result<LegalTrustReceipt, Error>
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
                self.mutation_input(input),
                Self::trust_access(),
                LegalSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("legal.manage")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let actor = LegalTrustActor {
                            holding_id: current.holding_id().clone(),
                            company_id: workspace.company_id.clone(),
                            user_id: current.user_id().clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                        };
                        authorize_trust_actor(&actor)?;
                        let identity = LegalTrustIdentity {
                            company: workspace.company_id.clone(),
                            branch: workspace.branch_id.clone(),
                            department: workspace.department_id.clone(),
                        };
                        let inspector = RustTrustMaterialInspector;
                        let assignment = selected.assignment().id.clone();
                        let receipt = execute_legal_trust_command_with_owner_bridge(
                            connection,
                            &actor,
                            &assignment,
                            &key,
                            &command,
                            &inspector,
                            clock.as_ref(),
                            identity,
                        )
                        .await?;
                        Ok(receipt)
                    })
                },
            )
            .await
    }

    fn trust_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "legal.manage".into(),
            module: Some("legal".into()),
            destination: "legal".into(),
            api: true,
        }
    }

    fn mutation_input<Input>(
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
}

pub struct LegalRetentionBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LegalConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> LegalRetentionBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: LegalConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            policy,
            scopes,
            authorizer,
            configuration,
            clock,
        }
    }
}

impl<C: Clock + Send + Sync + 'static> LegalRetentionHttpBackend for LegalRetentionBackend<C> {
    fn resolve_actor<'a>(
        &'a self,
        request: LegalRetentionHttpSessionRequest<'a>,
    ) -> LegalRetentionActorFuture<'a> {
        Box::pin(async move {
            let inbound = parse_inbound_account_request(&self.policy, request.headers)?;
            if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
                return Err(not_found());
            }
            let (branch, department) = self.scopes.slugs(request.headers)?;
            let selection = HoldingWorkspaceRequest {
                company_id: self.configuration.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            };
            let holding = self.configuration.holding.clone();
            let company = self.configuration.company.clone();
            self.authorizer
                .execute_with_sources_deferred(
                    request.host,
                    inbound.into_proof(),
                    move || Ok(selection),
                    WorkAuthorizationRequest {
                        permission: "legal.read".into(),
                        module: Some("legal".into()),
                        destination: "legal".into(),
                        api: true,
                    },
                    LegalSources,
                    move |_, current| {
                        let holding = holding.clone();
                        let company = company.clone();
                        Box::pin(async move {
                            if current.holding_id() != &holding
                                || current.selection().workspace().company_id != company
                            {
                                return Err(scope().into());
                            }
                            let selected = current.selection();
                            let workspace = selected.workspace();
                            Ok(LegalAuxiliaryActor {
                                user_id: current.user_id().clone(),
                                assignment_id: selected.assignment().id.clone(),
                                holding_id: current.holding_id().clone(),
                                company_id: workspace.company_id.clone(),
                                branch_id: workspace.branch_id.clone(),
                                department_id: workspace.department_id.clone(),
                                role: selected.assignment().role.clone(),
                                permissions: current
                                    .command_iam_metadata()
                                    .permissions
                                    .clone(),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: LegalRetentionHttpCommandRequest<'a>,
    ) -> LegalRetentionHttpFuture<'a> {
        Box::pin(async move {
            execute_retention(
                request.pool,
                request.actor,
                request.key,
                request.request_hash,
                request.raw,
                request.now,
            )
            .await
            .map_err(legal_retention_business_error)
        })
    }
}

pub struct LegalBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<LegalService<C>>,
}

impl<C: Clock + Send + Sync + 'static> LegalBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<LegalService<C>>,
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



impl<C: Clock + Send + Sync + 'static> LegalTrustHttpBackend for LegalBackend<C> {
    fn command<'a>(
        &'a self,
        request: LegalTrustHttpCommandRequest<'a>,
    ) -> LegalTrustHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .trust_command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key.clone(),
                    request.command.clone(),
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> LegalAuxiliaryHttpBackend for LegalBackend<C> {
    fn read<'a>(&'a self, request: LegalAuxiliaryHttpRequest<'a>) -> LegalAuxiliaryHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .auxiliary(request.host, proof, move || Ok(selection), request.read)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> LegalHttpBackend for LegalBackend<C> {
    fn read<'a>(
        &'a self,
        request: LegalHttpReadRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalHttpReadReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.read {
                LegalHttpRead::Register(query) => self
                    .service
                    .register(request.host, proof, move || Ok(selection), query.clone())
                    .await
                    .map_err(business),
                LegalHttpRead::Detail(id) => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), id)
                    .await
                    .map_err(business),
                LegalHttpRead::Export { id, format } => self
                    .service
                    .export(request.host, proof, move || Ok(selection), id, format)
                    .await
                    .map_err(business),
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: LegalHttpCommandRequest<'a>,
    ) -> LegalHttpFuture<'a, LegalCommandReply> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let body = request.body.clone();
            let key = request.key.to_owned();
            let request_hash = request.request_hash.to_owned();
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    body,
                    key,
                    request_hash,
                )
                .await
                .map_err(business)
        })
    }
}
