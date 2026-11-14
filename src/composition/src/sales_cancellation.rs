//! Native Sales confirmed-order cancellation composition.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use erpx_server_sales::public::sales_order_cancellation::{
    PgSalesCancellation, SalesCancellationActor, SalesCancellationCommand,
    cancel_confirmed_sales_order,
};
use erpx_server_sales::public::sales_order_cancellation_http::{
    SalesOrderCancellationHttpBackend, SalesOrderCancellationHttpFuture,
    SalesOrderCancellationHttpRequest, SalesOrderCancellationHttpResponse,
    SalesOrderCancellationHttpTarget,
};
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

#[derive(Clone, Copy)]
struct SalesCancellationSources;

impl HoldingWorkSourcePreparation for SalesCancellationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SalesCancellationConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SalesCancellationConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn actor(current: &AuthorizedWorkContext) -> Result<SalesCancellationActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let iam = current.command_iam_metadata();
    Ok(SalesCancellationActor {
        user_id: current.user_id().as_str().to_owned(),
        person_user_ids: iam
            .person_user_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selected.revision()),
        assignment_version: Some(assignment.version),
        permissions: iam.permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|v| v.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|v| v.as_str()),
        "assignmentId": selected.assignment().id.as_str(),
        "revision": selected.revision(),
        "assignmentVersion": selected.assignment().version,
    })
}

pub struct SalesCancellationService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SalesCancellationConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SalesCancellationService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SalesCancellationConfiguration,
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

    pub async fn cancel(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        order_id: String,
        expected_version: i64,
        key: String,
    ) -> Result<SalesOrderCancellationHttpResponse, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
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
                    permission: "records.write".into(),
                    module: Some("sales".into()),
                    destination: "sales".into(),
                    api: true,
                },
                SalesCancellationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let order_id = order_id.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = actor(current)?;
                        let command = SalesCancellationCommand {
                            key,
                            order_id,
                            expected_version,
                        };
                        let mut repository = PgSalesCancellation::new(connection);
                        let receipt = cancel_confirmed_sales_order(
                            &actor,
                            &command,
                            &mut repository,
                            clock.now(),
                        )
                        .await?;
                        Ok(SalesOrderCancellationHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct SalesCancellationBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<SalesCancellationService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SalesCancellationBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<SalesCancellationService<C>>,
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

impl<C: Clock + Send + Sync + 'static> SalesOrderCancellationHttpBackend
    for SalesCancellationBackend<C>
{
    fn cancel<'a>(
        &'a self,
        request: SalesOrderCancellationHttpRequest<'a>,
    ) -> SalesOrderCancellationHttpFuture<'a, SalesOrderCancellationHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .cancel(
                    request.host,
                    proof,
                    selection,
                    request.order_id.to_owned(),
                    request.expected_version,
                    request.idempotency_key.to_owned(),
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
    backend: Arc<dyn SalesOrderCancellationHttpBackend>,
) -> SalesOrderCancellationHttpTarget {
    SalesOrderCancellationHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}
