//! Native Platform Administration HTTP assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! provisioning, operations and command facts.
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
use erpx_server_platform_administration::public::platform_operations::{
    PgPlatformAdministration, PlatformActor, PlatformHoldingIdentity,
    PlatformHttpBackend, PlatformHttpCommand, PlatformHttpCommandRequest, PlatformHttpFuture,
    PlatformHttpResponse, PlatformHttpTarget, PlatformResourceSlot,
    platform_operations_snapshot, provision_holding, recover_dead_letter_job, run_holding_migration,
};
use erpx_server_platform_administration::public::platform_operations::PlatformHttpReadRequest;
use erpx_server_platform_administration::public::agent_evidence::{
    AgentEvidenceActor, AgentEvidenceKind, AgentEvidenceRecord, AgentEvidenceWorkspace,
    PgAgentEvidenceRead, agent_evidence_record, agent_evidence_workspace, lock_agent_evidence_sources,
};
use erpx_server_platform_administration::public::agent_evidence_http::{
    AgentEvidenceHttpBackend, AgentEvidenceHttpFuture, AgentEvidenceHttpQuery,
    AgentEvidenceHttpReadRequest, AgentEvidenceHttpResponse,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_ADMINISTRATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_ADMINISTRATION_UNAVAILABLE",
        "The Platform Administration workspace is unavailable.",
    )
}

fn database_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PLATFORM_DATABASE_UNAVAILABLE",
        "Platform Administration persistence is unavailable.",
    )
}

fn forbidden_platform() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PLATFORM_OPERATIONS_FORBIDDEN",
        "A current Platform assignment is required.",
    )
}

fn resource_slot_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "RESOURCE_SLOT_NOT_FOUND",
        "This Holding resource slot is not configured.",
    )
}

fn holding_not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Holding not found.")
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PlatformAdministrationSources;

impl HoldingWorkSourcePreparation for PlatformAdministrationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct PlatformAdministrationConfiguration {
    holding: HoldingId,
    company: CompanyId,
    store: Arc<PgPlatformAdministration>,
    slots: Vec<PlatformResourceSlot>,
    holdings: Vec<PlatformHoldingIdentity>,
    control: DatabasePool,
}

impl PlatformAdministrationConfiguration {
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        store: Arc<PgPlatformAdministration>,
        slots: Vec<PlatformResourceSlot>,
        holdings: Vec<PlatformHoldingIdentity>,
        control: DatabasePool,
    ) -> Self {
        Self {
            holding,
            company,
            store,
            slots,
            holdings,
            control,
        }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn context_value(current: &AuthorizedWorkContext) -> Value {
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

pub struct PlatformAdministrationService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PlatformAdministrationConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PlatformAdministrationService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PlatformAdministrationConfiguration,
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
            permission: "manage_erpx.access".into(),
            module: Some("platform_administration".into()),
            destination: "platform_administration".into(),
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

    pub async fn read_operations<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        platform_assignment_id: String,
    ) -> Result<PlatformHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let store = self.configuration.store.clone();
        let slots = self.configuration.slots.clone();
        let holdings = self.configuration.holdings.clone();
        let control = self.configuration.control.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PlatformAdministrationSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let store = store.clone();
                    let slots = slots.clone();
                    let holdings = holdings.clone();
                    let control = control.clone();
                    let platform_assignment_id = platform_assignment_id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("manage_erpx.access")?;
                        let now = clock.now().get();
                        let role: String = sqlx::query_scalar(
                            "SELECT role FROM platform_assignments WHERE id=$1 AND user_id=$2 \
                             AND revoked_at IS NULL AND valid_from <= $3 AND valid_until > $3",
                        )
                        .bind(&platform_assignment_id)
                        .bind(current.user_id().as_str())
                        .bind(now)
                        .fetch_optional(control.raw())
                        .await
                        .map_err(|_| database_unavailable())?
                        .ok_or_else(forbidden_platform)?;
                        if !matches!(role.as_str(), "platform_admin" | "platform_auditor") {
                            return Err(forbidden_platform().into());
                        }
                        let actor = PlatformActor {
                            user_id: current.user_id().as_str().to_owned(),
                            assignment_id: platform_assignment_id,
                            role,
                        };
                        let snapshot = platform_operations_snapshot(
                            &actor,
                            store.as_ref(),
                            &holdings,
                            &slots,
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(snapshot).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "PLATFORM_REPLY_INVALID",
                                "The Platform reply could not be rendered.",
                            )
                        })?;
                        Ok(PlatformHttpResponse {
                            data,
                            context: context_value(current),
                        })
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
        platform_assignment_id: String,
        key: String,
        command: PlatformHttpCommand,
    ) -> Result<PlatformHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let store = self.configuration.store.clone();
        let slots = self.configuration.slots.clone();
        let holdings = self.configuration.holdings.clone();
        let control = self.configuration.control.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PlatformAdministrationSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let store = store.clone();
                    let slots = slots.clone();
                    let holdings = holdings.clone();
                    let control = control.clone();
                    let platform_assignment_id = platform_assignment_id.clone();
                    let key = key.clone();
                    let command = command.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("manage_erpx.access")?;
                        let now = clock.now().get();
                        let role: String = sqlx::query_scalar(
                            "SELECT role FROM platform_assignments WHERE id=$1 AND user_id=$2 \
                             AND revoked_at IS NULL AND valid_from <= $3 AND valid_until > $3",
                        )
                        .bind(&platform_assignment_id)
                        .bind(current.user_id().as_str())
                        .bind(now)
                        .fetch_optional(control.raw())
                        .await
                        .map_err(|_| database_unavailable())?
                        .ok_or_else(forbidden_platform)?;
                        if !matches!(role.as_str(), "platform_admin" | "platform_auditor") {
                            return Err(forbidden_platform().into());
                        }
                        let actor = PlatformActor {
                            user_id: current.user_id().as_str().to_owned(),
                            assignment_id: platform_assignment_id,
                            role,
                        };
                        let data = match command {
                            PlatformHttpCommand::Provision(input) => {
                                let slot = slots
                                    .iter()
                                    .find(|slot| slot.id == input.holding_id)
                                    .cloned()
                                    .ok_or_else(resource_slot_not_found)?;
                                let result = provision_holding(
                                    &actor,
                                    store.as_ref(),
                                    &slot,
                                    &input,
                                    &key,
                                    now,
                                )
                                .await?;
                                serde_json::to_value(result).map_err(|_| {
                                    BusinessError::new(
                                        FailureKind::Unavailable,
                                        "PLATFORM_REPLY_INVALID",
                                        "The Platform reply could not be rendered.",
                                    )
                                })?
                            }
                            PlatformHttpCommand::Recover {
                                holding_id,
                                input,
                            } => {
                                let target = holdings
                                    .iter()
                                    .find(|holding| holding.id == holding_id)
                                    .cloned()
                                    .ok_or_else(holding_not_found)?;
                                let result = recover_dead_letter_job(
                                    &actor,
                                    store.as_ref(),
                                    &target,
                                    &input,
                                    &key,
                                    now,
                                )
                                .await?;
                                serde_json::to_value(result).map_err(|_| {
                                    BusinessError::new(
                                        FailureKind::Unavailable,
                                        "PLATFORM_REPLY_INVALID",
                                        "The Platform reply could not be rendered.",
                                    )
                                })?
                            }
                            PlatformHttpCommand::Migrate(input) => {
                                let target = holdings
                                    .iter()
                                    .find(|holding| holding.id == input.holding_id)
                                    .cloned()
                                    .ok_or_else(holding_not_found)?;
                                let result = run_holding_migration(
                                    &actor,
                                    store.as_ref(),
                                    &target,
                                    &input,
                                    &key,
                                    now,
                                )
                                .await?;
                                serde_json::to_value(result).map_err(|_| {
                                    BusinessError::new(
                                        FailureKind::Unavailable,
                                        "PLATFORM_REPLY_INVALID",
                                        "The Platform reply could not be rendered.",
                                    )
                                })?
                            }
                        };
                        Ok(PlatformHttpResponse {
                            data,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PlatformAdministrationBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PlatformAdministrationService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PlatformAdministrationBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PlatformAdministrationService<C>>,
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
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest, String), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let platform_assignment_id = unique_header(headers, HeaderName::from_static("x-erpx-platform-assignment"))?
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Forbidden,
                    "PLATFORM_AUTHORITY_REQUIRED",
                    "Your selected Platform assignment is unavailable.",
                )
            })?
            .to_owned();
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
            platform_assignment_id,
        ))
    }
}

use erpx_server_runtime::public::http_support::HeaderName;

impl<C: Clock + Send + Sync + 'static> PlatformHttpBackend for PlatformAdministrationBackend<C> {
    fn read<'a>(
        &'a self,
        request: PlatformHttpReadRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
        Box::pin(async move {
            let (proof, selection, platform_assignment_id) = self.context(request.headers)?;
            self.service
                .read_operations(request.host, proof, move || Ok(selection), platform_assignment_id)
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: PlatformHttpCommandRequest<'a>,
    ) -> PlatformHttpFuture<'a, PlatformHttpResponse> {
        Box::pin(async move {
            let (proof, selection, platform_assignment_id) = self.context(request.headers)?;
            let key = request.key.as_str().to_owned();
            let command = request.command.clone();
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    platform_assignment_id,
                    key,
                    command,
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<PlatformHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_platform_administration::public::platform_operations::platform_router(targets)
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "PLATFORM_REQUEST_INVALID",
            "Use one unambiguous request header value.",
        ));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| {
                    BusinessError::new(
                        FailureKind::InvalidInput,
                        "PLATFORM_REQUEST_INVALID",
                        "Use a valid request header.",
                    )
                })
        })
        .transpose()
}

#[derive(Clone, Copy)]
struct AgentEvidenceSources;

impl HoldingWorkSourcePreparation for AgentEvidenceSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct AgentEvidenceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl AgentEvidenceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }

    pub fn holding(&self) -> &HoldingId {
        &self.holding
    }
}

pub struct AgentEvidenceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: AgentEvidenceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> AgentEvidenceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: AgentEvidenceConfiguration,
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
            module: Some("agent".into()),
            destination: "agent_evidence".into(),
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

    pub async fn read_workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<AgentEvidenceHttpResponse, Error>
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
                AgentEvidenceSources,
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
                        current.require_permission("records.read")?;
                        let now = clock.now().get();
                        let actor = AgentEvidenceActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.as_str().to_owned(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        lock_agent_evidence_sources(connection).await?;
                        let mut repository = PgAgentEvidenceRead::new(connection);
                        let workspace =
                            agent_evidence_workspace(&actor, &mut repository, now).await?;
                        let data = serde_json::to_value(workspace).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "AGENT_EVIDENCE_REPLY_INVALID",
                                "The Agent evidence reply could not be rendered.",
                            )
                        })?;
                        Ok(AgentEvidenceHttpResponse {
                            data,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn read_record<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        kind: AgentEvidenceKind,
        id: String,
    ) -> Result<AgentEvidenceHttpResponse, Error>
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
                AgentEvidenceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now().get();
                        let actor = AgentEvidenceActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.as_str().to_owned(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        lock_agent_evidence_sources(connection).await?;
                        let mut repository = PgAgentEvidenceRead::new(connection);
                        let record =
                            agent_evidence_record(&actor, kind, &id, &mut repository, now).await?;
                        let data = serde_json::to_value(record).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "AGENT_EVIDENCE_REPLY_INVALID",
                                "The Agent evidence reply could not be rendered.",
                            )
                        })?;
                        Ok(AgentEvidenceHttpResponse {
                            data,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct AgentEvidenceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<AgentEvidenceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> AgentEvidenceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<AgentEvidenceService<C>>,
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

impl<C: Clock + Send + Sync + 'static> AgentEvidenceHttpBackend for AgentEvidenceBackend<C> {
    fn read<'a>(
        &'a self,
        request: AgentEvidenceHttpReadRequest<'a>,
    ) -> AgentEvidenceHttpFuture<'a, AgentEvidenceHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.query {
                AgentEvidenceHttpQuery::Workspace => self
                    .service
                    .read_workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                AgentEvidenceHttpQuery::Record(query) => {
                    let kind = query.kind;
                    let id = query.id.clone();
                    self.service
                        .read_record(request.host, proof, move || Ok(selection), kind, id)
                        .await
                        .map_err(business)
                }
            }
        })
    }
}
