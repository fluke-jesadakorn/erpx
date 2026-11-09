//! Native Customer Authority customer-access assembly. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply the
//! customer access workspace, detail, request and change facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_customer_authority::public::access::PgCustomerAccess;
use erpx_server_customer_authority::public::access::{
    AccessActor, AccessChange, AccessRequest, access_detail, access_workspace, change_access,
    request_access,
};
use erpx_server_customer_authority::public::access_http::{
    CustomerAccessHttpBackend, CustomerAccessHttpCommand, CustomerAccessHttpFuture,
    CustomerAccessHttpRequest, CustomerAccessHttpResponse, CustomerAccessHttpTarget,
};
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
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_AUTHORITY_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_AUTHORITY_UNAVAILABLE",
        "The Customer Authority workspace is unavailable.",
    )
}

fn invalid_request() -> BusinessError {
    erpx_server_customer_authority::public::access::request_invalid()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CustomerAuthoritySources;

impl HoldingWorkSourcePreparation for CustomerAuthoritySources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct CustomerAuthorityConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CustomerAuthorityConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn access_actor(current: &AuthorizedWorkContext) -> Result<AccessActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(AccessActor {
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

pub struct CustomerAuthorityService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CustomerAuthorityConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CustomerAuthorityService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CustomerAuthorityConfiguration,
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
            permission: "commerce.read".into(),
            module: Some("customer_authority".into()),
            destination: "customer_authority".into(),
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
    ) -> Result<CustomerAccessHttpResponse, Error>
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
                CustomerAuthoritySources,
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
                        current.require_permission("commerce.read")?;
                        let actor = access_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgCustomerAccess::new(connection);
                        let data = access_workspace(&mut repository, &actor, now).await?;
                        Ok(CustomerAccessHttpResponse {
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
    ) -> Result<CustomerAccessHttpResponse, Error>
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
                CustomerAuthoritySources,
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
                        current.require_permission("commerce.read")?;
                        let actor = access_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgCustomerAccess::new(connection);
                        let data = access_detail(&mut repository, &actor, &id, now).await?;
                        Ok(CustomerAccessHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn request<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command_input: AccessRequest,
        idempotency_key: String,
    ) -> Result<CustomerAccessHttpResponse, Error>
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
                CustomerAuthoritySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command_input = command_input.clone();
                    let idempotency_key = idempotency_key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = access_actor(current)?;
                        let key = CommandKey::new(idempotency_key)
                            .map_err(PgTransactionError::Business)?;
                        let now = clock.now().get();
                        let mut repository = PgCustomerAccess::new(connection);
                        let data =
                            request_access(&mut repository, &actor, &key, &command_input, now)
                                .await?;
                        Ok(CustomerAccessHttpResponse {
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
        binding_id: String,
        command_input: AccessChange,
        idempotency_key: String,
    ) -> Result<CustomerAccessHttpResponse, Error>
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
                CustomerAuthoritySources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let binding_id = binding_id.clone();
                    let command_input = command_input.clone();
                    let idempotency_key = idempotency_key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = access_actor(current)?;
                        let key = CommandKey::new(idempotency_key)
                            .map_err(PgTransactionError::Business)?;
                        let now = clock.now().get();
                        let mut repository = PgCustomerAccess::new(connection);
                        let data = change_access(
                            &mut repository,
                            &actor,
                            &key,
                            &binding_id,
                            &command_input,
                            now,
                        )
                        .await?;
                        Ok(CustomerAccessHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CustomerAuthorityBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CustomerAuthorityService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CustomerAuthorityBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CustomerAuthorityService<C>>,
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

impl<C: Clock + Send + Sync + 'static> CustomerAccessHttpBackend for CustomerAuthorityBackend<C> {
    fn execute<'a>(
        &'a self,
        request: CustomerAccessHttpRequest<'a>,
    ) -> CustomerAccessHttpFuture<'a, CustomerAccessHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                CustomerAccessHttpCommand::Workspace => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                CustomerAccessHttpCommand::Detail { binding_id } => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), binding_id)
                    .await
                    .map_err(business),
                CustomerAccessHttpCommand::Request {
                    body,
                    idempotency_key,
                } => {
                    let command_input: AccessRequest =
                        serde_json::from_value(body).map_err(|_| invalid_request())?;
                    self.service
                        .request(
                            request.host,
                            proof,
                            move || Ok(selection),
                            command_input,
                            idempotency_key,
                        )
                        .await
                        .map_err(business)
                }
                CustomerAccessHttpCommand::Change {
                    binding_id,
                    body,
                    idempotency_key,
                } => {
                    let command_input: AccessChange =
                        serde_json::from_value(body).map_err(|_| invalid_request())?;
                    self.service
                        .change(
                            request.host,
                            proof,
                            move || Ok(selection),
                            binding_id,
                            command_input,
                            idempotency_key,
                        )
                        .await
                        .map_err(business)
                }
            }
        })
    }
}

// Retain the router constructor in feature-specific builds.
#[allow(dead_code)]
fn _router(targets: Vec<CustomerAccessHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_customer_authority::public::access_http::customer_access_http_router(targets)
}
