//! Native generic Records create command. Support is the first bounded module;
//! current Identity authority, Company/workspace scope, command receipt,
//! version CAS, audit and outbox all live in the caller's serializable unit.
use crate::records::RecordsConfiguration;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_records::public::record_write::{
    ArchiveRecordInput, CreateRecordInput, MAX_SAFE, RecordFieldInput, UpdateRecordInput,
    ValidatedRecord, ValidatedRecordUpdate, validate_archive, validate_create, validate_update,
    version_conflict,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use sqlx::{Connection, PgConnection};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Choose a current role again.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "RECORDS_WRITE_UNAVAILABLE",
        "The Records write is unavailable.",
    )
}

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "RECORDS_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "RECORDS_MODULE_FORBIDDEN",
        "Change to a role with access to this module.",
    )
}

fn record_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Record not found in this workspace.",
    )
}

fn valid_record_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && !value.contains('\0')
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.write")?;
    if now.get() >= current.expires_at() {
        return Err(changed());
    }
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
            .map(|value| CommandField::Value(value.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| CommandField::Value(value.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: selected.revision(),
            assignment_version: assignment.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(
            &erpx_server_identity_access::public::command_metadata::project_command_iam(current)?,
        )?),
        assignment_scope_kind: match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: assignment
            .scope_node_id
            .as_ref()
            .map(|value| value.as_str().into()),
    })
}

fn require_module(current: &AuthorizedWorkContext, module_id: &str) -> Result<(), BusinessError> {
    if current.modules().iter().any(|value| value == module_id) {
        Ok(())
    } else {
        Err(forbidden())
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
}

impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !connection.is_in_transaction() {
            return Err(
                erpx_server_runtime::public::postgres::PersistenceError::new(
                    "RECORDS_WRITE_TRANSACTION_REQUIRED",
                )
                .into(),
            );
        }
        Ok(())
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if request.key != self.request.key
            || request.operation != self.request.operation
            || request.record_id != self.request.record_id
            || request.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.current, self.clock.now())?)
    }

    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != current {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    module_id: &'a str,
    input: &'a CreateRecordInput,
    validated: &'a ValidatedRecord,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,workspace_record_transitions \
             IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        )
        .execute(&mut *connection)
        .await
        .map_err(
            erpx_server_runtime::public::postgres::PersistenceError::pre_snapshot_lock_conflict,
        )?;
        Ok(())
    }

    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(
                &serde_json::json!({ "id": self.validated.id }),
            )?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let selected = self.current.selection().workspace();
        let now = self.clock.now().get();
        sqlx::query(
            "INSERT INTO workspace_records(module_id,id,section_id,title,description,status,\
             fields_json,company_id,branch_id,department_id,version,created_by,updated_by,\
             created_at,updated_at) VALUES($1,$2,$3,$4,$5,'draft',$6,$7,$8,$9,1,$10,$10,$11,$11)",
        )
        .bind(self.module_id)
        .bind(&self.validated.id)
        .bind(&self.validated.section_id)
        .bind(&self.validated.title)
        .bind(&self.validated.description)
        .bind(&self.validated.fields_json)
        .bind(selected.company_id.as_str())
        .bind(selected.branch_id.as_ref().map(|value| value.as_str()))
        .bind(selected.department_id.as_ref().map(|value| value.as_str()))
        .bind(current.user_id.as_str())
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let row: Option<(String, Option<String>, Option<String>, String, String, String, String, i64, String)> =
            sqlx::query_as(
                "SELECT company_id,branch_id,department_id,section_id,title,description,\
                 fields_json,version,created_by FROM workspace_records WHERE module_id=$1 AND id=$2",
            )
            .bind(self.module_id)
            .bind(&self.validated.id)
            .fetch_optional(&mut *connection)
            .await?;
        let selected = self.current.selection().workspace();
        let Some((
            company,
            branch,
            department,
            section,
            title,
            description,
            fields,
            version,
            created_by,
        )) = row
        else {
            return Err(unavailable().into());
        };
        if company != selected.company_id.as_str()
            || branch.as_deref() != selected.branch_id.as_ref().map(|value| value.as_str())
            || department.as_deref() != selected.department_id.as_ref().map(|value| value.as_str())
            || section != self.validated.section_id
            || title != self.validated.title
            || description != self.validated.description
            || fields != self.validated.fields_json
            || version != 1
            || created_by != current.user_id.as_str()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct UpdateEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    module_id: &'a str,
    record_id: &'a str,
    input: &'a UpdateRecordInput,
    prepared: Option<ValidatedRecordUpdate>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for UpdateEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,workspace_record_transitions \
             IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        )
        .execute(&mut *connection)
        .await
        .map_err(
            erpx_server_runtime::public::postgres::PersistenceError::pre_snapshot_lock_conflict,
        )?;
        Ok(())
    }

    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT fields_json,version FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3 FOR UPDATE",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let (fields_json, version) = row.ok_or_else(record_not_found)?;
        let current_fields: Vec<RecordFieldInput> =
            serde_json::from_str(&fields_json).map_err(|_| unavailable())?;
        let validated = validate_update(self.module_id, self.input, &current_fields, version)?;
        self.prepared = Some(validated);
        Ok(WorkflowOutput {
            version: self.input.expected_version,
            result: CommandJson::from_serializable(&serde_json::json!({ "id": self.record_id }))?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        let now = self.clock.now().get();
        let changed = sqlx::query(
            "UPDATE workspace_records SET fields_json=$1,version=$2,updated_by=$3,updated_at=$4 \
             WHERE module_id=$5 AND id=$6 AND company_id=$7 AND version=$8",
        )
        .bind(&prepared.fields_json)
        .bind(prepared.version)
        .bind(current.user_id.as_str())
        .bind(now)
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .bind(self.input.expected_version)
        .execute(&mut *connection)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(version_conflict().into());
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let prepared = self.prepared.as_ref().ok_or_else(unavailable)?;
        let row: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT fields_json,version,updated_by FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some((fields, version, updated_by)) = row else {
            return Err(record_not_found().into());
        };
        if fields != prepared.fields_json
            || version != prepared.version
            || updated_by != current.user_id.as_str()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct ArchiveEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    module_id: &'a str,
    record_id: &'a str,
    input: &'a ArchiveRecordInput,
    prepared: Option<i64>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for ArchiveEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,workspace_record_transitions \
             IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        )
        .execute(&mut *connection)
        .await
        .map_err(
            erpx_server_runtime::public::postgres::PersistenceError::pre_snapshot_lock_conflict,
        )?;
        Ok(())
    }

    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let row: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3 AND archived_at IS NULL FOR UPDATE",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let version = row.ok_or_else(record_not_found)?;
        validate_archive(self.module_id, self.input, version)?;
        self.prepared = Some(version);
        Ok(WorkflowOutput {
            version: self.input.expected_version,
            result: CommandJson::from_serializable(&serde_json::json!({ "id": self.record_id }))?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let version = self.prepared.ok_or_else(unavailable)?;
        let now = self.clock.now().get();
        let changed = sqlx::query(
            "UPDATE workspace_records SET archived_at=$1,version=version+1,updated_by=$2,updated_at=$1 \
             WHERE module_id=$3 AND id=$4 AND company_id=$5 AND version=$6 AND archived_at IS NULL",
        )
        .bind(now)
        .bind(current.user_id.as_str())
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .bind(version)
        .execute(&mut *connection)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(version_conflict().into());
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let version = self.prepared.ok_or_else(unavailable)?;
        let row: Option<(Option<i64>, i64, String)> = sqlx::query_as(
            "SELECT archived_at,version,updated_by FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some((archived_at, current_version, updated_by)) = row else {
            return Err(record_not_found().into());
        };
        if archived_at != Some(self.clock.now().get())
            || current_version != version + 1
            || updated_by != current.user_id.as_str()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct Ids;

impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

pub struct RecordsCommandService<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub configuration: RecordsConfiguration,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> RecordsCommandService<C> {
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

    fn access(module_id: &str) -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: erpx_server_identity_access::public::effective_authority::WorkPermissionRequirement::Exact("records.write".into()),
            module: Some(module_id.into()),
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
        let company = self.configuration.company().clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        }
    }

    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        create: CreateRecordInput,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        Ok(self
            .create_with_context(host, proof, input, module_id, create, key)
            .await?
            .0)
    }

    pub async fn create_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        create: CreateRecordInput,
        key: CommandKey,
    ) -> Result<(CommandJson, serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        validate_create(&module_id, key.as_str(), &create)?;
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let wire = CommandJson::from_serializable(&serde_json::json!({
            "moduleId": module_id,
            "sectionId": create.section_id,
            "title": create.title,
            "description": create.description,
            "fields": create.fields,
        }))?;
        let record_id = format!("rec-{module_id}-{}", key.as_str());
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(&module_id),
                crate::records::RecordsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let module_id = module_id.clone();
                    let wire = wire.clone();
                    let record_id = record_id.clone();
                    let create = create.clone();
                    let command_key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        require_module(current, &module_id)?;
                        let validated = validate_create(&module_id, command_key.as_str(), &create)?;
                        let request = WorkflowRequest {
                            key: command_key.as_str().to_owned(),
                            operation: "record.create".into(),
                            record_id,
                            input: wire,
                        };
                        let clock_ref = clock.as_ref();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                        };
                        let mut effects = Effects {
                            current,
                            request: &request,
                            clock: clock_ref,
                            module_id: &module_id,
                            input: &create,
                            validated: &validated,
                        };
                        let mut ids = Ids;
                        let mut ledger = PgCommandLedger::new(
                            current.selection().workspace().company_id.clone(),
                            connection,
                            &mut authority,
                            clock_ref,
                            &mut ids,
                        );
                        let outcome = ledger.execute(&request, &mut effects).await?;
                        // Return the persisted current row for both a new command and
                        // exact replay; a later owner transition must be visible here.
                        let row: (String, String, String, String, String, i64, String, i64) =
                            sqlx::query_as(
                                "SELECT section_id,title,description,status,fields_json,version,                                 created_by,updated_at FROM workspace_records                                  WHERE module_id=$1 AND id=$2 AND company_id=$3",
                            )
                            .bind(&module_id)
                            .bind(&validated.id)
                            .bind(current.selection().workspace().company_id.as_str())
                            .fetch_one(&mut *connection)
                            .await?;
                        let fields: serde_json::Value = serde_json::from_str(&row.4)
                            .map_err(|_| unavailable())?;
                        let view = serde_json::json!({
                            "moduleId": module_id,
                            "id": validated.id,
                            "sectionId": row.0,
                            "title": row.1,
                            "description": row.2,
                            "status": row.3,
                            "fields": fields,
                            "version": row.5,
                            "createdBy": row.6,
                            "updatedAt": row.7,
                        });
                        Ok((
                            outcome.result,
                            crate::records::context_value(current),
                            view,
                        ))
                    })
                },
            )
            .await
    }

    pub async fn update<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        update: UpdateRecordInput,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        Ok(self
            .update_with_context(host, proof, input, module_id, record_id, update, key)
            .await?
            .0)
    }

    pub async fn update_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        update: UpdateRecordInput,
        key: CommandKey,
    ) -> Result<(CommandJson, serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if module_id != "support" {
            return Err(BusinessError::invalid(
                "RECORD_MODULE_UNSUPPORTED",
                "This record module is not available in the native workbench yet.",
            )
            .into());
        }
        if !valid_record_id(&record_id) {
            return Err(record_not_found().into());
        }
        if !(1..=MAX_SAFE).contains(&update.expected_version) {
            return Err(BusinessError::invalid(
                "RECORD_VERSION_INVALID",
                "Reload the record and try again.",
            )
            .into());
        }
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let wire = CommandJson::from_serializable(&serde_json::json!({
            "moduleId": module_id,
            "expectedVersion": update.expected_version,
            "fields": update.fields,
        }))?;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(&module_id),
                crate::records::RecordsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let module_id = module_id.clone();
                    let wire = wire.clone();
                    let record_id = record_id.clone();
                    let update = update.clone();
                    let command_key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        require_module(current, &module_id)?;
                        let request = WorkflowRequest {
                            key: command_key.as_str().to_owned(),
                            operation: "record.update".into(),
                            record_id: record_id.clone(),
                            input: wire,
                        };
                        let clock_ref = clock.as_ref();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                        };
                        let mut effects = UpdateEffects {
                            current,
                            clock: clock_ref,
                            module_id: &module_id,
                            record_id: &record_id,
                            input: &update,
                            prepared: None,
                        };
                        let mut ids = Ids;
                        let mut ledger = PgCommandLedger::new(
                            current.selection().workspace().company_id.clone(),
                            connection,
                            &mut authority,
                            clock_ref,
                            &mut ids,
                        );
                        let outcome = ledger.execute(&request, &mut effects).await?;
                        // Return the persisted current row for both a new command and
                        // exact replay; a later owner transition must be visible here.
                        let row: (String, String, String, String, String, i64, String, i64) =
                            sqlx::query_as(
                                "SELECT section_id,title,description,status,fields_json,version,                                 created_by,updated_at FROM workspace_records                                  WHERE module_id=$1 AND id=$2 AND company_id=$3",
                            )
                            .bind(&module_id)
                            .bind(&record_id)
                            .bind(current.selection().workspace().company_id.as_str())
                            .fetch_one(&mut *connection)
                            .await?;
                        let fields: serde_json::Value = serde_json::from_str(&row.4)
                            .map_err(|_| unavailable())?;
                        let view = serde_json::json!({
                            "moduleId": module_id,
                            "id": record_id,
                            "sectionId": row.0,
                            "title": row.1,
                            "description": row.2,
                            "status": row.3,
                            "fields": fields,
                            "version": row.5,
                            "createdBy": row.6,
                            "updatedAt": row.7,
                        });
                        Ok((
                            outcome.result,
                            crate::records::context_value(current),
                            view,
                        ))
                    })
                },
            )
            .await
    }

    pub async fn archive<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        archive: ArchiveRecordInput,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        Ok(self
            .archive_with_context(host, proof, input, module_id, record_id, archive, key)
            .await?
            .0)
    }

    pub async fn archive_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        archive: ArchiveRecordInput,
        key: CommandKey,
    ) -> Result<(CommandJson, serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if module_id != "support" {
            return Err(BusinessError::invalid(
                "RECORD_MODULE_UNSUPPORTED",
                "This record module is not available in the native workbench yet.",
            )
            .into());
        }
        if !valid_record_id(&record_id) {
            return Err(record_not_found().into());
        }
        if !(1..=MAX_SAFE).contains(&archive.expected_version) {
            return Err(BusinessError::invalid(
                "RECORD_VERSION_INVALID",
                "Reload the record and try again.",
            )
            .into());
        }
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let wire = CommandJson::from_serializable(&serde_json::json!({
            "moduleId": module_id,
            "expectedVersion": archive.expected_version,
        }))?;
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(&module_id),
                crate::records::RecordsSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let module_id = module_id.clone();
                    let wire = wire.clone();
                    let record_id = record_id.clone();
                    let archive = archive.clone();
                    let command_key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        require_module(current, &module_id)?;
                        let request = WorkflowRequest {
                            key: command_key.as_str().to_owned(),
                            operation: "record.archive".into(),
                            record_id: record_id.clone(),
                            input: wire,
                        };
                        let clock_ref = clock.as_ref();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                        };
                        let mut effects = ArchiveEffects {
                            current,
                            clock: clock_ref,
                            module_id: &module_id,
                            record_id: &record_id,
                            input: &archive,
                            prepared: None,
                        };
                        let mut ids = Ids;
                        let mut ledger = PgCommandLedger::new(
                            current.selection().workspace().company_id.clone(),
                            connection,
                            &mut authority,
                            clock_ref,
                            &mut ids,
                        );
                        let outcome = ledger.execute(&request, &mut effects).await?;
                        Ok((
                            outcome.result,
                            crate::records::context_value(current),
                            serde_json::json!({
                                "id": record_id,
                                "moduleId": module_id,
                                "archived": true,
                            }),
                        ))
                    })
                },
            )
            .await
    }
}
