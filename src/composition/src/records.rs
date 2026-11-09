use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
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
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, FailureKind, HoldingId,
};
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

fn module_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "RECORDS_MODULE_FORBIDDEN",
        "Change to a role with access to this module.",
    )
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
pub(crate) struct RecordsSources;

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

    pub fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
}

#[derive(Clone, Copy)]
struct RecordsContextGuardImpl;

impl RecordContextGuard for RecordsContextGuardImpl {
    type Error = BusinessError;

    fn authorize_source<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, (), Self::Error> {
        Box::pin(async move {
            if actor.permits_module(module_id) {
                Ok(())
            } else {
                Err(module_forbidden())
            }
        })
    }

    fn target_visible<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
        _: &'a str,
    ) -> erpx_server_records::public::record_context::RecordContextFuture<'a, bool, Self::Error>
    {
        Box::pin(async move { Ok(actor.permits_module(module_id)) })
    }
}

#[derive(Clone, Copy)]
struct RecordsRegistryGuardImpl;

impl RecordRegistryGuard for RecordsRegistryGuardImpl {
    type Error = BusinessError;

    fn authorize_module<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        module_id: &'a str,
    ) -> erpx_server_records::public::record_registry::RecordRegistryFuture<'a, (), Self::Error>
    {
        Box::pin(async move {
            if actor.permits_module(module_id) {
                Ok(())
            } else {
                Err(module_forbidden())
            }
        })
    }

    fn row_visible<'a>(
        &'a mut self,
        actor: &'a RecordContextActor,
        row: &'a RecordRow,
    ) -> erpx_server_records::public::record_registry::RecordRegistryFuture<'a, bool, Self::Error>
    {
        Box::pin(async move { Ok(actor.permits_module(&row.module_id)) })
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
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
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

pub(crate) fn context_value(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "scopeKind": match assignment.scope_kind {
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
                        let data = record_context(
                            &mut repository,
                            &mut guard,
                            &actor,
                            &module_id,
                            &record_id,
                            now,
                        )
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
            RecordsRegistryHttpCommand::Create { .. } => {
                return Err(unavailable().into());
            }
            RecordsRegistryHttpCommand::Update { .. } => {
                return Err(unavailable().into());
            }
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
                            let query = RegistryQuery::page(
                                &module_id,
                                &section_id,
                                q.as_deref(),
                                cursor.as_deref(),
                            )
                            .map_err(BusinessError::from)?;
                            let page = record_registry_page(
                                &mut repository,
                                &mut guard,
                                &actor,
                                &query,
                                now,
                            )
                            .await?;
                            page.to_wire()
                        } else {
                            let query = RegistryQuery::list(&module_id, &section_id)
                                .map_err(BusinessError::from)?;
                            let items = record_registry_list(
                                &mut repository,
                                &mut guard,
                                &actor,
                                &query,
                                now,
                            )
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
    command: Arc<crate::records_command::RecordsCommandService<C>>,
}

impl<C: Clock + Send + Sync + 'static> RecordsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<RecordsService<C>>,
        command: Arc<crate::records_command::RecordsCommandService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
            command,
        }
    }

    fn write_request(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let company =
            unique_request_header(headers, "x-erpx-context-company")?.ok_or_else(scope)?;
        if company != self.service.company().as_str() {
            return Err(scope());
        }
        let assignment =
            unique_request_header(headers, "x-erpx-context-assignment")?.ok_or_else(scope)?;
        let revision: i64 = unique_request_header(headers, "x-erpx-context-revision")?
            .ok_or_else(scope)?
            .parse()
            .map_err(|_| scope())?;
        let assignment_version: i64 =
            unique_request_header(headers, "x-erpx-context-assignment-version")?
                .ok_or_else(scope)?
                .parse()
                .map_err(|_| scope())?;
        if !(1..=9_007_199_254_740_991).contains(&revision)
            || !(1..=9_007_199_254_740_991).contains(&assignment_version)
        {
            return Err(scope());
        }
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(ExpectedWorkSelection {
                    revision,
                    assignment_id: AssignmentId::new(assignment)?,
                    assignment_version,
                }),
                mutation: true,
            },
        ))
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
                .context(
                    request.host,
                    proof,
                    move || Ok(selection),
                    module_id,
                    record_id,
                )
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
            let create = matches!(
                request.command,
                RecordsRegistryHttpCommand::Create { .. }
                    | RecordsRegistryHttpCommand::Update { .. }
            );
            let (proof, selection) = if create {
                self.write_request(request.headers)?
            } else {
                self.context_request(request.headers)?
            };
            match request.command.clone() {
                RecordsRegistryHttpCommand::Create {
                    module_id,
                    input,
                    key,
                } => {
                    let (_, context, view) = self
                        .command
                        .create_with_context(
                            request.host,
                            proof,
                            move || Ok(selection),
                            module_id,
                            input,
                            erpx_server_shared::public::CommandKey::new(key)?,
                        )
                        .await
                        .map_err(business)?;
                    Ok(RecordsRegistryHttpResponse {
                        data: view,
                        context,
                    })
                }
                RecordsRegistryHttpCommand::Update {
                    module_id,
                    record_id,
                    input,
                    key,
                } => {
                    let (_, context, view) = self
                        .command
                        .update_with_context(
                            request.host,
                            proof,
                            move || Ok(selection),
                            module_id,
                            record_id,
                            input,
                            erpx_server_shared::public::CommandKey::new(key)?,
                        )
                        .await
                        .map_err(business)?;
                    Ok(RecordsRegistryHttpResponse {
                        data: view,
                        context,
                    })
                }
                command => self
                    .service
                    .registry(request.host, proof, move || Ok(selection), command)
                    .await
                    .map_err(business),
            }
        })
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use erpx_server_records::public::record_context::RecordContextGuard;
    use erpx_server_records::public::record_registry::RecordRegistryGuard;

    fn block<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn actor(modules: &[&str]) -> RecordContextActor {
        RecordContextActor {
            user_id: "user-1".into(),
            holding_id: "company".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment-1".into(),
            assignment_version: Some(1),
            authority_revision: Some(1),
            role: "member".into(),
            scope_kind: "holding".into(),
            modules: modules.iter().map(|value| (*value).into()).collect(),
            permissions: vec!["records.read".into()],
            expires_at: 9_007_199_254_740_991,
        }
    }

    fn row(module_id: &str) -> RecordRow {
        RecordRow {
            module_id: module_id.into(),
            id: "record-1".into(),
            section_id: "main".into(),
            title: "Record".into(),
            description: String::new(),
            status: "active".into(),
            fields_json: "[]".into(),
            company_id: "company".into(),
            branch_id: None,
            department_id: None,
            version: 1,
            created_by: "user-1".into(),
            updated_at: 1,
        }
    }

    #[test]
    fn owner_module_guards_hide_foreign_references_and_rows() {
        let member = actor(&["projects"]);
        let mut context = RecordsContextGuardImpl;
        assert!(block(context.authorize_source(&member, "projects", "record-1")).is_ok());
        assert!(block(context.authorize_source(&member, "finance", "record-1")).is_err());
        assert_eq!(
            block(context.target_visible(&member, "projects", "record-1")).unwrap(),
            true
        );
        assert_eq!(
            block(context.target_visible(&member, "finance", "record-1")).unwrap(),
            false
        );
        let mut registry = RecordsRegistryGuardImpl;
        assert!(block(registry.authorize_module(&member, "projects")).is_ok());
        assert!(block(registry.authorize_module(&member, "finance")).is_err());
        assert_eq!(
            block(registry.row_visible(&member, &row("projects"))).unwrap(),
            true
        );
        assert_eq!(
            block(registry.row_visible(&member, &row("finance"))).unwrap(),
            false
        );
    }
}
