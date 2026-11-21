//! Native Talent requisition assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the requisition workspace,
//! detail and command facts.
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
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use erpx_server_talent::public::requisition::{
    RequisitionChange, RequisitionRequest, TalentActor, change_requisition, create_requisition,
    requisition_detail, requisition_workspace,
};
use erpx_server_talent::public::requisition_http::{
    TalentRequisitionHttpBackend, TalentRequisitionHttpCommand, TalentRequisitionHttpFuture,
    TalentRequisitionHttpRequest, TalentRequisitionHttpResponse,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "TALENT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TALENT_UNAVAILABLE",
        "The Talent workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct TalentSources;

impl HoldingWorkSourcePreparation for TalentSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct TalentConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl TalentConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn talent_actor(current: &AuthorizedWorkContext) -> Result<TalentActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(TalentActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
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

pub struct TalentService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: TalentConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> TalentService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: TalentConfiguration,
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
            module: Some("talent".into()),
            destination: "talent".into(),
            api: true,
        }
    }

    fn read_input<Input>(
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

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<TalentRequisitionHttpResponse, Error>
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
                self.read_input(input),
                Self::access(),
                TalentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let now = clock.now().get();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = talent_actor(current)?;
                        let mut repository =
                            erpx_server_talent::public::requisition::PgTalentRequisitions::new(
                                connection,
                            );
                        let data = requisition_workspace(&mut repository, &actor, now).await?;
                        Ok(TalentRequisitionHttpResponse {
                            data,
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
    ) -> Result<TalentRequisitionHttpResponse, Error>
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
                self.read_input(input),
                Self::access(),
                TalentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let now = clock.now().get();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = talent_actor(current)?;
                        let mut repository =
                            erpx_server_talent::public::requisition::PgTalentRequisitions::new(
                                connection,
                            );
                        let data = requisition_detail(&mut repository, &actor, &id, now).await?;
                        Ok(TalentRequisitionHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        body: Value,
        key: String,
    ) -> Result<TalentRequisitionHttpResponse, Error>
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
                self.command_input(input),
                Self::access(),
                TalentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let now = clock.now().get();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        current.require_permission("records.write")?;
                        let actor = talent_actor(current)?;
                        let request: RequisitionRequest =
                            serde_json::from_value(body).map_err(|_| {
                                erpx_server_talent::public::requisition::request_invalid()
                            })?;
                        let command_key = CommandKey::new(key).map_err(|_| {
                            erpx_server_talent::public::requisition::request_invalid()
                        })?;
                        let mut repository =
                            erpx_server_talent::public::requisition::PgTalentRequisitions::new(
                                connection,
                            );
                        let data = create_requisition(
                            &mut repository,
                            &actor,
                            &command_key,
                            &request,
                            now,
                        )
                        .await?;
                        Ok(TalentRequisitionHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn change<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
        body: Value,
        key: String,
    ) -> Result<TalentRequisitionHttpResponse, Error>
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
                self.command_input(input),
                Self::access(),
                TalentSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let body = body.clone();
                    let key = key.clone();
                    let now = clock.now().get();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        current.require_permission("records.write")?;
                        let actor = talent_actor(current)?;
                        let request: RequisitionChange =
                            serde_json::from_value(body).map_err(|_| {
                                erpx_server_talent::public::requisition::request_invalid()
                            })?;
                        let command_key = CommandKey::new(key).map_err(|_| {
                            erpx_server_talent::public::requisition::request_invalid()
                        })?;
                        let mut repository =
                            erpx_server_talent::public::requisition::PgTalentRequisitions::new(
                                connection,
                            );
                        let data = change_requisition(
                            &mut repository,
                            &actor,
                            &command_key,
                            &id,
                            &request,
                            now,
                        )
                        .await?;
                        Ok(TalentRequisitionHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct TalentBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<TalentService<C>>,
}

impl<C: Clock + Send + Sync + 'static> TalentBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<TalentService<C>>,
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

impl<C: Clock + Send + Sync + 'static> TalentRequisitionHttpBackend for TalentBackend<C> {
    fn execute<'a>(
        &'a self,
        request: TalentRequisitionHttpRequest<'a>,
    ) -> TalentRequisitionHttpFuture<'a, TalentRequisitionHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                TalentRequisitionHttpCommand::Workspace => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                TalentRequisitionHttpCommand::Detail { requisition_id } => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), requisition_id)
                    .await
                    .map_err(business),
                TalentRequisitionHttpCommand::Create {
                    body,
                    idempotency_key,
                } => self
                    .service
                    .create(
                        request.host,
                        proof,
                        move || Ok(selection),
                        body,
                        idempotency_key,
                    )
                    .await
                    .map_err(business),
                TalentRequisitionHttpCommand::Change {
                    requisition_id,
                    body,
                    idempotency_key,
                } => self
                    .service
                    .change(
                        request.host,
                        proof,
                        move || Ok(selection),
                        requisition_id,
                        body,
                        idempotency_key,
                    )
                    .await
                    .map_err(business),
            }
        })
    }
}

#[allow(dead_code)]
fn _router(
    targets: Vec<erpx_server_talent::public::requisition_http::TalentRequisitionHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    erpx_server_talent::public::requisition_http::talent_requisition_http_router(targets)
}
