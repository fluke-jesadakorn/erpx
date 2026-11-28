//! Native Customer 360 timeline assembly. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the federated
//! customer timeline read.
use crate::business_http::BusinessScopeSlugs;
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    current_package_catalogue,
};
use erpx_customer_360_server::public::customer_activity::{
    ExecutiveCustomerActivityQuery, ExecutiveCustomerActivityRow, executive_customer_activity,
};
use erpx_customer_360_server::public::customer_activity_http::{
    CustomerActivityHttpBackend, CustomerActivityHttpFuture, CustomerActivityHttpReadRequest,
    CustomerActivityHttpTarget, customer_activity_http_router,
};
use erpx_customer_360_server::public::customer_activity_metric::{
    CustomerActivityMetricGroup, CustomerActivityMetricQuery, PgCustomerActivityMetric,
    customer_activity_metric,
};
use erpx_customer_360_server::public::customer_activity_metric_http::{
    CustomerActivityMetricHttpBackend, CustomerActivityMetricHttpFuture,
    CustomerActivityMetricHttpReadRequest, CustomerActivityMetricHttpTarget,
};
use erpx_customer_360_server::public::customer_directory_http::{
    CustomerDirectoryHttpBackend, CustomerDirectoryHttpFuture, CustomerDirectoryHttpReadRequest,
    CustomerDirectoryHttpTarget,
};
use erpx_customer_360_server::public::customer_timeline::{
    CustomerTimelineActor, CustomerTimelineQuery, CustomerTimelineSupportEntitlement,
    FinalRootEventSources, PgCustomerTimelineRead, customer_timeline,
};
use erpx_customer_360_server::public::customer_timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow, PgCustomerTimelineActivityRead,
    customer_timeline_activity,
};
use erpx_customer_360_server::public::customer_timeline_activity_http::{
    CustomerTimelineActivityHttpBackend, CustomerTimelineActivityHttpFuture,
    CustomerTimelineActivityHttpReadRequest, CustomerTimelineActivityHttpTarget,
    customer_timeline_activity_http_router,
};
use erpx_customer_360_server::public::customer_timeline_http::{
    CustomerTimelineHttpBackend, CustomerTimelineHttpFuture, CustomerTimelineHttpResponse,
    CustomerTimelineHttpTarget, customer_timeline_http_router,
};
use erpx_customer_360_server::public::directory::{
    CustomerDirectoryPage, CustomerDirectoryQuery, PgCustomerDirectoryRead, customer_directory_page,
};
use erpx_field_service_server::public::final_root_consumer::final_root_event_sql;
use erpx_field_service_server::public::final_root_projection::{
    inspect_final_root_projection_sources, lock_final_root_projection_sources,
};
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_platform_runtime_server::public::http_support::HeaderMap;
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{
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

/// Owner utility fence for the Field final-root sources embedded in the
/// Customer 360 timeline. The lock is acquired before the Holding snapshot;
/// no fact is read and no access is granted here.
#[derive(Clone, Copy)]
pub struct Customer360Sources;

impl HoldingWorkSourcePreparation for Customer360Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_final_root_projection_sources(c).await?;
            Ok(())
        })
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

    pub fn holding(&self) -> &HoldingId {
        &self.holding
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
    ) -> erpx_customer_360_server::public::customer_timeline::CustomerTimelineFuture<
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
                        let witness = inspect_final_root_projection_sources(connection).await?;
                        let (agreement_events, order_events) = final_root_event_sql(&witness);
                        let sources = FinalRootEventSources {
                            agreement_events,
                            order_events,
                        };
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
                        let page = {
                            let mut repository =
                                PgCustomerTimelineRead::with_sources(connection, sources);
                            let mut entitlement = SupportEntitlement { purchased };
                            customer_timeline(
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
                            })?
                        };
                        witness.require_current(connection).await?;
                        let data = serde_json::to_value(page)
                            .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(CustomerTimelineHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
    pub async fn activity(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        query: ExecutiveCustomerActivityQuery,
    ) -> Result<Vec<ExecutiveCustomerActivityRow>, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(move || Ok(input)),
                Self::access(),
                Customer360Sources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = customer_timeline_actor(current)?;
                        let witness = inspect_final_root_projection_sources(connection).await?;
                        let (agreement_events, order_events) = final_root_event_sql(&witness);
                        let sources = FinalRootEventSources {
                            agreement_events,
                            order_events,
                        };
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
                        let rows = {
                            let mut repository =
                                PgCustomerTimelineRead::with_sources(connection, sources);
                            let mut entitlement = SupportEntitlement { purchased };
                            executive_customer_activity(
                                &actor,
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
                            })?
                        };
                        witness.require_current(connection).await?;
                        Ok(rows)
                    })
                },
            )
            .await
    }

    pub async fn timeline_activity(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        query: CustomerTimelineActivityQuery,
    ) -> Result<Vec<CustomerTimelineActivityRow>, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(move || Ok(input)),
                Self::access(),
                Customer360Sources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = customer_timeline_actor(current)?;
                        let witness = inspect_final_root_projection_sources(connection).await?;
                        let (agreement_events, order_events) = final_root_event_sql(&witness);
                        let sources = FinalRootEventSources {
                            agreement_events,
                            order_events,
                        };
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
                        let rows = {
                            let mut repository =
                                PgCustomerTimelineActivityRead::with_sources(connection, sources);
                            let mut entitlement = SupportEntitlement { purchased };
                            customer_timeline_activity(
                                &actor,
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
                            })?
                        };
                        witness.require_current(connection).await?;
                        Ok(rows)
                    })
                },
            )
            .await
    }

    pub async fn directory_page(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        query: CustomerDirectoryQuery,
    ) -> Result<CustomerDirectoryPage, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(move || Ok(input)),
                Self::access(),
                Customer360Sources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = customer_timeline_actor(current)?;
                        let mut repository = PgCustomerDirectoryRead::new(connection);
                        customer_directory_page(&actor, &query, &mut repository, clock.now()).await
                    })
                },
            )
            .await
    }

    pub async fn activity_metric(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        query: CustomerActivityMetricQuery,
    ) -> Result<Vec<CustomerActivityMetricGroup>, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(move || Ok(input)),
                Self::access(),
                Customer360Sources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = customer_timeline_actor(current)?;
                        let witness = inspect_final_root_projection_sources(connection).await?;
                        let (agreement_events, order_events) = final_root_event_sql(&witness);
                        let sources = FinalRootEventSources {
                            agreement_events,
                            order_events,
                        };
                        let groups = {
                            let mut repository =
                                PgCustomerActivityMetric::with_sources(connection, sources);
                            let mut entitlement = SupportEntitlement { purchased: false };
                            customer_activity_metric(
                                &actor,
                                &query,
                                &mut repository,
                                &mut entitlement,
                                clock.now(),
                            )
                            .await?
                        };
                        witness.require_current(connection).await?;
                        Ok(groups)
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

impl<C: Clock + Send + Sync + 'static> CustomerDirectoryHttpBackend for Customer360Backend<C> {
    fn read<'a>(
        &'a self,
        request: CustomerDirectoryHttpReadRequest<'a>,
    ) -> CustomerDirectoryHttpFuture<'a, CustomerDirectoryPage> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .directory_page(request.host, proof, selection, request.query)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> CustomerActivityMetricHttpBackend for Customer360Backend<C> {
    fn read<'a>(
        &'a self,
        request: CustomerActivityMetricHttpReadRequest<'a>,
    ) -> CustomerActivityMetricHttpFuture<'a, Vec<CustomerActivityMetricGroup>> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .activity_metric(request.host, proof, selection, request.query)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> CustomerTimelineHttpBackend for Customer360Backend<C> {
    fn read<'a>(
        &'a self,
        request: erpx_customer_360_server::public::customer_timeline_http::CustomerTimelineHttpReadRequest<
            'a,
        >,
    ) -> CustomerTimelineHttpFuture<'a, CustomerTimelineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let customer_id = request.customer_id.to_owned();
            let query = request.query.clone();
            self.service
                .read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    customer_id,
                    query,
                )
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

impl<C: Clock + Send + Sync + 'static> CustomerActivityHttpBackend for Customer360Backend<C> {
    fn read<'a>(
        &'a self,
        request: CustomerActivityHttpReadRequest<'a>,
    ) -> CustomerActivityHttpFuture<'a, Vec<ExecutiveCustomerActivityRow>> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .activity(request.host, proof, selection, request.query.clone())
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> CustomerTimelineActivityHttpBackend
    for Customer360Backend<C>
{
    fn read<'a>(
        &'a self,
        request: CustomerTimelineActivityHttpReadRequest<'a>,
    ) -> CustomerTimelineActivityHttpFuture<'a, Vec<CustomerTimelineActivityRow>> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .timeline_activity(request.host, proof, selection, request.query.clone())
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _timeline_activity_router(
    targets: Vec<CustomerTimelineActivityHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    customer_timeline_activity_http_router(targets)
}

#[allow(dead_code)]
fn _activity_router(
    targets: Vec<CustomerActivityHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    customer_activity_http_router(targets)
}
