//! Native Support service-team and auxiliary-read composition.
//!
//! Account session, current IAM authority and the serializable Holding unit are
//! resolved here; business rules remain in the Support owner adapters.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof,
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
use erpx_server_support::public::service_teams::{
    PgServiceTeams, ServiceTeamActor, ServiceTeamPreview, ServiceTeamResult, preview, save,
};
use erpx_server_support::public::service_teams_http::{
    ServiceTeamHttpBackend, SupportHttpFuture, ServiceTeamListHttpRequest,
    ServiceTeamOptionsHttpRequest, ServiceTeamPreviewHttpRequest, ServiceTeamSaveHttpRequest,
};
use erpx_server_support::public::support_auxiliary::{
    PgSupportAuxiliaryReads, SupportAuxiliaryReadPorts, SupportAuxiliaryScope,
    SupportSalesWorkspace, SupportTicketSourceSnapshot, SupportPolicySourceSnapshot,
    SupportAlertSourceSnapshot, SupportIncidentSourceSnapshot, SupportSessionSourceSnapshot,
};
use erpx_server_support::public::support_auxiliary_http::{
    SupportAuxiliaryHttpBackend, SupportAuxiliaryReadHttpRequest,
    SupportAuxiliarySourceKind, SupportAuxiliarySourceReply, SupportAuxiliaryWorkspaceHttpRequest,
};
use erpx_server_support::public::support_ticket_read::SupportReadScope as OwnerReadScope;
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPORT_UNAVAILABLE",
        "The Support workspace is unavailable.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct SupportSources;

impl HoldingWorkSourcePreparation for SupportSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct SupportWorkspaceConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl SupportWorkspaceConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

pub struct SupportWorkspaceService<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: SupportWorkspaceConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportWorkspaceService<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: SupportWorkspaceConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            policy,
            scopes,
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.configuration.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }

    async fn unit<Action, Output>(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
        permission: &'static str,
        action: Action,
    ) -> Result<Output, Error>
    where
        Action: for<'a, 'c> Fn(
                &'a mut PgConnection,
                &'a AuthorizedWorkContext,
                &'a (dyn Clock + Sync),
            ) -> PgWorkFuture<'a, Output, BusinessError>
            + Clone
            + Send
            + Sync
            + 'static,
        Output: Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                move || Ok(selection),
                WorkAuthorizationRequest {
                    permission: permission.into(),
                    module: None,
                    destination: "support".into(),
                    api: true,
                },
                SupportSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let action = action.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_SCOPE_REQUIRED",
                                "Choose the current authorized Company workspace.",
                            )
                            .into());
                        }
                        action(connection, current, clock.as_ref()).await
                    })
                },
            )
            .await
    }
}

fn service_team_actor(current: &AuthorizedWorkContext) -> ServiceTeamActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    ServiceTeamActor {
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        holding_owner: current.has_permission("organization.manage"),
    }
}

fn read_scope(current: &AuthorizedWorkContext) -> OwnerReadScope {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    OwnerReadScope {
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        role: assignment.role.clone(),
        support_read: current.has_permission("support.read"),
        records_read: current.has_permission("records.read"),
        support_create: current.has_permission("support.create"),
        support_work: current.has_permission("support.work"),
        support_manage: current.has_permission("support.manage"),
        commerce_read: current.has_permission("commerce.read"),
    }
}

fn auxiliary_scope(current: &AuthorizedWorkContext) -> SupportAuxiliaryScope {
    SupportAuxiliaryScope {
        read: read_scope(current),
        records_write: current.has_permission("records.write"),
        support_create: current.has_permission("support.create"),
    }
}

fn authority_json(current: &AuthorizedWorkContext) -> String {
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": current.selection().workspace().company_id.as_str(),
        "assignmentId": current.selection().assignment().id.as_str(),
        "revision": current.selection().revision(),
    })
    .to_string()
}

pub struct SupportWorkspaceBackend<C> {
    service: Arc<SupportWorkspaceService<C>>,
}

impl<C: Clock + Send + Sync + 'static> SupportWorkspaceBackend<C> {
    pub fn new(service: Arc<SupportWorkspaceService<C>>) -> Self {
        Self { service }
    }
}

impl<C: Clock + Send + Sync + 'static> ServiceTeamHttpBackend for SupportWorkspaceBackend<C> {
    fn options<'a>(
        &'a self,
        request: ServiceTeamOptionsHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.service.context(request.headers)?;
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.read",
                    |connection, current, clock| {
                        Box::pin(async move {
                            let actor = service_team_actor(current);
                            let mut ports = PgServiceTeams::new(
                                connection,
                                authority_json(current),
                                clock.now().get(),
                            );
                            ports.lock_sources().await?;
                            ports.options(&actor).await.map_err(Into::into)
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn list<'a>(
        &'a self,
        request: ServiceTeamListHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value> {
        Box::pin(async move {
            let (proof, selection) = self.service.context(request.headers)?;
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.read",
                    |connection, current, clock| {
                        Box::pin(async move {
                            let actor = service_team_actor(current);
                            let mut ports = PgServiceTeams::new(
                                connection,
                                authority_json(current),
                                clock.now().get(),
                            );
                            ports.lock_sources().await?;
                            ports.list(&actor).await.map_err(Into::into)
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn preview<'a>(
        &'a self,
        request: ServiceTeamPreviewHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, ServiceTeamPreview> {
        Box::pin(async move {
            let (proof, selection) = self.service.context(request.headers)?;
            let id = request.id.map(str::to_owned);
            let body = request.body.clone();
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.read",
                    move |connection, current, clock| {
                        let id = id.clone();
                        let body = body.clone();
                        Box::pin(async move {
                            let actor = service_team_actor(current);
                            let mut ports = PgServiceTeams::new(
                                connection,
                                authority_json(current),
                                clock.now().get(),
                            );
                            ports.lock_sources().await?;
                            preview(&mut ports, &actor, id.as_deref(), &body).await.map_err(Into::into)
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn save<'a>(
        &'a self,
        request: ServiceTeamSaveHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, ServiceTeamResult> {
        Box::pin(async move {
            let (proof, mut selection) = self.service.context(request.headers)?;
            selection.mutation = true;
            let id = request.id.map(str::to_owned);
            let body = request.body.clone();
            let key = request.key.as_str().to_owned();
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.manage",
                    move |connection, current, clock| {
                        let id = id.clone();
                        let body = body.clone();
                        let key = key.clone();
                        Box::pin(async move {
                            let actor = service_team_actor(current);
                            let mut ports = PgServiceTeams::new(
                                connection,
                                authority_json(current),
                                clock.now().get(),
                            );
                            ports.lock_sources().await?;
                            let value = save(&mut ports, &actor, id.as_deref(), &body, &key)
                                .await
                                .map_err(Error::from)?;
                            let object = value.as_object().ok_or_else(|| {
                                Error::from(BusinessError::invalid(
                                    "SUPPORT_RESPONSE_INVALID",
                                    "Invalid Support result.",
                                ))
                            })?;
                            let id = object
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    Error::from(BusinessError::invalid(
                                        "SUPPORT_RESPONSE_INVALID",
                                        "Invalid Support result.",
                                    ))
                                })?;
                            let version = object
                                .get("version")
                                .and_then(Value::as_i64)
                                .ok_or_else(|| {
                                    Error::from(BusinessError::invalid(
                                        "SUPPORT_RESPONSE_INVALID",
                                        "Invalid Support result.",
                                    ))
                                })?;
                            Ok(ServiceTeamResult { id, version })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> SupportAuxiliaryHttpBackend
    for SupportWorkspaceBackend<C>
{
    fn workspace<'a>(
        &'a self,
        request: SupportAuxiliaryWorkspaceHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, SupportSalesWorkspace> {
        Box::pin(async move {
            let (proof, selection) = self.service.context(request.headers)?;
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.read",
                    |connection, current, _| {
                        Box::pin(async move {
                            let mut ports =
                                PgSupportAuxiliaryReads::new(connection, auxiliary_scope(current));
                            ports.workspace().await.map_err(Into::into)
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }

    fn source<'a>(
        &'a self,
        request: SupportAuxiliaryReadHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Option<SupportAuxiliarySourceReply>> {
        Box::pin(async move {
            let (proof, selection) = self.service.context(request.headers)?;
            let id = request.id.to_owned();
            let kind = request.kind;
            self.service
                .unit(
                    request.host,
                    proof,
                    selection,
                    "support.read",
                    move |connection, current, _| {
                        let id = id.clone();
                        Box::pin(async move {
                            let mut ports =
                                PgSupportAuxiliaryReads::new(connection, auxiliary_scope(current));
                            Ok(match kind {
                                SupportAuxiliarySourceKind::Ticket => ports
                                    .ticket(&id)
                                    .await?
                                    .map(SupportAuxiliarySourceReply::Ticket),
                                SupportAuxiliarySourceKind::Policy => ports
                                    .policy(&id)
                                    .await?
                                    .map(SupportAuxiliarySourceReply::Policy),
                                SupportAuxiliarySourceKind::Alert => ports
                                    .alert(&id)
                                    .await?
                                    .map(SupportAuxiliarySourceReply::Alert),
                                SupportAuxiliarySourceKind::Incident => ports
                                    .incident(&id)
                                    .await?
                                    .map(SupportAuxiliarySourceReply::Incident),
                                SupportAuxiliarySourceKind::Session => ports
                                    .session(&id)
                                    .await?
                                    .map(SupportAuxiliarySourceReply::Session),
                            })
                        })
                    },
                )
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _unused(
    _: Option<SupportTicketSourceSnapshot>,
    _: Option<SupportPolicySourceSnapshot>,
    _: Option<SupportAlertSourceSnapshot>,
    _: Option<SupportIncidentSourceSnapshot>,
    _: Option<SupportSessionSourceSnapshot>,
) {
}
