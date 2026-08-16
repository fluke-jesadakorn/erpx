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
    pub response: R,
    pub audit: AuditWrite,
    pub events: Vec<OutboxEventWrite<E>>,
}

/// Read-only command identity exposed to a context guard before normalized
/// authority is written. The guard may lock and validate context-owned rows,
/// but it cannot commit or access a raw pool.
#[derive(Debug, Clone, Copy)]
pub struct GuardCommandContext<'a> {
    pub context: &'a str,
    pub partition: &'a str,
    pub aggregate_type: &'a str,
    pub aggregate_id: AggregateId,
    pub expected_version: Option<u64>,
}

/// The committed aggregate version exposed to a context guard after the
/// normalized write and before audit/outbox/receipt completion.
#[derive(Debug, Clone, Copy)]
pub struct AppliedGuardContext<'a> {
    pub context: &'a str,
    pub partition: &'a str,
    pub aggregate_type: &'a str,
    pub aggregate_id: AggregateId,
    pub aggregate_version: u64,
}

/// Context-owned invariant/uniqueness hook executed inside the exact shared
/// command transaction. `validate` is the lock/read phase; `apply` persists
/// the next typed relational state after the optimistic version fence passes.
/// Any error rolls back the authority update, receipt, audit, and outbox.
#[async_trait::async_trait]
pub trait AtomicCommandGuard: Send + Sync {
    /// Locks and returns the context-owned authoritative aggregate version.
    ///
    /// Context adapters using [`CommandStore::commit_normalized`] must
    /// implement this method against their normalized write model. The
    /// default deliberately fails closed so every adapter must define its
    /// normalized concurrency fence.
    async fn authoritative_version(
        &self,
        _transaction: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        Err(CqrsSqlxError::GuardRejected(
            "normalized authoritative version is not implemented",
        ))
    }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtomicCommandOutcome<R> {
    Committed(CommandReceipt<R>),
    Replayed(CommandReceipt<R>),
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
        let mut transaction = self.pool.begin().await?;
        let statement = if self.pool.tenant_id().is_some() {
            "SELECT command_id, request_hash, status, receipt
             FROM erpx_cqrs_write.command_receipts
             WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
               AND tenant_id = $4"
        } else {
            "SELECT command_id, request_hash, status, receipt
             FROM erpx_cqrs_write.command_receipts
             WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3"
        };
        let query = sqlx::query(statement)
            .bind(context)
            .bind(partition)
            .bind(idempotency_key.as_str());
        let row = match self.pool.tenant_id() {
            Some(tenant_id) => {
                query
                    .bind(tenant_id)
                    .fetch_optional(&mut *transaction)
                    .await?
            }
            None => query.fetch_optional(&mut *transaction).await?,
        };
        transaction.commit().await?;
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

    /// Commits a command whose authoritative state is stored exclusively in
    /// context-owned normalized tables.
    ///
    /// The guard must lock and report the normalized current version and
    /// persist the next version from `apply`. Receipts, audit evidence, and
    /// outbox facts remain in the same transaction.
    pub async fn commit_normalized<R, E, G>(
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
        let mut transaction = self.pool.begin().await?;
        let tenant_id = self.pool.tenant_id();
        let reservation = reserve_receipt(&mut transaction, tenant_id, &command).await?;
        if !reservation {
            let receipt = load_replay_receipt(&mut transaction, tenant_id, &command).await?;
            transaction.commit().await?;
            return Ok(AtomicCommandOutcome::Replayed(receipt));
        }

        let guard_context = GuardCommandContext {
            context: &command.context,
            partition: &command.partition,
            aggregate_type: &command.aggregate_type,
            aggregate_id: command.aggregate_id,
            expected_version: command.expected_version,
        };
        let observed = guard
            .authoritative_version(&mut transaction, &guard_context)
            .await?;
        if observed != command.expected_version {
            return Err(CqrsSqlxError::OptimisticConcurrency {
                aggregate_type: command.aggregate_type.clone(),
                aggregate_id: command.aggregate_id.as_uuid(),
                expected: command.expected_version,
                actual: observed,
            });
        }
        let version = observed
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(CqrsSqlxError::NumericRange)?;
        guard.validate(&mut transaction, &guard_context).await?;

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
            expected_version: _,
            response,
            audit,
            events,
        } = command;
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
                    tenant_id,
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
            tenant_id,
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
            tenant_id,
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
    tenant_id: Option<Uuid>,
    command: &AtomicCommand<R, E>,
) -> Result<bool, CqrsSqlxError> {
    let statement = if tenant_id.is_some() {
        "INSERT INTO erpx_cqrs_write.command_receipts
            (context_name, partition_key, idempotency_key, command_id, request_hash,
             correlation_id, status, created_at, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, 'reserved', now(), $7)
         ON CONFLICT DO NOTHING"
    } else {
        "INSERT INTO erpx_cqrs_write.command_receipts
            (context_name, partition_key, idempotency_key, command_id, request_hash,
             correlation_id, status, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'reserved', now())
         ON CONFLICT DO NOTHING"
    };
    let query = sqlx::query(statement)
        .bind(&command.context)
        .bind(&command.partition)
        .bind(command.idempotency_key.as_str())
        .bind(command.command_id.as_uuid())
        .bind(command.request_hash.as_bytes().as_slice())
        .bind(command.correlation_id.as_uuid());
    let inserted = match tenant_id {
        Some(tenant_id) => query.bind(tenant_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    if inserted.rows_affected() == 1 {
        return Ok(true);
    }

    let statement = if tenant_id.is_some() {
        "SELECT command_id, request_hash, receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
           AND tenant_id = $4 FOR UPDATE"
    } else {
        "SELECT command_id, request_hash, receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
         FOR UPDATE"
    };
    let query = sqlx::query(statement)
        .bind(&command.context)
        .bind(&command.partition)
        .bind(command.idempotency_key.as_str());
    let by_key = match tenant_id {
        Some(tenant_id) => {
            query
                .bind(tenant_id)
                .fetch_optional(&mut **transaction)
                .await?
        }
        None => query.fetch_optional(&mut **transaction).await?,
    };
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
    tenant_id: Option<Uuid>,
    command: &AtomicCommand<R, E>,
) -> Result<CommandReceipt<R>, CqrsSqlxError>
where
    R: DeserializeOwned,
{
    let statement = if tenant_id.is_some() {
        "SELECT receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
           AND tenant_id = $4"
    } else {
        "SELECT receipt
         FROM erpx_cqrs_write.command_receipts
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3"
    };
    let query = sqlx::query_scalar(statement)
        .bind(&command.context)
        .bind(&command.partition)
        .bind(command.idempotency_key.as_str());
    let value: Value = match tenant_id {
        Some(tenant_id) => query.bind(tenant_id).fetch_one(&mut **transaction).await?,
        None => query.fetch_one(&mut **transaction).await?,
    };
    let mut receipt: CommandReceipt<R> = serde_json::from_value(value)?;
    receipt.replayed = true;
    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
async fn append_outbox<E: Serialize>(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Option<Uuid>,
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
    let statement = if tenant_id.is_some() {
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id, context_name, partition_key, command_id, event_type, event_version,
             aggregate_type, aggregate_id, aggregate_version, tenant_scope, actor,
             correlation_id, causation_id, payload, occurred_at, available_at, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now(), $16)
         RETURNING global_sequence"
    } else {
        "INSERT INTO erpx_cqrs_write.outbox_events
            (event_id, context_name, partition_key, command_id, event_type, event_version,
             aggregate_type, aggregate_id, aggregate_version, tenant_scope, actor,
             correlation_id, causation_id, payload, occurred_at, available_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
         RETURNING global_sequence"
    };
    let query = sqlx::query_scalar(statement)
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
        .bind(event.occurred_at);
    let global_sequence: i64 = match tenant_id {
        Some(tenant_id) => query.bind(tenant_id).fetch_one(&mut **transaction).await?,
        None => query.fetch_one(&mut **transaction).await?,
    };
    Ok(EventPosition {
        event_id: event.event_id,
        global_sequence: to_u64(global_sequence)?,
    })
}

#[allow(clippy::too_many_arguments)]
async fn persist_audit(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Option<Uuid>,
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
    let statement = if tenant_id.is_some() {
        "INSERT INTO erpx_cqrs_write.audit_log
            (audit_id, context_name, partition_key, command_id, correlation_id,
             tenant_scope, actor, audit_context, action, aggregate_type,
             aggregate_id, aggregate_version, details, occurred_at, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"
    } else {
        "INSERT INTO erpx_cqrs_write.audit_log
            (audit_id, context_name, partition_key, command_id, correlation_id,
             tenant_scope, actor, audit_context, action, aggregate_type,
             aggregate_id, aggregate_version, details, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"
    };
    let query = sqlx::query(statement)
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
        .bind(audit.occurred_at);
    match tenant_id {
        Some(tenant_id) => query.bind(tenant_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    Ok(())
}

async fn complete_receipt<R: Serialize>(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: Option<Uuid>,
    context: &str,
    partition: &str,
    idempotency_key: &str,
    receipt: &CommandReceipt<R>,
    completed_at: DateTime<Utc>,
) -> Result<(), CqrsSqlxError> {
    let statement = if tenant_id.is_some() {
        "UPDATE erpx_cqrs_write.command_receipts
         SET status = 'completed', receipt = $4, completed_at = $5
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
           AND status = 'reserved' AND tenant_id = $6"
    } else {
        "UPDATE erpx_cqrs_write.command_receipts
         SET status = 'completed', receipt = $4, completed_at = $5
         WHERE context_name = $1 AND partition_key = $2 AND idempotency_key = $3
           AND status = 'reserved'"
    };
    let query = sqlx::query(statement)
        .bind(context)
        .bind(partition)
        .bind(idempotency_key)
        .bind(serde_json::to_value(receipt)?)
        .bind(completed_at);
    let updated = match tenant_id {
        Some(tenant_id) => query.bind(tenant_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
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
