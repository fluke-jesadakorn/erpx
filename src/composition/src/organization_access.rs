//! Native Organization access and position-move composition.
//!
//! The selected Account/Holding context, current IAM authority, owner reads and
//! the caller-owned serializable unit stay in one place. Business rules remain
//! in the Organization crate; this module only binds transport to owner ports.
use crate::business_http::BusinessScopeSlugs;
use crate::organization_nodes::{OrganizationSources, organization_actor};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::AuthorizedWorkContext,
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_organization::public::organization_access::{
    OrganizationAccessActor, OrganizationAccessData, OrganizationAccessHttpBackend,
    OrganizationAccessHttpFuture, OrganizationAccessHttpRequest, OrganizationAccessHttpTarget,
    PgOrganizationAccess,
};
use erpx_server_organization::public::organization_nodes::OrganizationScopeKind;
use erpx_server_organization::public::position_moves::{
    CancelPositionMoveHttpRequest, PgPositionMoves, PositionMoveHttpBackend, PositionMoveHttpFuture,
    PositionMoveHttpTarget, PositionMoveListHttpRequest, PositionMovePreviewHttpRequest,
    PositionMoveSubmitHttpRequest,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_UNAVAILABLE",
        "Organization data is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone)]
pub struct OrganizationAccessConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OrganizationAccessConfiguration {
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

pub struct OrganizationAccessService<C> {
    authorizer: Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
    configuration: OrganizationAccessConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationAccessService<C> {
    pub fn new(
        authorizer: Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
        configuration: OrganizationAccessConfiguration,
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

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<OrganizationAccessData, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest {
                    permission: "people.manage".into(),
                    module: None,
                    destination: "people/access".into(),
                    api: true,
                },
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = access_actor(current).map_err(Error::from)?;
                        let mut ports = PgOrganizationAccess::new(connection);
                        let data = ports.read(&actor, clock.as_ref()).await?;
                        Ok(data)
                    })
                },
            )
            .await
    }
}

fn access_actor(current: &AuthorizedWorkContext) -> Result<OrganizationAccessActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    let scope = match assignment.scope_kind {
        WorkScopeKind::Holding => OrganizationScopeKind::Holding,
        WorkScopeKind::Branch => OrganizationScopeKind::Branch,
        WorkScopeKind::Department => OrganizationScopeKind::Department,
    };
    Ok(OrganizationAccessActor {
        actor_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        scope_kind: scope,
        assignment_scope: scope,
        may_read_iam: current.has_permission("iam.read"),
        may_manage_people: current.has_permission("people.manage"),
        may_manage_organization: current.has_permission("organization.manage"),
    })
}

pub struct OrganizationAccessBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<OrganizationAccessService<C>>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationAccessBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<OrganizationAccessService<C>>,
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
            return Err(scope());
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

impl<C: Clock + Send + Sync + 'static> OrganizationAccessHttpBackend
    for OrganizationAccessBackend<C>
{
    fn read<'a>(
        &'a self,
        request: OrganizationAccessHttpRequest<'a>,
    ) -> OrganizationAccessHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let data = self
                .service
                .read(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)?;
            record_value(&data).map_err(Into::into)
        })
    }
}

pub struct PositionMoveService<C> {
    authorizer: Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
    configuration: OrganizationAccessConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> PositionMoveService<C> {
    pub fn new(
        authorizer: Arc<erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer>,
        configuration: OrganizationAccessConfiguration,
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

    async fn unit<Input, Action>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        mutation: bool,
        action: Action,
    ) -> Result<serde_json::Value, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Action: for<'a, 'c> Fn(
                &'a mut PgPositionMoves<'c>,
                &'a AuthorizedWorkContext,
                &'a (dyn Clock + Sync),
            ) -> PgWorkFuture<'a, serde_json::Value, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let holding = self.configuration.holding().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = mutation;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest {
                    permission: "organization.manage".into(),
                    module: None,
                    destination: "people/organization/position-moves".into(),
                    api: true,
                },
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let action = action.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let mut ports = PgPositionMoves::new(connection);
                        action(&mut ports, current, clock.as_ref()).await
                    })
                },
            )
            .await
    }
}

pub struct PositionMoveBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<PositionMoveService<C>>,
}

impl<C: Clock + Send + Sync + 'static> PositionMoveBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<PositionMoveService<C>>,
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
            return Err(scope());
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

fn record_value<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, BusinessError> {
    serde_json::to_value(value).map_err(|_| unavailable())
}

impl<C: Clock + Send + Sync + 'static> PositionMoveHttpBackend for PositionMoveBackend<C> {
    fn list<'a>(&'a self, request: PositionMoveListHttpRequest<'a>) -> PositionMoveHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, false)?;
            let id = request.move_id.map(str::to_owned);
            self.service
                .unit(request.host, proof, move || Ok(selection), false, move |ports, current, _| {
                    let id = id.clone();
                    Box::pin(async move {
                        let actor = organization_actor(current).map_err(Error::from)?;
                        let rows = ports.list(&actor.projection(), id.as_deref()).await?;
                        record_value(&rows).map_err(Error::from)
                    })
                })
                .await
                .map_err(business)
        })
    }

    fn preview<'a>(
        &'a self,
        request: PositionMovePreviewHttpRequest<'a>,
    ) -> PositionMoveHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, false)?;
            let input = request.input.clone();
            self.service
                .unit(request.host, proof, move || Ok(selection), false, move |ports, current, clock| {
                    let input = input.clone();
                    Box::pin(async move {
                        let actor = organization_actor(current).map_err(Error::from)?;
                        let preview =
                            ports.preview(&actor.projection(), &input, clock).await?;
                        record_value(&preview).map_err(Error::from)
                    })
                })
                .await
                .map_err(business)
        })
    }

    fn submit<'a>(
        &'a self,
        request: PositionMoveSubmitHttpRequest<'a>,
    ) -> PositionMoveHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, true)?;
            let input = request.input.clone();
            let token = request.preview_token.to_owned();
            let expires = request.expires_at;
            let key = request.key.to_owned();
            self.service
                .unit(request.host, proof, move || Ok(selection), true, move |ports, current, clock| {
                    let input = input.clone();
                    let token = token.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let actor = organization_actor(current).map_err(Error::from)?;
                        let value = ports
                            .submit(&actor, &input, &token, expires, &key, clock)
                            .await?;
                        Ok(value)
                    })
                })
                .await
                .map_err(business)
        })
    }

    fn cancel<'a>(
        &'a self,
        request: CancelPositionMoveHttpRequest<'a>,
    ) -> PositionMoveHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers, true)?;
            let key = request.key.to_owned();
            let id = request.move_id.to_owned();
            let expected = request.expected_version;
            self.service
                .unit(request.host, proof, move || Ok(selection), true, move |ports, current, clock| {
                    let id = id.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        let actor = organization_actor(current).map_err(Error::from)?;
                        let value = ports.cancel(&actor, &id, expected, &key, clock).await?;
                        Ok(value)
                    })
                })
                .await
                .map_err(business)
        })
    }
}

/// Targets are constructed by the private API assembly.
pub fn access_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn OrganizationAccessHttpBackend>,
) -> OrganizationAccessHttpTarget {
    OrganizationAccessHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}

pub fn position_target(
    host: String,
    holding_slug: String,
    origin: String,
    backend: Arc<dyn PositionMoveHttpBackend>,
) -> PositionMoveHttpTarget {
    PositionMoveHttpTarget {
        host,
        holding_slug,
        origin,
        backend,
    }
}
