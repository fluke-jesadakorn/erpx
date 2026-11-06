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
use erpx_server_operations::public::field_exceptions::Exception;
use erpx_server_operations::public::field_policy::{OpenException as FieldOpenException, QueuePolicy};
use erpx_server_operations::public::operations_commands::{
    ChangePlan, CommandActor, CommandReceipt, OpenPlan, OperationsCommandFuture,
    OperationsCommandRepository, OperationsCommandSources, PolicyDraftPlan, PolicyReviewPlan,
    StoredReceipt, change_operations_exception, draft_operations_policy, open_operations_exception,
    review_operations_policy,
};
use erpx_server_operations::public::operations_http::{
    OperationsHttpBackend, OperationsHttpCommand, OperationsHttpCommandRequest, OperationsHttpFuture,
    OperationsHttpResponse, OperationsHttpReadKind, OperationsHttpReadRequest, OperationsHttpTarget,
};
use erpx_server_operations::public::queue_read::{
    OperationsPeopleDirectory, OperationsReadActor, OperationsSourceVisibility,
    PersonOption, QueueException, SourceView, operations_detail, operations_workspace,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{
    PersistenceError, PgTransactionError, PgWorkFuture,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, CommandKey, FailureKind, HoldingId,
};
use erpx_server_shared::public::record_source_contract::{SourceRecord, SourceReference};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "OPERATIONS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_UNAVAILABLE",
        "The Operations workspace is unavailable.",
    )
}

fn command_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "OPERATIONS_COMMAND_UNAVAILABLE",
        "Native Operations commands are not enabled yet.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_STATE_CHANGED",
        "Review the current exception state.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

async fn serializable_checked(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("OPERATIONS_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("OPERATIONS_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct OperationsSources;

impl HoldingWorkSourcePreparation for OperationsSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

impl OperationsSourceVisibility for OperationsSources {
    type Error = BusinessError;
    fn visible<'a>(
        &'a mut self,
        row: &'a QueueException,
    ) -> erpx_server_operations::public::queue_read::OperationsReadFuture<
        'a,
        Option<SourceView>,
        Self::Error,
    > {
        Box::pin(async move {
            Ok(Some(SourceView {
                kind: row.source_kind.clone(),
                id: row.source_id.clone(),
                version: row.source_version,
                title: row.title.clone(),
                href: format!("/records/{}", row.source_id),
                label: format!("Source · {}", row.title),
                company_id: row.company_id.clone(),
                branch_id: row.branch_id.clone(),
                department_id: row.department_id.clone(),
                status: row.status.clone(),
            }))
        })
    }
}

impl OperationsPeopleDirectory for OperationsSources {
    type Error = BusinessError;
    fn people(
        &mut self,
    ) -> erpx_server_operations::public::queue_read::OperationsReadFuture<
        '_,
        Vec<PersonOption>,
        Self::Error,
    > {
        // Real People owner port is not wired; fail closed rather than present an
        // empty directory that silently permits arbitrary owner assignment.
        Box::pin(async move {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_PEOPLE_UNAVAILABLE",
                "Operations people directory is not wired.",
            ))
        })
    }
}

impl OperationsCommandSources for OperationsSources {
    type Error = BusinessError;
    fn source<'a>(
        &'a mut self,
        _actor: &'a CommandActor,
        _reference: &'a SourceReference,
    ) -> OperationsCommandFuture<'a, SourceRecord, Self::Error> {
        // Real Records source port is not wired; never fabricate a source record.
        Box::pin(async move {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_SOURCE_UNAVAILABLE",
                "Operations source records are not wired.",
            ))
        })
    }
    fn recipient<'a>(
        &'a mut self,
        _actor: &'a CommandActor,
        _scope: &'a erpx_server_operations::public::operations_commands::CommandScope,
        _user_id: &'a str,
        _review: bool,
    ) -> OperationsCommandFuture<'a, (), Self::Error> {
        // Real People/IAM recipient port is not wired; fail closed.
        Box::pin(async move {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_RECIPIENT_UNAVAILABLE",
                "Operations recipient validation is not wired.",
            ))
        })
    }
    fn same_person<'a>(
        &'a mut self,
        _actor: &'a CommandActor,
        _user_id: &'a str,
    ) -> OperationsCommandFuture<'a, bool, Self::Error> {
        // Cannot determine identity without a real owner port; fail closed.
        Box::pin(async move {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "OPERATIONS_SAME_PERSON_UNAVAILABLE",
                "Operations same-person check is not wired.",
            ))
        })
    }
}

#[derive(Clone)]
pub struct OperationsConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl OperationsConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn read_actor(current: &AuthorizedWorkContext) -> Result<OperationsReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(OperationsReadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn command_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(CommandActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|value| value.as_str().to_owned()),
        department_id: workspace.department_id.as_ref().map(|value| value.as_str().to_owned()),
        assignment_scope: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding".into(),
            WorkScopeKind::Branch => "branch".into(),
            WorkScopeKind::Department => "department".into(),
        },
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
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

pub struct OperationsService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: OperationsConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> OperationsService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: OperationsConfiguration,
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
            permission: "operations.read".into(),
            module: Some("operations".into()),
            destination: "operations".into(),
            api: true,
        }
    }

    fn command_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("operations".into()),
            destination: "operations".into(),
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

    pub async fn workspace<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<OperationsHttpResponse, Error>
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
                self.input(input),
                Self::access(),
                OperationsSources,
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
                        current.require_permission("operations.read")?;
                        let actor = read_actor(current)?;
                        let now = clock.now().get();
                        let mut repository =
                            erpx_server_operations::public::queue_read::PgOperationsRead::new(
                                connection,
                            );
                        let mut sources = OperationsSources;
                        let mut people = OperationsSources;
                        let data = operations_workspace(
                            &actor,
                            &mut repository,
                            &mut sources,
                            &mut people,
                            now,
                        )
                        .await?;
                        Ok(OperationsHttpResponse {
                            data: serde_json::to_value(data).map_err(|_| unavailable())?,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<OperationsHttpResponse, Error>
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
                self.input(input),
                Self::access(),
                OperationsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let id = id.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("operations.read")?;
                        let actor = read_actor(current)?;
                        let now = clock.now().get();
                        let mut repository =
                            erpx_server_operations::public::queue_read::PgOperationsRead::new(
                                connection,
                            );
                        let mut sources = OperationsSources;
                        let mut people = OperationsSources;
                        let data = operations_detail(
                            &actor,
                            &mut repository,
                            &mut sources,
                            &mut people,
                            &id,
                            now,
                        )
                        .await?;
                        Ok(OperationsHttpResponse {
                            data: serde_json::to_value(data).map_err(|_| unavailable())?,
                            context: context_value(current),
                        })
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
        command: OperationsHttpCommand,
        key: CommandKey,
    ) -> Result<OperationsHttpResponse, Error>
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
                {
                    let company = company.clone();
                    move || {
                        let mut request = input()?;
                        if request.company_id != company {
                            return Err(scope());
                        }
                        request.mutation = true;
                        Ok(request)
                    }
                },
                Self::command_access(),
                OperationsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    let key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        let actor = command_actor(current)?;
                        let now = clock.now().get();
                        let mut repository = PgOperationsCommandRepository { connection };
                        let mut sources = OperationsSources;
                        let receipt = match command {
                            OperationsHttpCommand::OpenException(input) => {
                                open_operations_exception(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &input,
                                    key.as_str(),
                                    now,
                                )
                                .await?
                            }
                            OperationsHttpCommand::ChangeException(input) => {
                                change_operations_exception(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &input,
                                    key.as_str(),
                                    now,
                                )
                                .await?
                            }
                            OperationsHttpCommand::DraftPolicy(input) => {
                                draft_operations_policy(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &input,
                                    key.as_str(),
                                    now,
                                )
                                .await?
                            }
                            OperationsHttpCommand::ReviewPolicy(input) => {
                                review_operations_policy(
                                    &actor,
                                    &mut repository,
                                    &mut sources,
                                    &input,
                                    key.as_str(),
                                    now,
                                )
                                .await?
                            }
                        };
                        Ok(OperationsHttpResponse {
                            data: serde_json::to_value(receipt).map_err(|_| unavailable())?,
                            context: context_value(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct OperationsBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<OperationsService<C>>,
}

impl<C: Clock + Send + Sync + 'static> OperationsBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<OperationsService<C>>,
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

impl<C: Clock + Send + Sync + 'static> OperationsHttpBackend for OperationsBackend<C> {
    fn read<'a>(
        &'a self,
        request: OperationsHttpReadRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            match request.kind {
                OperationsHttpReadKind::Workspace => self
                    .service
                    .workspace(request.host, proof, move || Ok(selection))
                    .await
                    .map_err(business),
                OperationsHttpReadKind::Detail(id) => self
                    .service
                    .detail(request.host, proof, move || Ok(selection), id)
                    .await
                    .map_err(business),
            }
        })
    }

    fn command<'a>(
        &'a self,
        request: OperationsHttpCommandRequest<'a>,
    ) -> OperationsHttpFuture<'a, OperationsHttpResponse> {
        Box::pin(async move {
            let (_proof, _selection) = self.context(request.headers)?;
            let _ = request.command;
            let _ = request.key;
            let _ = request.host;
            let _ = request.holding_slug;
            // Native Operations commands remain unavailable until real source/recipient/
            // same-person owner ports replace fabricated composition stubs (reviewer blocker).
            Err(command_unavailable())
        })
    }
}

struct PgOperationsCommandRepository<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> OperationsCommandRepository for PgOperationsCommandRepository<'a> {
    type Error = Error;

    fn stored_receipt<'b>(
        &'b mut self,
        key: &'b str,
    ) -> OperationsCommandFuture<'b, Option<StoredReceipt>, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
                "SELECT operation, record_id, request_hash, result_json FROM command_receipts WHERE command_id=$1",
            )
            .bind(key)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(row.map(|(operation, record_id, request_hash, result_json)| StoredReceipt {
                operation,
                record_id,
                request_fingerprint: request_hash,
                result_json,
            }))
        })
    }

    fn exception<'b>(
        &'b mut self,
        company_id: &'b str,
        branch_id: Option<&'b str>,
        department_id: Option<&'b str>,
        exception_id: &'b str,
    ) -> OperationsCommandFuture<'b, Option<Exception>, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let row = sqlx::query(
                "SELECT id, company_id, branch_id, department_id, area, source_kind, source_id, source_version, source_reference_json, policy_id, policy_snapshot_json, title, issue, priority, status, owner_id, escalation_owner_id, resolution, resolved_by, reviewed_by, version, created_by, created_at, updated_at, due_at FROM operations_exceptions WHERE company_id=$1 AND ($2::text IS NULL OR branch_id=$2) AND ($3::text IS NULL OR department_id=$3) AND id=$4",
            )
            .bind(company_id)
            .bind(branch_id)
            .bind(department_id)
            .bind(exception_id)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(row.map(|row| {
                use sqlx::Row;
                Exception {
                    id: row.get("id"),
                    company_id: row.get("company_id"),
                    branch_id: row.get("branch_id"),
                    department_id: row.get("department_id"),
                    area: row.get("area"),
                    source_kind: row.get("source_kind"),
                    source_id: row.get("source_id"),
                    source_version: row.get("source_version"),
                    source_reference_json: row.get("source_reference_json"),
                    policy_id: row.get("policy_id"),
                    policy_snapshot_json: row.get("policy_snapshot_json"),
                    title: row.get("title"),
                    issue: row.get("issue"),
                    priority: row.get("priority"),
                    status: row.get("status"),
                    owner_id: row.get("owner_id"),
                    escalation_owner_id: row.get("escalation_owner_id"),
                    resolution: row.get("resolution"),
                    resolved_by: row.get("resolved_by"),
                    reviewed_by: row.get("reviewed_by"),
                    version: row.get("version"),
                    created_by: row.get("created_by"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    due_at: row.get("due_at"),
                }
            }))
        })
    }

    fn policy<'b>(
        &'b mut self,
        company_id: &'b str,
        policy_id: &'b str,
    ) -> OperationsCommandFuture<'b, Option<QueuePolicy>, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let row = sqlx::query_as::<
                _,
                (
                    String,
                    String,
                    String,
                    String,
                    i64,
                    String,
                    i64,
                    String,
                    String,
                    Option<String>,
                    i64,
                    i64,
                    i64,
                ),
            >(
                "SELECT id, company_id, area, name, revision, source_kinds_json, target_minutes, status, created_by, reviewed_by, version, created_at, updated_at FROM operations_queue_policies WHERE company_id=$1 AND id=$2",
            )
            .bind(company_id)
            .bind(policy_id)
            .fetch_optional(&mut *connection)
            .await?;
            Ok(row.map(
                |(
                    id,
                    company_id,
                    area,
                    name,
                    revision,
                    source_kinds_json,
                    target_minutes,
                    status,
                    created_by,
                    reviewed_by,
                    version,
                    created_at,
                    updated_at,
                )| QueuePolicy {
                    id,
                    company_id,
                    area,
                    name,
                    revision,
                    source_kinds_json,
                    target_minutes,
                    status,
                    created_by,
                    reviewed_by,
                    version,
                    created_at,
                    updated_at,
                },
            ))
        })
    }

    fn open<'b>(
        &'b mut self,
        plan: OpenPlan,
    ) -> OperationsCommandFuture<'b, CommandReceipt, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let company = CompanyId::new(&plan.company_id)
                .map_err(|_| BusinessError::invalid("OPERATIONS_INPUT_INVALID", "Invalid company."))?;
            let input = FieldOpenException {
                source_id: plan.input.source.id.clone(),
                source_version: plan.input.source.version,
                policy_id: plan.input.policy_id.clone(),
                policy_version: plan.input.policy_version,
                title: plan.input.title.clone(),
                issue: plan.input.issue.clone(),
                priority: plan.input.priority.clone(),
                owner_id: plan.input.owner_id.clone(),
            };
            let source_json: Value =
                serde_json::from_str(&plan.source_reference_json).unwrap_or(Value::Null);
            let _retained = erpx_server_operations::public::field_exceptions::open_operations_exception(
                connection,
                &company,
                &plan.key,
                &plan.actor_id,
                &input,
                &plan.source,
                &source_json,
                &plan.policy,
                plan.now,
            )
            .await?;
            let receipt = CommandReceipt::new(&plan.record_id, "open", 1);
            sqlx::query(
                "INSERT INTO command_receipts(command_id, operation, actor_id, request_hash, record_id, result_json, created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind("operations.exception.open")
            .bind(&plan.actor_id)
            .bind(&plan.request_fingerprint)
            .bind(&plan.record_id)
            .bind(serde_json::to_string(&receipt).map_err(|_| unavailable())?)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            Ok(receipt)
        })
    }

    fn change<'b>(
        &'b mut self,
        plan: ChangePlan,
    ) -> OperationsCommandFuture<'b, CommandReceipt, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let n = sqlx::query(
                "UPDATE operations_exceptions SET status=$1, owner_id=$2, priority=$3, escalation_owner_id=$4, resolution=$5, resolved_by=$6, reviewed_by=$7, version=$8, updated_at=$9 WHERE id=$10 AND company_id=$11 AND version=$12 AND status=$13",
            )
            .bind(&plan.transition.status)
            .bind(&plan.transition.owner)
            .bind(&plan.transition.priority)
            .bind(&plan.transition.escalation_owner)
            .bind(&plan.transition.resolution)
            .bind(&plan.transition.resolved_by)
            .bind(&plan.transition.reviewed_by)
            .bind(plan.transition.version)
            .bind(plan.now)
            .bind(&plan.row.id)
            .bind(&plan.company_id)
            .bind(plan.row.version)
            .bind(&plan.row.status)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if n != 1 {
                return Err(changed().into());
            }
            let event_id = format!("operations-event-{}", plan.key.as_str());
            let details = serde_json::json!({
                "action": plan.input.action,
                "expectedVersion": plan.input.expected_version,
                "reason": plan.input.reason.trim(),
            })
            .to_string();
            sqlx::query(
                "INSERT INTO operations_exception_events(id, exception_id, version, action, actor_id, reason, status, details_json, created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(&event_id)
            .bind(&plan.row.id)
            .bind(plan.transition.version)
            .bind(&plan.input.action)
            .bind(&plan.actor_id)
            .bind(plan.input.reason.trim())
            .bind(&plan.transition.status)
            .bind(&details)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            let receipt = CommandReceipt::new(&plan.row.id, &plan.transition.status, plan.transition.version);
            sqlx::query(
                "INSERT INTO command_receipts(command_id, operation, actor_id, request_hash, record_id, result_json, created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind(format!("operations.exception.{}", plan.input.action))
            .bind(&plan.actor_id)
            .bind(&plan.request_fingerprint)
            .bind(&plan.row.id)
            .bind(serde_json::to_string(&receipt).map_err(|_| unavailable())?)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            Ok(receipt)
        })
    }

    fn draft_policy<'b>(
        &'b mut self,
        plan: PolicyDraftPlan,
    ) -> OperationsCommandFuture<'b, CommandReceipt, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let revision: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(revision), 0) + 1 FROM operations_queue_policies WHERE company_id=$1 AND area=$2",
            )
            .bind(&plan.company_id)
            .bind(&plan.input.area)
            .fetch_one(&mut *connection)
            .await?;
            let source_kinds_json = serde_json::to_string(&plan.input.source_kinds)
                .map_err(|_| unavailable())?;
            let id = plan.record_id.clone();
            sqlx::query(
                "INSERT INTO operations_queue_policies(id, company_id, area, name, revision, source_kinds_json, target_minutes, status, created_by, version, created_at, updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,'draft',$8,1,$9,$9)",
            )
            .bind(&id)
            .bind(&plan.company_id)
            .bind(&plan.input.area)
            .bind(&plan.input.name)
            .bind(revision)
            .bind(&source_kinds_json)
            .bind(plan.input.target_minutes)
            .bind(&plan.actor_id)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            let receipt = CommandReceipt::new(&id, "draft", 1);
            sqlx::query(
                "INSERT INTO command_receipts(command_id, operation, actor_id, request_hash, record_id, result_json, created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind("operations.policy.draft")
            .bind(&plan.actor_id)
            .bind(&plan.request_fingerprint)
            .bind(&id)
            .bind(serde_json::to_string(&receipt).map_err(|_| unavailable())?)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            Ok(receipt)
        })
    }

    fn review_policy<'b>(
        &'b mut self,
        plan: PolicyReviewPlan,
    ) -> OperationsCommandFuture<'b, CommandReceipt, Self::Error> {
        let connection = &mut *self.connection;
        Box::pin(async move {
            serializable_checked(connection).await?;
            let status = plan.action.status();
            let n = sqlx::query(
                "UPDATE operations_queue_policies SET status=$1, reviewed_by=$2, version=$3, updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7",
            )
            .bind(status)
            .bind(&plan.actor_id)
            .bind(plan.next_version)
            .bind(plan.now)
            .bind(&plan.record_id)
            .bind(&plan.company_id)
            .bind(plan.expected_version)
            .execute(&mut *connection)
            .await?
            .rows_affected();
            if n != 1 {
                return Err(changed().into());
            }
            let receipt = CommandReceipt::new(&plan.record_id, status, plan.next_version);
            sqlx::query(
                "INSERT INTO command_receipts(command_id, operation, actor_id, request_hash, record_id, result_json, created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(plan.key.as_str())
            .bind(format!("operations.policy.{}", plan.action.as_str()))
            .bind(&plan.actor_id)
            .bind(&plan.request_fingerprint)
            .bind(&plan.record_id)
            .bind(serde_json::to_string(&receipt).map_err(|_| unavailable())?)
            .bind(plan.now)
            .execute(&mut *connection)
            .await?;
            Ok(receipt)
        })
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<OperationsHttpTarget>) -> Result<axum::Router, BusinessError> {
    erpx_server_operations::public::operations_http::operations_router(targets)
}
