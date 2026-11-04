//! Native Organization node/tree, sharing, lifecycle and business-unit
//! assembly. Retained Account/Holding authority is supplied by the selected
//! coordinator; this module only projects the actor, runs the owner use case
//! and renders the bounded reply.
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_organization::public::organization_nodes::{
    ChangeFilters, ClockSource, NodeChangeInput, NodeDecisionInput, OrganizationCommand,
    OrganizationCommandActor, OrganizationWorkspace, PgOrganizationNodes, PreviewResult,
    cancel_change, preview_change, read_workspace, review_change, save_change,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use erpx_server_workflow::public::commands::CommandJson;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;
type CommandFuture<'a> = Pin<Box<dyn Future<Output = Result<Value, Error>> + Send + 'a>>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORGANIZATION_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

#[derive(Clone)]
pub struct OrganizationNodesConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OrganizationNodesConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}

#[derive(Clone, Copy)]
pub(crate) struct OrganizationSources;

impl HoldingWorkSourcePreparation for OrganizationSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OrganizationWorkspaceResponse {
    pub data: OrganizationWorkspace,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrganizationCommandResponse {
    pub data: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrganizationPreviewResponse {
    pub data: PreviewResult,
}

pub(crate) fn command_actor(current: &AuthorizedWorkContext) -> Result<OrganizationCommand, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(OrganizationCommand {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.as_ref().map(|id| id.as_str().into()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| id.as_str().into()),
        authority: Some(
            erpx_server_organization::public::organization_nodes::OrganizationAuthority {
                session_id: current.session_id().as_str().into(),
                revision: selected.revision(),
                assignment_version: assignment.version,
            },
        ),
        iam_json: CommandJson::from_serializable(&project_command_iam(current)?)?
            .as_str()
            .to_owned(),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|id| id.as_str().into()),
    })
}

pub(crate) fn organization_actor(
    current: &AuthorizedWorkContext,
) -> Result<OrganizationCommandActor, BusinessError> {
    Ok(OrganizationCommandActor {
        holding_id: current.holding_id().clone(),
        command: command_actor(current)?,
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
    })
}

fn filters(input: &std::collections::BTreeMap<String, String>) -> ChangeFilters {
    ChangeFilters::parse(input)
}

pub struct OrganizationNodeService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: OrganizationNodesConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OrganizationNodeService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: OrganizationNodesConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.configuration.company
    }

    fn access(write: bool) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: if write {
                "organization.manage".into()
            } else {
                "people.manage".into()
            },
            module: None,
            destination: "people/organization".into(),
            api: true,
        }
    }

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: std::collections::BTreeMap<String, String>,
    ) -> Result<OrganizationWorkspaceResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        let expected = company.clone();
        let holding = self.configuration.holding.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(false),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let expected = expected.clone();
                    let query = query.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != expected
                        {
                            return Err(scope().into());
                        }
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationNodes::new(connection);
                        let data = read_workspace(&mut ports, &actor, &filters(&query)).await?;
                        Ok(OrganizationWorkspaceResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn save<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<OrganizationCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(
            host,
            proof,
            input,
            prepare,
            move |ports, actor, value, key, clock| {
                Box::pin(async move {
                    let id = value
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned);
                    let submit = value
                        .get("submit")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let change = NodeChangeInput::parse(&value).map_err(Error::from)?;
                    save_change(ports, actor, key, id.as_deref(), change, submit, clock).await
                })
            },
        )
        .await
    }

    pub async fn preview<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<OrganizationPreviewResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.preview_command(host, proof, input, prepare).await
    }

    async fn preview_command<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<OrganizationPreviewResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        let company = self.configuration.company.clone();
        let expected = company.clone();
        let holding = self.configuration.holding.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(true),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let expected = expected.clone();
                    let prepare = prepare.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != expected
                        {
                            return Err(scope().into());
                        }
                        let (_key, value) = prepare()?;
                        let change = NodeChangeInput::parse(&value)?;
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationNodes::new(connection);
                        let data = preview_change(&mut ports, actor, change).await?;
                        Ok(OrganizationPreviewResponse { data })
                    })
                },
            )
            .await
    }

    pub async fn review<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        change_id: String,
    ) -> Result<OrganizationCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(
            host,
            proof,
            input,
            prepare,
            move |ports, actor, value, key, clock| {
                let change_id = change_id.clone();
                Box::pin(async move {
                    let decision = NodeDecisionInput::parse(&value).map_err(Error::from)?;
                    review_change(ports, actor, &change_id, decision, key, clock).await
                })
            },
        )
        .await
    }

    pub async fn cancel<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        change_id: String,
    ) -> Result<OrganizationCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(
            host,
            proof,
            input,
            prepare,
            move |ports, actor, value, key, clock| {
                let change_id = change_id.clone();
                Box::pin(async move {
                    let expected_version = value
                        .get("expectedVersion")
                        .and_then(Value::as_i64)
                        .filter(|value| *value >= 1)
                        .ok_or_else(|| {
                            BusinessError::invalid(
                                "VALIDATION_FAILED",
                                "Provide the change version.",
                            )
                        })
                        .map_err(Error::from)?;
                    cancel_change(ports, actor, &change_id, expected_version, key, clock).await
                })
            },
        )
        .await
    }

    async fn command<Input, Prepare, Action>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        action: Action,
    ) -> Result<OrganizationCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
        Action: for<'a, 'c> Fn(
                &'a mut PgOrganizationNodes<'c>,
                OrganizationCommandActor,
                Value,
                &'a str,
                &'a dyn ClockSource,
            ) -> CommandFuture<'a>
            + Clone
            + Send
            + Sync
            + 'static,
    {
        let company = self.configuration.company.clone();
        let expected = company.clone();
        let holding = self.configuration.holding.clone();
        let clock = self.clock.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(true),
                OrganizationSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let expected = expected.clone();
                    let clock = clock.clone();
                    let prepare = prepare.clone();
                    let action = action.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != expected
                        {
                            return Err(scope().into());
                        }
                        let (key, value) = prepare()?;
                        let actor = organization_actor(current)?;
                        let mut ports = PgOrganizationNodes::new(connection);
                        let data =
                            action(&mut ports, actor, value, key.as_str(), clock.as_ref()).await?;
                        Ok(OrganizationCommandResponse { data })
                    })
                },
            )
            .await
    }
}
