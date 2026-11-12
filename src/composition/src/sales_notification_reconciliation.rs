//! Native Sales customer-notice reconciliation composition.
//!
//! Current Account/Holding authority, the selected Sales approver and the
//! purchase mode are resolved here; the owner keeps every source, provider,
//! independence, version and immutable-evidence rule.
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
use erpx_server_sales::public::sales_notification_reconciliation::{
    PgNotificationReconciliation, ProviderObservationPort, ReconcileActor, ReconcileHttpBackend,
    ReconcileHttpFuture, ReconcileHttpRequest, ReconcileHttpResponse, ReconcileHttpTarget,
    ReconcileInput, configured_observation, disabled, reconcile_customer_notification,
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
struct ReconciliationSources;

impl HoldingWorkSourcePreparation for ReconciliationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SalesNotificationReconciliationConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SalesNotificationReconciliationConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn actor(current: &AuthorizedWorkContext) -> Result<ReconcileActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = current.command_iam_metadata();
    Ok(ReconcileActor {
        user_id: current.user_id().as_str().to_owned(),
        person_user_ids: iam
            .person_user_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        role: Some(assignment.role.clone()),
        session_id: Some(current.session_id().as_str().to_owned()),
        assignment_version: Some(assignment.version),
        permissions: iam.permissions.clone(),
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

pub struct SalesNotificationReconciliationService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SalesNotificationReconciliationConfiguration,
    clock: Arc<C>,
    mode: PackageEnforcementMode,
    provider: Arc<dyn ProviderObservationPort>,
}

impl<C: Clock + Send + Sync + 'static> SalesNotificationReconciliationService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesNotificationReconciliationConfiguration,
        clock: Arc<C>,
        mode: PackageEnforcementMode,
        provider: Arc<dyn ProviderObservationPort>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
            mode,
            provider,
        }
    }

    /// Current-runtime constructor: read the reviewed package mode and the
    /// bounded provider credential once per assembled service.
    pub fn from_runtime(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesNotificationReconciliationConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self::new(
            authorizer,
            configuration,
            clock,
            package_mode(),
            Arc::from(configured_observation()),
        )
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    pub async fn reconcile(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        notification_id: String,
        idempotency_key: String,
        input: ReconcileInput,
    ) -> Result<ReconcileHttpResponse, Error> {
        if self.mode == PackageEnforcementMode::Disabled {
            return Err(disabled().into());
        }
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let mode = self.mode;
        let provider = Arc::clone(&self.provider);
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
                    permission: "commerce.approve".into(),
                    module: Some("sales".into()),
                    destination: "sales".into(),
                    api: true,
                },
                ReconciliationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let notification_id = notification_id.clone();
                    let idempotency_key = idempotency_key.clone();
                    let input = input.clone();
                    let provider = Arc::clone(&provider);
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.approve")?;
                        let actor = actor(current)?;
                        actor.require_approve()?;
                        let mut repository = PgNotificationReconciliation::new(connection, mode);
                        let receipt = reconcile_customer_notification(
                            &actor,
                            &notification_id,
                            input,
                            &idempotency_key,
                            &mut repository,
                            provider.as_ref(),
                            clock.now().get(),
                        )
                        .await?;
                        Ok(ReconcileHttpResponse {
                            data: receipt.wire(),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct SalesNotificationReconciliationBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SalesNotificationReconciliationService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SalesNotificationReconciliationBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SalesNotificationReconciliationService<C>>,
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

impl<C: Clock + Send + Sync + 'static> ReconcileHttpBackend
    for SalesNotificationReconciliationBackend<C>
{
    fn execute<'a>(
        &'a self,
        request: ReconcileHttpRequest<'a>,
    ) -> ReconcileHttpFuture<'a, ReconcileHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .reconcile(
                    request.host,
                    proof,
                    selection,
                    request.notification_id,
                    request.idempotency_key,
                    request.input,
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
    backend: Arc<dyn ReconcileHttpBackend>,
) -> ReconcileHttpTarget {
    ReconcileHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}
