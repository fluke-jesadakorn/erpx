//! Native Billing composition HTTP mount. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the Billing
//! workspace and cost-review command facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_billing::public::billing_http::{
    BillingCommandHttpRequest, BillingHttpBackend, BillingHttpFuture, BillingHttpResponse,
    BillingWorkspaceHttpRequest,
};
use erpx_server_billing::public::cost::PgBillingCostCommand;
use erpx_server_billing::public::cost::{BillingCostCommand, execute_cost_command};
use erpx_server_billing::public::target_evidence_command::{
    BillingTargetEvidenceCommand, execute_target_evidence,
};
use erpx_server_billing::public::target_provider_command::{
    BillingTargetProviderCommand, execute_target_provider,
};
use erpx_server_billing::public::target_provider_http::{
    BillingTargetProviderHttpBackend, BillingTargetProviderHttpFuture,
    BillingTargetProviderHttpRequest, BillingTargetProviderHttpResponse,
};
use erpx_server_billing::public::target_rollout_http::{
    BillingTargetRolloutHttpBackend, BillingTargetRolloutHttpFuture,
    BillingTargetRolloutHttpRequest, BillingTargetRolloutHttpResponse,
};
use erpx_server_billing::public::workspace::PgBillingWorkspace;
use erpx_server_billing::public::workspace::{BillingActor, BillingWorkspace, billing_workspace};
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
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BILLING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_UNAVAILABLE",
        "The Billing workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// The trusted Beta storage cap is deployment configuration, never request
/// input. The TypeScript oracle reads the same variable for rollout readiness,
/// so the native approval evaluates the exact same allowance.
fn beta_storage_limit() -> Result<i64, BusinessError> {
    parse_beta_storage_limit(std::env::var("BETA_STORAGE_BYTES").ok().as_deref())
}

fn parse_beta_storage_limit(raw: Option<&str>) -> Result<i64, BusinessError> {
    const DEFAULT_STORAGE_BYTES: i64 = 1_073_741_824;
    const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    let Some(raw) = raw else {
        return Ok(DEFAULT_STORAGE_BYTES);
    };
    let value = raw.trim().parse::<i64>().map_err(|_| unavailable())?;
    if value <= 0 || value > MAX_SAFE_INTEGER {
        return Err(unavailable());
    }
    Ok(value)
}

#[derive(Clone, Copy)]
struct BillingSources;

impl HoldingWorkSourcePreparation for BillingSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct BillingConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl BillingConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

async fn billing_actor(
    current: &AuthorizedWorkContext,
    connection: &mut PgConnection,
    now: TimestampMillis,
) -> Result<BillingActor, BusinessError> {
    let platform_assignment = sqlx::query(
        "SELECT id, role FROM platform_assignments \
         WHERE user_id=$1 AND revoked_at IS NULL AND valid_from <= $2 AND valid_until > $2 \
         ORDER BY valid_from DESC, id LIMIT 1",
    )
    .bind(current.user_id().as_str())
    .bind(now.get())
    .fetch_optional(connection)
    .await
    .map_err(|_| unavailable())?;

    if let Some(row) = platform_assignment {
        let assignment_id: String = row.try_get("id").map_err(|_| unavailable())?;
        let role: String = row.try_get("role").map_err(|_| unavailable())?;
        Ok(BillingActor {
            user_id: current.user_id().clone(),
            assignment_id: AssignmentId::new(assignment_id).map_err(|_| unavailable())?,
            holding_id: None,
            platform: true,
            role,
            expires_at: Some(current.expires_at()),
        })
    } else {
        let selected = current.selection();
        let assignment = selected.assignment();
        Ok(BillingActor {
            user_id: current.user_id().clone(),
            assignment_id: assignment.id.clone(),
            holding_id: Some(current.holding_id().clone()),
            platform: false,
            role: assignment.role.clone(),
            expires_at: Some(current.expires_at()),
        })
    }
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

pub struct BillingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: BillingConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> BillingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: BillingConfiguration,
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
            module: Some("billing".into()),
            destination: "billing".into(),
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
    ) -> Result<BillingHttpResponse, Error>
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
                BillingSources,
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
                        let now = clock.now();
                        let actor = billing_actor(current, connection, now).await?;
                        let mut repository = PgBillingWorkspace::new(connection);
                        let data =
                            billing_workspace(&actor, &holding, &mut repository, now).await?;
                        Ok(BillingHttpResponse {
                            data: serde_json::to_value(data).map_err(|_| unavailable())?,
                            context: context(current),
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
        command: BillingCostCommand,
    ) -> Result<BillingHttpResponse, Error>
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
                BillingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let actor = billing_actor(current, connection, now).await?;
                        let mut repository = PgBillingCostCommand::new(connection);
                        let _receipt =
                            execute_cost_command(&actor, &holding, &command, &mut repository, now)
                                .await
                                .map_err(|error| match error {
                                    PgTransactionError::Business(error) => error,
                                    PgTransactionError::Persistence(_) => unavailable(),
                                })?;
                        Ok(BillingHttpResponse {
                            data: serde_json::json!({"ok": true}),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn target_rollout<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: BillingTargetEvidenceCommand,
    ) -> Result<BillingHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !matches!(command, BillingTargetEvidenceCommand::Rollout(_)) {
            return Err(BusinessError::invalid(
                "BILLING_REQUEST_INVALID",
                "Use one bounded Billing rollout request.",
            )
            .into());
        }
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                BillingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let actor = billing_actor(current, connection, now).await?;
                        execute_target_evidence(
                            connection,
                            &actor,
                            &holding,
                            &command,
                            now,
                            beta_storage_limit()?,
                        )
                        .await?;
                        Ok(BillingHttpResponse {
                            data: serde_json::json!({"ok": true}),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn target_provider<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: BillingTargetProviderCommand,
    ) -> Result<BillingHttpResponse, Error>
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
                BillingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let now = clock.now();
                        let actor = billing_actor(current, connection, now).await?;
                        execute_target_provider(connection, &actor, &holding, &command, now)
                            .await?;
                        Ok(BillingHttpResponse {
                            data: serde_json::json!({"ok": true}),
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct BillingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<BillingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> BillingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<BillingService<C>>,
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

impl<C: Clock + Send + Sync + 'static> BillingHttpBackend for BillingBackend<C> {
    fn workspace<'a>(
        &'a self,
        request: BillingWorkspaceHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .workspace(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: BillingCommandHttpRequest<'a>,
    ) -> BillingHttpFuture<'a, BillingHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let command = request.command;
            self.service
                .command(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> BillingTargetRolloutHttpBackend for BillingBackend<C> {
    fn approve<'a>(
        &'a self,
        request: BillingTargetRolloutHttpRequest<'a>,
    ) -> BillingTargetRolloutHttpFuture<'a, BillingTargetRolloutHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .target_rollout(request.host, proof, move || Ok(selection), request.command)
                .await
                .map(|response| BillingTargetRolloutHttpResponse {
                    data: response.data,
                    context: response.context,
                })
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> BillingTargetProviderHttpBackend for BillingBackend<C> {
    fn execute<'a>(
        &'a self,
        request: BillingTargetProviderHttpRequest<'a>,
    ) -> BillingTargetProviderHttpFuture<'a, BillingTargetProviderHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .target_provider(request.host, proof, move || Ok(selection), request.command)
                .await
                .map(|response| BillingTargetProviderHttpResponse {
                    data: response.data,
                    context: response.context,
                })
                .map_err(business)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_storage_limit_defaults_and_rejects_invalid_deployment_values() {
        assert_eq!(parse_beta_storage_limit(None).unwrap(), 1_073_741_824);
        assert_eq!(
            parse_beta_storage_limit(Some(" 20000000000 ")).unwrap(),
            20_000_000_000
        );
        for invalid in ["", "0", "-1", "abc", "1.5", "9007199254740992"] {
            let error = parse_beta_storage_limit(Some(invalid))
                .err()
                .expect("invalid trusted storage caps fail closed");
            assert_eq!(error.code, "BILLING_UNAVAILABLE");
        }
    }
}
