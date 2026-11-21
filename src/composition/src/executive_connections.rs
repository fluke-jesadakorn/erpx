//! Native Executive external-connection composition.
//!
//! Account session, selected Work authority and the exact registered Holding
//! are resolved here. Runtime owns the Control token/audit/rate-limit schema and
//! the Holding assignment/organization/engagement witnesses; this module only
//! assembles them and hands the owner application one request.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{
        CapturedHoldingMutation, parse_inbound_account_request, unique_request_header,
    },
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{
        CurrentWorkSelection, HoldingWorkContextInspector, HoldingWorkspaceRequest,
        WorkAssignmentFact, WorkScopeKind, WorkspaceLineageFact,
    },
};
use erpx_server_runtime::public::executive_connections::{
    ConnectionActor, ConnectionScope, ExecutiveConnectionsHttpBackend,
    ExecutiveConnectionsHttpCommandRequest, ExecutiveConnectionsHttpFuture,
    ExecutiveConnectionsHttpReadRequest, ExecutiveConnectionsHttpTarget, ExecutiveConnectionsStore,
    ExecutiveMcpRateLimitBackend, ExternalConnection, ExternalConnections, PgExecutiveConnections,
    commit_create, connections, consume_rate, hash_secret, is_revoke, mint_secret, parse_revoke,
    prepare_create, revoke, revoke_failed, token_secret,
};
use erpx_server_runtime::public::http_support::{HeaderMap, Router};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "EXECUTIVE_UNAVAILABLE",
        "Connection storage is unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "EXECUTIVE_CONNECTION_FORBIDDEN",
        "An active Executive access grant and eligible executive work role are required.",
    )
}

/// Retained admission codes collapse to the browser-visible connection denial.
fn admission(error: BusinessError) -> BusinessError {
    match error.code {
        "PERMISSION_REQUIRED"
        | "MODULE_UNAVAILABLE"
        | "ROLE_UNAVAILABLE"
        | "BETA_CAPABILITY_UNAVAILABLE"
        | "IDENTITY_SUSPENDED" => forbidden(),
        _ => error,
    }
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => admission(error),
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
pub struct ExecutiveConnectionSources;

impl HoldingWorkSourcePreparation for ExecutiveConnectionSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "executive.access".into(),
        module: None,
        destination: "executive/connections".into(),
        api: true,
    }
}

fn scope_kind(kind: WorkScopeKind) -> &'static str {
    match kind {
        WorkScopeKind::Holding => "holding",
        WorkScopeKind::Branch => "branch",
        WorkScopeKind::Department => "department",
    }
}

fn actor(
    user_id: &str,
    holding: &HoldingId,
    company_id: &CompanyId,
    assignment: &WorkAssignmentFact,
    workspace: &WorkspaceLineageFact,
) -> ConnectionActor {
    ConnectionActor {
        user_id: user_id.to_owned(),
        holding_id: holding.as_str().to_owned(),
        company_id: company_id.as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        assignment_version: assignment.version,
        role: assignment.role.clone(),
        scope_kind: scope_kind(assignment.scope_kind).to_owned(),
        scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
    }
}

fn actor_from_authority(
    current: &AuthorizedWorkContext,
    holding: &HoldingId,
) -> Result<ConnectionActor, BusinessError> {
    if current.holding_id() != holding {
        return Err(not_found());
    }
    let workspace = current.selection().workspace();
    Ok(actor(
        current.user_id().as_str(),
        holding,
        &workspace.company_id,
        current.selection().assignment(),
        workspace,
    ))
}

fn actor_from_selection(selection: &CurrentWorkSelection, holding: &HoldingId) -> ConnectionActor {
    let workspace = selection.workspace();
    actor(
        selection.assignment().user_id.as_str(),
        holding,
        &workspace.company_id,
        selection.assignment(),
        workspace,
    )
}

fn list_value(
    can_create: bool,
    scopes: Vec<ConnectionScope>,
    connections: Vec<ExternalConnection>,
) -> Value {
    serde_json::to_value(ExternalConnections {
        can_create,
        scopes,
        connections,
    })
    .unwrap_or(Value::Null)
}

#[derive(Clone)]
pub struct ExecutiveConnectionsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    inspector: Arc<HoldingWorkContextInspector>,
    store: Arc<PgExecutiveConnections>,
    holding: HoldingId,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ExecutiveConnectionsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        inspector: Arc<HoldingWorkContextInspector>,
        store: Arc<PgExecutiveConnections>,
        holding: HoldingId,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            inspector,
            store,
            holding,
            clock,
        }
    }

    /// Whether this selected authority may create another connection: the same
    /// `executive.access` admission and emergency exclusion as the old route.
    async fn admitted(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
    ) -> Result<bool, BusinessError> {
        let result = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || Ok(workspace),
                access(),
                ExecutiveConnectionSources,
                |_connection, current| {
                    Box::pin(async move { Ok(current.emergency_until().is_none()) })
                },
            )
            .await;
        match result {
            Ok(admitted) => Ok(admitted),
            Err(PgTransactionError::Business(error))
                if matches!(
                    error.code,
                    "PERMISSION_REQUIRED"
                        | "BETA_CAPABILITY_UNAVAILABLE"
                        | "EMERGENCY_ACCESS_REQUIRED"
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(business(error)),
        }
    }

    pub async fn list(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
    ) -> Result<Value, BusinessError> {
        let selection = self
            .inspector
            .inspect(host, proof.clone(), workspace.clone())
            .await
            .map_err(business)?;
        let actor = actor_from_selection(&selection, &self.holding);
        let now = self.clock.now().get();
        let connections = connections(self.store.as_ref(), now, &actor).await?;
        let can_create =
            self.admitted(host, proof, workspace).await? && self.store.eligible(&actor).await?;
        let scopes = if can_create {
            self.store.scopes(&actor).await?
        } else {
            Vec::new()
        };
        Ok(list_value(can_create, scopes, connections))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        host: &str,
        proof: DeviceProof,
        recheck: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        key: &str,
        raw: &Value,
        secret: &str,
        secret_hash: &str,
    ) -> Result<Value, BusinessError> {
        let now = self.clock.now().get();
        let holding = self.holding.clone();
        let store = self.store.clone();
        let key = key.to_owned();
        let raw = raw.clone();
        let secret = secret.to_owned();
        let secret_hash = secret_hash.to_owned();
        let recheck_workspace = workspace.clone();
        let prepared = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || Ok(workspace),
                access(),
                ExecutiveConnectionSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    let store = store.clone();
                    let key = key.clone();
                    let raw = raw.clone();
                    let secret = secret.clone();
                    let secret_hash = secret_hash.clone();
                    Box::pin(async move {
                        let actor = actor_from_authority(current, &holding)?;
                        Ok(prepare_create(
                            store.as_ref(),
                            now,
                            &actor,
                            &raw,
                            &key,
                            &secret,
                            &secret_hash,
                        )
                        .await?)
                    })
                },
            )
            .await
            .map_err(business)?;
        // The Control token and its final audit commit under one principal lock.
        commit_create(self.store.as_ref(), &prepared).await?;
        // A permission/identity change in the same request must not release the
        // secret; the just-persisted connection is revoked before the response.
        let holding = self.holding.clone();
        let checked = self
            .authorizer
            .execute_with_sources_deferred(
                host,
                recheck,
                move || Ok(recheck_workspace),
                access(),
                ExecutiveConnectionSources,
                move |_connection, current| {
                    let holding = holding.clone();
                    Box::pin(async move {
                        actor_from_authority(current, &holding)?;
                        Ok(())
                    })
                },
            )
            .await;
        if let Err(error) = checked {
            let _ = revoke_failed(
                self.store.as_ref(),
                &prepared.response.id,
                self.clock.now().get(),
            )
            .await;
            return Err(business(error));
        }
        serde_json::to_value(&prepared.response).map_err(|_| unavailable())
    }

    pub async fn revoke(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: HoldingWorkspaceRequest,
        id: &str,
    ) -> Result<Value, BusinessError> {
        let selection = self
            .inspector
            .inspect(host, proof, workspace)
            .await
            .map_err(business)?;
        let actor = actor_from_selection(&selection, &self.holding);
        revoke(self.store.as_ref(), self.clock.now().get(), &actor, id).await?;
        Ok(json!({"id": id}))
    }
}

pub struct ExecutiveConnectionsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ExecutiveConnectionsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ExecutiveConnectionsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ExecutiveConnectionsService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    /// Exact registered Holding audience, application origin and rendered Work
    /// selection. A second independent proof is parsed for the post-commit
    /// authority recheck; neither proof is retained.
    fn context(
        &self,
        headers: &HeaderMap,
        mutation: bool,
    ) -> Result<(DeviceProof, DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        if unique_request_header(headers, "origin")?
            .is_some_and(|value| value != inbound.expected_origin())
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use the application origin.",
            ));
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let mut workspace =
            CapturedHoldingMutation::capture(headers, branch, department).resolve()?;
        workspace.mutation = mutation;
        let recheck = parse_inbound_account_request(&self.policy, headers)?.into_proof();
        Ok((inbound.into_proof(), recheck, workspace))
    }
}

impl<C: Clock + Send + Sync + 'static> ExecutiveConnectionsHttpBackend
    for ExecutiveConnectionsBackend<C>
{
    fn read<'a>(
        &'a self,
        request: ExecutiveConnectionsHttpReadRequest<'a>,
    ) -> ExecutiveConnectionsHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, _, workspace) = self.context(request.headers, false)?;
            self.service.list(request.host, proof, workspace).await
        })
    }

    fn command<'a>(
        &'a self,
        request: ExecutiveConnectionsHttpCommandRequest<'a>,
    ) -> ExecutiveConnectionsHttpFuture<'a, Value> {
        Box::pin(async move {
            if is_revoke(request.body) {
                let id = parse_revoke(request.body)?;
                let (proof, _, workspace) = self.context(request.headers, true)?;
                return self
                    .service
                    .revoke(request.host, proof, workspace, &id)
                    .await;
            }
            if !request.body.is_object() {
                return Err(BusinessError::invalid(
                    "CONNECTION_INPUT_INVALID",
                    "Use a connection object.",
                ));
            }
            let (proof, recheck, workspace) = self.context(request.headers, true)?;
            let secret = mint_secret().map_err(|_| unavailable())?;
            let secret_hash = hash_secret(&secret);
            self.service
                .create(
                    request.host,
                    proof,
                    recheck,
                    workspace,
                    request.key,
                    request.body,
                    &secret,
                    &secret_hash,
                )
                .await
        })
    }
}

pub struct ExecutiveMcpRateLimit<C> {
    control: DatabasePool,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ExecutiveMcpRateLimit<C> {
    pub fn new(control: DatabasePool, clock: Arc<C>) -> Self {
        Self { control, clock }
    }
}

impl<C: Clock + Send + Sync + 'static> ExecutiveMcpRateLimitBackend for ExecutiveMcpRateLimit<C> {
    fn consume<'a>(
        &'a self,
        authorization: Option<&'a str>,
    ) -> ExecutiveConnectionsHttpFuture<'a, i64> {
        Box::pin(async move {
            let secret = authorization.and_then(token_secret).ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unauthenticated,
                    "INVALID_TOKEN",
                    "This Executive token is invalid, expired or revoked.",
                )
            })?;
            consume_rate(&self.control, &hash_secret(secret), self.clock.now().get()).await
        })
    }
}

pub fn executive_connections_target<C>(
    host: String,
    holding_slug: String,
    backend: Arc<ExecutiveConnectionsBackend<C>>,
) -> ExecutiveConnectionsHttpTarget
where
    C: Clock + Send + Sync + 'static,
{
    ExecutiveConnectionsHttpTarget {
        host,
        holding_slug,
        backend,
    }
}

/// The single private MCP budget route. The bearer secret authenticates itself;
/// no browser session or forwarded user id is accepted.
pub fn executive_mcp_rate_limit_router<C>(control: DatabasePool, clock: Arc<C>) -> Router
where
    C: Clock + Send + Sync + 'static,
{
    erpx_server_runtime::public::executive_connections::executive_mcp_rate_limit_router(Arc::new(
        ExecutiveMcpRateLimit::new(control, clock),
    ))
}
