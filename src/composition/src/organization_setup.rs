//! Native Organization setup assembly. The retained Account/Holding authority
//! is supplied by the selected coordinator; this module only projects the
//! current actor, runs the owner unit and hands the bounded reply to the
//! owner-local HTTP ingress.
use crate::{
    business_http::BusinessScopeSlugs,
    organization_nodes::{OrganizationSources, organization_actor},
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_organization::public::organization_setup::{
    ApplyTemplateInput, PgOrganizationSetup, TemplateInput, apply_template, install_levels,
    save_template,
};
use erpx_server_organization::public::organization_setup_http::{
    ApplyTemplateHttpRequest, InstallLevelsHttpRequest, OrganizationSetupHttpBackend,
    OrganizationSetupHttpFuture, SaveTemplateHttpRequest,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_UNAVAILABLE",
        "The organization command is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone)]
pub struct OrganizationSetupConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OrganizationSetupConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct OrganizationSetupService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: OrganizationSetupConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationSetupService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: OrganizationSetupConfiguration,
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
            destination: "people/organization".into(),
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
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn install_levels<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: &str,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let key = key.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("organization.manage")?;
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationSetup::new(connection);
                        install_levels(&mut ports, actor, &key, clock.as_ref())
                            .await
                            .map_err(Error::from)
                    })
                },
            )
            .await
    }

    pub async fn save_template<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: &str,
        id: Option<String>,
        template: TemplateInput,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let key = key.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let id = id.clone();
                    let template = template.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("organization.manage")?;
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationSetup::new(connection);
                        save_template(
                            &mut ports,
                            actor,
                            &key,
                            id.as_deref(),
                            template,
                            clock.as_ref(),
                        )
                        .await
                        .map_err(Error::from)
                    })
                },
            )
            .await
    }

    pub async fn apply_template<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        key: &str,
        id: String,
        template: ApplyTemplateInput,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let key = key.to_owned();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let id = id.clone();
                    let template = template.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("organization.manage")?;
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationSetup::new(connection);
                        apply_template(&mut ports, actor, &key, &id, template, clock.as_ref())
                            .await
                            .map_err(Error::from)
                    })
                },
            )
            .await
    }
}

pub struct OrganizationSetupBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<OrganizationSetupService<C>>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationSetupBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<OrganizationSetupService<C>>,
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
    ) -> Result<(DeviceProof, Option<String>, Option<String>), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((inbound.into_proof(), branch, department))
    }

    fn request(
        &self,
        proof: DeviceProof,
        branch: Option<String>,
        department: Option<String>,
    ) -> (DeviceProof, HoldingWorkspaceRequest) {
        let company = self.service.company().clone();
        (
            proof,
            HoldingWorkspaceRequest {
                company_id: company,
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: true,
            },
        )
    }
}

impl<C: Clock + Send + Sync + 'static> OrganizationSetupHttpBackend
    for OrganizationSetupBackend<C>
{
    fn install_levels<'a>(
        &'a self,
        request: InstallLevelsHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        Box::pin(async move {
            let (proof, branch, department) = self.context(request.headers)?;
            let (proof, selection) = self.request(proof, branch, department);
            self.service
                .install_levels(request.host, proof, move || Ok(selection), request.key)
                .await
                .map_err(business)
        })
    }

    fn save_template<'a>(
        &'a self,
        request: SaveTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        Box::pin(async move {
            let (proof, branch, department) = self.context(request.headers)?;
            let (proof, selection) = self.request(proof, branch, department);
            let id = request.template_id.map(str::to_owned);
            let input = request.input.clone();
            self.service
                .save_template(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key,
                    id,
                    input,
                )
                .await
                .map_err(business)
        })
    }

    fn apply_template<'a>(
        &'a self,
        request: ApplyTemplateHttpRequest<'a>,
    ) -> OrganizationSetupHttpFuture<'a> {
        Box::pin(async move {
            let (proof, branch, department) = self.context(request.headers)?;
            let (proof, selection) = self.request(proof, branch, department);
            let id = request.template_id.to_owned();
            let input = request.input.clone();
            self.service
                .apply_template(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.key,
                    id,
                    input,
                )
                .await
                .map_err(business)
        })
    }
}
