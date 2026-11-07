use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_records::public::record_context::{
    PgRecordContext, RecordContextActor, RecordContextGuard, RecordRow, record_context,
};
use erpx_server_records::public::record_context_http::{
    RecordsContextHttpBackend, RecordsContextHttpCommand, RecordsContextHttpFuture,
    RecordsContextHttpRequest, RecordsContextHttpResponse,
};
use erpx_server_records::public::record_registry::{
    PgRecordRegistry, RecordRegistryGuard, RecordView, RegistryQuery, record_registry_list,
    record_registry_page,
};
use erpx_server_records::public::record_registry_http::{
    RecordsRegistryHttpBackend, RecordsRegistryHttpCommand, RecordsRegistryHttpFuture,
    RecordsRegistryHttpRequest, RecordsRegistryHttpResponse,
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
        "RECORDS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECORDS_UNAVAILABLE",
        "The Records workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct RecordsSources;

impl HoldingWorkSourcePreparation for RecordsSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct RecordsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl RecordsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

#[derive(Clone, Copy)]
struct RecordsContextGuardImpl;

impl RecordContextGuard for RecordsContextGuardImpl {
    type Error = BusinessError;

    fn authorize_source<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }

    fn target_visible<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, bool, Self::Error> {
        Box::pin(async { Ok(true) })
    }
}

#[derive(Clone, Copy)]
struct RecordsRegistryGuardImpl;

impl RecordRegistryGuard for RecordsRegistryGuardImpl {
    type Error = BusinessError;

    fn authorize_module<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a str,
    ) -> erpx_server_records::public::record_registry::RecordRegistryFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }

    fn row_visible<'a>(
        &'a mut self,
        _: &'a RecordContextActor,
        _: &'a RecordRow,
    ) -> erpx_server_records::public::record_registry::RecordRegistryFuture<'a, bool, Self::Error> {
        Box::pin(async { Ok(true) })
    }
}

fn records_actor(current: &AuthorizedWorkContext) -> Result<RecordContextActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(RecordContextActor {
        user_id: current.user_id().as_str().to_owned(),
        holding_id: current.holding_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        assignment_id: assignment.id.as_str().to_owned(),
        assignment_version: Some(assignment.version),
        authority_revision: Some(selected.revision()),
        role: assignment.role.clone(),
        scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding".to_owned(),
            WorkScopeKind::Branch => "branch".to_owned(),
            WorkScopeKind::Department => "department".to_owned(),
        },
        modules: current.modules().to_vec(),
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    })
}

fn context_value(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

pub struct RecordsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: RecordsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> RecordsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: RecordsConfiguration,
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
            permission: "records.read".into(),
            module: Some("records".into()),
            destination: "records".into(),
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

    pub async fn context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
    ) -> Result<RecordsContextHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let module_id_task = module_id.clone();
        let record_id_task = record_id.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                RecordsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let module_id = module_id_task.clone();
                    let record_id = record_id_task.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = records_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgRecordContext::new(connection);
                        let mut guard = RecordsContextGuardImpl;
                        let data = record_context(&mut repository, &mut guard, &actor, &module_id, &record_id, now)
                            .await?;
                        Ok(RecordsContextHttpResponse {
                            data,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn registry<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: RecordsRegistryHttpCommand,
    ) -> Result<RecordsRegistryHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let (is_page, module_id, section_id, q, cursor) = match command {
            RecordsRegistryHttpCommand::List {
                module_id,
                section_id,
            } => (false, module_id, section_id, None, None),
            RecordsRegistryHttpCommand::Page {
                module_id,
                section_id,
                q,
                cursor,
            } => (true, module_id, section_id, q, cursor),
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                RecordsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let module_id = module_id.clone();
                    let section_id = section_id.clone();
                    let q = q.clone();
                    let cursor = cursor.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = records_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgRecordRegistry::new(connection);
                        let mut guard = RecordsRegistryGuardImpl;
                        let data = if is_page {
                            let query = RegistryQuery::page(&module_id, &section_id, q.as_deref(), cursor.as_deref())
                                .map_err(BusinessError::from)?;
                            let page = record_registry_page(&mut repository, &mut guard, &actor, &query, now)
                                .await?;
                            page.to_wire()
                        } else {
                            let query = RegistryQuery::list(&module_id, &section_id)
                                .map_err(BusinessError::from)?;
                            let items = record_registry_list(&mut repository, &mut guard, &actor, &query, now)
                                .await?;
                            Value::Array(items.iter().map(RecordView::to_wire).collect())
                        };
                        Ok(RecordsRegistryHttpResponse {
                            data,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct RecordsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<RecordsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> RecordsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<RecordsService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context_request(
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

impl<C: Clock + Send + Sync + 'static> RecordsContextHttpBackend for RecordsBackend<C> {
    fn execute<'a>(
        &'a self,
        request: RecordsContextHttpRequest<'a>,
    ) -> RecordsContextHttpFuture<'a, RecordsContextHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context_request(request.headers)?;
            let (module_id, record_id) = match &request.command {
                RecordsContextHttpCommand::Context {
                    module_id,
                    record_id,
                } => (module_id.clone(), record_id.clone()),
            };
            self.service
                .context(request.host, proof, move || Ok(selection), module_id, record_id)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> RecordsRegistryHttpBackend for RecordsBackend<C> {
    fn execute<'a>(
        &'a self,
        request: RecordsRegistryHttpRequest<'a>,
    ) -> RecordsRegistryHttpFuture<'a, RecordsRegistryHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context_request(request.headers)?;
            let command = request.command.clone();
            self.service
                .registry(request.host, proof, move || Ok(selection), command)
                .await
                .map_err(business)
        })
    }
}
