//! Native Commercial company package HTTP mount. Current Account/Holding
//! authority remains with the selected coordinator; owner ports supply Company
//! package overview and purchase admission facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CompanyPackageReadHttpBackend, CompanyPackageReadHttpFuture, CompanyPackageReadHttpRequest,
    CompanyPackageReadHttpResponse, CompanyPackageReadHttpKind, CompanyPackageReadHttpTarget,
    CompanyPurchaseAdmission, CompanyPackageOverview, CommercialCatalogue, CompanyPurchaseAdmissionPort, PgCompanyPurchaseReader,
    PurchaseRequest, company_package_overview, company_package_router, current_package_catalogue,
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
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "COMMERCIAL_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCIAL_UNAVAILABLE",
        "The Commercial workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CommercialSources;

impl HoldingWorkSourcePreparation for CommercialSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct CommercialConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CommercialConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
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

pub struct CommercialService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CommercialConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CommercialService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CommercialConfiguration,
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
            module: Some("commercial".into()),
            destination: "commercial".into(),
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
        kind: CompanyPackageReadHttpKind,
    ) -> Result<CompanyPackageReadHttpResponse, Error>
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
                CommercialSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let kind = kind.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let catalogue: CommercialCatalogue =
                            current_package_catalogue().map_err(|_| {
                                PgTransactionError::Business(unavailable())
                            })?;
                        let now: TimestampMillis = clock.now();
                        let data = match kind {
                            CompanyPackageReadHttpKind::Overview => {
                                let mut reader = PgCompanyPurchaseReader::new(
                                    current.holding_id().clone(),
                                    current.selection().workspace().company_id.clone(),
                                    connection,
                                    &catalogue,
                                );
                                let overview: CompanyPackageOverview =
                                    company_package_overview(&mut reader, &catalogue, &company, now)
                                        .await
                                        .map_err(|error| match error {
                                            PgTransactionError::Business(error) => {
                                                PgTransactionError::Business(error)
                                            }
                                            PgTransactionError::Persistence(_) => {
                                                PgTransactionError::Business(unavailable())
                                            }
                                        })?;
                                serde_json::to_value(overview)
                                    .map_err(|_| PgTransactionError::Business(unavailable()))?
                            }
                            CompanyPackageReadHttpKind::Admission {
                                capabilities,
                                purpose,
                                mode,
                            } => {
                                let mut reader = PgCompanyPurchaseReader::new(
                                    current.holding_id().clone(),
                                    current.selection().workspace().company_id.clone(),
                                    connection,
                                    &catalogue,
                                );
                                let admission: CompanyPurchaseAdmission = reader
                                    .inspect_admission(
                                        &PurchaseRequest {
                                            capabilities,
                                            purpose,
                                            mode,
                                        },
                                        now,
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
                                serde_json::to_value(admission)
                                    .map_err(|_| PgTransactionError::Business(unavailable()))?
                            }
                        };
                        Ok(CompanyPackageReadHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CommercialBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CommercialService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CommercialBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CommercialService<C>>,
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

impl<C: Clock + Send + Sync + 'static> CompanyPackageReadHttpBackend for CommercialBackend<C> {
    fn read<'a>(
        &'a self,
        request: CompanyPackageReadHttpRequest<'a>,
    ) -> CompanyPackageReadHttpFuture<'a, CompanyPackageReadHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let kind = request.kind.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), kind)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<CompanyPackageReadHttpTarget>) -> Result<axum::Router, BusinessError> {
    company_package_router(targets)
}
