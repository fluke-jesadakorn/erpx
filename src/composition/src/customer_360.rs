//! Native Customer 360 timeline assembly. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the federated
//! customer timeline read.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    current_package_catalogue,
};
use erpx_server_customer_360::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineQuery, CustomerTimelineSupportEntitlement,
    PgCustomerTimelineRead, customer_timeline,
};
use erpx_server_customer_360::public::customer_timeline_http::{
    CustomerTimelineHttpBackend, CustomerTimelineHttpFuture, CustomerTimelineHttpResponse,
    CustomerTimelineHttpTarget, customer_timeline_http_router,
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
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CUSTOMER_360_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_360_UNAVAILABLE",
        "The Customer 360 workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct Customer360Sources;

impl HoldingWorkSourcePreparation for Customer360Sources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct Customer360Configuration {
    holding: HoldingId,
    company: CompanyId,
}

impl Customer360Configuration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

struct SupportEntitlement {
    purchased: bool,
}

impl CustomerTimelineSupportEntitlement for SupportEntitlement {
    type Error = BusinessError;

    fn support_history<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_customer_360::public::customer_timeline::CustomerTimelineFuture<
        'a,
        bool,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.purchased) })
    }
}

fn customer_timeline_actor(
    current: &AuthorizedWorkContext,
) -> Result<CustomerTimelineActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CustomerTimelineActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        role: assignment.role.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        assignment_id: assignment.id.as_str().to_owned(),
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

pub struct Customer360Service<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: Customer360Configuration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> Customer360Service<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: Customer360Configuration,
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
            module: Some("customer_360".into()),
            destination: "customer_360".into(),
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

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        customer_id: String,
        query: CustomerTimelineQuery,
    ) -> Result<CustomerTimelineHttpResponse, Error>
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
                Customer360Sources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let customer_id = customer_id.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = customer_timeline_actor(current)?;
                        let purchased = if current.has_permission("support.read") {
                            let catalogue = current_package_catalogue()
                                .map_err(|_| PgTransactionError::Business(unavailable()))?;
                            let mut reader = PgCompanyPurchaseReader::new(
                                current.holding_id().clone(),
                                current.selection().workspace().company_id.clone(),
                                connection,
                                &catalogue,
                            );
                            let inspection = reader
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["support.tickets".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    clock.now(),
                                )
                                .await
                                .map_err(|error| match error {
                                    PgTransactionError::Business(error) => {
                                        PgTransactionError::Business(error)
                                    }
                                    PgTransactionError::Persistence(_) => {
                                        PgTransactionError::Business(unavailable())
                                    }
                                })?;
                            inspection.require_allowed().is_ok()
                        } else {
                            false
                        };
                        let mut repository = PgCustomerTimelineRead::new(connection);
                        let mut entitlement = SupportEntitlement { purchased };
                        let page = customer_timeline(
                            &actor,
                            &customer_id,
                            &query,
                            &mut repository,
                            &mut entitlement,
                            clock.now(),
                        )
                        .await
                        .map_err(|error| match error {
                            PgTransactionError::Business(error) => {
                                PgTransactionError::Business(error)
                            }
                            PgTransactionError::Persistence(_) => {
                                PgTransactionError::Business(unavailable())
                            }
                        })?;
                        let data = serde_json::to_value(page).map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(CustomerTimelineHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct Customer360Backend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<Customer360Service<C>>,
}

impl<C: Clock + Send + Sync + 'static> Customer360Backend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<Customer360Service<C>>,
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

impl<C: Clock + Send + Sync + 'static> CustomerTimelineHttpBackend for Customer360Backend<C> {
    fn read<'a>(
        &'a self,
        request: erpx_server_customer_360::public::customer_timeline_http::CustomerTimelineHttpReadRequest<
            'a,
        >,
    ) -> CustomerTimelineHttpFuture<'a, CustomerTimelineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let customer_id = request.customer_id.to_owned();
            let query = request.query.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), customer_id, query)
                .await
                .map_err(business)
        })
    }
}

// Retain the router constructor in feature-specific builds.
#[allow(dead_code)]
fn _router(targets: Vec<CustomerTimelineHttpTarget>) -> Result<axum::Router, BusinessError> {
    customer_timeline_http_router(targets)
}
