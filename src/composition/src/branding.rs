//! Native Branding composition.
//!
//! The HTTP transport supplies only a bounded body and idempotency key. Current
//! Account/Holding authority, Branding policy, serializable snapshots, receipts,
//! audit and outbox remain in this composition unit.
use crate::brand_identity::{BrandSources, brand_actor};
use crate::business_http::BusinessScopeSlugs;
use erpx_server_brand::public::branding::{
    BrandPresentationContext, PgBranding, attach_brand_media, list_brand_profiles,
    read_brand_profile_evidence, read_branding_workspace, read_storefront_app_brand,
    resolve_document_brand, resolve_workspace_brand, update_brand_profile, update_storefront,
};
use erpx_server_brand::public::branding_http::{
    BrandingHttpBackend, BrandingHttpCommandRequest, BrandingHttpFuture, BrandingHttpReadRequest,
    BrandingHttpTarget, BrandingPresentationHttpRequest, BrandingPresentationReadKind,
    branding_http_router,
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

/// Reproduce the retained `encodeURIComponent` calls in `workspaceBase`.
fn encode_uri_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(*byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Reproduce `workspaceBase(publicScope(actor))` from the selected scope so the
/// legacy attachment URLs stay `/api/v1{base}/attachments/{id}/content`.
fn workspace_api_base(
    holding_slug: &str,
    branch_slug: Option<&str>,
    department_slug: Option<&str>,
) -> String {
    let holding = format!("/holdings/{}", encode_uri_component(holding_slug));
    let base = match (branch_slug, department_slug) {
        (_, Some(department)) => {
            let branch = branch_slug
                .map(|branch| format!("/branches/{}", encode_uri_component(branch)))
                .unwrap_or_default();
            format!(
                "{holding}{branch}/departments/{}",
                encode_uri_component(department)
            )
        }
        (Some(branch), None) => format!("{holding}/branches/{}", encode_uri_component(branch)),
        (None, None) => holding,
    };
    format!("/api/v1{base}")
}

/// `attachmentsV2Readable()` from the Files owner feature flags: approved v2
/// media is readable when the platform is enabled, and the emergency rollback
/// keeps immutable evidence readable. The Brand owner renders the legacy
/// attachment URL only outside this state.
fn attachments_readable() -> bool {
    std::env::var("ERPX_ATTACHMENTS_V2").as_deref() == Ok("true")
        || std::env::var("ERPX_ATTACHMENTS_READ_ONLY").as_deref() == Ok("true")
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
    /// Retained Brand presentation read. The current Holding/Company authority,
    /// the legacy attachment readability and the workspace API base are resolved
    /// on this same unit; the owner renders the projection the switched consumers
    /// previously read from TypeScript SQL.
    pub async fn presentation(
        &self,
        host: &str,
        proof: DeviceProof,
        input: HoldingWorkspaceRequest,
        holding_slug: String,
        holding_name: String,
        kind: BrandingPresentationReadKind,
        branch_id: Option<String>,
        requested_mode: Option<String>,
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
                    let holding_slug = holding_slug.clone();
                    let holding_name = holding_name.clone();
                    let branch_id = branch_id.clone();
                    let requested_mode = requested_mode.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope_required().into());
                        }
                        let actor = brand_actor(current)?;
                        let workspace = current.selection().workspace();
                        let context = BrandPresentationContext::new(
                            &holding_name,
                            &holding_slug,
                            &workspace_api_base(
                                &holding_slug,
                                workspace.branch_slug.as_deref(),
                                workspace.department_slug.as_deref(),
                            ),
                            attachments_readable(),
                        )?;
                        let mut ports = PgBranding::new(connection, &actor, clock.now().get());
                        let data = match kind {
                            BrandingPresentationReadKind::Workspace => {
                                serde_json::to_value(resolve_workspace_brand(&mut ports, &context).await?)
                            }
                            BrandingPresentationReadKind::Document => serde_json::to_value(
                                resolve_document_brand(
                                    &mut ports,
                                    &context,
                                    branch_id.as_deref(),
                                    requested_mode.as_deref(),
                                )
                                .await?,
                            ),
                            BrandingPresentationReadKind::Profiles => {
                                serde_json::to_value(list_brand_profiles(&mut ports, &context).await?)
                            }
                            BrandingPresentationReadKind::Storefront => serde_json::to_value(
                                read_storefront_app_brand(&mut ports, &context).await?,
                            ),
                            BrandingPresentationReadKind::Evidence => serde_json::to_value(
                                read_brand_profile_evidence(
                                    &mut ports,
                                    branch_id.as_deref().unwrap_or_default(),
                                )
                                .await?,
                            ),
                        }
                        .map_err(|_| PgTransactionError::Business(unavailable()))?;
                        Ok(data)
                    })
                },
            )
            .await
    }
    pub async fn command(        &self,
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
    fn presentation<'a>(
        &'a self,
        request: BrandingPresentationHttpRequest<'a>,
    ) -> BrandingHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.read_context(request.headers)?;
            self.service
                .presentation(
                    request.host,
                    proof,
                    selection,
                    request.holding_slug.to_owned(),
                    request.holding_name.to_owned(),
                    request.kind,
                    request.branch_id.map(str::to_owned),
                    request.requested_mode.map(str::to_owned),
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
