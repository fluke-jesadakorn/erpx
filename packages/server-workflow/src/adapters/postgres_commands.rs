use crate::{
    application::commands::{
        CommandActor, CommandEventScope, command_fingerprint, command_fingerprint_source_ordered,
    },
    domain::{
        command_json::{CommandJson, OrderedJson},
        guest_account_compat::{
            canonical_guest_account_json, legacy_guest_account_json, legacy_guest_account_operation,
        },
        record_identity::WorkflowRecordIdentity,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
/// Trusted request identity. No result preparation or current business state is
/// required to inspect an exact receipt; this data supplies no authority.
pub struct WorkflowRequest {
    pub key: String,
    pub operation: String,
    pub record_id: String,
    pub input: CommandJson,
}
/// Source-ordered raw key and logical record data. This is not authority.
pub struct SourceOrderedWorkflowRequest {
    pub key: String,
    pub operation: String,
    pub record_id: WorkflowRecordIdentity,
    pub input: CommandJson,
}
impl SourceOrderedWorkflowRequest {
    fn physical_request(&self) -> WorkflowRequest {
        WorkflowRequest {
            key: self.key.clone(),
            operation: self.operation.clone(),
            record_id: self.record_id.physical_text().to_owned(),
            input: self.input.clone(),
        }
    }
}
/// Prepared only for a new command after current policy and exact replay lookup.
pub struct WorkflowOutput {
    pub version: i64,
    pub result: CommandJson,
    pub event_scope: Option<CommandEventScope>,
}
pub struct CommandOutcome {
    pub result: CommandJson,
    pub replayed: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandDisposition {
    Execute,
    Replay,
}
/// Actual composition supplies current Identity, permissions, purchase/readiness
/// and original assignment scope. No permissive default or actor DTO is supplied.
pub trait PgCommandAuthority: Send {
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn authorize(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<CommandActor, Error>> + Send;
    /// Replay is supplied only after this exact historical request has matched.
    /// Owners retain their real History/NewWork/settlement purchase purpose.
    fn verify_command(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
/// The owner must implement all phases on this same protected connection.
/// Prepare is utility-only before the first snapshot; retain validates its exact
/// post-effects source/receipt and finishes any required deferred effects.
pub trait PgOwnerCommandEffects: Send {
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    /// Read-only canonical owner preparation. Never call providers or mutate
    /// canonical business rows here; staged guard data remains in this unit.
    /// Exact replay never calls this method.
    fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<WorkflowOutput, Error>> + Send;
    fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn retain(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowRequest,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
/// Caller owns the Holding transaction/Control lease and propagates any error
/// into that unit. No pool checkout, independent commit/retry or external call.
pub struct PgCommandLedger<'c, Authority, ClockSource, Identifiers> {
    company: CompanyId,
    connection: &'c mut PgConnection,
    authority: &'c mut Authority,
    clock: &'c ClockSource,
    identifiers: &'c mut Identifiers,
}
impl<'c, A: PgCommandAuthority, C: Clock + Sync, Ids: IdentifierSource>
    PgCommandLedger<'c, A, C, Ids>
{
    pub fn new(
        company: CompanyId,
        connection: &'c mut PgConnection,
        authority: &'c mut A,
        clock: &'c C,
        identifiers: &'c mut Ids,
    ) -> Self {
        Self {
            company,
            connection,
            authority,
            clock,
            identifiers,
        }
    }
    /// A missing receipt never prepares effects or grants History/NewWork.
    /// Caller must retain this enclosing transaction and source fences.
    pub async fn replay<E: PgOwnerCommandEffects>(
        &mut self,
        command: &WorkflowRequest,
        effects: &mut E,
    ) -> Result<Option<CommandOutcome>, Error> {
        let actor = self.lock_and_current(command, effects).await?;
        self.read_receipt(command, &actor, None).await
    }
    pub async fn execute<E: PgOwnerCommandEffects>(
        &mut self,
        command: &WorkflowRequest,
        effects: &mut E,
    ) -> Result<CommandOutcome, Error> {
        let actor = self.lock_and_current(command, effects).await?;
        if let Some(outcome) = self.read_receipt(command, &actor, None).await? {
            return Ok(outcome);
        }
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        self.require_same(command, &actor).await?;
        let output = effects.prepare(self.connection, &actor, command).await?;
        if !(0..=9_007_199_254_740_991).contains(&output.version) {
            return Err(BusinessError::invalid(
                "COMMAND_OUTPUT_INVALID",
                "Provide a valid owner result version.",
            )
            .into());
        }
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        self.require_same(command, &actor).await?;
        let digest = command_fingerprint(
            &actor,
            &command.operation,
            &command.record_id,
            &command.input,
        )?;
        let now = self.clock.now().get();
        let authority = actor.receipt_authority()?;
        // A conflicting row committed after an earlier SERIALIZABLE snapshot
        // raises real40001 before this no-op UPDATE can modify that receipt.
        // There is no provisional result: the owner prepared its typed output.
        let inserted:String=sqlx::query_scalar("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash")
            .bind(&command.key).bind(&command.operation).bind(actor.user_id.as_str()).bind(&digest).bind(&command.record_id).bind(output.result.as_str()).bind(now).bind(authority.as_str()).fetch_one(&mut *self.connection).await?;
        if inserted != digest {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to another request.",
            )
            .into());
        }
        effects.apply(self.connection, &actor, command).await?;
        let audit_id = self.identifiers.next();
        let event_id = self.identifiers.next();
        let aggregate = command
            .operation
            .split('.')
            .take(2)
            .collect::<Vec<_>>()
            .join("_");
        sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,(SELECT authority_json FROM command_receipts WHERE command_id=$2))")
            .bind(audit_id.as_str()).bind(&command.key).bind(actor.user_id.as_str()).bind(actor.assignment_id.as_str()).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(output.version).bind(now).execute(&mut *self.connection).await?;
        let value = output.result.value()?;
        let state = match value.get("state") {
            None | Some(OrderedJson::Null) => value.get("status"),
            other => other,
        }
        .and_then(OrderedJson::string);
        let scope = output
            .event_scope
            .clone()
            .unwrap_or_else(|| CommandEventScope::from(&actor));
        let payload = actor.outbox_payload(&command.record_id, output.version, state, &scope)?;
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(event_id.as_str()).bind(&command.key).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(output.version).bind(payload.as_str()).bind(now).execute(&mut *self.connection).await?;
        effects.retain(self.connection, &actor, command).await?;
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(&command.key)
            .execute(&mut *self.connection)
            .await?;
        self.require_same(command, &actor).await?;
        Ok(CommandOutcome {
            result: output.result,
            replayed: false,
        })
    }
    /// Exact historical replay, without key-format admission. Current actor,
    /// transaction, source and history verification remain mandatory.
    pub async fn replay_source_ordered<E: PgOwnerCommandEffects>(
        &mut self,
        source: &SourceOrderedWorkflowRequest,
        effects: &mut E,
    ) -> Result<Option<CommandOutcome>, Error> {
        let command = &source.physical_request();
        let actor = self.lock_and_current_unchecked(command, effects).await?;
        self.read_receipt(command, &actor, Some(&source.record_id))
            .await
    }
    /// The caller has retained the real source and pre-key quote before this
    /// admission; no effect or receipt may be committed by that preparation.
    pub async fn execute_source_ordered<E: PgOwnerCommandEffects>(
        &mut self,
        source: &SourceOrderedWorkflowRequest,
        effects: &mut E,
    ) -> Result<CommandOutcome, Error> {
        validate_key(&source.key)?;
        let command = &source.physical_request();
        let actor = self.lock_and_current_unchecked(command, effects).await?;
        if let Some(outcome) = self
            .read_receipt(command, &actor, Some(&source.record_id))
            .await?
        {
            return Ok(outcome);
        }
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        self.require_same(command, &actor).await?;
        let output = effects.prepare(self.connection, &actor, command).await?;
        if !(0..=9_007_199_254_740_991).contains(&output.version) {
            return Err(BusinessError::invalid(
                "COMMAND_OUTPUT_INVALID",
                "Provide a valid owner result version.",
            )
            .into());
        }
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        self.require_same(command, &actor).await?;
        let digest = command_fingerprint_source_ordered(
            &actor,
            &command.operation,
            &source.record_id,
            &command.input,
        )?;
        let now = self.clock.now().get();
        let authority = actor.receipt_authority()?;
        // A conflicting row committed after an earlier SERIALIZABLE snapshot
        // raises real40001 before this no-op UPDATE can modify that receipt.
        // There is no provisional result: the owner prepared its typed output.
        let inserted:String=sqlx::query_scalar("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash")
            .bind(&command.key).bind(&command.operation).bind(actor.user_id.as_str()).bind(&digest).bind(&command.record_id).bind(output.result.as_str()).bind(now).bind(authority.as_str()).fetch_one(&mut *self.connection).await?;
        if inserted != digest {
            return Err(conflict(
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to another request.",
            )
            .into());
        }
        effects.apply(self.connection, &actor, command).await?;
        let audit_id = self.identifiers.next();
        let event_id = self.identifiers.next();
        let aggregate = command
            .operation
            .split('.')
            .take(2)
            .collect::<Vec<_>>()
            .join("_");
        sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$5,$9,(SELECT authority_json FROM command_receipts WHERE command_id=$2))")
            .bind(audit_id.as_str()).bind(&command.key).bind(actor.user_id.as_str()).bind(actor.assignment_id.as_str()).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(output.version).bind(now).execute(&mut *self.connection).await?;
        let value = output.result.value()?;
        let state = match value.get("state") {
            None | Some(OrderedJson::Null) => value.get("status"),
            other => other,
        }
        .and_then(OrderedJson::string);
        let scope = output
            .event_scope
            .clone()
            .unwrap_or_else(|| CommandEventScope::from(&actor));
        let payload = actor.outbox_payload_source_ordered(
            &source.record_id,
            output.version,
            state,
            &scope,
        )?;
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(event_id.as_str()).bind(&command.key).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(output.version).bind(payload.as_str()).bind(now).execute(&mut *self.connection).await?;
        effects.retain(self.connection, &actor, command).await?;
        self.authority
            .verify_command(
                self.connection,
                &actor,
                command,
                CommandDisposition::Execute,
            )
            .await?;
        sqlx::query("DELETE FROM command_assertions WHERE command_id=$1")
            .bind(&command.key)
            .execute(&mut *self.connection)
            .await?;
        self.require_same(command, &actor).await?;
        Ok(CommandOutcome {
            result: output.result,
            replayed: false,
        })
    }
    async fn lock_and_current<E: PgOwnerCommandEffects>(
        &mut self,
        command: &WorkflowRequest,
        effects: &mut E,
    ) -> Result<CommandActor, Error> {
        validate(command)?;
        self.lock_and_current_unchecked(command, effects).await
    }
    async fn lock_and_current_unchecked<E: PgOwnerCommandEffects>(
        &mut self,
        command: &WorkflowRequest,
        effects: &mut E,
    ) -> Result<CommandActor, Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into());
        }
        effects.lock_sources(self.connection, command).await?;
        self.authority
            .lock_sources(self.connection, command)
            .await?;
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("WORKFLOW_SERIALIZABLE_REQUIRED").into());
        }
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("workflow.command:{}", command.key))
            .execute(&mut *self.connection)
            .await?;
        self.current(command).await
    }
    async fn read_receipt(
        &mut self,
        command: &WorkflowRequest,
        actor: &CommandActor,
        logical: Option<&WorkflowRecordIdentity>,
    ) -> Result<Option<CommandOutcome>, Error> {
        let fingerprint = |actor: &CommandActor, operation: &str, input: &CommandJson| match logical
        {
            Some(id) => command_fingerprint_source_ordered(actor, operation, id, input),
            None => command_fingerprint(actor, operation, &command.record_id, input),
        };
        let digest = fingerprint(actor, &command.operation, &command.input)?;
        // Bound persisted legacy data before transferring it into this process.
        // A parser limit after fetching an arbitrary TEXT value is too late.
        let row = sqlx::query("SELECT
            CASE WHEN octet_length(request_hash)<=$2 THEN request_hash END request_hash,
            CASE WHEN octet_length(result_json)<=$2 THEN result_json END result_json,
            CASE WHEN octet_length(operation)<=$2 THEN operation END operation,
            CASE WHEN octet_length(actor_id)<=$2 THEN actor_id END actor_id,
            CASE WHEN octet_length(record_id)<=$2 THEN record_id END record_id,
            CASE WHEN octet_length(authority_json)<=$2 THEN authority_json END authority_json,
            (COALESCE(octet_length(request_hash),0)>$2 OR COALESCE(octet_length(result_json),0)>$2
             OR COALESCE(octet_length(operation),0)>$2 OR COALESCE(octet_length(actor_id),0)>$2
             OR COALESCE(octet_length(record_id),0)>$2 OR COALESCE(octet_length(authority_json),0)>$2) oversized
            FROM command_receipts WHERE command_id=$1 FOR SHARE")
            .bind(&command.key).bind(crate::domain::command_json::LIMIT as i32)
            .fetch_optional(&mut *self.connection).await?;
        if let Some(row) = row {
            if row.try_get::<bool, _>("oversized")? {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "COMMAND_RECEIPT_TOO_LARGE",
                    "The retained command exceeds the bounded receipt reader.",
                )
                .into());
            }
            let old_hash: String = row.try_get("request_hash")?;
            let historical_operation = legacy_guest_account_operation(&command.operation);
            let historical_input = legacy_guest_account_json(&command.input)?;
            let mut matches = old_hash == digest
                || old_hash == fingerprint(actor, &historical_operation, &historical_input)?;
            if !matches
                && historical_operation != command.operation
                && row.try_get::<String, _>("operation")? == historical_operation
                && row.try_get::<String, _>("actor_id")? == actor.user_id.as_str()
                && row.try_get::<String, _>("record_id")? == command.record_id
                && let Some(saved) = row.try_get::<Option<String>, _>("authority_json")?
                && let Some(historical) = crate::application::commands::historical_actor(
                    actor,
                    &CommandJson::parse(&saved)?,
                )
            {
                matches =
                    old_hash == fingerprint(&historical, &historical_operation, &historical_input)?;
            }
            if !matches {
                return Err(conflict(
                    "IDEMPOTENCY_CONFLICT",
                    "This command key belongs to another request.",
                )
                .into());
            }
            self.authority
                .verify_command(self.connection, actor, command, CommandDisposition::Replay)
                .await?;
            self.require_same(command, actor).await?;
            let result = canonical_guest_account_json(&CommandJson::parse(
                &row.try_get::<String, _>("result_json")?,
            )?)?;
            return Ok(Some(CommandOutcome {
                result,
                replayed: true,
            }));
        }
        Ok(None)
    }
    async fn current(&mut self, command: &WorkflowRequest) -> Result<CommandActor, Error> {
        let actor = self.authority.authorize(self.connection, command).await?;
        actor.validate()?;
        if actor.company_id != self.company {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "The command belongs to another Company.",
            )
            .into());
        }
        Ok(actor)
    }
    async fn require_same(
        &mut self,
        command: &WorkflowRequest,
        actor: &CommandActor,
    ) -> Result<(), Error> {
        if self.current(command).await? != *actor {
            return Err(conflict(
                "WORK_CONTEXT_CHANGED",
                "Current authority changed during this command.",
            )
            .into());
        }
        Ok(())
    }
}
fn validate(command: &WorkflowRequest) -> Result<(), BusinessError> {
    validate_key(&command.key)?;
    if command.operation.is_empty() || command.record_id.is_empty() {
        return Err(BusinessError::invalid(
            "COMMAND_DESCRIPTOR_INVALID",
            "Provide the typed owner command identity.",
        ));
    }
    Ok(())
}
fn validate_key(key: &str) -> Result<(), BusinessError> {
    if !(8..=180).contains(&key.len())
        || !key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.' | b'-'))
    {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        ));
    }
    Ok(())
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
