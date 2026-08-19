use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{EventPosition, IdempotencyKey};
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
};
use erpx_kernel::{AggregateId, CommandId, EventId};
use erpx_operations::{
    PortError, SourceCommit, SourceDatabase, SourceHighWatermarkPort, SourceIdentity,
    SourceUnitOfWork, StoredSourceCommit,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    CONTEXT_NAME, EVENT_VERSION, EXCEPTION_AGGREGATE_TYPE, EXCEPTION_SNAPSHOT_EVENT,
    authority::{apply_exception_authority, load_exception_authority},
    command::{deterministic_uuid, exception_directory_draft, map_store_error},
    exception_partition,
    projection::{OperationsOutboxPayload, OperationsSnapshot, source_context_name},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCheckpoint {
    pub database: SourceDatabase,
    pub source_key: String,
    pub partition: String,
    pub sequence: u64,
    pub event_id: Option<EventId>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OperationsSourceAdapter {
    store: CommandStore,
    pool: WritePool,
}

#[derive(Debug, Clone)]
pub struct OperationsSourceHighWatermarkReader {
    pool: WritePool,
}

impl OperationsSourceHighWatermarkReader {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SourceHighWatermarkPort for OperationsSourceHighWatermarkReader {
    async fn source_high_watermark(&self) -> Result<u64, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let position: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(global_sequence), 0)
             FROM erpx_cqrs_write.outbox_events
             WHERE holding_id=erpx_cqrs_write.current_holding_id()
               AND context_name=$1",
        )
        .bind(CONTEXT_NAME)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        u64::try_from(position).map_err(|_| PortError::Internal)
    }
}

impl OperationsSourceAdapter {
    /// The legacy raw handle is accepted only for composition compatibility;
    /// every source receipt/checkpoint query uses the verified holding-bound
    /// write capability and therefore fails closed without an exact holding.
    pub fn new(write: WritePool, _pool: PgPool) -> Self {
        Self {
            store: CommandStore::new(write.clone()),
            pool: write,
        }
    }

    pub async fn checkpoint(
        &self,
        database: SourceDatabase,
        source_key: &str,
        partition: &str,
    ) -> Result<Option<SourceCheckpoint>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query("SELECT source_sequence,source_event_id,source_occurred_at,updated_at FROM erpx_operations_write.source_checkpoint_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND source_database=$1 AND source_key=$2 AND source_partition=$3")
            .bind(database.as_str()).bind(source_key).bind(partition)
            .fetch_optional(&mut *transaction).await.map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        row.map(|row| {
            Ok(SourceCheckpoint {
                database,
                source_key: source_key.into(),
                partition: partition.into(),
                sequence: u64::try_from(
                    row.try_get::<i64, _>("source_sequence")
                        .map_err(|_| PortError::Internal)?,
                )
                .map_err(|_| PortError::Internal)?,
                event_id: row
                    .try_get::<Option<Uuid>, _>("source_event_id")
                    .map_err(|_| PortError::Internal)?
                    .map(EventId::try_from_uuid)
                    .transpose()
                    .map_err(|_| PortError::Internal)?,
                occurred_at: row
                    .try_get("source_occurred_at")
                    .map_err(|_| PortError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
            })
        })
        .transpose()
    }

    async fn load_receipt(
        &self,
        identity: &SourceIdentity,
    ) -> Result<Option<StoredSourceCommit>, PortError> {
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query("SELECT source_fingerprint,disposition,exception_id,applied_at FROM erpx_operations_write.source_receipt_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND source_event_id=$1")
            .bind(identity.source.event_id().as_uuid()).fetch_optional(&mut *transaction).await.map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let stored: Vec<u8> = row
            .try_get("source_fingerprint")
            .map_err(|_| PortError::Internal)?;
        if stored.as_slice() != source_hash(identity)?.as_bytes() {
            return Err(PortError::SourceIdentityConflict);
        }
        let disposition: String = row
            .try_get("disposition")
            .map_err(|_| PortError::Internal)?;
        let exception_id: Option<Uuid> = row
            .try_get("exception_id")
            .map_err(|_| PortError::Internal)?;
        if (disposition == "ignored") != exception_id.is_none() {
            return Err(PortError::Internal);
        }
        let exception = if let Some(id) = exception_id {
            let id =
                erpx_operations::ExceptionId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
            load_exception_authority(&self.pool, identity.source.scope(), id).await?
        } else {
            None
        };
        if disposition == "detected" && exception.is_none() {
            return Err(PortError::Internal);
        }
        if exception.as_ref().is_some_and(|value| {
            value.source().context != identity.source.source_context()
                || value.source().event_id != identity.source.event_id()
        }) {
            return Err(PortError::SourceIdentityConflict);
        }
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let rows = sqlx::query("SELECT event_id,global_sequence FROM erpx_cqrs_write.outbox_events WHERE context_name=$1 AND command_id=$2 ORDER BY global_sequence")
            .bind(CONTEXT_NAME).bind(identity.source.event_id().as_uuid()).fetch_all(&mut *transaction).await.map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let committed_events = rows
            .into_iter()
            .map(|row| {
                Ok(EventPosition {
                    event_id: EventId::try_from_uuid(
                        row.try_get("event_id").map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                    global_sequence: u64::try_from(
                        row.try_get::<i64, _>("global_sequence")
                            .map_err(|_| PortError::Internal)?,
                    )
                    .map_err(|_| PortError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>, PortError>>()?;
        Ok(Some(StoredSourceCommit {
            source_event_id: identity.source.event_id(),
            exception,
            committed_events,
            applied_at: row.try_get("applied_at").map_err(|_| PortError::Internal)?,
            replayed: true,
        }))
    }
}

#[async_trait]
impl SourceUnitOfWork for OperationsSourceAdapter {
    async fn find_source_receipt(
        &self,
        identity: &SourceIdentity,
    ) -> Result<Option<StoredSourceCommit>, PortError> {
        identity.position.validate(&identity.source)?;
        self.load_receipt(identity).await
    }

    async fn apply_source(&self, request: SourceCommit) -> Result<StoredSourceCommit, PortError> {
        request
            .identity
            .position
            .validate(&request.identity.source)?;
        if let Some(value) = self.load_receipt(&request.identity).await? {
            return Ok(value);
        }
        if let Some(aggregate) = request.aggregate.clone() {
            self.apply_detected(
                request.identity,
                request.actor,
                request.audit,
                request.evaluated_at,
                aggregate,
                request.integration_events,
            )
            .await
        } else {
            self.apply_ignored(request).await
        }
    }
}

impl OperationsSourceAdapter {
    async fn apply_ignored(&self, request: SourceCommit) -> Result<StoredSourceCommit, PortError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        lock_and_validate_position(&mut tx, &request.identity)
            .await
            .map_err(map_store_error)?;
        let fingerprint = source_hash(&request.identity)?;
        let inserted = sqlx::query("INSERT INTO erpx_operations_write.source_receipt_authority (holding_id,source_event_id,source_context,source_fingerprint,disposition,exception_id,applied_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,'ignored',NULL,$4) ON CONFLICT (holding_id,source_event_id) DO NOTHING")
            .bind(request.identity.source.event_id().as_uuid()).bind(source_context_name(request.identity.source.source_context())).bind(fingerprint.as_bytes().as_slice()).bind(request.evaluated_at)
            .execute(&mut *tx).await.map_err(|_| PortError::Unavailable)?;
        if inserted.rows_affected() != 1 {
            tx.rollback().await.ok();
            return self
                .load_receipt(&request.identity)
                .await?
                .ok_or(PortError::SourceIdentityConflict);
        }
        advance_checkpoint(&mut tx, &request.identity, request.evaluated_at)
            .await
            .map_err(map_store_error)?;
        let command_id = CommandId::try_from_uuid(request.identity.source.event_id().as_uuid())
            .map_err(|_| PortError::Internal)?;
        sqlx::query("INSERT INTO erpx_cqrs_write.audit_log (audit_id,context_name,partition_key,command_id,correlation_id,active_scope,actor,audit_context,action,aggregate_type,aggregate_id,aggregate_version,details,occurred_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12,$13)")
            .bind(deterministic_uuid(command_id.as_uuid(), b"operations-source-ignore-audit-v1")).bind(CONTEXT_NAME)
            .bind(&request.identity.position.partition).bind(command_id.as_uuid()).bind(request.identity.source.correlation_id().as_uuid())
            .bind(serde_json::to_value(request.identity.source.scope()).map_err(|_| PortError::Internal)?)
            .bind(serde_json::to_value(request.actor).map_err(|_| PortError::Internal)?)
            .bind(serde_json::to_value(request.audit).map_err(|_| PortError::Internal)?)
            .bind("operations.source.ignore").bind("operations_source_receipt").bind(request.identity.source.event_id().as_uuid())
            .bind(json!({"source_context": source_context_name(request.identity.source.source_context()), "source_sequence": request.identity.position.sequence}))
            .bind(request.evaluated_at).execute(&mut *tx).await.map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        Ok(StoredSourceCommit {
            source_event_id: request.identity.source.event_id(),
            exception: None,
            committed_events: Vec::new(),
            applied_at: request.evaluated_at,
            replayed: false,
        })
    }

    async fn apply_detected(
        &self,
        identity: SourceIdentity,
        actor: erpx_kernel::ActorAttribution,
        audit: erpx_kernel::AuditContext,
        evaluated_at: DateTime<Utc>,
        aggregate: erpx_operations::OperationsException,
        integration_events: Vec<erpx_integration_contracts::operations::Event>,
    ) -> Result<StoredSourceCommit, PortError> {
        if aggregate.version() != 1 || aggregate.source().event_id != identity.source.event_id() {
            return Err(PortError::Internal);
        }
        let command_id = CommandId::try_from_uuid(identity.source.event_id().as_uuid())
            .map_err(|_| PortError::Internal)?;
        let partition = exception_partition(aggregate.id());
        let mut events = integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_source_event_id(
                    identity.source.event_id(),
                    event.event_type(),
                    index,
                ),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: Some(identity.source.event_id()),
                occurred_at: evaluated_at,
                payload: OperationsOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();
        events.push(OutboxEventWrite {
            event_id: deterministic_source_event_id(
                identity.source.event_id(),
                EXCEPTION_SNAPSHOT_EVENT,
                events.len(),
            ),
            event_type: EXCEPTION_SNAPSHOT_EVENT.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: Some(identity.source.event_id()),
            occurred_at: evaluated_at,
            payload: OperationsOutboxPayload::Snapshot(OperationsSnapshot::exception(
                &aggregate,
                identity.source.event_version(),
                Some(identity.position.clone()),
            )),
        });
        events.push(OutboxEventWrite {
            event_id: deterministic_source_event_id(
                identity.source.event_id(),
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                events.len(),
            ),
            event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            partition: partition.clone(),
            causation_id: Some(identity.source.event_id()),
            occurred_at: evaluated_at,
            payload: OperationsOutboxPayload::Directory(exception_directory_draft(&aggregate)?),
        });
        let idempotency_key = IdempotencyKey::try_new(format!(
            "operations-source:{}:{}",
            source_context_name(identity.source.source_context()),
            identity.source.event_id().as_uuid()
        ))
        .map_err(|_| PortError::Internal)?;
        let request_hash = source_hash(&identity)?;
        let aggregate_id = AggregateId::try_from_uuid(aggregate.id().as_uuid())
            .map_err(|_| PortError::Internal)?;
        let guard = SourceAtomicGuard {
            identity: identity.clone(),
            aggregate: aggregate.clone(),
            evaluated_at,
        };
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: identity.source.correlation_id(),
            idempotency_key,
            request_hash,
            active_scope: aggregate.scope().clone(),
            actor,
            audit_context: audit,
            aggregate_type: EXCEPTION_AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: None,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(
                    command_id.as_uuid(),
                    b"operations-source-detect-audit-v1",
                ),
                action: "operations.source.detect".into(),
                occurred_at: evaluated_at,
                details: json!({"source_context": source_context_name(identity.source.source_context()), "source_sequence": identity.position.sequence}),
            },
            events,
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(v) | AtomicCommandOutcome::Replayed(v) => v,
        };
        Ok(StoredSourceCommit {
            source_event_id: identity.source.event_id(),
            exception: Some(receipt.response),
            committed_events: receipt.committed_events,
            applied_at: receipt.committed_at,
            replayed,
        })
    }
}

struct SourceAtomicGuard {
    identity: SourceIdentity,
    aggregate: erpx_operations::OperationsException,
    evaluated_at: DateTime<Utc>,
}

#[async_trait]
impl AtomicCommandGuard for SourceAtomicGuard {
    async fn authoritative_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        _context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        sqlx::query_scalar::<_, i64>(
            "SELECT aggregate_version FROM erpx_operations_write.exception_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND exception_id=$1 FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .map(|value| u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
        .transpose()
    }

    async fn validate(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != EXCEPTION_AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != exception_partition(self.aggregate.id())
            || context.expected_version.is_some()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Operations source command metadata mismatch",
            ));
        }
        lock_and_validate_position(tx, &self.identity).await?;
        let existing: Option<Vec<u8>> = sqlx::query_scalar("SELECT source_fingerprint FROM erpx_operations_write.source_receipt_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND source_event_id=$1 FOR UPDATE")
            .bind(self.identity.source.event_id().as_uuid()).fetch_optional(&mut **tx).await?;
        if existing.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "Operations source receipt already exists",
            ));
        }
        let collision: Option<Uuid> = sqlx::query_scalar("SELECT exception_id FROM erpx_operations_write.exception_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND source_context=$1 AND source_event_id=$2 FOR UPDATE")
            .bind(source_context_name(self.identity.source.source_context())).bind(self.identity.source.event_id().as_uuid()).fetch_optional(&mut **tx).await?;
        if collision.is_some() {
            return Err(CqrsSqlxError::GuardRejected(
                "Operations source event already detected",
            ));
        }
        Ok(())
    }

    async fn apply(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.aggregate_version != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "Operations source aggregate must start at version one",
            ));
        }
        apply_exception_authority(tx, &self.aggregate).await?;
        let source = &self.identity.source;
        let fingerprint = source_hash(&self.identity).map_err(|_| {
            CqrsSqlxError::InvalidInput("Operations source identity cannot be hashed")
        })?;
        sqlx::query("INSERT INTO erpx_operations_write.source_receipt_authority (holding_id,source_event_id,source_context,source_fingerprint,disposition,exception_id,applied_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,'detected',$4,$5)")
            .bind(source.event_id().as_uuid()).bind(source_context_name(source.source_context())).bind(fingerprint.as_bytes().as_slice()).bind(self.aggregate.id().as_uuid()).bind(self.evaluated_at)
            .execute(&mut **tx).await?;
        advance_checkpoint(tx, &self.identity, self.evaluated_at).await?;
        Ok(())
    }
}

async fn lock_and_validate_position(
    tx: &mut Transaction<'_, Postgres>,
    identity: &SourceIdentity,
) -> Result<(), CqrsSqlxError> {
    identity
        .position
        .validate(&identity.source)
        .map_err(|_| CqrsSqlxError::InvalidInput("invalid Operations source position"))?;
    let current: Option<i64> = sqlx::query_scalar("SELECT source_sequence FROM erpx_operations_write.source_checkpoint_authority WHERE holding_id=erpx_cqrs_write.current_holding_id() AND source_database=$1 AND source_key=$2 AND source_partition=$3 FOR UPDATE")
        .bind(identity.position.database.as_str()).bind(&identity.position.source_key).bind(&identity.position.partition).fetch_optional(&mut **tx).await?;
    if current.is_some_and(|value| {
        u64::try_from(value)
            .ok()
            .is_none_or(|value| value >= identity.position.sequence)
    }) {
        return Err(CqrsSqlxError::GuardRejected(
            "Operations source position is duplicate or out of order",
        ));
    }
    Ok(())
}

async fn advance_checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    identity: &SourceIdentity,
    at: DateTime<Utc>,
) -> Result<(), CqrsSqlxError> {
    let result = sqlx::query("INSERT INTO erpx_operations_write.source_checkpoint_authority (holding_id,source_database,source_key,source_partition,source_sequence,source_event_id,source_occurred_at,updated_at) VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7) ON CONFLICT (holding_id,source_database,source_key,source_partition) DO UPDATE SET source_sequence=EXCLUDED.source_sequence,source_event_id=EXCLUDED.source_event_id,source_occurred_at=EXCLUDED.source_occurred_at,updated_at=EXCLUDED.updated_at WHERE erpx_operations_write.source_checkpoint_authority.source_sequence < EXCLUDED.source_sequence")
        .bind(identity.position.database.as_str()).bind(&identity.position.source_key).bind(&identity.position.partition)
        .bind(i64::try_from(identity.position.sequence).map_err(|_| CqrsSqlxError::NumericRange)?).bind(identity.source.event_id().as_uuid()).bind(identity.source.occurred_at()).bind(at)
        .execute(&mut **tx).await?;
    if result.rows_affected() != 1 {
        return Err(CqrsSqlxError::GuardRejected(
            "Operations source checkpoint CAS failed",
        ));
    }
    Ok(())
}

fn source_hash(value: &SourceIdentity) -> Result<RequestHash, PortError> {
    Ok(RequestHash::new(
        Sha256::digest(serde_json::to_vec(value).map_err(|_| PortError::Internal)?).into(),
    ))
}

fn deterministic_source_event_id(source: EventId, event_type: &str, index: usize) -> EventId {
    EventId::try_from_uuid(deterministic_uuid(
        source.as_uuid(),
        format!("operations-source-event-v1:{event_type}:{index}").as_bytes(),
    ))
    .expect("non-nil deterministic event")
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_path_has_separate_ignored_and_detected_atomic_flows() {
        let source = include_str!("source.rs");
        assert!(source.contains("apply_ignored"));
        assert!(source.contains("apply_detected"));
        assert!(source.contains("SourceAtomicGuard"));
        assert!(source.contains("advance_checkpoint"));
        assert!(source.contains("source_fingerprint"));
    }

    #[test]
    fn source_positions_are_never_compared_across_database_keys() {
        let source = include_str!("source.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(
            production.contains("source_database=$1 AND source_key=$2 AND source_partition=$3")
        );
        assert!(!production.contains("MAX(source_sequence)"));
    }

    #[test]
    fn source_high_watermark_uses_the_committed_cqrs_position() {
        let source = include_str!("source.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("COALESCE(MAX(global_sequence), 0)"));
        assert!(production.contains("context_name=$1"));
        assert!(!production.contains("operations_events"));
        assert!(!production.contains("COUNT(*) FROM erpx_cqrs_write.outbox_events"));
    }
}
