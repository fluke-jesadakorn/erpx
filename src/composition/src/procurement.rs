//! Native Procurement read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the supplier promise
//! history facts.
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
use erpx_server_procurement::public::supplier_promise_history::{
    PgSupplierPromiseHistoryRead, SupplierPromiseHistory, SupplierPromiseReadActor,
    supplier_promise_history,
};
use erpx_server_procurement::public::supplier_promise_history_http::{
    SupplierPromiseHistoryHttpBackend, SupplierPromiseHistoryHttpFuture,
    SupplierPromiseHistoryHttpRequest, SupplierPromiseHistoryHttpResponse,
    SupplierPromiseHistoryHttpTarget, supplier_promise_history_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROCUREMENT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_UNAVAILABLE",
        "The Procurement workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct ProcurementSources;

impl HoldingWorkSourcePreparation for ProcurementSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct ProcurementConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ProcurementConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn procurement_actor(current: &AuthorizedWorkContext) -> Result<SupplierPromiseReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(SupplierPromiseReadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
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

pub struct ProcurementService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ProcurementConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ProcurementService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ProcurementConfiguration,
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
            module: Some("procurement".into()),
            destination: "procurement".into(),
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

    pub async fn history<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<SupplierPromiseHistoryHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let order_id = order_id.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                ProcurementSources,
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
                        let actor = procurement_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgSupplierPromiseHistoryRead::new(connection);
                        let data: SupplierPromiseHistory =
                            supplier_promise_history(&actor, &mut repository, &order_id, now).await?;
                        let data = serde_json::to_value(data).map_err(|_| unavailable())?;
                        Ok(SupplierPromiseHistoryHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct ProcurementBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ProcurementService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ProcurementBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ProcurementService<C>>,
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

impl<C: Clock + Send + Sync + 'static> SupplierPromiseHistoryHttpBackend for ProcurementBackend<C> {
    fn read<'a>(
        &'a self,
        request: SupplierPromiseHistoryHttpRequest<'a>,
    ) -> SupplierPromiseHistoryHttpFuture<'a, SupplierPromiseHistoryHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let order_id = request.order_id.to_owned();
            self.service
                .history(request.host, proof, move || Ok(selection), order_id)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<SupplierPromiseHistoryHttpTarget>) -> Result<axum::Router, BusinessError> {
    supplier_promise_history_router(targets)
}
