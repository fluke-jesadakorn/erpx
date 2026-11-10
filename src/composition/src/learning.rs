//! Native Learning read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded workspace,
//! assignment detail and eligibility facts.
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
use erpx_server_learning::public::eligibility_facts::{
    BenefitConsumptionActor, BenefitConsumptionCommand, EligibilityQuery, consume_benefit,
    learning_eligibility_facts, lock_learning_benefit_sources,
};
use erpx_server_learning::public::learning::{
    LearningActor, PgLearning, learning_detail, learning_workspace,
};
use erpx_server_learning::public::learning_http::{
    LearningHttpBackend, LearningHttpCommand, LearningHttpFuture, LearningHttpRequest,
    LearningHttpResponse, LearningHttpTarget, learning_http_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "LEARNING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEARNING_UNAVAILABLE",
        "The Learning workspace is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "LEARNING_COMMAND_UNAVAILABLE",
        "Native Learning commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct LearningSources;

impl HoldingWorkSourcePreparation for LearningSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct LearningConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl LearningConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn learning_actor(current: &AuthorizedWorkContext) -> Result<LearningActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(LearningActor {
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
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    json!({
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

pub struct LearningService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: LearningConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> LearningService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: LearningConfiguration,
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
            module: Some("learning".into()),
            destination: "learning".into(),
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
        personal: bool,
    ) -> Result<LearningHttpResponse, Error>
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
                LearningSources,
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
                        let actor = learning_actor(current)?;
                        let mut repository = PgLearning::new(connection);
                        let data = learning_workspace(&mut repository, &actor, personal, now.get())
                            .await?;
                        Ok(LearningHttpResponse {
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
        assignment_id: String,
    ) -> Result<LearningHttpResponse, Error>
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
                LearningSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let assignment_id = assignment_id.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = learning_actor(current)?;
                        let mut repository = PgLearning::new(connection);
                        let data =
                            learning_detail(&mut repository, &actor, &assignment_id, now.get())
                                .await?;
                        Ok(LearningHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn eligibility<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: EligibilityQuery,
    ) -> Result<LearningHttpResponse, Error>
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
                LearningSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let query = query.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = learning_actor(current)?;
                        let mut repository = PgLearning::new(connection);
                        let data =
                            learning_eligibility_facts(&mut repository, &actor, &query, now.get())
                                .await?;
                        Ok(LearningHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn consume_benefit<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command_input: BenefitConsumptionCommand,
        idempotency_key: String,
    ) -> Result<LearningHttpResponse, Error>
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
                Self::access(),
                LearningSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let command_input = command_input.clone();
                    let idempotency_key = idempotency_key.clone();
                    let now = clock.now();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let selected = current.selection();
                        let workspace = selected.workspace();
                        let assignment = selected.assignment();
                        let actor = BenefitConsumptionActor {
                            user_id: current.user_id().clone(),
                            assignment_id: assignment.id.clone(),
                            company_id: workspace.company_id.clone(),
                            branch_id: workspace.branch_id.clone(),
                            department_id: workspace.department_id.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            modules: current.modules().to_vec(),
                            expires_at: current.expires_at(),
                        };
                        let key = erpx_server_shared::public::CommandKey::new(idempotency_key)?;
                        lock_learning_benefit_sources(connection).await?;
                        let mut repository = PgLearning::new(connection);
                        let data = consume_benefit(
                            &mut repository,
                            &actor,
                            &key,
                            &command_input,
                            now.get(),
                        )
                        .await?;
                        Ok(LearningHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct LearningBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<LearningService<C>>,
}

impl<C: Clock + Send + Sync + 'static> LearningBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<LearningService<C>>,
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

impl<C: Clock + Send + Sync + 'static> LearningHttpBackend for LearningBackend<C> {
    fn execute<'a>(
        &'a self,
        request: LearningHttpRequest<'a>,
    ) -> LearningHttpFuture<'a, LearningHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                LearningHttpCommand::Workspace { personal } => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection), personal)
                    .await
                    .map_err(business),
                LearningHttpCommand::Detail { assignment_id } => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), assignment_id)
                    .await
                    .map_err(business),
                LearningHttpCommand::EligibilityFacts { query } => self
                    .service
                    .eligibility(request.host, proof, move || Ok(selection), query)
                    .await
                    .map_err(business),
                LearningHttpCommand::ConsumeBenefit {
                    input,
                    idempotency_key,
                } => {
                    self.service
                        .consume_benefit(
                            request.host,
                            proof,
                            move || Ok(selection),
                            input,
                            idempotency_key,
                        )
                        .await
                        .map_err(business)
                }
                _ => Err(command_unavailable()),
            }
        })
    }
}

// Retain the router constructor in feature-specific builds.
#[allow(dead_code)]
fn _router(targets: Vec<LearningHttpTarget>) -> Result<axum::Router, BusinessError> {
    learning_http_router(targets)
}
