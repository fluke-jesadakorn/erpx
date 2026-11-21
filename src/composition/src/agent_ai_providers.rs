//! Native Agent AI-provider composition.
//!
//! Account session, current work authority and the serializable Holding unit are
//! resolved here; provider settings and selection rules stay in the Agent owner
//! crate. Credential sealing and provider transport remain A1-deferred and the
//! owner adapter fails closed for those ports.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_agent::public::providers::{
    AiActor, AiProviderHttpBackend, AiProviderHttpCommandRequest, AiProviderHttpFuture,
    AiProviderHttpReadRequest, AiProviderHttpTarget, PgAiProvider, activate_ai_connection,
    ai_provider_overview, disable_ai_connection, discover_ai_models, discover_saved_ai_models,
    save_ai_connection, select_ai_providers, test_ai_connection,
};
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
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "AI_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "AI_PROVIDERS_UNAVAILABLE",
        "The native AI provider service is unavailable.",
    )
}

fn key_required() -> BusinessError {
    BusinessError::invalid("COMMAND_KEY_REQUIRED", "Idempotency-Key is required.")
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct AiProviderSources;

impl HoldingWorkSourcePreparation for AiProviderSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Copy)]
pub struct AiProviderRuntime {
    pub deployment_available: bool,
    pub key_storage_ready: bool,
    pub production: bool,
}

impl AiProviderRuntime {
    /// Deployment facts that the retained Administration page reads. Missing
    /// configuration is reported as unavailable, never inferred.
    pub fn from_environment() -> Self {
        let value = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            deployment_available: value("AI_BASE_URL").is_some()
                && value("AI_MODEL").is_some()
                && value("AI_API_KEY").is_some(),
            key_storage_ready: value("AI_CREDENTIAL_KEYS").is_some(),
            production: value("ERPX_ENVIRONMENT").as_deref() == Some("production")
                || value("NODE_ENV").as_deref() == Some("production"),
        }
    }
}

#[derive(Clone)]
pub struct AiProviderConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl AiProviderConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn ai_actor(
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
) -> Result<AiActor, BusinessError> {
    let command = crate::finance_journal_actor::command_actor(current)?;
    let roles = vec![command.role.clone()];
    Ok(AiActor::new(holding.as_str(), command, roles))
}

pub struct AiProviderService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: AiProviderConfiguration,
    runtime: AiProviderRuntime,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> AiProviderService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: AiProviderConfiguration,
        runtime: AiProviderRuntime,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            runtime,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "ai.manage".into(),
            module: Some("agent".into()),
            destination: "ai-providers".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
        mutation: bool,
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
            request.mutation = mutation;
            Ok(request)
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        path: Vec<String>,
    ) -> Result<Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let runtime = self.runtime;
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input, false),
                Self::access(),
                AiProviderSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let path = path.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("ai.manage")?;
                        let actor = ai_actor(current, &holding)?;
                        let now = clock.now().get();
                        let mut ports = PgAiProvider::new(connection, &actor, now);
                        match path.as_slice() {
                            [] => Ok(ai_provider_overview(
                                &mut ports,
                                runtime.deployment_available,
                                runtime.key_storage_ready,
                                runtime.production,
                            )
                            .await?),
                            [id, resource] if resource == "models" => {
                                Ok(discover_saved_ai_models(&mut ports, id).await?)
                            }
                            _ => Err(not_found().into()),
                        }
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
        path: Vec<String>,
        key: Option<String>,
        body: Value,
    ) -> Result<Value, Error>
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
                self.input(input, true),
                Self::access(),
                AiProviderSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let path = path.clone();
                    let key = key.clone();
                    let body = body.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("ai.manage")?;
                        let actor = ai_actor(current, &holding)?;
                        let now = clock.now().get();
                        let mut ports = PgAiProvider::new(connection, &actor, now);
                        match path.as_slice() {
                            [] => {
                                let key = key.ok_or_else(key_required)?;
                                Ok(save_ai_connection(&mut ports, None, &body, &key).await?)
                            }
                            [selection] if selection == "selection" => {
                                let key = key.ok_or_else(key_required)?;
                                Ok(select_ai_providers(&mut ports, &body, &key).await?)
                            }
                            [discover] if discover == "discover" => {
                                Ok(discover_ai_models(&mut ports, &body).await?)
                            }
                            [id] => {
                                let key = key.ok_or_else(key_required)?;
                                Ok(save_ai_connection(&mut ports, Some(id), &body, &key).await?)
                            }
                            [id, action] if action == "test" => {
                                let key = key.ok_or_else(key_required)?;
                                Ok(test_ai_connection(&mut ports, id, &body, &key).await?)
                            }
                            [id, action] if action == "activate" => {
                                let key = key.ok_or_else(key_required)?;
                                Ok(activate_ai_connection(&mut ports, id, &body, &key).await?)
                            }
                            [id, action] if action == "disable" || action == "delete" => {
                                let key = key.ok_or_else(key_required)?;
                                let mut body = body;
                                if let Some(object) = body.as_object_mut() {
                                    object.insert("remove".into(), Value::Bool(action == "delete"));
                                }
                                Ok(disable_ai_connection(&mut ports, id, &body, &key).await?)
                            }
                            _ => Err(not_found().into()),
                        }
                    })
                },
            )
            .await
    }
}

pub struct AiProviderBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<AiProviderService<C>>,
}

impl<C: Clock + Send + Sync + 'static> AiProviderBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<AiProviderService<C>>,
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
        mutation: bool,
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
                mutation,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> AiProviderHttpBackend for AiProviderBackend<C> {
    fn read<'a>(
        &'a self,
        request: AiProviderHttpReadRequest<'a>,
    ) -> AiProviderHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, false)?;
            self.service
                .read(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.path.clone(),
                )
                .await
                .map_err(business)
        })
    }

    fn command<'a>(
        &'a self,
        request: AiProviderHttpCommandRequest<'a>,
    ) -> AiProviderHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, true)?;
            self.service
                .command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.path.clone(),
                    request.key.map(str::to_owned),
                    request.body.clone(),
                )
                .await
                .map_err(business)
        })
    }
}

pub fn ai_provider_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn AiProviderHttpBackend>,
) -> AiProviderHttpTarget {
    AiProviderHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}
