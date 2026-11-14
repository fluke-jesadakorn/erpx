//! Native Sales customer follow-up command composition.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
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
use erpx_server_sales::public::customer_followup_commands::{
    ChangeFollowup, FollowupActor, FollowupHttpBackend, FollowupHttpCommand, FollowupHttpFuture,
    FollowupHttpRequest, FollowupHttpResponse, FollowupHttpTarget, PgFollowups,
    change_customer_followup, create_customer_followup, customer_followups_router,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "SALES_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SALES_UNAVAILABLE",
        "The Sales service is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

fn package_mode() -> PackageEnforcementMode {
    match std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned())
        .as_str()
    {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        _ => PackageEnforcementMode::Enforced,
    }
}

#[derive(Clone, Copy)]
struct FollowupSources;

impl HoldingWorkSourcePreparation for FollowupSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SalesFollowupConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SalesFollowupConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn actor(current: &AuthorizedWorkContext) -> Result<FollowupActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(FollowupActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        session_id: Some(current.session_id().as_str().to_owned()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
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

pub struct SalesFollowupService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SalesFollowupConfiguration,
    clock: Arc<C>,
    mode: PackageEnforcementMode,
}

impl<C: Clock + Send + Sync + 'static> SalesFollowupService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesFollowupConfiguration,
        clock: Arc<C>,
        mode: PackageEnforcementMode,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            mode,
        }
    }

    pub fn from_runtime(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesFollowupConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self::new(authorizer, configuration, clock, package_mode())
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn execute(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        key: String,
        command: FollowupHttpCommand,
    ) -> Result<FollowupHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let mode = self.mode;
        let expected = company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || {
                    let mut request = selection;
                    if request.company_id != expected {
                        return Err(scope());
                    }
                    request.mutation = true;
                    Ok(request)
                },
                WorkAuthorizationRequest {
                    permission: "commerce.sales".into(),
                    module: Some("sales".into()),
                    destination: "sales".into(),
                    api: true,
                },
                FollowupSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.sales")?;
                        let actor = actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgFollowups::new(connection, mode);
                        let receipt = match command {
                            FollowupHttpCommand::Create { input } => {
                                create_customer_followup(&actor, input, &key, &mut repository, now)
                                    .await?
                            }
                            FollowupHttpCommand::Change { id, input } => {
                                let input: ChangeFollowup = input;
                                change_customer_followup(
                                    &actor,
                                    &id,
                                    input,
                                    &key,
                                    &mut repository,
                                    now,
                                )
                                .await?
                            }
                        };
                        Ok(FollowupHttpResponse {
                            data: receipt.wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct SalesFollowupBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SalesFollowupService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SalesFollowupBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SalesFollowupService<C>>,
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

impl<C: Clock + Send + Sync + 'static> FollowupHttpBackend for SalesFollowupBackend<C> {
    fn execute<'a>(
        &'a self,
        request: FollowupHttpRequest<'a>,
    ) -> FollowupHttpFuture<'a, FollowupHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .execute(
                    request.host,
                    proof,
                    selection,
                    request.idempotency_key,
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

pub fn target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn FollowupHttpBackend>,
) -> FollowupHttpTarget {
    FollowupHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}

pub fn router(
    targets: Vec<FollowupHttpTarget>,
) -> Result<erpx_server_runtime::public::http_support::Router, BusinessError> {
    customer_followups_router(targets)
}
