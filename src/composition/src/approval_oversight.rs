//! Native Workflow approval oversight assembly. The retained Account/Holding
//! authority is supplied by the selected coordinator; this module projects the
//! current actor, runs the owner read and returns the bounded evidence context.
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
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId};
use erpx_server_workflow::public::approval_oversight::{
    ApprovalEvidenceQuery, ApprovalOversightActor, PgApprovalOversight, approval_evidence,
    approval_queue_value,
};
use erpx_server_workflow::public::approval_oversight_http::{
    ApprovalOversightHttpBackend, ApprovalOversightHttpCommand, ApprovalOversightHttpFuture,
    ApprovalOversightHttpRequest, ApprovalOversightHttpResponse,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "APPROVAL_EVIDENCE_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "APPROVAL_EVIDENCE_UNAVAILABLE",
        "The approval evidence is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone)]
pub struct ApprovalOversightConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ApprovalOversightConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

#[derive(Clone, Copy)]
struct ApprovalOversightSources;

impl HoldingWorkSourcePreparation for ApprovalOversightSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn approval_actor(current: &AuthorizedWorkContext) -> Result<ApprovalOversightActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(ApprovalOversightActor {
        user_id: current.user_id().clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        permissions: current.command_iam_metadata().permissions.clone(),
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
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "scopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "permissions": current.command_iam_metadata().permissions,
    })
}

pub struct ApprovalOversightService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ApprovalOversightConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ApprovalOversightService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ApprovalOversightConfiguration,
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
            permission: "organization.manage".into(),
            module: None,
            destination: "workflow/approvals".into(),
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

    pub async fn queue<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<ApprovalOversightHttpResponse, Error>
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
                ApprovalOversightSources,
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
                        current.require_permission("organization.manage")?;
                        let actor = approval_actor(current)?;
                        let mut ports = PgApprovalOversight::new(connection);
                        let data =
                            approval_queue_value(&mut ports, &actor, now.get()).await?;
                        Ok(ApprovalOversightHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn evidence<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: ApprovalEvidenceQuery,
    ) -> Result<ApprovalOversightHttpResponse, Error>
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
                ApprovalOversightSources,
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
                        current.require_permission("organization.manage")?;
                        let actor = approval_actor(current)?;
                        let mut ports = PgApprovalOversight::new(connection);
                        let data = approval_evidence(&mut ports, &actor, &query, now.get())
                            .await?
                            .ok_or(not_found())?;
                        Ok(ApprovalOversightHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct ApprovalOversightBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ApprovalOversightService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ApprovalOversightBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ApprovalOversightService<C>>,
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

impl<C: Clock + Send + Sync + 'static> ApprovalOversightHttpBackend
    for ApprovalOversightBackend<C>
{
    fn execute<'a>(
        &'a self,
        request: ApprovalOversightHttpRequest<'a>,
    ) -> ApprovalOversightHttpFuture<'a, ApprovalOversightHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.command {
                ApprovalOversightHttpCommand::Queue => {
                    self.service
                        .queue(request.host, proof, move || Ok(selection))
                        .await
                        .map_err(business)
                }
                ApprovalOversightHttpCommand::Evidence { query } => {
                    self.service
                        .evidence(request.host, proof, move || Ok(selection), query)
                        .await
                        .map_err(business)
                }
            }
        })
    }
}

// Keep the command type referenced in feature-specific builds.
#[allow(dead_code)]
fn _command(_: CommandKey) {}
