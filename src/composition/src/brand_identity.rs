//! Native Brand identity assembly. Retained Account/Holding authority is
//! supplied by the selected coordinator; this module only projects the actor,
//! executes the owner use case and renders the bounded reply.
use erpx_server_brand::public::identity::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

type BrandActionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, PgTransactionError<BusinessError>>> + Send + 'a>>;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BRAND_SCOPE_REQUIRED",
        "Choose the current authorized Brand workspace.",
    )
}

fn rendered_integer(value: i64) -> Result<i64, BusinessError> {
    if (0..=9_007_199_254_740_991).contains(&value) {
        Ok(value)
    } else {
        Err(BusinessError::new(
            FailureKind::Unavailable,
            "BRAND_CONTEXT_UNAVAILABLE",
            "The current Brand work context is unavailable.",
        ))
    }
}

#[derive(Clone)]
pub struct BrandIdentityConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl BrandIdentityConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company_id(&self) -> &CompanyId {
        &self.company
    }
}

#[derive(Clone, Copy)]
struct BrandSources;

impl HoldingWorkSourcePreparation for BrandSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandRenderedWorkContext {
    pub user_id: String,
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    pub assignment_version: i64,
    pub revision: i64,
    pub role: String,
    pub assignment_scope_kind: String,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrandIdentityWorkspaceResponse {
    pub data: BrandIdentityWorkspace,
    pub context: BrandRenderedWorkContext,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrandIdentityCommandResponse {
    pub data: Value,
}

fn rendered(current: &AuthorizedWorkContext) -> Result<BrandRenderedWorkContext, BusinessError> {
    let selection = current.selection();
    let workspace = selection.workspace();
    let grant = selection.assignment();
    Ok(BrandRenderedWorkContext {
        user_id: current.user_id().as_str().into(),
        holding_id: current.holding_id().as_str().into(),
        company_id: workspace.company_id.as_str().into(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().into()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().into()),
        branch_slug: workspace.branch_slug.clone(),
        department_slug: workspace.department_slug.clone(),
        assignment_id: grant.id.as_str().into(),
        assignment_version: rendered_integer(grant.version)?,
        revision: rendered_integer(selection.revision())?,
        role: grant.role.clone(),
        assignment_scope_kind: match grant.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: rendered_integer(current.expires_at())?,
    })
}

fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: assignment.id.clone(),
        role: assignment.role.clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|id| CommandField::Value(id.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
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

fn brand_actor(current: &AuthorizedWorkContext) -> Result<BrandActor, BusinessError> {
    Ok(BrandActor::new(
        current.holding_id().as_str(),
        command_actor(current)?,
        current.modules().to_vec(),
        current.command_iam_metadata().permissions.clone(),
    ))
}

pub struct BrandIdentityService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: BrandIdentityConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> BrandIdentityService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: BrandIdentityConfiguration,
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
                "records.read".into()
            },
            module: None,
            destination: "brand/identity".into(),
            api: true,
        }
    }

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<BrandIdentityWorkspaceResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
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
            request.mutation = false;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::access(false),
                BrandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let expected = expected.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != expected
                        {
                            return Err(scope().into());
                        }
                        let actor = brand_actor(current)?;
                        let mut ports =
                            PgBrandIdentityRead::new(connection, &actor, clock.now().get());
                        let data = read_brand_identity_workspace(&mut ports).await?;
                        Ok(BrandIdentityWorkspaceResponse {
                            data,
                            context: rendered(current)?,
                        })
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
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(host, proof, input, prepare, move |ports, input, key| {
            Box::pin(async move { save_brand_identity_draft(ports, &input, key).await })
        })
        .await
    }

    pub async fn publish<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        draft_id: String,
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(host, proof, input, prepare, move |ports, input, key| {
            let draft_id = draft_id.clone();
            Box::pin(async move { publish_brand_identity(ports, &draft_id, &input, key).await })
        })
        .await
    }

    pub async fn rollback<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(host, proof, input, prepare, move |ports, input, key| {
            Box::pin(async move { rollback_brand_identity(ports, &input, key).await })
        })
        .await
    }

    pub async fn change_lock<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(host, proof, input, prepare, move |ports, input, key| {
            Box::pin(async move { change_brand_category_lock(ports, &input, key).await })
        })
        .await
    }

    pub async fn cancel<Input, Prepare>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        draft_id: String,
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
    {
        self.command(host, proof, input, prepare, move |ports, input, key| {
            let draft_id = draft_id.clone();
            Box::pin(async move { cancel_brand_draft(ports, &draft_id, &input, key).await })
        })
        .await
    }

    async fn command<Input, Prepare, Action>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        prepare: Prepare,
        action: Action,
    ) -> Result<BrandIdentityCommandResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
        Prepare: Fn() -> Result<(CommandKey, Value), BusinessError> + Clone + Send + Sync + 'static,
        Action: for<'a, 'b> Fn(&'a mut PgBrandIdentity<'b>, Value, &'a str) -> BrandActionFuture<'a>
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
                BrandSources,
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
                        let actor = brand_actor(current)?;
                        let age = clock.now().get();
                        let mut ports = PgBrandIdentity::new(connection, &actor, age);
                        let result = action(&mut ports, value, key.as_str()).await?;
                        Ok(BrandIdentityCommandResponse { data: result })
                    })
                },
            )
            .await
    }
}
