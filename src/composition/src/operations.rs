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
use erpx_server_operations::public::operations_http::{
    OperationsHttpBackend, OperationsHttpCommandRequest, OperationsHttpFuture,
    OperationsHttpReadRequest, OperationsHttpResponse, OperationsHttpTarget,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "OPERATIONS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_UNAVAILABLE",
        "The Operations workspace is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_COMMAND_UNAVAILABLE",
        "Native Operations commands are not enabled yet.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct OperationsSources;

impl HoldingWorkSourcePreparation for OperationsSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct OperationsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OperationsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
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

pub struct OperationsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: OperationsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OperationsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: OperationsConfiguration,
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
}

pub struct OperationsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<OperationsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> OperationsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<OperationsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> OperationsHttpBackend for OperationsBackend<C> {
    fn read<'a>(
        &'a self,
        request: OperationsHttpReadRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        Box::pin(async move {
            let (_proof, _selection) = self.context(request.headers)?;
            let _ = request.kind;
            let _ = request.host;
            let _ = request.holding_slug;
            Err(unavailable())
        })
    }

    fn command<'a>(
        &'a self,
        request: OperationsHttpCommandRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        Box::pin(async move {
            let (_proof, _selection) = self.context(request.headers)?;
            let _ = request.command;
            let _ = request.key;
            let _ = request.host;
            let _ = request.holding_slug;
            Err(command_unavailable())
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<OperationsHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_operations::public::operations_http::operations_router(targets)
}
