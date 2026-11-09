//! Native Manage ERPX onboarding assembly. Current Account/Holding authority
//! remains with the selected coordinator; owner ports supply the onboarding
//! view facts.
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
use erpx_server_manage_erpx::public::onboarding::{
    OnboardingActor, OnboardingView, read_onboarding_view,
};
use erpx_server_manage_erpx::public::onboarding_http::{
    ManageErpxHttpBackend, ManageErpxHttpFuture, ManageErpxHttpResponse,
    ManageErpxOnboardingRequest, manage_erpx_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MANAGE_ERPX_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANAGE_ERPX_UNAVAILABLE",
        "The Manage ERPX workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct ManageErpxSources;

impl HoldingWorkSourcePreparation for ManageErpxSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct ManageErpxConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ManageErpxConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn manage_actor(current: &AuthorizedWorkContext) -> OnboardingActor {
    OnboardingActor {
        user_id: current.user_id().clone(),
        email_verified: true,
    }
}

pub struct ManageErpxService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ManageErpxConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ManageErpxService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ManageErpxConfiguration,
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
            permission: "manage_erpx.access".into(),
            module: Some("manage_erpx".into()),
            destination: "manage_erpx".into(),
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

    pub async fn onboarding_view<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<ManageErpxHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                ManageErpxSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("manage_erpx.access")?;
                        let actor = manage_actor(current);
                        let view: OnboardingView =
                            read_onboarding_view(connection, &actor, true).await?;
                        let data = view.value();
                        Ok(ManageErpxHttpResponse {
                            data,
                            context: serde_json::json!({
                                "userId": current.user_id().as_str(),
                            }),
                        })
                    })
                },
            )
            .await
    }
}

pub struct ManageErpxBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ManageErpxService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ManageErpxBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ManageErpxService<C>>,
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

impl<C: Clock + Send + Sync + 'static> ManageErpxHttpBackend for ManageErpxBackend<C> {
    fn onboarding_view<'a>(
        &'a self,
        request: ManageErpxOnboardingRequest<'a>,
    ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .onboarding_view(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(
    targets: Vec<erpx_server_manage_erpx::public::onboarding_http::ManageErpxHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    manage_erpx_router(targets)
}
