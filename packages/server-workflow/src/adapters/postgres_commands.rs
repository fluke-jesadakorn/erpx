use crate::{
    application::commands::{CommandActor, CommandEventScope, command_fingerprint},
    domain::{
        command_json::{CommandJson, OrderedJson},
        guest_account_compat::{
            canonical_guest_account_json, legacy_guest_account_json, legacy_guest_account_operation,
        },
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, IdentifierSource};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
/// Trusted application descriptor. Input/result are normalized typed-owner data,
/// not inbound actor, permission, SQL or transaction instructions.
pub struct WorkflowCommand {
    pub key: String,
    pub operation: String,
    pub record_id: String,
    pub version: i64,
    pub input: CommandJson,
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
        command: &WorkflowCommand,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn authorize(
        &mut self,
        connection: &mut PgConnection,
        command: &WorkflowCommand,
    ) -> impl Future<Output = Result<CommandActor, Error>> + Send;
    /// Replay is supplied only after this exact historical request has matched.
    /// Owners retain their real History/NewWork/settlement purchase purpose.
    fn verify_command(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowCommand,
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
        command: &WorkflowCommand,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowCommand,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn retain(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &WorkflowCommand,
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
    pub async fn execute<E: PgOwnerCommandEffects>(
        &mut self,
        command: &WorkflowCommand,
        effects: &mut E,
    ) -> Result<CommandOutcome, Error> {
        validate(command)?;
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
        let actor = self.current(command).await?;
        let digest = command_fingerprint(
            &actor,
            &command.operation,
            &command.record_id,
            &command.input,
        )?;
        let row=sqlx::query("SELECT request_hash,result_json,operation,actor_id,record_id,authority_json FROM command_receipts WHERE command_id=$1 FOR SHARE").bind(&command.key).fetch_optional(&mut *self.connection).await?;
        if let Some(row) = row {
            let old_hash: String = row.try_get("request_hash")?;
            let historical_operation = legacy_guest_account_operation(&command.operation);
            let historical_input = legacy_guest_account_json(&command.input)?;
            let mut matches = old_hash == digest
                || old_hash
                    == command_fingerprint(
                        &actor,
                        &historical_operation,
                        &command.record_id,
                        &historical_input,
                    )?;
            if !matches
                && historical_operation != command.operation
                && row.try_get::<String, _>("operation")? == historical_operation
                && row.try_get::<String, _>("actor_id")? == actor.user_id.as_str()
                && row.try_get::<String, _>("record_id")? == command.record_id
                && let Some(saved) = row.try_get::<Option<String>, _>("authority_json")?
                && let Some(historical) = crate::application::commands::historical_actor(
                    &actor,
                    &CommandJson::parse(&saved)?,
                )
            {
                matches = old_hash
                    == command_fingerprint(
                        &historical,
                        &historical_operation,
                        &command.record_id,
                        &historical_input,
                    )?;
            }
            if !matches {
                return Err(conflict(
                    "IDEMPOTENCY_CONFLICT",
                    "This command key belongs to another request.",
                )
                .into());
            }
            self.authority
                .verify_command(self.connection, &actor, command, CommandDisposition::Replay)
                .await?;
            self.require_same(command, &actor).await?;
            let result = canonical_guest_account_json(&CommandJson::parse(
                &row.try_get::<String, _>("result_json")?,
            )?)?;
            return Ok(CommandOutcome {
                result,
                replayed: true,
            });
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
        let now = self.clock.now().get();
        let authority = actor.receipt_authority()?;
        // A conflicting row committed after an earlier SERIALIZABLE snapshot
        // raises real40001 before this no-op UPDATE can modify that receipt.
        // There is no provisional result: the typed owner predeclares its output.
        let inserted:String=sqlx::query_scalar("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO UPDATE SET command_id=excluded.command_id RETURNING request_hash")
            .bind(&command.key).bind(&command.operation).bind(actor.user_id.as_str()).bind(&digest).bind(&command.record_id).bind(command.result.as_str()).bind(now).bind(authority.as_str()).fetch_one(&mut *self.connection).await?;
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
            .bind(audit_id.as_str()).bind(&command.key).bind(actor.user_id.as_str()).bind(actor.assignment_id.as_str()).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(command.version).bind(now).execute(&mut *self.connection).await?;
        let value = command.result.value()?;
        let state = match value.get("state") {
            None | Some(OrderedJson::Null) => value.get("status"),
            other => other,
        }
        .and_then(OrderedJson::string);
        let scope = command
            .event_scope
            .clone()
            .unwrap_or_else(|| CommandEventScope::from(&actor));
        let payload = actor.outbox_payload(&command.record_id, command.version, state, &scope)?;
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(event_id.as_str()).bind(&command.key).bind(&command.operation).bind(&aggregate).bind(&command.record_id).bind(command.version).bind(payload.as_str()).bind(now).execute(&mut *self.connection).await?;
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
            result: command.result.clone(),
            replayed: false,
        })
    }
    async fn current(&mut self, command: &WorkflowCommand) -> Result<CommandActor, Error> {
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
        command: &WorkflowCommand,
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
fn validate(command: &WorkflowCommand) -> Result<(), BusinessError> {
    if !(8..=180).contains(&command.key.len())
        || !command
            .key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b':' | b'_' | b'.' | b'-'))
    {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        ));
    }
    if command.operation.is_empty()
        || command.record_id.is_empty()
        || command.version < 0
        || command.version > 9_007_199_254_740_991
    {
        return Err(BusinessError::invalid(
            "COMMAND_DESCRIPTOR_INVALID",
            "Provide the typed owner command identity.",
        ));
    }
    Ok(())
}
fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
