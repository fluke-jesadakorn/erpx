//! Native generic Records command for all modules. Current Identity authority,
//! Company/workspace scope, module membership, command receipt, version CAS,
//! audit and outbox all live in the caller's serializable unit.
use crate::records::RecordsConfiguration;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest},
    records_principal_search::{active_assignment_on, active_principal_display_name},
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_records::public::native_command::{
    NativeCommand, PgNativeCommandRepository, execute_native_command,
};
use erpx_server_records::public::record_context::{
    PgRecordContext, RecordContextActor, RecordContextDocument, RecordContextRepository,
    record_context, record_context_document,
};
use erpx_server_records::public::record_social::{
    AddPrincipalMentionInput, FIELD_WRITE_STATEMENT, MENTION_STATEMENT, MentionWrite, OP_MENTION,
    OP_SUGGESTION, OP_SUGGESTION_DECIDE, SUGGESTION_STATE_STATEMENT, SUGGESTION_STATEMENT,
    SuggestionInput, guard_social_owner, mention_target_not_eligible, mention_target_unavailable,
    parse_proposed_value, parse_sources, plan_mention, plan_suggestion_request,
    suggestion_conflict, suggestion_unavailable, validate_mention_input,
};
use erpx_server_records::public::record_write::{
    ArchiveRecordInput, CreateRecordInput, MAX_REFERENCES, MAX_SAFE, RecordFieldInput,
    ReferenceRecordInput, TRANSITION_ACTIONS, TransitionRecordInput, UpdateRecordInput,
    ValidatedRecord, ValidatedRecordUpdate, reference_limit, validate_archive, validate_create,
    validate_reference, validate_transition, validate_update, version_conflict,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use sha2::{Digest, Sha256};
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
    prepared: Option<(i64, i64)>,
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
        self.prepared = Some((version, self.clock.now().get()));
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
        let (version, archived_at) = self.prepared.ok_or_else(unavailable)?;
        let changed = sqlx::query(
            "UPDATE workspace_records SET archived_at=$1,version=version+1,updated_by=$2,updated_at=$1 \
             WHERE module_id=$3 AND id=$4 AND company_id=$5 AND version=$6 AND archived_at IS NULL",
        )
        .bind(archived_at)
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
        let (version, expected_archived_at) = self.prepared.ok_or_else(unavailable)?;
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
        if archived_at != Some(expected_archived_at)
            || current_version != version + 1
            || updated_by != current.user_id.as_str()
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct ReferenceEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    module_id: &'a str,
    record_id: &'a str,
    input: &'a ReferenceRecordInput,
    prepared: Option<i64>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for ReferenceEffects<'_, C> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,record_reference_edges \
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
        let company = self.current.selection().workspace().company_id.clone();
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3 AND archived_at IS NULL FOR UPDATE",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let version = version.ok_or_else(record_not_found)?;
        let target: Option<String> = sqlx::query_scalar(
            "SELECT id FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3 AND archived_at IS NULL",
        )
        .bind(&self.input.target_module)
        .bind(&self.input.target_id)
        .bind(company.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        if target.is_none() {
            return Err(record_not_found().into());
        }
        validate_reference(self.module_id, self.record_id, self.input)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM record_reference_edges WHERE host_module=$1 AND host_id=$2",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .fetch_one(&mut *connection)
        .await?;
        if count >= MAX_REFERENCES {
            return Err(reference_limit().into());
        }
        self.prepared = Some(version);
        Ok(WorkflowOutput {
            version,
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
        let _version = self.prepared.ok_or_else(unavailable)?;
        let now = self.clock.now().get();
        sqlx::query(
            "INSERT INTO record_reference_edges(host_module,host_id,target_module,target_id,kind,\
             ordinal,created_by,created_at) \
             VALUES($1,$2,$3,$4,$5,COALESCE((SELECT MAX(ordinal)+1 FROM \
             record_reference_edges WHERE host_module=$1 AND host_id=$2),0),$6,$7) \
             ON CONFLICT DO NOTHING",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(&self.input.target_module)
        .bind(&self.input.target_id)
        .bind(&self.input.kind)
        .bind(current.user_id.as_str())
        .bind(now)
        .execute(&mut *connection)
        .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let _version = self.prepared.ok_or_else(unavailable)?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM record_reference_edges \
             WHERE host_module=$1 AND host_id=$2 AND target_module=$3 AND target_id=$4",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(&self.input.target_module)
        .bind(&self.input.target_id)
        .fetch_one(&mut *connection)
        .await?;
        if count != 1 {
            return Err(changed().into());
        }
        Ok(())
    }
}

struct MentionEffects<'a> {
    display_name: &'a Result<Option<String>, BusinessError>,
    actor: &'a RecordContextActor,
    module_id: &'a str,
    record_id: &'a str,
    input: &'a AddPrincipalMentionInput,
    now: i64,
    write: Option<MentionWrite>,
}

// The workflow ledger fingerprints the captured input in legacy field order.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MentionReceiptInput<'a> {
    module_id: &'a str,
    #[serde(flatten)]
    mention: &'a AddPrincipalMentionInput,
}

impl PgOwnerCommandEffects for MentionEffects<'_> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,record_principal_mentions \
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
        let record = PgRecordContext::new(connection)
            .record(self.actor, self.module_id, self.record_id)
            .await?
            .ok_or_else(record_not_found)?;
        if !active_assignment_on(connection, &self.input.target_id, self.now).await? {
            return Err(mention_target_not_eligible().into());
        }
        let name = self
            .display_name
            .as_ref()
            .map_err(Clone::clone)?
            .as_deref()
            .ok_or_else(mention_target_unavailable)?;
        self.write = Some(plan_mention(
            self.module_id,
            self.record_id,
            self.input,
            name,
        )?);
        Ok(WorkflowOutput {
            version: record.version,
            result: CommandJson::from_serializable(&serde_json::json!({"id": self.record_id}))?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<(), Error> {
        let write = self.write.as_ref().ok_or_else(unavailable)?;
        let statement = NativeCommand {
            kind: MENTION_STATEMENT.to_owned(),
            values: write.values(current.user_id.as_str(), self.now),
            now: self.now,
        };
        execute_native_command(
            &statement,
            &command.key,
            &mut PgNativeCommandRepository::new(connection),
        )
        .await?;
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let rows = PgRecordContext::new(connection)
            .mentions(self.module_id, self.record_id)
            .await?;
        if rows
            .iter()
            .any(|row| row.target_kind == "person" && row.target_id == self.input.target_id)
        {
            Ok(())
        } else {
            Err(changed().into())
        }
    }
}

#[derive(Clone)]
pub(crate) enum SuggestionOperation {
    Request {
        field_key: String,
    },
    Decide {
        suggestion_id: String,
        expected_version: i64,
        apply: bool,
    },
}

impl SuggestionOperation {
    fn receipt_input(&self, module_id: &str) -> Result<(&'static str, CommandJson), BusinessError> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Request<'a> {
            module_id: &'a str,
            field_key: &'a str,
        }
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Decision<'a> {
            module_id: &'a str,
            suggestion_id: &'a str,
            expected_version: i64,
            action: &'a str,
        }
        match self {
            Self::Request { field_key } => Ok((
                OP_SUGGESTION,
                CommandJson::from_serializable(&Request {
                    module_id,
                    field_key,
                })?,
            )),
            Self::Decide {
                suggestion_id,
                expected_version,
                apply,
            } => Ok((
                OP_SUGGESTION_DECIDE,
                CommandJson::from_serializable(&Decision {
                    module_id,
                    suggestion_id,
                    expected_version: *expected_version,
                    action: if *apply { "apply" } else { "reject" },
                })?,
            )),
        }
    }
}

struct StagedSuggestion {
    id: String,
    state: &'static str,
    version: i64,
    writes: Vec<NativeCommand>,
}

struct SuggestionEffects<'a> {
    actor: &'a RecordContextActor,
    module_id: &'a str,
    record_id: &'a str,
    operation: &'a SuggestionOperation,
    now: i64,
    staged: Option<StagedSuggestion>,
}

fn suggestion_write(now: i64, kind: &str, values: Vec<serde_json::Value>) -> NativeCommand {
    NativeCommand {
        kind: kind.into(),
        values,
        now,
    }
}

impl SuggestionEffects<'_> {
    fn stage_request(
        &self,
        actor: &CommandActor,
        context: RecordContextDocument,
        field_key: &str,
    ) -> Result<StagedSuggestion, BusinessError> {
        let fields: Vec<RecordFieldInput> = serde_json::from_value(context.record.fields)
            .map_err(|_| erpx_server_records::public::record_context::source_invalid())?;
        let id = Ids.next().as_str().to_owned();
        let plan = plan_suggestion_request(
            &SuggestionInput {
                field_key: field_key.into(),
            },
            self.module_id,
            self.record_id,
            &fields,
            &context.references,
            &id,
            |raw| {
                Ok(hex::encode(Sha256::digest(
                    CommandJson::parse(raw)?.as_str(),
                )))
            },
        )?;
        Ok(StagedSuggestion {
            id,
            state: "ready",
            version: context.record.version,
            writes: vec![suggestion_write(
                self.now,
                SUGGESTION_STATEMENT,
                plan.values(actor.user_id.as_str(), self.now),
            )],
        })
    }
}

impl PgOwnerCommandEffects for SuggestionEffects<'_> {
    async fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        sqlx::query(
            "LOCK TABLE workspace_records,record_reference_edges,record_field_suggestions \
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
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let mut repository = PgRecordContext::new(connection);
        let context = record_context_document(
            &mut repository,
            &mut crate::records::RecordsContextGuardImpl,
            self.actor,
            self.module_id,
            self.record_id,
            self.now,
        )
        .await?;
        let original_version = context.record.version;
        let staged = match self.operation {
            SuggestionOperation::Request { field_key } => {
                self.stage_request(current, context, field_key)?
            }
            SuggestionOperation::Decide {
                suggestion_id,
                expected_version,
                apply,
            } => {
                let stored = context
                    .suggestions
                    .iter()
                    .find(|row| row.id == *suggestion_id && row.state == "ready")
                    .ok_or_else(suggestion_unavailable)?;
                if original_version != *expected_version {
                    return Err(suggestion_conflict().into());
                }
                let mut writes = Vec::new();
                let mut version = original_version;
                let state = if *apply {
                    // The preview-only Modules in the current workspace catalogue.
                    let preview = matches!(
                        self.module_id,
                        "brand"
                            | "commercial"
                            | "organization"
                            | "platform_administration"
                            | "quality"
                            | "manage_erpx"
                    );
                    guard_social_owner(self.module_id, preview)?;
                    for source in parse_sources(&stored.source_versions_json)? {
                        let (module, id) = source
                            .record
                            .split_once('/')
                            .ok_or_else(suggestion_conflict)?;
                        if !self.actor.permits_module(module) {
                            return Err(forbidden().into());
                        }
                        let fact = repository
                            .link_target(self.actor, module, id)
                            .await?
                            .ok_or_else(record_not_found)?;
                        if fact.version != source.version {
                            return Err(BusinessError::new(
                                FailureKind::Conflict,
                                "CONFLICT",
                                "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
                            ).into());
                        }
                    }
                    let mut fields = context.record.fields;
                    let field = fields
                        .as_array_mut()
                        .and_then(|fields| {
                            fields.iter_mut().find(|field| {
                                field.get("key").and_then(serde_json::Value::as_str)
                                    == Some(&stored.field_key)
                            })
                        })
                        .and_then(|field| field.get_mut("value"))
                        .ok_or_else(suggestion_conflict)?;
                    *field = parse_proposed_value(&stored.proposed_value_json)?;
                    writes.push(suggestion_write(
                        self.now,
                        FIELD_WRITE_STATEMENT,
                        vec![
                            CommandJson::from_serializable(&fields)?.as_str().into(),
                            current.user_id.as_str().into(),
                            self.now.into(),
                            self.module_id.into(),
                            self.record_id.into(),
                        ],
                    ));
                    version += 1;
                    "applied"
                } else {
                    "rejected"
                };
                writes.push(suggestion_write(
                    self.now,
                    SUGGESTION_STATE_STATEMENT,
                    vec![
                        state.into(),
                        current.user_id.as_str().into(),
                        self.now.into(),
                        suggestion_id.clone().into(),
                    ],
                ));
                StagedSuggestion {
                    id: suggestion_id.clone(),
                    state,
                    version,
                    writes,
                }
            }
        };
        self.staged = Some(staged);
        Ok(WorkflowOutput {
            version: original_version,
            result: CommandJson::from_serializable(&serde_json::json!({"id": self.record_id}))?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        command: &WorkflowRequest,
    ) -> Result<(), Error> {
        let staged = self.staged.as_ref().ok_or_else(unavailable)?;
        let mut repository = PgNativeCommandRepository::new(connection);
        for write in &staged.writes {
            if execute_native_command(write, &command.key, &mut repository)
                .await?
                .rows
                != 1
            {
                return Err(suggestion_conflict().into());
            }
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let staged = self.staged.as_ref().ok_or_else(unavailable)?;
        let mut repository = PgRecordContext::new(connection);
        let record = repository
            .record(self.actor, self.module_id, self.record_id)
            .await?;
        let suggestions = repository
            .suggestions(self.module_id, self.record_id)
            .await?;
        if record.is_some_and(|record| record.version == staged.version)
            && suggestions
                .iter()
                .any(|row| row.id == staged.id && row.state == staged.state)
        {
            Ok(())
        } else {
            Err(suggestion_conflict().into())
        }
    }
}

struct TransitionEffects<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    module_id: &'a str,
    record_id: &'a str,
    key: &'a str,
    input: &'a TransitionRecordInput,
    prepared: Option<(i64, String, String)>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for TransitionEffects<'_, C> {
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
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let row: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT status,version,created_by FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3 AND archived_at IS NULL FOR UPDATE",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let (status, version, created_by) = row.ok_or_else(record_not_found)?;
        let can_review = self.current.require_permission("records.review").is_ok();
        let to_status = validate_transition(
            self.module_id,
            self.input,
            &status,
            version,
            can_review,
            &created_by,
            actor.user_id.as_str(),
        )?
        .to_owned();
        self.prepared = Some((version, status, to_status.clone()));
        Ok(WorkflowOutput {
            version: version + 1,
            result: CommandJson::from_serializable(&serde_json::json!({
                "id": self.record_id,
                "status": to_status,
                "version": version + 1,
            }))?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let (version, from_status, to_status) =
            self.prepared.as_ref().ok_or_else(unavailable)?.clone();
        let now = self.clock.now().get();
        let updated = sqlx::query(
            "UPDATE workspace_records SET status=$1,version=$2,updated_by=$3,updated_at=$4 \
             WHERE module_id=$5 AND id=$6 AND company_id=$7 AND version=$8 AND archived_at IS NULL",
        )
        .bind(&to_status)
        .bind(version + 1)
        .bind(current.user_id.as_str())
        .bind(now)
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .bind(version)
        .execute(&mut *connection)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(version_conflict().into());
        }
        let inserted = sqlx::query(
            "INSERT INTO workspace_record_transitions(id,module_id,record_id,from_status,to_status,\
             action,note,actor_id,assignment_id,record_version,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(self.key)
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(&from_status)
        .bind(&to_status)
        .bind(&self.input.action)
        .bind(&self.input.note)
        .bind(current.user_id.as_str())
        .bind(current.assignment_id.as_str())
        .bind(version + 1)
        .bind(now)
        .execute(&mut *connection)
        .await?;
        if inserted.rows_affected() != 1 {
            return Err(changed().into());
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        current: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let (version, from_status, to_status) =
            self.prepared.as_ref().ok_or_else(unavailable)?.clone();
        let row: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT status,version,updated_by FROM workspace_records \
             WHERE module_id=$1 AND id=$2 AND company_id=$3",
        )
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(self.current.selection().workspace().company_id.as_str())
        .fetch_optional(&mut *connection)
        .await?;
        let Some((status, current_version, updated_by)) = row else {
            return Err(record_not_found().into());
        };
        if status != to_status
            || current_version != version + 1
            || updated_by != current.user_id.as_str()
        {
            return Err(changed().into());
        }
        let transition: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM workspace_record_transitions \
             WHERE id=$1 AND module_id=$2 AND record_id=$3 AND from_status=$4 AND to_status=$5 \
             AND action=$6 AND note=$7 AND actor_id=$8 AND assignment_id=$9 AND record_version=$10",
        )
        .bind(self.key)
        .bind(self.module_id)
        .bind(self.record_id)
        .bind(&from_status)
        .bind(&to_status)
        .bind(&self.input.action)
        .bind(&self.input.note)
        .bind(current.user_id.as_str())
        .bind(current.assignment_id.as_str())
        .bind(version + 1)
        .fetch_one(&mut *connection)
        .await?;
        if transition != 1 {
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

    pub async fn reference<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        reference: ReferenceRecordInput,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        Ok(self
            .reference_with_context(host, proof, input, module_id, record_id, reference, key)
            .await?
            .0)
    }

    pub async fn reference_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        reference: ReferenceRecordInput,
        key: CommandKey,
    ) -> Result<(CommandJson, serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        validate_reference(&module_id, &record_id, &reference)?;
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let wire = CommandJson::from_serializable(&serde_json::json!({
            "moduleId": module_id,
            "targetModule": reference.target_module,
            "targetId": reference.target_id,
            "kind": reference.kind,
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
                    let reference = reference.clone();
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
                            operation: "record.reference".into(),
                            record_id: record_id.clone(),
                            input: wire,
                        };
                        let clock_ref = clock.as_ref();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                        };
                        let mut effects = ReferenceEffects {
                            current,
                            clock: clock_ref,
                            module_id: &module_id,
                            record_id: &record_id,
                            input: &reference,
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
                            }),
                        ))
                    })
                },
            )
            .await
    }

    pub async fn mention_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        control: DatabasePool,
        module_id: String,
        record_id: String,
        mention: AddPrincipalMentionInput,
        key: Option<CommandKey>,
    ) -> Result<(serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        erpx_server_records::public::record_context::validate_request(&module_id, &record_id)?;
        validate_mention_input(&mention)?;
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let key = match key {
            Some(key) => key,
            None => CommandKey::new(Ids.next().as_str())?,
        };
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: OP_MENTION.into(),
            record_id,
            input: CommandJson::from_serializable(&MentionReceiptInput {
                module_id: &module_id,
                mention: &mention,
            })?,
        };
        // Resolve the Control fact before the authorizer takes its Control
        // lease, so concurrent Holding writes never wait for a second slot.
        let display_name = if proof.active.is_some() {
            active_principal_display_name(&control, &mention.target_id).await
        } else {
            Ok(None)
        };
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
                    let mention = mention.clone();
                    let request = request.clone();
                    let display_name = display_name.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        current.require_permission("records.read")?;
                        require_module(current, &module_id)?;
                        let actor = crate::records::records_actor(current)?;
                        let now = clock.now().get();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                        };
                        let mut effects = MentionEffects {
                            display_name: &display_name,
                            actor: &actor,
                            module_id: &module_id,
                            record_id: &request.record_id,
                            input: &mention,
                            now,
                            write: None,
                        };
                        let mut ids = Ids;
                        PgCommandLedger::new(
                            company,
                            connection,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        let data = record_context(
                            &mut PgRecordContext::new(connection),
                            &mut crate::records::RecordsContextGuardImpl,
                            &actor,
                            &module_id,
                            &request.record_id,
                            now,
                        )
                        .await?;
                        Ok((data, crate::records::context_value(current)))
                    })
                },
            )
            .await
    }

    pub(crate) async fn suggestion_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        operation: SuggestionOperation,
        key: Option<CommandKey>,
    ) -> Result<(serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        erpx_server_records::public::record_context::validate_request(&module_id, &record_id)?;
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let key = match key {
            Some(key) => key,
            None => CommandKey::new(Ids.next().as_str())?,
        };
        let (name, wire) = operation.receipt_input(&module_id)?;
        let request = WorkflowRequest {
            key: key.as_str().to_owned(),
            operation: name.into(),
            record_id,
            input: wire,
        };
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
                    let operation = operation.clone();
                    let request = request.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.read")?;
                        let actor = crate::records::records_actor(current)?;
                        let now = clock.now().get();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock.as_ref(),
                        };
                        let mut effects = SuggestionEffects {
                            actor: &actor,
                            module_id: &module_id,
                            record_id: &request.record_id,
                            operation: &operation,
                            now,
                            staged: None,
                        };
                        let mut ids = Ids;
                        PgCommandLedger::new(
                            company,
                            connection,
                            &mut authority,
                            clock.as_ref(),
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        let data = record_context(
                            &mut PgRecordContext::new(connection),
                            &mut crate::records::RecordsContextGuardImpl,
                            &actor,
                            &module_id,
                            &request.record_id,
                            now,
                        )
                        .await?;
                        Ok((data, crate::records::context_value(current)))
                    })
                },
            )
            .await
    }

    pub async fn transition<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        transition: TransitionRecordInput,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        Ok(self
            .transition_with_context(host, proof, input, module_id, record_id, transition, key)
            .await?
            .0)
    }

    pub async fn transition_with_context<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        module_id: String,
        record_id: String,
        transition: TransitionRecordInput,
        key: CommandKey,
    ) -> Result<(CommandJson, serde_json::Value, serde_json::Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        if !valid_record_id(&record_id)
            || !(1..=MAX_SAFE).contains(&transition.expected_version)
            || !TRANSITION_ACTIONS.contains(&transition.action.as_str())
            || transition.note.encode_utf16().count() > 1_000
        {
            return Err(BusinessError::invalid(
                "TRANSITION_INPUT_INVALID",
                "Choose a valid transition and note.",
            )
            .into());
        }
        let holding = self.configuration.holding_id().clone();
        let company = self.configuration.company().clone();
        let clock = self.clock.clone();
        let wire = CommandJson::from_serializable(&serde_json::json!({
            "expectedVersion": transition.expected_version,
            "action": transition.action,
            "note": transition.note,
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
                    let transition = transition.clone();
                    let command_key = key.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("records.write")?;
                        require_module(current, &module_id)?;
                        if matches!(transition.action.as_str(), "approve" | "reject") {
                            current.require_permission("records.review")?;
                        }
                        let request = WorkflowRequest {
                            key: command_key.as_str().to_owned(),
                            operation: format!("record.{}", transition.action),
                            record_id: record_id.clone(),
                            input: wire,
                        };
                        let clock_ref = clock.as_ref();
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                        };
                        let mut effects = TransitionEffects {
                            current,
                            clock: clock_ref,
                            module_id: &module_id,
                            record_id: &record_id,
                            key: command_key.as_str(),
                            input: &transition,
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
                        let row: (String, String, String, String, String, i64, String, i64) =
                            sqlx::query_as(
                                "SELECT section_id,title,description,status,fields_json,version,                                 created_by,updated_at FROM workspace_records                                  WHERE module_id=$1 AND id=$2 AND company_id=$3",
                            )
                            .bind(&module_id)
                            .bind(&record_id)
                            .bind(current.selection().workspace().company_id.as_str())
                            .fetch_one(&mut *connection)
                            .await?;
                        let fields: serde_json::Value =
                            serde_json::from_str(&row.4).map_err(|_| unavailable())?;
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
}
