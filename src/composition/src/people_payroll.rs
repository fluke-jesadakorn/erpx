//! Native People Payroll read assembly. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the bounded
//! payroll summary and run-list facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_people_payroll::public::payroll_summary::{
    PayrollReadActor, PgPayrollRead, payroll_runs, payroll_summary,
};
use erpx_server_people_payroll::public::payroll_summary_http::{
    PayrollReadHttpBackend, PayrollReadHttpFuture, PayrollReadHttpKind, PayrollReadHttpRequest,
    PayrollReadHttpResponse, PayrollReadHttpTarget, people_payroll_router,
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
        "PEOPLE_PAYROLL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PEOPLE_PAYROLL_UNAVAILABLE",
        "The People Payroll workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct PeoplePayrollSources;

impl HoldingWorkSourcePreparation for PeoplePayrollSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct PeoplePayrollConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl PeoplePayrollConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn payroll_actor(current: &AuthorizedWorkContext) -> Result<PayrollReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(PayrollReadActor {
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
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

pub struct PeoplePayrollService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: PeoplePayrollConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: PeoplePayrollConfiguration,
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
            permission: WorkPermissionRequirement::AnyOf(vec![
                "payroll.read".into(),
                "payroll.payment.read".into(),
            ]),
            module: Some("people_payroll".into()),
            destination: "people/payroll".into(),
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

    pub async fn summary<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        pay_month: Option<String>,
    ) -> Result<PayrollReadHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let pay_month = pay_month.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                PeoplePayrollSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let pay_month = pay_month.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("payroll.read")?;
                        let actor = payroll_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgPayrollRead::new(connection);
                        let rows = payroll_summary(&actor, &mut repository, pay_month.as_deref(), now)
                            .await?;
                        let data = serde_json::to_value(rows).map_err(|_| unavailable())?;
                        Ok(PayrollReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn runs<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<PayrollReadHttpResponse, Error>
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
                PeoplePayrollSources,
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
                        let actor = payroll_actor(current)?;
                        let now = clock.now();
                        let mut repository = PgPayrollRead::new(connection);
                        let rows = payroll_runs(&actor, &mut repository, now).await?;
                        let data = serde_json::to_value(rows).map_err(|_| unavailable())?;
                        Ok(PayrollReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct PeoplePayrollBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PeoplePayrollService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PeoplePayrollBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PeoplePayrollService<C>>,
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

impl<C: Clock + Send + Sync + 'static> PayrollReadHttpBackend for PeoplePayrollBackend<C> {
    fn read<'a>(
        &'a self,
        request: PayrollReadHttpRequest<'a>,
    ) -> PayrollReadHttpFuture<'a, PayrollReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.kind {
                PayrollReadHttpKind::Summary { pay_month } => self
                    .service
                    .summary(request.host, proof, move || Ok(selection), pay_month)
                    .await
                    .map_err(business),
                PayrollReadHttpKind::Runs => self
                    .service
                    .runs(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
            }
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<PayrollReadHttpTarget>) -> Result<axum::Router, BusinessError> {
    people_payroll_router(targets)
}
