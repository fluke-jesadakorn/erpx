use chrono::{DateTime, Utc};
use erpx_cqrs::{AggregateVersion, CommandReceipt, EventPosition, IdempotencyKey};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuditContext, CommandId, CorrelationId, EventId, TenantScope,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{CqrsSqlxError, WritePool};

const MAX_CONTEXT_LENGTH: usize = 128;
const MAX_AGGREGATE_TYPE_LENGTH: usize = 128;
const MAX_EVENT_TYPE_LENGTH: usize = 256;
const MAX_PARTITION_LENGTH: usize = 256;
const MAX_AUDIT_ACTION_LENGTH: usize = 256;

/// Stable digest of the canonical command request. Hashing stays in the
/// application layer so transport-specific encodings can never affect replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestHash([u8; 32]);

impl RequestHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct AuditWrite {
    pub audit_id: Uuid,
    pub action: String,
    pub occurred_at: DateTime<Utc>,
    pub details: Value,
}

/// An event remains typed until the atomic commit serializes it at the SQLx
/// boundary. A context normally uses its published integration-event enum as E.
#[derive(Debug, Clone)]
pub struct OutboxEventWrite<E> {
    pub event_id: EventId,
    pub event_type: String,
    pub event_version: u32,
    pub partition: String,
    pub causation_id: Option<EventId>,
    pub occurred_at: DateTime<Utc>,
    pub payload: E,
}

#[derive(Debug, Clone)]
pub struct AtomicCommand<R, E> {
    pub context: String,
    pub partition: String,
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub idempotency_key: IdempotencyKey,
    pub request_hash: RequestHash,
    pub tenant_scope: TenantScope,
    pub actor: ActorAttribution,
    pub audit_context: AuditContext,
    pub aggregate_type: String,
    pub aggregate_id: AggregateId,
    pub expected_version: Option<u64>,
    pub aggregate_state: Value,
    pub response: R,
    pub audit: AuditWrite,
    pub events: Vec<OutboxEventWrite<E>>,
}

/// Read-only command identity exposed to a context guard before the shared
/// snapshot is written. The guard may lock and validate context-owned guard
/// rows, but it cannot commit or access a raw pool.
#[derive(Debug, Clone, Copy)]
pub struct GuardCommandContext<'a> {
    pub context: &'a str,
    pub partition: &'a str,
    pub aggregate_type: &'a str,
    pub aggregate_id: AggregateId,
    pub expected_version: Option<u64>,
}

/// The committed aggregate version exposed to a context guard after the
/// snapshot write and before audit/outbox/receipt completion.
#[derive(Debug, Clone, Copy)]
pub struct AppliedGuardContext<'a> {
    pub context: &'a str,
    pub partition: &'a str,
    pub aggregate_type: &'a str,
    pub aggregate_id: AggregateId,
    pub aggregate_version: u64,
}

/// Context-owned invariant/uniqueness hook executed inside the exact shared
/// command transaction. `validate` is the lock/read phase; `apply` updates the
/// context guard rows only after optimistic snapshot persistence succeeds.
/// Any error rolls back the reservation, snapshot, guard, audit, and outbox.
#[async_trait::async_trait]
pub trait AtomicCommandGuard: Send + Sync {
    async fn validate(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError>;

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopAtomicCommandGuard;

#[async_trait::async_trait]
impl AtomicCommandGuard for NoopAtomicCommandGuard {
    async fn validate(
        &self,
        _transaction: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        Ok(())
    }

    async fn apply(
        &self,
        _transaction: &mut Transaction<'_, Postgres>,
        _context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicCommandOutcome<R> {
    Committed(CommandReceipt<R>),
    Replayed(CommandReceipt<R>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSnapshot<T> {
    pub version: u64,
    pub state: T,
}

impl<R> AtomicCommandOutcome<R> {
    pub fn receipt(&self) -> &CommandReceipt<R> {
        match self {
            Self::Committed(receipt) | Self::Replayed(receipt) => receipt,
        }
    }

    pub const fn was_replayed(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

#[derive(Debug, Clone)]
pub struct CommandStore {
    pool: WritePool,
}

impl CommandStore {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    /// Loads authoritative aggregate state for a command handler. This method
    /// exists only on the write-side store and is intentionally absent from all
    /// query/read-side adapters.
    pub async fn load_snapshot<T>(
        &self,
        context: &str,
        partition: &str,
        aggregate_type: &str,
        aggregate_id: AggregateId,
    ) -> Result<Option<StoredSnapshot<T>>, CqrsSqlxError>
    where
        T: DeserializeOwned,
    {
        validate_text(context, MAX_CONTEXT_LENGTH)?;
        validate_text(partition, MAX_PARTITION_LENGTH)?;
        validate_text(aggregate_type, MAX_AGGREGATE_TYPE_LENGTH)?;
        let row = sqlx::query(
            "SELECT aggregate_version, state
             FROM erpx_cqrs_write.aggregate_snapshots
             WHERE context_name = $1 AND partition_key = $2
               AND aggregate_type = $3 AND aggregate_id = $4",
        )
        .bind(context)
        .bind(partition)
        .bind(aggregate_type)
        .bind(aggregate_id.as_uuid())
        .fetch_optional(self.pool.inner())
        .await?;
        row.map(|row| {
            let version = to_u64(row.try_get("aggregate_version")?)?;
            let state = serde_json::from_value(row.try_get("state")?)?;
            Ok(StoredSnapshot { version, state })
        })
        .transpose()
    }

    /// Returns a completed receipt only when the stable command identity and
    /// canonical request hash are exact. A reused key fails closed.
    pub async fn find_receipt<R>(
        &self,
        context: &str,
        partition: &str,
        idempotency_key: &IdempotencyKey,
        command_id: CommandId,
        request_hash: RequestHash,
    ) -> Result<Option<CommandReceipt<R>>, CqrsSqlxError>
    where
        R: DeserializeOwned,
    {
        validate_text(context, MAX_CONTEXT_LENGTH)?;
        validate_text(partition, MAX_PARTITION_LENGTH)?;
        let row = sqlx::query(
            "SELECT command_id, request_hash, status, receipt
             FROM erpx_cqrs_write.command_receipts
             WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3",
        )
        .bind(context)
        .bind(partition)
        .bind(idempotency_key.as_str())
        .fetch_optional(self.pool.inner())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let stored_command_id: Uuid = row.try_get("command_id")?;
        if stored_command_id != command_id.as_uuid() {
            return Err(CqrsSqlxError::CommandReplayConflict);
        }
        let stored_hash: Vec<u8> = row.try_get("request_hash")?;
        if stored_hash.as_slice() != request_hash.as_bytes() {
            return Err(CqrsSqlxError::IdempotencyConflict);
        }
        let status: String = row.try_get("status")?;
        let receipt: Option<Value> = row.try_get("receipt")?;
        if status != "completed" || receipt.is_none() {
            return Err(CqrsSqlxError::InvalidInput(
                "persisted command reservation is incomplete",
            ));
        }
        let mut receipt: CommandReceipt<R> =
            serde_json::from_value(receipt.expect("receipt presence checked"))?;
        receipt.replayed = true;
        Ok(Some(receipt))
    }

    /// Atomically writes the snapshot, replay receipt, audit evidence, and all
    /// outbox events. Any error drops the transaction and rolls back the whole
    /// command. Existing matching receipts are returned without touching state.
    pub async fn commit<R, E>(
        &self,
        command: AtomicCommand<R, E>,
    ) -> Result<AtomicCommandOutcome<R>, CqrsSqlxError>
    where
        R: Serialize + DeserializeOwned,
        E: Serialize,
    {
        self.commit_guarded(command, &NoopAtomicCommandGuard).await
    }

    /// Commit through a context-owned guard without exposing `WritePool` or a
    /// transaction completion capability to the adapter.
    pub async fn commit_guarded<R, E, G>(
        &self,
        command: AtomicCommand<R, E>,
        guard: &G,
    ) -> Result<AtomicCommandOutcome<R>, CqrsSqlxError>
    where
        R: Serialize + DeserializeOwned,
        E: Serialize,
        G: AtomicCommandGuard + ?Sized,
    {
        validate_command(&command)?;
        let mut transaction = self.pool.inner().begin().await?;
        let reservation = reserve_receipt(&mut transaction, &command).await?;
        if !reservation {
            let receipt = load_replay_receipt(&mut transaction, &command).await?;
            transaction.commit().await?;
            return Ok(AtomicCommandOutcome::Replayed(receipt));
        }

        guard
            .validate(
                &mut transaction,
                &GuardCommandContext {
                    context: &command.context,
                    partition: &command.partition,
                    aggregate_type: &command.aggregate_type,
                    aggregate_id: command.aggregate_id,
                    expected_version: command.expected_version,
                },
            )
            .await?;

        let AtomicCommand {
            context,
            partition,
            command_id,
            correlation_id,
            idempotency_key,
            request_hash: _,
            tenant_scope,
            actor,
            audit_context,
            aggregate_type,
            aggregate_id,
            expected_version,
            aggregate_state,
            response,
            audit,
            events,
        } = command;

        let version = persist_snapshot(
            &mut transaction,
            &context,
            &partition,
            &aggregate_type,
            aggregate_id,
            expected_version,
            aggregate_state,
        )
        .await?;
        guard
            .apply(
                &mut transaction,
                &AppliedGuardContext {
                    context: &context,
                    partition: &partition,
                    aggregate_type: &aggregate_type,
                    aggregate_id,
                    aggregate_version: version,
                },
            )
            .await?;
        let mut positions = Vec::with_capacity(events.len());
        for event in events {
            positions.push(
                append_outbox(
                    &mut transaction,
                    &context,
                    &partition,
                    command_id,
                    correlation_id,
                    &tenant_scope,
                    &actor,
                    &aggregate_type,
                    aggregate_id,
                    version,
                    event,
                )
                .await?,
            );
        }
        persist_audit(
            &mut transaction,
            &context,
            &partition,
            command_id,
            correlation_id,
            &tenant_scope,
            &actor,
            &audit_context,
            &aggregate_type,
            aggregate_id,
            version,
            audit,
        )
        .await?;

        let committed_at = Utc::now();
        let receipt = CommandReceipt {
            command_id,
            correlation_id,
            aggregate: AggregateVersion {
                context: context.clone(),
                aggregate_type,
                aggregate_id,
                version,
            },
            committed_events: positions,
            committed_at,
            replayed: false,
            response,
        };
        complete_receipt(
            &mut transaction,
            &context,
            &partition,
            idempotency_key.as_str(),
            &receipt,
            committed_at,
        )
        .await?;
        transaction.commit().await?;
        Ok(AtomicCommandOutcome::Committed(receipt))
    }
}

fn validate_command<R, E>(command: &AtomicCommand<R, E>) -> Result<(), CqrsSqlxError> {
    validate_text(&command.context, MAX_CONTEXT_LENGTH)?;
    validate_text(&command.partition, MAX_PARTITION_LENGTH)?;
    validate_text(&command.aggregate_type, MAX_AGGREGATE_TYPE_LENGTH)?;
    validate_text(&command.audit.action, MAX_AUDIT_ACTION_LENGTH)?;
    if command.audit.audit_id.is_nil() {
        return Err(CqrsSqlxError::InvalidInput("audit id must not be nil"));
    }
    command
        .tenant_scope
        .validate()
        .map_err(|_| CqrsSqlxError::InvalidInput("tenant scope is invalid"))?;
    command
        .actor
        .validate()
        .map_err(|_| CqrsSqlxError::InvalidInput("actor is invalid"))?;
    command
        .audit_context
        .validate()
        .map_err(|_| CqrsSqlxError::InvalidInput("audit context is invalid"))?;
    for event in &command.events {
        validate_text(&event.event_type, MAX_EVENT_TYPE_LENGTH)?;
        validate_text(&event.partition, MAX_PARTITION_LENGTH)?;
        if event.event_version == 0 {
            return Err(CqrsSqlxError::InvalidInput(
                "event version must be greater than zero",
            ));
        }
        if event.partition != command.partition {
            return Err(CqrsSqlxError::InvalidInput(
                "event partition must match command partition",
            ));
        }
    }
    Ok(())
}

fn validate_text(value: &str, maximum: usize) -> Result<(), CqrsSqlxError> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(CqrsSqlxError::InvalidInput(
            "bounded persistence identifier is invalid",
        ));
    }
    Ok(())
}

async fn reserve_receipt<R, E>(
    transaction: &mut Transaction<'_, Postgres>,
    command: &AtomicCommand<R, E>,
) -> Result<bool, CqrsSqlxError> {
    let inserted = sqlx::query(
        "INSERT INTO erpx_cqrs_write.command_receipts
            (context_name, partition_key, idempotency_key, command_id, request_hash,
             correlation_id, status, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'reserved', now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&command.context)
    .bind(&command.partition)
    .bind(command.idempotency_key.as_str())
    .bind(command.command_id.as_uuid())
    .bind(command.request_hash.as_bytes().as_slice())
    .bind(command.correlation_id.as_uuid())
    .execute(&mut **transaction)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(true);
    }

    let by_key = sqlx::query(
        "SELECT command_id, request_hash, receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
         FOR UPDATE",
    )
    .bind(&command.context)
    .bind(&command.partition)
    .bind(command.idempotency_key.as_str())
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = by_key else {
        return Err(CqrsSqlxError::CommandReplayConflict);
    };
    let existing_command: Uuid = row.try_get("command_id")?;
    let existing_hash: Vec<u8> = row.try_get("request_hash")?;
    if existing_command != command.command_id.as_uuid() {
        return Err(CqrsSqlxError::CommandReplayConflict);
    }
    if existing_hash.as_slice() != command.request_hash.as_bytes() {
        return Err(CqrsSqlxError::IdempotencyConflict);
    }
    let receipt: Option<Value> = row.try_get("receipt")?;
    if receipt.is_none() {
        return Err(CqrsSqlxError::InvalidInput(
            "persisted command reservation has no receipt",
        ));
    }
    Ok(false)
}

async fn load_replay_receipt<R, E>(
    transaction: &mut Transaction<'_, Postgres>,
    command: &AtomicCommand<R, E>,
) -> Result<CommandReceipt<R>, CqrsSqlxError>
where
    R: DeserializeOwned,
{
    let value: Value = sqlx::query_scalar(
        "SELECT receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3",
    )
    .bind(&command.context)
    .bind(&command.partition)
    .bind(command.idempotency_key.as_str())
    .fetch_one(&mut **transaction)
    .await?;
    let mut receipt: CommandReceipt<R> = serde_json::from_value(value)?;
    receipt.replayed = true;
    Ok(receipt)
}

async fn persist_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    context: &str,
    partition: &str,
    aggregate_type: &str,
    aggregate_id: AggregateId,
    expected_version: Option<u64>,
    state: Value,
) -> Result<u64, CqrsSqlxError> {
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT aggregate_version
         FROM erpx_cqrs_write.aggregate_snapshots
         WHERE context_name = $1 AND partition_key = $2
           AND aggregate_type = $3 AND aggregate_id = $4
         FOR UPDATE",
    )
    .bind(context)
    .bind(partition)
    .bind(aggregate_type)
    .bind(aggregate_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await?;
    let current = current.map(to_u64).transpose()?;
    if let Some(expected) = expected_version {
        let observed = current.unwrap_or(0);
        if observed != expected {
            return Err(CqrsSqlxError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id: aggregate_id.as_uuid(),
                expected: Some(expected),
                actual: current,
            });
        }
    }
    let next = current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(CqrsSqlxError::NumericRange)?;
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.aggregate_snapshots
            (context_name, partition_key, aggregate_type, aggregate_id,
             aggregate_version, state, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, now())
         ON CONFLICT (context_name, partition_key, aggregate_type, aggregate_id)
         DO UPDATE SET aggregate_version = EXCLUDED.aggregate_version,
                       state = EXCLUDED.state, updated_at = now()",
    )
    .bind(context)
    .bind(partition)
    .bind(aggregate_type)
    .bind(aggregate_id.as_uuid())
    .bind(to_i64(next)?)
    .bind(state)
    .execute(&mut **transaction)
    .await?;
    Ok(next)
}

#[allow(clippy::too_many_arguments)]
async fn append_outbox<E: Serialize>(
    transaction: &mut Transaction<'_, Postgres>,
    context: &str,
    partition: &str,
    command_id: CommandId,
    correlation_id: CorrelationId,
    tenant_scope: &TenantScope,
    actor: &ActorAttribution,
    aggregate_type: &str,
    aggregate_id: AggregateId,
    aggregate_version: u64,
    event: OutboxEventWrite<E>,
) -> Result<EventPosition, CqrsSqlxError> {
    let payload = serde_json::to_value(event.payload)?;
    let tenant_scope = serde_json::to_value(tenant_scope)?;
    let actor = serde_json::to_value(actor)?;
    let global_sequence: i64 = sqlx::query_scalar(
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id, context_name, partition_key, command_id, event_type, event_version,
             aggregate_type, aggregate_id, aggregate_version, tenant_scope, actor,
             correlation_id, causation_id, payload, occurred_at, available_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
         RETURNING global_sequence",
    )
    .bind(event.event_id.as_uuid())
    .bind(context)
    .bind(partition)
    .bind(command_id.as_uuid())
    .bind(event.event_type)
    .bind(i32::try_from(event.event_version).map_err(|_| CqrsSqlxError::NumericRange)?)
    .bind(aggregate_type)
    .bind(aggregate_id.as_uuid())
    .bind(to_i64(aggregate_version)?)
    .bind(tenant_scope)
    .bind(actor)
    .bind(correlation_id.as_uuid())
    .bind(event.causation_id.map(EventId::as_uuid))
    .bind(payload)
    .bind(event.occurred_at)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(EventPosition {
        event_id: event.event_id,
        global_sequence: to_u64(global_sequence)?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn persist_audit(
    transaction: &mut Transaction<'_, Postgres>,
    context: &str,
    partition: &str,
    command_id: CommandId,
    correlation_id: CorrelationId,
    tenant_scope: &TenantScope,
    actor: &ActorAttribution,
    audit_context: &AuditContext,
    aggregate_type: &str,
    aggregate_id: AggregateId,
    aggregate_version: u64,
    audit: AuditWrite,
) -> Result<(), CqrsSqlxError> {
    sqlx::query(
        "INSERT INTO erpx_cqrs_write.audit_log
            (audit_id, context_name, partition_key, command_id, correlation_id,
             tenant_scope, actor, audit_context, action, aggregate_type,
             aggregate_id, aggregate_version, details, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(audit.audit_id)
    .bind(context)
    .bind(partition)
    .bind(command_id.as_uuid())
    .bind(correlation_id.as_uuid())
    .bind(serde_json::to_value(tenant_scope)?)
    .bind(serde_json::to_value(actor)?)
    .bind(serde_json::to_value(audit_context)?)
    .bind(audit.action)
    .bind(aggregate_type)
    .bind(aggregate_id.as_uuid())
    .bind(to_i64(aggregate_version)?)
    .bind(audit.details)
    .bind(audit.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn complete_receipt<R: Serialize>(
    transaction: &mut Transaction<'_, Postgres>,
    context: &str,
    partition: &str,
    idempotency_key: &str,
    receipt: &CommandReceipt<R>,
    completed_at: DateTime<Utc>,
) -> Result<(), CqrsSqlxError> {
    let updated = sqlx::query(
        "UPDATE erpx_cqrs_write.command_receipts
         SET status = 'completed', receipt = $4, completed_at = $5
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
           AND status = 'reserved'",
    )
    .bind(context)
    .bind(partition)
    .bind(idempotency_key)
    .bind(serde_json::to_value(receipt)?)
    .bind(completed_at)
    .execute(&mut **transaction)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(CqrsSqlxError::InvalidInput(
            "command receipt reservation was not completed",
        ));
    }
    Ok(())
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn to_u64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_hash_has_a_fixed_database_representation() {
        let hash = RequestHash::new([7; 32]);
        assert_eq!(hash.as_bytes().len(), 32);
        assert_eq!(hash.as_bytes(), &[7; 32]);
    }

    #[test]
    fn replay_outcome_preserves_the_receipt_but_marks_the_outcome() {
        let receipt = CommandReceipt {
            command_id: CommandId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(2)).unwrap(),
            aggregate: AggregateVersion {
                context: "quality".into(),
                aggregate_type: "inspection".into(),
                aggregate_id: AggregateId::try_from_uuid(Uuid::from_u128(3)).unwrap(),
                version: 1,
            },
            committed_events: Vec::new(),
            committed_at: Utc::now(),
            replayed: true,
            response: "ok",
        };
        let outcome = AtomicCommandOutcome::Replayed(receipt);
        assert!(outcome.was_replayed());
        assert_eq!(outcome.receipt().response, "ok");
    }

    #[test]
    fn database_ranges_reject_negative_and_oversized_positions() {
        assert_eq!(
            to_u64(-1).unwrap_err().to_string(),
            CqrsSqlxError::NumericRange.to_string()
        );
        assert!(to_i64(u64::MAX).is_err());
    }
}
