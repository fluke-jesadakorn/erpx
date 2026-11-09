//! Native Branding composition.
//!
//! The HTTP transport supplies only a bounded body and idempotency key. Current
//! Account/Holding authority, Branding policy, serializable snapshots, receipts,
//! audit and outbox remain in this composition unit.
use crate::brand_identity::{BrandSources, brand_actor};
use crate::business_http::BusinessScopeSlugs;
use erpx_server_brand::public::branding::{
    PgBranding, attach_brand_media, read_branding_workspace, update_brand_profile,
    update_storefront,
};
use erpx_server_brand::public::branding_http::{
    BrandingHttpBackend, BrandingHttpCommandRequest, BrandingHttpFuture, BrandingHttpReadRequest,
    BrandingHttpTarget, branding_http_router,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{holding_mutation_request, parse_inbound_account_request},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRANDING_UNAVAILABLE",
        "Branding is unavailable.",
    )
}
fn scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BRANDING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}
fn request_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BRANDING_REQUEST_INVALID",
        "Use one bounded Branding command.",
    )
}
fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone)]
pub struct BrandingConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl BrandingConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
    pub fn holding(&self) -> &HoldingId {
        &self.holding
    }
    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct BrandingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: BrandingConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> BrandingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: BrandingConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }
    fn access(write: bool) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: if write {
                "organization.manage".into()
            } else {
                "records.read".into()
            },
            module: Some("brand".into()),
            destination: "administration/brand".into(),
            api: true,
        }
    }
    pub async fn read(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
    ) -> Result<Value, Error> {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || Ok(input),
                Self::access(false),
                BrandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = brand_actor(current)?;
                        let mut ports = PgBranding::new(connection, &actor, clock.now().get());
                        let workspace = read_branding_workspace(&mut ports).await?;
                        serde_json::to_value(workspace)
                            .map_err(|_| PgTransactionError::Business(unavailable()))
                    })
                },
            )
            .await
    }
    pub async fn command(
        &self,
        host: &str,
        proof: DeviceProof,
        mut input: HoldingWorkspaceRequest,
        key: String,
        body: Value,
    ) -> Result<Value, Error> {
        input.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || Ok(input),
                Self::access(true),
                BrandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = brand_actor(current)?;
                        let mut ports = PgBranding::new(connection, &actor, clock.now().get());
                        if body.get("shortName").is_some()
                            || body.get("accentColor").is_some()
                            || body.get("removeLogo").is_some()
                        {
                            update_storefront(&mut ports, &body, &key).await
                        } else if body.get("attachmentId").is_some()
                            || body.get("variant").is_some()
                            || body.get("publicationId").is_some()
                        {
                            attach_brand_media(&mut ports, &body, &key).await
                        } else if body.get("scopeKind").is_some()
                            || body.get("documentMode").is_some()
                            || body.get("remove").is_some()
                        {
                            update_brand_profile(&mut ports, &body, &key).await
                        } else {
                            Err(request_invalid().into())
                        }
                    })
                },
            )
            .await
    }
}

pub struct BrandingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<BrandingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> BrandingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<BrandingService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }
    fn read_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.configuration.company.clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            holding_mutation_request(headers, branch, department)?,
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> BrandingHttpBackend for BrandingBackend<C> {
    fn read<'a>(
        &'a self,
        request: BrandingHttpReadRequest<'a>,
    ) -> BrandingHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.read_context(request.headers)?;
            self.service
                .read(request.host, proof, selection)
                .await
                .map_err(business)
        })
    }
    fn command<'a>(
        &'a self,
        request: BrandingHttpCommandRequest<'a>,
    ) -> BrandingHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            self.service
                .command(
                    request.host,
                    proof,
                    selection,
                    request.key.to_owned(),
                    request.body.clone(),
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<BrandingHttpTarget>) -> Result<axum::Router, BusinessError> {
    branding_http_router(targets)
}
