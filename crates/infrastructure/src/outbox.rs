use chrono::{DateTime, Duration, Utc};
use erpx_domain::cqrs::OutboxEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::worker::WorkerErrorContext;
use crate::{PersistenceError, SqlxControlPlane};

pub const MAX_OUTBOX_CLAIM_BATCH_SIZE: u32 = 1_000;
const MAX_PERSISTED_WORKER_ERROR_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    pub tenant_scope: Option<Value>,
    pub actor: Option<Value>,
    pub correlation_id: Option<Uuid>,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub event_version: u32,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            event_version: 1,
            ..Self::default_without_version()
        }
    }
}

impl EventMetadata {
    fn default_without_version() -> Self {
        Self {
            tenant_scope: None,
            actor: None,
            correlation_id: None,
            causation_id: None,
            idempotency_key: None,
            event_version: 0,
        }
    }
}

impl EventMetadata {
    pub fn for_event(event: &OutboxEvent) -> Self {
        Self {
            correlation_id: Some(event.id),
            event_version: 1,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEnvelope {
    pub id: Uuid,
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub tenant_scope: Option<Value>,
    pub actor: Option<Value>,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub attempts: u32,
    pub max_attempts: u32,
    pub claimed_by: Option<Uuid>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_context: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyState {
    Reserved,
    InProgress { command_id: Option<Uuid> },
    Completed { response: Value },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyReservation {
    pub key: String,
    pub state: IdempotencyState,
}

/// Receipt for an authenticated dead-letter replay. The receipt deliberately
/// contains only safe event identity and command correlation metadata; the
/// outbox payload, worker error, and MFA code never cross this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterReplayReceipt {
    pub event_id: Uuid,
    pub correlation_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetterReplayCommand<'a> {
    pub event_id: Uuid,
    pub available_at: DateTime<Utc>,
    pub actor_ref: &'a str,
    pub purpose: &'a str,
    pub correlation_id: Uuid,
    pub idempotency_key: &'a str,
    pub command_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionCheckpointRecord {
    pub projection_name: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub last_event_id: Option<Uuid>,
    pub last_aggregate_version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionCheckpointDecision {
    Apply,
    AlreadyApplied,
    ConflictingEvent { expected_event_id: Option<Uuid> },
    OutOfOrder { expected_version: u64 },
}

#[derive(Debug)]
pub struct ControlPlaneWriteTransaction<'a> {
    pub(crate) tx: Transaction<'a, Postgres>,
}

impl<'a> ControlPlaneWriteTransaction<'a> {
    pub(crate) fn new(tx: Transaction<'a, Postgres>) -> Self {
        Self { tx }
    }

    pub async fn append_outbox_event(
        &mut self,
        event: &OutboxEvent,
        metadata: &EventMetadata,
    ) -> Result<(), PersistenceError> {
        validate_outbox_event_for_append(event, metadata)?;
        let event_version = metadata.event_version;
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id, aggregate_version,
                 tenant_scope, actor, correlation_id, causation_id, idempotency_key, payload,
                 occurred_at, available_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, now())",
        )
        .bind(event.id)
        .bind(&event.event_type)
        .bind(i32::try_from(event_version).map_err(|_| {
            PersistenceError::InvalidData("event version exceeds database range".into())
        })?)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(as_i64(event.aggregate_version, "event aggregate version")?)
        .bind(&metadata.tenant_scope)
        .bind(&metadata.actor)
        .bind(metadata.correlation_id.unwrap_or(event.id))
        .bind(metadata.causation_id)
        .bind(&metadata.idempotency_key)
        .bind(&event.payload)
        .bind(event.occurred_at)
        .execute(&mut *self.tx)
        .await
        .map_err(map_unique_idempotency)?;
        Ok(())
    }

    pub async fn reserve_idempotency(
        &mut self,
        key: &str,
        request_hash: &str,
        command_id: Option<Uuid>,
    ) -> Result<IdempotencyReservation, PersistenceError> {
        validate_idempotency_key(key)?;
        if request_hash.trim().is_empty() {
            return Err(PersistenceError::InvalidData(
                "idempotency request hash cannot be empty".into(),
            ));
        }
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.idempotency_keys
                (idempotency_key, request_hash, command_id, status)
             VALUES ($1, $2, $3, 'in_progress')
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(key)
        .bind(request_hash)
        .bind(command_id)
        .execute(&mut *self.tx)
        .await?;
        if inserted.rows_affected() == 1 {
            return Ok(IdempotencyReservation {
                key: key.into(),
                state: IdempotencyState::Reserved,
            });
        }

        let row = sqlx::query(
            "SELECT request_hash, command_id, status, response
             FROM erpx_control.idempotency_keys WHERE idempotency_key = $1 FOR UPDATE",
        )
        .bind(key)
        .fetch_one(&mut *self.tx)
        .await?;
        let stored_hash = row.try_get::<String, _>("request_hash")?;
        if stored_hash != request_hash {
            return Err(PersistenceError::IdempotencyConflict { key: key.into() });
        }
        let status = row.try_get::<String, _>("status")?;
        let state = match status.as_str() {
            "in_progress" => IdempotencyState::InProgress {
                command_id: row.try_get("command_id")?,
            },
            "completed" => IdempotencyState::Completed {
                response: row
                    .try_get::<Option<Value>, _>("response")?
                    .ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "completed idempotency record has no response".into(),
                        )
                    })?,
            },
            value => {
                return Err(PersistenceError::InvalidData(format!(
                    "unknown idempotency status {value}"
                )));
            }
        };
        Ok(IdempotencyReservation {
            key: key.into(),
            state,
        })
    }

    pub async fn complete_idempotency(
        &mut self,
        key: &str,
        response: Value,
    ) -> Result<(), PersistenceError> {
        validate_idempotency_key(key)?;
        let result = sqlx::query(
            "UPDATE erpx_control.idempotency_keys
             SET status = 'completed', response = $2, completed_at = now()
             WHERE idempotency_key = $1 AND status = 'in_progress'",
        )
        .bind(key)
        .bind(response)
        .execute(&mut *self.tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(PersistenceError::InvalidData(
                "idempotency key is missing or not in progress".into(),
            ));
        }
        Ok(())
    }

    /// Append typed audit evidence in the same transaction as a normalized
    /// aggregate commit and its outbox event.
    pub async fn append_audit_event(
        &mut self,
        event: &erpx_application::AuditEvent,
    ) -> Result<(), PersistenceError> {
        let payload = serde_json::to_value(AuditEventPayload::from(event))?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(&event.actor_ref)
        .bind(event.customer_account_id)
        .bind(&event.action)
        .bind(&event.purpose)
        .bind(event.correlation_id)
        .bind(payload)
        .execute(&mut *self.tx)
        .await?;
        Ok(())
    }

    /// Finish idempotency with an immutable receipt bound to the normalized
    /// aggregate identity and event position.
    pub async fn complete_command(
        &mut self,
        key: &str,
        receipt: &erpx_application::CommandReceipt,
    ) -> Result<(), PersistenceError> {
        let serialized = serde_json::to_value(receipt)?;
        let result = sqlx::query(
            "UPDATE erpx_control.idempotency_keys
             SET status = 'completed', response = $2, completed_at = now(),
                 aggregate_type = $3, aggregate_id = $4, aggregate_version = $5, event_id = $6
             WHERE idempotency_key = $1 AND status = 'in_progress'",
        )
        .bind(key)
        .bind(serialized)
        .bind(&receipt.aggregate_type)
        .bind(receipt.aggregate_id)
        .bind(as_i64(receipt.aggregate_version, "aggregate version")?)
        .bind(receipt.event_id)
        .execute(&mut *self.tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "command idempotency key is not in progress".into(),
            ));
        }
        Ok(())
    }

    /// Load a completed normalized command receipt and verify that its indexed
    /// identity is identical to its serialized representation.
    pub async fn read_command_receipt(
        &mut self,
        key: &str,
    ) -> Result<Option<erpx_application::CommandReceipt>, PersistenceError> {
        let row = sqlx::query(
            "SELECT command_id, response, aggregate_type, aggregate_id,
                    aggregate_version, event_id
             FROM erpx_control.idempotency_keys
             WHERE idempotency_key = $1 AND status = 'completed' FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *self.tx)
        .await?;
        row.map(|row| {
            let response: Value = row.try_get("response")?;
            let receipt: erpx_application::CommandReceipt = serde_json::from_value(response)?;
            let stored_command_id: Option<Uuid> = row.try_get("command_id")?;
            let stored_aggregate_type: String = row.try_get("aggregate_type")?;
            let stored_aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let stored_aggregate_version = as_u64(row.try_get("aggregate_version")?)?;
            let stored_event_id: Uuid = row.try_get("event_id")?;
            if stored_command_id != Some(receipt.command_id)
                || stored_aggregate_type != receipt.aggregate_type
                || stored_aggregate_id != receipt.aggregate_id
                || stored_aggregate_version != receipt.aggregate_version
                || stored_event_id != receipt.event_id
            {
                return Err(PersistenceError::InvalidData(
                    "completed command receipt does not match its idempotency source".into(),
                ));
            }
            Ok(receipt)
        })
        .transpose()
    }

    pub async fn commit(self) -> Result<(), PersistenceError> {
        self.tx.commit().await.map_err(PersistenceError::Database)
    }

    pub async fn rollback(self) -> Result<(), PersistenceError> {
        self.tx.rollback().await.map_err(PersistenceError::Database)
    }
}

#[derive(Debug, Serialize)]
struct AuditEventPayload<'a> {
    actor_ref: &'a str,
    customer_account_id: Option<Uuid>,
    action: &'a str,
    purpose: &'a str,
    correlation_id: Uuid,
}

impl<'a> From<&'a erpx_application::AuditEvent> for AuditEventPayload<'a> {
    fn from(event: &'a erpx_application::AuditEvent) -> Self {
        Self {
            actor_ref: &event.actor_ref,
            customer_account_id: event.customer_account_id,
            action: &event.action,
            purpose: &event.purpose,
            correlation_id: event.correlation_id,
        }
    }
}

impl SqlxControlPlane {
    pub async fn claim_outbox(
        &self,
        worker_id: Uuid,
        limit: u32,
        now: DateTime<Utc>,
        claim_timeout: Duration,
    ) -> Result<Vec<OutboxEnvelope>, PersistenceError> {
        self.claim_outbox_for_aggregate_types(worker_id, limit, now, claim_timeout, &[])
            .await
    }

    pub async fn claim_outbox_for_aggregate_types(
        &self,
        worker_id: Uuid,
        limit: u32,
        now: DateTime<Utc>,
        claim_timeout: Duration,
        aggregate_types: &[&str],
    ) -> Result<Vec<OutboxEnvelope>, PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "outbox worker identity is required".into(),
            ));
        }
        if limit == 0 || limit > MAX_OUTBOX_CLAIM_BATCH_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "outbox claim batch must be 1..={MAX_OUTBOX_CLAIM_BATCH_SIZE}"
            )));
        }
        if claim_timeout <= Duration::zero() {
            return Err(PersistenceError::InvalidData(
                "outbox claim timeout must be positive".into(),
            ));
        }
        if aggregate_types.len() > 64
            || aggregate_types
                .iter()
                .any(|aggregate_type| aggregate_type.trim().is_empty())
        {
            return Err(PersistenceError::InvalidData(
                "outbox aggregate filter must contain at most 64 non-empty values".into(),
            ));
        }
        let stale_before = now - claim_timeout;
        let aggregate_types = aggregate_types
            .iter()
            .map(|aggregate_type| (*aggregate_type).to_owned())
            .collect::<Vec<_>>();
        let rows = sqlx::query(
            "WITH exhausted AS (
                UPDATE erpx_control.outbox_events
                SET dead_lettered_at = $1,
                    claimed_by = NULL,
                    claimed_at = NULL,
                    last_error = COALESCE(last_error, 'worker attempt limit exhausted')
                WHERE published_at IS NULL
                  AND dead_lettered_at IS NULL
                  AND attempts >= max_attempts
                  AND (claimed_by IS NULL OR claimed_at IS NULL OR claimed_at < $2)
                  AND ($5::text[] = '{}'::text[] OR aggregate_type = ANY($5::text[]))
                RETURNING id
             ), candidates AS (
                SELECT e.id
                FROM erpx_control.outbox_events e
                WHERE e.published_at IS NULL
                  AND e.dead_lettered_at IS NULL
                  AND e.available_at <= $1
                  AND e.attempts < e.max_attempts
                  AND (e.claimed_by IS NULL OR e.claimed_at IS NULL OR e.claimed_at < $2)
                  AND ($5::text[] = '{}'::text[] OR e.aggregate_type = ANY($5::text[]))
                  AND NOT EXISTS (
                    SELECT 1 FROM erpx_control.outbox_events prior
                    WHERE prior.aggregate_type = e.aggregate_type
                      AND prior.aggregate_id = e.aggregate_id
                      AND prior.aggregate_version < e.aggregate_version
                      AND prior.published_at IS NULL
                  )
                ORDER BY e.occurred_at, e.id
                FOR UPDATE SKIP LOCKED
                LIMIT $3
             )
             UPDATE erpx_control.outbox_events e
             SET claimed_by = $4, claimed_at = $1, attempts = e.attempts + 1
             FROM candidates c
             WHERE e.id = c.id
             RETURNING e.*",
        )
        .bind(now)
        .bind(stale_before)
        .bind(i64::from(limit))
        .bind(worker_id)
        .bind(aggregate_types)
        .fetch_all(&self.pools.write)
        .await?;
        rows.into_iter().map(outbox_from_row).collect()
    }

    pub async fn mark_outbox_published(
        &self,
        event_id: Uuid,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let result = sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET published_at = $3, claimed_by = NULL, claimed_at = NULL,
                 last_error = NULL, last_error_context = NULL
             WHERE id = $1 AND claimed_by = $2 AND published_at IS NULL AND dead_lettered_at IS NULL",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() == 0 {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id,
            });
        }
        Ok(())
    }

    pub async fn retry_outbox(
        &self,
        event_id: Uuid,
        worker_id: Uuid,
        next_attempt_at: DateTime<Utc>,
        error: &str,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        self.retry_outbox_with_context(event_id, worker_id, next_attempt_at, error, None, at)
            .await
    }

    pub async fn retry_outbox_with_context(
        &self,
        event_id: Uuid,
        worker_id: Uuid,
        next_attempt_at: DateTime<Utc>,
        error: &str,
        context: Option<&WorkerErrorContext>,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        let error = sanitize_persisted_worker_error(error);
        let context = context
            .map(serde_json::to_value)
            .transpose()
            .map_err(PersistenceError::Serialization)?;
        let result = sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET available_at = $3,
                 last_error = $4,
                 last_error_context = $5,
                 claimed_by = NULL,
                 claimed_at = NULL,
                 dead_lettered_at = CASE WHEN attempts >= max_attempts THEN $6 ELSE NULL END
             WHERE id = $1 AND claimed_by = $2 AND published_at IS NULL AND dead_lettered_at IS NULL
             RETURNING dead_lettered_at IS NOT NULL AS is_dead_lettered",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(next_attempt_at)
        .bind(error)
        .bind(context)
        .bind(at)
        .fetch_optional(&self.pools.write)
        .await?
        .ok_or(PersistenceError::ClaimLost {
            worker_id,
            event_id,
        })?;
        Ok(result.try_get("is_dead_lettered")?)
    }

    pub async fn replay_dead_letter(
        &self,
        event_id: Uuid,
        available_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let result = sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET dead_lettered_at = NULL, available_at = GREATEST($2, now()),
                 claimed_by = NULL, claimed_at = NULL, last_error = NULL,
                 last_error_context = NULL
             WHERE id = $1 AND dead_lettered_at IS NOT NULL AND attempts < max_attempts",
        )
        .bind(event_id)
        .bind(available_at)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() == 0 {
            let status = sqlx::query(
                "SELECT dead_lettered_at, attempts, max_attempts
                 FROM erpx_control.outbox_events WHERE id = $1",
            )
            .bind(event_id)
            .fetch_optional(&self.pools.write)
            .await?;
            return match status {
                None => Err(PersistenceError::NotFound {
                    entity: "dead-lettered outbox event",
                    id: event_id,
                }),
                Some(row)
                    if row
                        .try_get::<Option<DateTime<Utc>>, _>("dead_lettered_at")?
                        .is_none() =>
                {
                    Err(PersistenceError::InvalidData(
                        "outbox event is not dead-lettered".into(),
                    ))
                }
                Some(row) => {
                    let attempts: i32 = row.try_get("attempts")?;
                    let max_attempts: i32 = row.try_get("max_attempts")?;
                    if attempts >= max_attempts {
                        Err(PersistenceError::InvalidData(
                            "outbox replay attempt budget exhausted".into(),
                        ))
                    } else {
                        Err(PersistenceError::InvalidData(
                            "outbox event cannot be replayed".into(),
                        ))
                    }
                }
            };
        }
        Ok(())
    }

    /// Atomically requeue one dead-lettered event and record the operator
    /// action in the append-only control-plane audit log. Idempotency is
    /// reserved and completed in the same transaction as the outbox update
    /// and audit insert, so a retried operator request cannot replay twice or
    /// create an unaudited state transition.
    pub async fn replay_dead_letter_with_audit(
        &self,
        command: DeadLetterReplayCommand<'_>,
    ) -> Result<DeadLetterReplayReceipt, PersistenceError> {
        if command.event_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "dead-letter event ID cannot be nil".into(),
            ));
        }
        if command.correlation_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "dead-letter replay correlation ID cannot be nil".into(),
            ));
        }
        if command.command_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "dead-letter replay command ID cannot be nil".into(),
            ));
        }
        validate_replay_text(command.actor_ref, "dead-letter replay actor", 128)?;
        validate_replay_text(command.purpose, "dead-letter replay purpose", 256)?;
        validate_idempotency_key(command.idempotency_key)?;
        if command.available_at > Utc::now() + Duration::minutes(5) {
            return Err(PersistenceError::InvalidData(
                "dead-letter replay availability is too far in the future".into(),
            ));
        }

        let request_hash = hex::encode(Sha256::digest(
            format!(
                "erpx.dead-letter.replay|{}|{}|{}|{}|{}|{}",
                command.event_id,
                command.available_at,
                command.actor_ref,
                command.purpose,
                command.correlation_id,
                command.command_id,
            )
            .as_bytes(),
        ));
        let mut transaction = self.begin_write().await?;
        let reservation = transaction
            .reserve_idempotency(
                command.idempotency_key,
                &request_hash,
                Some(command.command_id),
            )
            .await?;
        match reservation.state {
            IdempotencyState::Completed { response } => {
                let mut receipt: DeadLetterReplayReceipt = serde_json::from_value(response)?;
                receipt.replayed = true;
                transaction.commit().await?;
                return Ok(receipt);
            }
            IdempotencyState::InProgress { .. } => {
                let _ = transaction.rollback().await;
                return Err(PersistenceError::InvalidData(
                    "dead-letter replay idempotency key is already in progress".into(),
                ));
            }
            IdempotencyState::Reserved => {}
        }

        let row = sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET dead_lettered_at = NULL, available_at = GREATEST($2, now()),
                 claimed_by = NULL, claimed_at = NULL, last_error = NULL,
                 last_error_context = NULL
             WHERE id = $1 AND dead_lettered_at IS NOT NULL AND attempts < max_attempts
             RETURNING event_type, aggregate_type, aggregate_id, aggregate_version,
                       available_at",
        )
        .bind(command.event_id)
        .bind(command.available_at)
        .fetch_optional(&mut *transaction.tx)
        .await?;
        let row = match row {
            Some(row) => row,
            None => {
                let _ = transaction.rollback().await;
                let status = sqlx::query(
                    "SELECT dead_lettered_at, attempts, max_attempts
                     FROM erpx_control.outbox_events WHERE id = $1",
                )
                .bind(command.event_id)
                .fetch_optional(&self.pools.write)
                .await?;
                return match status {
                    None => Err(PersistenceError::NotFound {
                        entity: "dead-lettered outbox event",
                        id: command.event_id,
                    }),
                    Some(row)
                        if row
                            .try_get::<Option<DateTime<Utc>>, _>("dead_lettered_at")?
                            .is_none() =>
                    {
                        Err(PersistenceError::InvalidData(
                            "outbox event is not dead-lettered".into(),
                        ))
                    }
                    Some(row) => {
                        let attempts: i32 = row.try_get("attempts")?;
                        let max_attempts: i32 = row.try_get("max_attempts")?;
                        if attempts >= max_attempts {
                            Err(PersistenceError::InvalidData(
                                "outbox replay attempt budget exhausted".into(),
                            ))
                        } else {
                            Err(PersistenceError::InvalidData(
                                "outbox event cannot be replayed".into(),
                            ))
                        }
                    }
                };
            }
        };

        let audit_payload = json!({
            "event_id": command.event_id,
            "event_type": row.try_get::<String, _>("event_type")?,
            "aggregate_type": row.try_get::<String, _>("aggregate_type")?,
            "aggregate_id": row.try_get::<Uuid, _>("aggregate_id")?,
            "aggregate_version": row.try_get::<i64, _>("aggregate_version")?,
            "available_at": row.try_get::<DateTime<Utc>, _>("available_at")?,
            "command_id": command.command_id,
        });
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, action, purpose, correlation_id, payload)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(command.actor_ref)
        .bind("platform.outbox.dead_letter.replay")
        .bind(command.purpose)
        .bind(command.correlation_id)
        .bind(audit_payload)
        .execute(&mut *transaction.tx)
        .await?;

        let receipt = DeadLetterReplayReceipt {
            event_id: command.event_id,
            correlation_id: command.correlation_id,
            replayed: false,
        };
        transaction
            .complete_idempotency(command.idempotency_key, serde_json::to_value(&receipt)?)
            .await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    pub async fn dead_letter_outbox(
        &self,
        event_id: Uuid,
        worker_id: Uuid,
        error: &str,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        self.dead_letter_outbox_with_context(event_id, worker_id, error, None, at)
            .await
    }

    pub async fn dead_letter_outbox_with_context(
        &self,
        event_id: Uuid,
        worker_id: Uuid,
        error: &str,
        context: Option<&WorkerErrorContext>,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let error = sanitize_persisted_worker_error(error);
        let context = context
            .map(serde_json::to_value)
            .transpose()
            .map_err(PersistenceError::Serialization)?;
        let result = sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET dead_lettered_at = $3, last_error = $4, last_error_context = $5,
                 claimed_by = NULL, claimed_at = NULL
             WHERE id = $1 AND claimed_by = $2 AND published_at IS NULL AND dead_lettered_at IS NULL",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .bind(error)
        .bind(context)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() == 0 {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id,
            });
        }
        Ok(())
    }

    pub async fn advance_projection_checkpoint(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
        at: DateTime<Utc>,
    ) -> Result<bool, PersistenceError> {
        let result = sqlx::query(
            "INSERT INTO erpx_control.projection_checkpoints
                (projection_name, aggregate_type, aggregate_id, last_event_id, last_aggregate_version, updated_at)
             SELECT $1, $2, $3, $4, $5, $6
             WHERE $5 = 1
                OR EXISTS (
                    SELECT 1
                    FROM erpx_control.projection_checkpoints existing
                    WHERE existing.projection_name = $1
                      AND existing.aggregate_type = $2
                      AND existing.aggregate_id = $3
                )
             ON CONFLICT (projection_name, aggregate_type, aggregate_id) DO UPDATE SET
                last_event_id = EXCLUDED.last_event_id,
                last_aggregate_version = EXCLUDED.last_aggregate_version,
                updated_at = EXCLUDED.updated_at
             WHERE erpx_control.projection_checkpoints.last_aggregate_version
                   = EXCLUDED.last_aggregate_version - 1",
        )
        .bind(projection_name)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(event.id)
        .bind(as_i64(event.aggregate_version, "checkpoint version")?)
        .bind(at)
        .execute(&self.pools.write)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn projection_checkpoint_accepts(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<bool, PersistenceError> {
        Ok(matches!(
            self.projection_checkpoint_decision(projection_name, event)
                .await?,
            ProjectionCheckpointDecision::Apply
        ))
    }

    pub async fn projection_checkpoint_decision(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
    ) -> Result<ProjectionCheckpointDecision, PersistenceError> {
        validate_projection_name(projection_name)?;
        if event.aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "projection event aggregate version must be positive".into(),
            ));
        }
        let current = sqlx::query(
            "SELECT last_event_id, last_aggregate_version
             FROM erpx_control.projection_checkpoints
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3",
        )
        .bind(projection_name)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .fetch_optional(&self.pools.write)
        .await?
        .map(|row| {
            Ok::<_, sqlx::Error>((
                row.try_get::<Option<Uuid>, _>("last_event_id")?,
                row.try_get::<i64, _>("last_aggregate_version")?,
            ))
        })
        .transpose()?;
        let Some((last_event_id, current)) = current else {
            return Ok(if event.aggregate_version == 1 {
                ProjectionCheckpointDecision::Apply
            } else {
                ProjectionCheckpointDecision::OutOfOrder {
                    expected_version: 1,
                }
            });
        };
        let current = as_u64(current)?;
        if event.aggregate_version <= current {
            if event.aggregate_version == current && last_event_id != Some(event.id) {
                return Ok(ProjectionCheckpointDecision::ConflictingEvent {
                    expected_event_id: last_event_id,
                });
            }
            Ok(ProjectionCheckpointDecision::AlreadyApplied)
        } else {
            let expected_version = current.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("projection checkpoint version overflow".into())
            })?;
            if event.aggregate_version == expected_version {
                Ok(ProjectionCheckpointDecision::Apply)
            } else {
                Ok(ProjectionCheckpointDecision::OutOfOrder { expected_version })
            }
        }
    }

    pub async fn try_claim_projection_event(
        &self,
        projection_name: &str,
        event: &OutboxEnvelope,
        worker_id: Uuid,
        now: DateTime<Utc>,
        claim_timeout: Duration,
    ) -> Result<bool, PersistenceError> {
        validate_projection_name(projection_name)?;
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "projection worker identity is required".into(),
            ));
        }
        if claim_timeout <= Duration::zero() {
            return Err(PersistenceError::InvalidData(
                "projection claim timeout must be positive".into(),
            ));
        }
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.projection_event_receipts
                (projection_name, event_id, aggregate_type, aggregate_id, aggregate_version,
                 claimed_by, claimed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (projection_name, event_id) DO NOTHING",
        )
        .bind(projection_name)
        .bind(event.id)
        .bind(&event.aggregate_type)
        .bind(event.aggregate_id)
        .bind(as_i64(event.aggregate_version, "projection event version")?)
        .bind(worker_id)
        .bind(now)
        .execute(&self.pools.write)
        .await?;
        if inserted.rows_affected() == 1 {
            return Ok(true);
        }

        let existing = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version, completed_at
             FROM erpx_control.projection_event_receipts
             WHERE projection_name = $1 AND event_id = $2",
        )
        .bind(projection_name)
        .bind(event.id)
        .fetch_optional(&self.pools.write)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData("projection event receipt disappeared".into())
        })?;
        let same_identity = existing.try_get::<String, _>("aggregate_type")?
            == event.aggregate_type
            && existing.try_get::<Uuid, _>("aggregate_id")? == event.aggregate_id
            && as_u64(existing.try_get::<i64, _>("aggregate_version")?)? == event.aggregate_version;
        if !same_identity {
            return Err(PersistenceError::InvalidData(
                "projection event identity conflict".into(),
            ));
        }
        if existing
            .try_get::<Option<DateTime<Utc>>, _>("completed_at")?
            .is_some()
        {
            return Ok(false);
        }

        let stale_before = now - claim_timeout;
        let reclaimed = sqlx::query(
            "UPDATE erpx_control.projection_event_receipts
             SET claimed_by = $3, claimed_at = $4
             WHERE projection_name = $1 AND event_id = $2
               AND completed_at IS NULL
               AND (claimed_at IS NULL OR claimed_at < $5)",
        )
        .bind(projection_name)
        .bind(event.id)
        .bind(worker_id)
        .bind(now)
        .bind(stale_before)
        .execute(&self.pools.write)
        .await?;
        Ok(reclaimed.rows_affected() == 1)
    }

    pub async fn complete_projection_event(
        &self,
        projection_name: &str,
        event_id: Uuid,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        let result = sqlx::query(
            "UPDATE erpx_control.projection_event_receipts
             SET completed_at = $4, claimed_by = NULL, claimed_at = NULL
             WHERE projection_name = $1 AND event_id = $2 AND claimed_by = $3
               AND completed_at IS NULL",
        )
        .bind(projection_name)
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id,
            });
        }
        Ok(())
    }

    pub async fn release_projection_event(
        &self,
        projection_name: &str,
        event_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), PersistenceError> {
        let result = sqlx::query(
            "DELETE FROM erpx_control.projection_event_receipts
             WHERE projection_name = $1 AND event_id = $2 AND claimed_by = $3
               AND completed_at IS NULL",
        )
        .bind(projection_name)
        .bind(event_id)
        .bind(worker_id)
        .execute(&self.pools.write)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id,
                event_id,
            });
        }
        Ok(())
    }

    pub async fn reset_projection(&self, projection_name: &str) -> Result<(), PersistenceError> {
        validate_projection_name(projection_name)?;
        let mut tx = self.pools.write.begin().await?;
        sqlx::query(
            "DELETE FROM erpx_control.projection_event_receipts WHERE projection_name = $1",
        )
        .bind(projection_name)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM erpx_control.projection_checkpoints WHERE projection_name = $1")
            .bind(projection_name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn events_for_rebuild(&self) -> Result<Vec<OutboxEnvelope>, PersistenceError> {
        let rows = sqlx::query(
            "SELECT * FROM erpx_control.outbox_events
             ORDER BY aggregate_type, aggregate_id, aggregate_version, occurred_at, id",
        )
        .fetch_all(&self.pools.write)
        .await?;
        rows.into_iter().map(outbox_from_row).collect()
    }
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds database range")))
}

fn as_u64(value: i64) -> Result<u64, PersistenceError> {
    u64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData("negative database version".into()))
}

fn validate_idempotency_key(key: &str) -> Result<(), PersistenceError> {
    if key.trim().is_empty() || key.len() > 255 {
        return Err(PersistenceError::InvalidData(
            "idempotency key must contain 1 to 255 non-whitespace bytes".into(),
        ));
    }
    Ok(())
}

fn validate_outbox_event_for_append(
    event: &OutboxEvent,
    metadata: &EventMetadata,
) -> Result<(), PersistenceError> {
    if event.id.is_nil()
        || event.aggregate_id.is_nil()
        || event.event_type.trim().is_empty()
        || event.aggregate_type.trim().is_empty()
        || event.aggregate_version == 0
    {
        return Err(PersistenceError::InvalidData(
            "outbox source event identity and version are required".into(),
        ));
    }
    if metadata.event_version == 0 {
        return Err(PersistenceError::InvalidData(
            "outbox source event contract version is required".into(),
        ));
    }
    if metadata.correlation_id.is_some_and(|value| value.is_nil())
        || metadata.causation_id.is_some_and(|value| value.is_nil())
    {
        return Err(PersistenceError::InvalidData(
            "outbox source event correlation identity cannot be nil".into(),
        ));
    }
    if let Some(key) = &metadata.idempotency_key {
        validate_idempotency_key(key)?;
    }
    Ok(())
}

fn validate_replay_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must contain 1 to {max_bytes} non-control bytes"
        )));
    }
    Ok(())
}

fn validate_projection_name(name: &str) -> Result<(), PersistenceError> {
    if name.trim().is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
        || name.contains('/')
    {
        return Err(PersistenceError::InvalidData(
            "projection name is invalid".into(),
        ));
    }
    Ok(())
}

fn sanitize_persisted_worker_error(error: &str) -> String {
    let error = error.trim();
    if error.is_empty()
        || error.len() > MAX_PERSISTED_WORKER_ERROR_BYTES
        || error.chars().any(char::is_control)
        || ["password", "secret", "token", "postgres://", "://"]
            .iter()
            .any(|marker| error.to_ascii_lowercase().contains(marker))
    {
        return "worker event failed".into();
    }
    error.into()
}

fn map_unique_idempotency(error: sqlx::Error) -> PersistenceError {
    if let sqlx::Error::Database(database) = &error
        && database.constraint() == Some("outbox_idempotency_key_unique")
    {
        return PersistenceError::IdempotencyConflict {
            key: "outbox event".into(),
        };
    }
    PersistenceError::Database(error)
}

fn outbox_from_row(row: sqlx::postgres::PgRow) -> Result<OutboxEnvelope, PersistenceError> {
    Ok(OutboxEnvelope {
        id: row.try_get("id")?,
        event_type: row.try_get("event_type")?,
        event_version: u32::try_from(row.try_get::<i32, _>("event_version")?)
            .map_err(|_| PersistenceError::InvalidData("negative event version".into()))?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: as_u64(row.try_get("aggregate_version")?)?,
        tenant_scope: row.try_get("tenant_scope")?,
        actor: row.try_get("actor")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
        available_at: row.try_get("available_at")?,
        attempts: u32::try_from(row.try_get::<i32, _>("attempts")?)
            .map_err(|_| PersistenceError::InvalidData("negative outbox attempts".into()))?,
        max_attempts: u32::try_from(row.try_get::<i32, _>("max_attempts")?)
            .map_err(|_| PersistenceError::InvalidData("negative outbox max attempts".into()))?,
        claimed_by: row.try_get("claimed_by")?,
        claimed_at: row.try_get("claimed_at")?,
        published_at: row.try_get("published_at")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
        last_error: row.try_get("last_error")?,
        last_error_context: row.try_get("last_error_context")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_metadata_defaults_to_a_stable_contract_version() {
        let event = OutboxEvent {
            id: Uuid::new_v4(),
            aggregate_type: "customer".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            event_type: "customer.created".into(),
            payload: serde_json::json!({"account_code": "ACME"}),
            occurred_at: Utc::now(),
        };
        let metadata = EventMetadata::for_event(&event);
        assert_eq!(metadata.event_version, 1);
        assert_eq!(metadata.correlation_id, Some(event.id));
        assert!(metadata.idempotency_key.is_none());
    }

    #[test]
    fn event_envelope_round_trips_json_without_losing_context() {
        let event = OutboxEnvelope {
            id: Uuid::new_v4(),
            event_type: "customer.updated".into(),
            event_version: 2,
            aggregate_type: "customer".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 7,
            tenant_scope: Some(serde_json::json!({"holding_id": "h-1"})),
            actor: Some(serde_json::json!({"principal_ref": "worker"})),
            correlation_id: Uuid::new_v4(),
            causation_id: Some(Uuid::new_v4()),
            idempotency_key: Some("customer-7".into()),
            payload: serde_json::json!({"status": "active"}),
            occurred_at: Utc::now(),
            available_at: Utc::now(),
            attempts: 0,
            max_attempts: 10,
            claimed_by: None,
            claimed_at: None,
            published_at: None,
            dead_lettered_at: None,
            last_error: None,
            last_error_context: None,
        };
        let encoded = serde_json::to_string(&event).expect("encode");
        let decoded: OutboxEnvelope = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(decoded, event);
    }

    #[test]
    fn empty_idempotency_keys_are_rejected_before_database_access() {
        assert!(validate_idempotency_key("").is_err());
        assert!(validate_idempotency_key("   ").is_err());
        assert!(validate_idempotency_key(&"x".repeat(256)).is_err());
        assert!(validate_idempotency_key("command-1").is_ok());
    }

    #[test]
    fn append_rejects_unbound_source_envelopes_without_normalizing_them() {
        let event = OutboxEvent {
            id: Uuid::from_u128(30),
            aggregate_type: "customer".into(),
            aggregate_id: Uuid::from_u128(31),
            aggregate_version: 1,
            event_type: "customer.created".into(),
            payload: serde_json::json!({"status": "active"}),
            occurred_at: Utc::now(),
        };
        assert!(validate_outbox_event_for_append(&event, &EventMetadata::default()).is_ok());

        let invalid_metadata = EventMetadata {
            event_version: 0,
            ..EventMetadata::default()
        };
        assert!(validate_outbox_event_for_append(&event, &invalid_metadata).is_err());

        let mut invalid_event = event.clone();
        invalid_event.id = Uuid::nil();
        assert!(
            validate_outbox_event_for_append(&invalid_event, &EventMetadata::default()).is_err()
        );

        let mut invalid_event = event;
        invalid_event.aggregate_version = 0;
        assert!(
            validate_outbox_event_for_append(&invalid_event, &EventMetadata::default()).is_err()
        );
    }
}
