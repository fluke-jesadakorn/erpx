//! Native Commerce order HTTP mount. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply quotation and sales-order
//! facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commerce::public::order::{
    CommerceOrderActor, CommerceOrderReceipt, SalesOrderDocument, TransitionCommand, order_detail,
    transition_order,
};
use erpx_server_commerce::public::order_http::{
    CommerceOrderHttpBackend, CommerceOrderHttpCommand, CommerceOrderHttpFuture,
    CommerceOrderHttpRequest, CommerceOrderHttpResponse, CommerceOrderHttpTarget,
};
use erpx_server_commerce::public::order::PgCommerceOrders;
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
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
};
use serde_json::Value;
use std::sync::Arc;
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_UNAVAILABLE",
        "The Commerce workspace is unavailable.",
    )
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid("COMMERCE_REQUEST_INVALID", "The Commerce request is invalid.")
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CommerceSources;

impl HoldingWorkSourcePreparation for CommerceSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct CommerceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CommerceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn commerce_actor(current: &AuthorizedWorkContext) -> Result<CommerceOrderActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommerceOrderActor {
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

pub struct CommerceService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CommerceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CommerceService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CommerceConfiguration,
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
            module: Some("sales".into()),
            destination: "commerce".into(),
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

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<CommerceOrderHttpResponse, Error>
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
                CommerceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let order_id = order_id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let mut repository = PgCommerceOrders::new(connection);
                        let now = clock.now();
                        let data = order_detail(&actor, &order_id, &mut repository, now).await?;
                        Ok(CommerceOrderHttpResponse {
                            data: data.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
        idempotency_key: String,
        action: erpx_server_commerce::public::order::OrderAction,
        expected_version: i64,
        reason: Option<String>,
    ) -> Result<CommerceOrderHttpResponse, Error>
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
                CommerceSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let order_id = order_id.clone();
                    let idempotency_key = idempotency_key.clone();
                    let action = action;
                    let expected_version = expected_version;
                    let reason = reason.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = commerce_actor(current)?;
                        let order_record = RecordId::new(&order_id).map_err(|_| request_invalid())?;
                        let command_key =
                            CommandKey::new(&idempotency_key).map_err(|_| request_invalid())?;
                        let command = TransitionCommand {
                            key: command_key,
                            order_id: order_record,
                            action,
                            expected_version,
                            reason,
                        };
                        let mut repository = PgCommerceOrders::new(connection);
                        let now = clock.now();
                        let receipt = transition_order(&actor, &command, &mut repository, now).await?;
                        Ok(CommerceOrderHttpResponse {
                            data: receipt.to_wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CommerceBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CommerceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CommerceBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CommerceService<C>>,
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

impl<C: Clock + Send + Sync + 'static> CommerceOrderHttpBackend for CommerceBackend<C> {
    fn execute<'a>(
        &'a self,
        request: CommerceOrderHttpRequest<'a>,
    ) -> CommerceOrderHttpFuture<'a, CommerceOrderHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                CommerceOrderHttpCommand::Detail { order_id } => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), order_id)
                    .await
                    .map_err(business),
                CommerceOrderHttpCommand::Transition {
                    order_id,
                    idempotency_key,
                    action,
                    expected_version,
                    reason,
                } => self
                    .service
                    .transition(
                        request.host,
                        proof,
                        move || Ok(selection),
                        order_id,
                        idempotency_key,
                        action,
                        expected_version,
                        reason,
                    )
                    .await
                    .map_err(business),
            }
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CommerceOrderHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_commerce::public::order_http::commerce_order_http_router(targets)
}
