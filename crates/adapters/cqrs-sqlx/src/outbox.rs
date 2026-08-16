use std::time::Duration;

use chrono::{DateTime, Utc};
use erpx_cqrs::OutboxDeliveryDisposition;
use erpx_kernel::{AggregateId, CommandId, CorrelationId, EventId};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{CqrsSqlxError, WritePool};

pub const MAX_OUTBOX_CLAIM_SIZE: u32 = 1_000;
const MAX_OUTBOX_LEASE: Duration = Duration::from_secs(5 * 60);
const MAX_FAILURE_MESSAGE_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxRetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub maximum_delay: Duration,
}

impl Default for OutboxRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            base_delay: Duration::from_millis(250),
            maximum_delay: Duration::from_secs(30),
        }
    }
}

impl OutboxRetryPolicy {
    pub fn validate(self) -> Result<Self, CqrsSqlxError> {
        if self.max_attempts == 0
            || self.base_delay.is_zero()
            || self.maximum_delay < self.base_delay
        {
            return Err(CqrsSqlxError::InvalidInput(
                "outbox retry policy is invalid",
            ));
        }
        Ok(self)
    }

    pub fn delay_for(self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(31);
        self.base_delay
            .saturating_mul(1_u32 << exponent)
            .min(self.maximum_delay)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedOutboxEvent {
    pub claim_token: Uuid,
    pub event_id: EventId,
    pub global_sequence: u64,
    pub context: String,
    pub partition: String,
    pub command_id: CommandId,
    pub event_type: String,
    pub event_version: u32,
    pub aggregate_type: String,
    pub aggregate_id: AggregateId,
    pub aggregate_version: u64,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
    pub attempt: u32,
}

#[derive(Debug, Clone)]
pub struct OutboxDeliveryStore {
    pool: WritePool,
}

impl OutboxDeliveryStore {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    /// Lease a bounded due batch. `SKIP LOCKED` permits parallel workers while
    /// the claim token prevents a stale worker from acknowledging another
    /// worker's retry.
    pub async fn claim(
        &self,
        claim_token: Uuid,
        limit: u32,
        lease: Duration,
    ) -> Result<Vec<ClaimedOutboxEvent>, CqrsSqlxError> {
        if claim_token.is_nil()
            || limit == 0
            || limit > MAX_OUTBOX_CLAIM_SIZE
            || lease.is_zero()
            || lease > MAX_OUTBOX_LEASE
        {
            return Err(CqrsSqlxError::InvalidInput(
                "outbox claim request is invalid",
            ));
        }
        let lease_millis =
            i64::try_from(lease.as_millis()).map_err(|_| CqrsSqlxError::NumericRange)?;
        let statement = if self.pool.tenant_id().is_some() {
            "WITH due AS (
                 SELECT tenant_id, event_id
                 FROM erpx_cqrs_write.outbox_events
                 WHERE published_at IS NULL AND dead_lettered_at IS NULL
                   AND available_at <= now()
                   AND (claim_until IS NULL OR claim_until <= now())
                   AND tenant_id = $4
                 ORDER BY available_at ASC, global_sequence ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             UPDATE erpx_cqrs_write.outbox_events event
             SET claim_token = $2,
                 claim_until = now() + ($3 * INTERVAL '1 millisecond'),
                 publish_attempts = publish_attempts + 1
             FROM due
             WHERE event.tenant_id = due.tenant_id AND event.event_id = due.event_id
               AND event.tenant_id = $4
             RETURNING event.event_id, event.global_sequence, event.context_name,
                       event.partition_key, event.command_id, event.event_type,
                       event.event_version, event.aggregate_type, event.aggregate_id,
                       event.aggregate_version, event.correlation_id, event.causation_id,
                       event.payload, event.occurred_at, event.publish_attempts"
        } else {
            "WITH due AS (
                 SELECT event_id
                 FROM erpx_cqrs_write.outbox_events
                 WHERE published_at IS NULL AND dead_lettered_at IS NULL
                   AND available_at <= now()
                   AND (claim_until IS NULL OR claim_until <= now())
                 ORDER BY available_at ASC, global_sequence ASC
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             UPDATE erpx_cqrs_write.outbox_events event
             SET claim_token = $2,
                 claim_until = now() + ($3 * INTERVAL '1 millisecond'),
                 publish_attempts = publish_attempts + 1
             FROM due
             WHERE event.event_id = due.event_id
             RETURNING event.event_id, event.global_sequence, event.context_name,
                       event.partition_key, event.command_id, event.event_type,
                       event.event_version, event.aggregate_type, event.aggregate_id,
                       event.aggregate_version, event.correlation_id, event.causation_id,
                       event.payload, event.occurred_at, event.publish_attempts"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query(statement)
            .bind(i64::from(limit))
            .bind(claim_token)
            .bind(lease_millis);
        let rows = match self.pool.tenant_id() {
            Some(tenant_id) => query.bind(tenant_id).fetch_all(&mut *transaction).await?,
            None => query.fetch_all(&mut *transaction).await?,
        };
        transaction.commit().await?;
        rows.into_iter()
            .map(|row| row_to_claim(row, claim_token))
            .collect()
    }

    pub async fn mark_published(
        &self,
        event_id: EventId,
        claim_token: Uuid,
        published_at: DateTime<Utc>,
    ) -> Result<OutboxDeliveryDisposition, CqrsSqlxError> {
        if claim_token.is_nil() {
            return Err(CqrsSqlxError::InvalidInput("outbox claim token is invalid"));
        }
        let statement = if self.pool.tenant_id().is_some() {
            "UPDATE erpx_cqrs_write.outbox_events
             SET published_at = $3, claim_token = NULL, claim_until = NULL,
                 publish_error = NULL
             WHERE event_id = $1 AND claim_token = $2 AND tenant_id = $4
               AND published_at IS NULL AND dead_lettered_at IS NULL
             RETURNING publish_attempts"
        } else {
            "UPDATE erpx_cqrs_write.outbox_events
             SET published_at = $3, claim_token = NULL, claim_until = NULL,
                 publish_error = NULL
             WHERE event_id = $1 AND claim_token = $2
               AND published_at IS NULL AND dead_lettered_at IS NULL
             RETURNING publish_attempts"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query_scalar(statement)
            .bind(event_id.as_uuid())
            .bind(claim_token)
            .bind(published_at);
        let attempt: Option<i32> = match self.pool.tenant_id() {
            Some(tenant_id) => {
                query
                    .bind(tenant_id)
                    .fetch_optional(&mut *transaction)
                    .await?
            }
            None => query.fetch_optional(&mut *transaction).await?,
        };
        transaction.commit().await?;
        let attempt = attempt.ok_or(CqrsSqlxError::OutboxClaimLost)?;
        Ok(OutboxDeliveryDisposition::Published {
            attempt: to_u32(attempt)?,
            published_at,
        })
    }

    pub async fn record_failure(
        &self,
        event_id: EventId,
        claim_token: Uuid,
        failure: &str,
        policy: OutboxRetryPolicy,
        failed_at: DateTime<Utc>,
    ) -> Result<OutboxDeliveryDisposition, CqrsSqlxError> {
        let policy = policy.validate()?;
        if claim_token.is_nil() {
            return Err(CqrsSqlxError::InvalidInput("outbox claim token is invalid"));
        }
        let mut transaction = self.pool.begin().await?;
        let statement = if self.pool.tenant_id().is_some() {
            "SELECT publish_attempts
             FROM erpx_cqrs_write.outbox_events
             WHERE event_id = $1 AND claim_token = $2 AND tenant_id = $3
               AND published_at IS NULL AND dead_lettered_at IS NULL
             FOR UPDATE"
        } else {
            "SELECT publish_attempts
             FROM erpx_cqrs_write.outbox_events
             WHERE event_id = $1 AND claim_token = $2
               AND published_at IS NULL AND dead_lettered_at IS NULL
             FOR UPDATE"
        };
        let query = sqlx::query_scalar(statement)
            .bind(event_id.as_uuid())
            .bind(claim_token);
        let attempt: Option<i32> = match self.pool.tenant_id() {
            Some(tenant_id) => {
                query
                    .bind(tenant_id)
                    .fetch_optional(&mut *transaction)
                    .await?
            }
            None => query.fetch_optional(&mut *transaction).await?,
        };
        let attempt = to_u32(attempt.ok_or(CqrsSqlxError::OutboxClaimLost)?)?;
        let message = bounded_failure(failure);
        if attempt >= policy.max_attempts {
            let statement = if self.pool.tenant_id().is_some() {
                "UPDATE erpx_cqrs_write.outbox_events
                 SET claim_token = NULL, claim_until = NULL, publish_error = $3,
                     dead_lettered_at = $4
                 WHERE event_id = $1 AND claim_token = $2 AND tenant_id = $5"
            } else {
                "UPDATE erpx_cqrs_write.outbox_events
                 SET claim_token = NULL, claim_until = NULL, publish_error = $3,
                     dead_lettered_at = $4
                 WHERE event_id = $1 AND claim_token = $2"
            };
            let query = sqlx::query(statement)
                .bind(event_id.as_uuid())
                .bind(claim_token)
                .bind(message)
                .bind(failed_at);
            match self.pool.tenant_id() {
                Some(tenant_id) => query.bind(tenant_id).execute(&mut *transaction).await?,
                None => query.execute(&mut *transaction).await?,
            };
            transaction.commit().await?;
            return Ok(OutboxDeliveryDisposition::DeadLettered {
                attempts: attempt,
                dead_lettered_at: failed_at,
            });
        }

        let delay = chrono::Duration::from_std(policy.delay_for(attempt))
            .map_err(|_| CqrsSqlxError::NumericRange)?;
        let available_at = failed_at + delay;
        let statement = if self.pool.tenant_id().is_some() {
            "UPDATE erpx_cqrs_write.outbox_events
             SET claim_token = NULL, claim_until = NULL, publish_error = $3,
                 available_at = $4
             WHERE event_id = $1 AND claim_token = $2 AND tenant_id = $5"
        } else {
            "UPDATE erpx_cqrs_write.outbox_events
             SET claim_token = NULL, claim_until = NULL, publish_error = $3,
                 available_at = $4
             WHERE event_id = $1 AND claim_token = $2"
        };
        let query = sqlx::query(statement)
            .bind(event_id.as_uuid())
            .bind(claim_token)
            .bind(message)
            .bind(available_at);
        match self.pool.tenant_id() {
            Some(tenant_id) => query.bind(tenant_id).execute(&mut *transaction).await?,
            None => query.execute(&mut *transaction).await?,
        };
        transaction.commit().await?;
        Ok(OutboxDeliveryDisposition::RetryScheduled {
            attempt,
            available_at,
        })
    }
}

fn row_to_claim(
    row: sqlx::postgres::PgRow,
    claim_token: Uuid,
) -> Result<ClaimedOutboxEvent, CqrsSqlxError> {
    let event_id = EventId::try_from_uuid(row.try_get("event_id")?)
        .map_err(|_| CqrsSqlxError::InvalidInput("persisted outbox event id is nil"))?;
    let command_id = CommandId::try_from_uuid(row.try_get("command_id")?)
        .map_err(|_| CqrsSqlxError::InvalidInput("persisted command id is nil"))?;
    let aggregate_id = AggregateId::try_from_uuid(row.try_get("aggregate_id")?)
        .map_err(|_| CqrsSqlxError::InvalidInput("persisted aggregate id is nil"))?;
    let correlation_id = CorrelationId::try_from_uuid(row.try_get("correlation_id")?)
        .map_err(|_| CqrsSqlxError::InvalidInput("persisted correlation id is nil"))?;
    let causation_id: Option<Uuid> = row.try_get("causation_id")?;
    Ok(ClaimedOutboxEvent {
        claim_token,
        event_id,
        global_sequence: to_u64(row.try_get("global_sequence")?)?,
        context: row.try_get("context_name")?,
        partition: row.try_get("partition_key")?,
        command_id,
        event_type: row.try_get("event_type")?,
        event_version: to_u32(row.try_get("event_version")?)?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id,
        aggregate_version: to_u64(row.try_get("aggregate_version")?)?,
        correlation_id,
        causation_id: causation_id
            .map(EventId::try_from_uuid)
            .transpose()
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted causation id is nil"))?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
        attempt: to_u32(row.try_get("publish_attempts")?)?,
    })
}

fn bounded_failure(value: &str) -> String {
    if value.len() <= MAX_FAILURE_MESSAGE_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_FAILURE_MESSAGE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn to_u64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn to_u32(value: i32) -> Result<u32, CqrsSqlxError> {
    u32::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_and_failure_evidence_are_bounded() {
        let policy = OutboxRetryPolicy {
            max_attempts: 4,
            base_delay: Duration::from_secs(1),
            maximum_delay: Duration::from_secs(3),
        }
        .validate()
        .unwrap();
        assert_eq!(policy.delay_for(1), Duration::from_secs(1));
        assert_eq!(policy.delay_for(2), Duration::from_secs(2));
        assert_eq!(policy.delay_for(3), Duration::from_secs(3));
        assert_eq!(policy.delay_for(99), Duration::from_secs(3));

        let failure = "ก".repeat(MAX_FAILURE_MESSAGE_BYTES);
        let bounded = bounded_failure(&failure);
        assert!(bounded.len() <= MAX_FAILURE_MESSAGE_BYTES);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn invalid_retry_policies_fail_closed() {
        assert!(
            OutboxRetryPolicy {
                max_attempts: 0,
                ..OutboxRetryPolicy::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            OutboxRetryPolicy {
                base_delay: Duration::ZERO,
                ..OutboxRetryPolicy::default()
            }
            .validate()
            .is_err()
        );
    }
}
