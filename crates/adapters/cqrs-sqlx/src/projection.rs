use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, AggregateId, AuthorityScope, CommandId, CorrelationId, EventId,
};
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{CqrsSqlxError, ProjectionFailure, ProjectionPool, WritePool};

pub const MAX_PROJECTION_PAGE_SIZE: u32 = 1_000;
const MAX_FAILURE_MESSAGE_BYTES: usize = 512;
const MAX_PROJECTION_SOURCE_KEY_LENGTH: usize = 255;
const MAX_QUALIFIED_PARTITION_LENGTH: usize = erpx_cqrs::MAX_PARTITION_KEY_LENGTH;

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionEvent {
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
    pub active_scope: AuthorityScope,
    pub actor: ActorAttribution,
    pub correlation_id: CorrelationId,
    pub causation_id: Option<EventId>,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventPage {
    pub events: Vec<ProjectionEvent>,
    pub after_sequence: u64,
}

#[derive(Debug, Clone)]
pub struct ProjectionSource {
    pool: WritePool,
}

impl ProjectionSource {
    pub const fn new(pool: WritePool) -> Self {
        Self { pool }
    }

    pub async fn page_after(
        &self,
        after_sequence: u64,
        event_types: &[String],
        limit: u32,
    ) -> Result<EventPage, CqrsSqlxError> {
        if limit == 0 || limit > MAX_PROJECTION_PAGE_SIZE {
            return Err(CqrsSqlxError::InvalidInput(
                "projection page size is outside the allowed range",
            ));
        }
        let statement = if self.pool.holding_id().is_some() {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1 AND available_at <= now()
               AND (cardinality($2::text[]) = 0 OR event_type = ANY($2::text[]))
               AND holding_id = $4
             ORDER BY global_sequence ASC LIMIT $3"
        } else {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1 AND available_at <= now()
               AND (cardinality($2::text[]) = 0 OR event_type = ANY($2::text[]))
             ORDER BY global_sequence ASC
             LIMIT $3"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query(statement)
            .bind(to_i64(after_sequence)?)
            .bind(event_types)
            .bind(i64::from(limit));
        let rows = match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).fetch_all(&mut *transaction).await?,
            None => query.fetch_all(&mut *transaction).await?,
        };
        transaction.commit().await?;
        let events = rows
            .into_iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EventPage {
            events,
            after_sequence,
        })
    }

    /// Return the last currently available sequence that can affect this
    /// projector. Filtering by event type is important: a rebuild fence must
    /// remain reachable even when unrelated events follow the last matching
    /// event in the same outbox.
    pub async fn high_watermark(&self, event_types: &[String]) -> Result<u64, CqrsSqlxError> {
        let statement = if self.pool.holding_id().is_some() {
            "SELECT COALESCE(MAX(global_sequence), 0)
             FROM erpx_cqrs_write.outbox_events
             WHERE available_at <= now()
               AND (cardinality($1::text[]) = 0 OR event_type = ANY($1::text[]))
               AND holding_id = $2"
        } else {
            "SELECT COALESCE(MAX(global_sequence), 0)
             FROM erpx_cqrs_write.outbox_events
             WHERE available_at <= now()
               AND (cardinality($1::text[]) = 0 OR event_type = ANY($1::text[]))"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query_scalar(statement).bind(event_types);
        let sequence: i64 = match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).fetch_one(&mut *transaction).await?,
            None => query.fetch_one(&mut *transaction).await?,
        };
        transaction.commit().await?;
        to_u64(sequence)
    }

    /// Return the last committed matching sequence, independent of outbox
    /// delivery retry scheduling. Security projections use this source so a
    /// lower-sequence event whose broker delivery was delayed cannot be
    /// skipped after the cursor observes a later committed event.
    pub async fn durable_high_watermark(
        &self,
        event_types: &[String],
    ) -> Result<u64, CqrsSqlxError> {
        let statement = if self.pool.holding_id().is_some() {
            "SELECT COALESCE(MAX(global_sequence), 0)
             FROM erpx_cqrs_write.outbox_events
             WHERE (cardinality($1::text[]) = 0 OR event_type = ANY($1::text[]))
               AND holding_id = $2"
        } else {
            "SELECT COALESCE(MAX(global_sequence), 0)
             FROM erpx_cqrs_write.outbox_events
             WHERE (cardinality($1::text[]) = 0 OR event_type = ANY($1::text[]))"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query_scalar(statement).bind(event_types);
        let sequence: i64 = match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).fetch_one(&mut *transaction).await?,
            None => query.fetch_one(&mut *transaction).await?,
        };
        transaction.commit().await?;
        to_u64(sequence)
    }

    /// Page a source without crossing an optional, source-local rebuild
    /// fence. Global sequences from different write databases are never
    /// compared by this API.
    pub async fn page_between(
        &self,
        after_sequence: u64,
        through_sequence: Option<u64>,
        event_types: &[String],
        limit: u32,
    ) -> Result<EventPage, CqrsSqlxError> {
        if limit == 0 || limit > MAX_PROJECTION_PAGE_SIZE {
            return Err(CqrsSqlxError::InvalidInput(
                "projection page size is outside the allowed range",
            ));
        }
        if through_sequence.is_some_and(|through| through < after_sequence) {
            return Err(CqrsSqlxError::InvalidInput(
                "projection source fence is behind its cursor",
            ));
        }
        let through_sequence = through_sequence.map(to_i64).transpose()?;
        let statement = if self.pool.holding_id().is_some() {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1
               AND ($2::bigint IS NULL OR global_sequence <= $2)
               AND available_at <= now()
               AND (cardinality($3::text[]) = 0 OR event_type = ANY($3::text[]))
               AND holding_id = $5
             ORDER BY global_sequence ASC LIMIT $4"
        } else {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1
               AND ($2::bigint IS NULL OR global_sequence <= $2)
               AND available_at <= now()
               AND (cardinality($3::text[]) = 0 OR event_type = ANY($3::text[]))
             ORDER BY global_sequence ASC
             LIMIT $4"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query(statement)
            .bind(to_i64(after_sequence)?)
            .bind(through_sequence)
            .bind(event_types)
            .bind(i64::from(limit));
        let rows = match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).fetch_all(&mut *transaction).await?,
            None => query.fetch_all(&mut *transaction).await?,
        };
        transaction.commit().await?;
        let events = rows
            .into_iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EventPage {
            events,
            after_sequence,
        })
    }

    /// Page committed matching events without consulting `available_at`.
    /// Delivery availability belongs to broker publication; it is not an
    /// ordering fence for an in-database authorization projection.
    pub async fn page_durable_between(
        &self,
        after_sequence: u64,
        through_sequence: Option<u64>,
        event_types: &[String],
        limit: u32,
    ) -> Result<EventPage, CqrsSqlxError> {
        if limit == 0 || limit > MAX_PROJECTION_PAGE_SIZE {
            return Err(CqrsSqlxError::InvalidInput(
                "projection page size is outside the allowed range",
            ));
        }
        if through_sequence.is_some_and(|through| through < after_sequence) {
            return Err(CqrsSqlxError::InvalidInput(
                "projection source fence is behind its cursor",
            ));
        }
        let through_sequence = through_sequence.map(to_i64).transpose()?;
        let statement = if self.pool.holding_id().is_some() {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1
               AND ($2::bigint IS NULL OR global_sequence <= $2)
               AND (cardinality($3::text[]) = 0 OR event_type = ANY($3::text[]))
               AND holding_id = $5
             ORDER BY global_sequence ASC LIMIT $4"
        } else {
            "SELECT event_id, global_sequence, context_name, partition_key, command_id,
                    event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, active_scope, actor, correlation_id, causation_id,
                    payload, occurred_at
             FROM erpx_cqrs_write.outbox_events
             WHERE global_sequence > $1
               AND ($2::bigint IS NULL OR global_sequence <= $2)
               AND (cardinality($3::text[]) = 0 OR event_type = ANY($3::text[]))
             ORDER BY global_sequence ASC LIMIT $4"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query(statement)
            .bind(to_i64(after_sequence)?)
            .bind(through_sequence)
            .bind(event_types)
            .bind(i64::from(limit));
        let rows = match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).fetch_all(&mut *transaction).await?,
            None => query.fetch_all(&mut *transaction).await?,
        };
        transaction.commit().await?;
        let events = rows
            .into_iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EventPage {
            events,
            after_sequence,
        })
    }
}

/// Stable identity of one physical projection source. For holding fan-in this
/// is normally `holding:<holding-id>` and must remain stable across restarts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectionSourceKey(String);

impl ProjectionSourceKey {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CqrsSqlxError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_PROJECTION_SOURCE_KEY_LENGTH
            || value.trim() != value
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'.' | b'_' | b'-')
            })
        {
            return Err(CqrsSqlxError::InvalidInput(
                "projection source key is invalid",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Qualify a source-local partition before it reaches the shared applied
    /// event, checkpoint, receipt, retry, and read-your-writes tables.
    pub fn qualify_partition(&self, partition: &str) -> Result<String, CqrsSqlxError> {
        if partition.trim().is_empty() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection partition is invalid",
            ));
        }
        let qualified = format!("{}/{partition}", self.as_str());
        if qualified.len() > MAX_QUALIFIED_PARTITION_LENGTH {
            return Err(CqrsSqlxError::InvalidInput(
                "source-qualified projection partition is too long",
            ));
        }
        Ok(qualified)
    }

    /// Recover source provenance and the original source-local partition in a
    /// multi-source projector without adding source identity to single-source
    /// projection events.
    pub fn parse_qualified_partition(partition: &str) -> Result<(Self, &str), CqrsSqlxError> {
        if partition.len() > MAX_QUALIFIED_PARTITION_LENGTH {
            return Err(CqrsSqlxError::InvalidInput(
                "source-qualified projection partition is invalid",
            ));
        }
        let (source, local_partition) =
            partition
                .split_once('/')
                .ok_or(CqrsSqlxError::InvalidInput(
                    "source-qualified projection partition is invalid",
                ))?;
        if local_partition.trim().is_empty() {
            return Err(CqrsSqlxError::InvalidInput(
                "source-qualified projection partition is invalid",
            ));
        }
        Ok((Self::try_new(source)?, local_partition))
    }
}

impl TryFrom<String> for ProjectionSourceKey {
    type Error = CqrsSqlxError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl AsRef<str> for ProjectionSourceKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSourceFence {
    pub source_key: ProjectionSourceKey,
    pub target_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSourceCheckpoint {
    pub projection: String,
    pub generation: Uuid,
    pub source_key: ProjectionSourceKey,
    pub source_sequence: u64,
    pub rebuild_target_sequence: Option<u64>,
}

impl ProjectionSourceCheckpoint {
    pub fn rebuild_ready(&self) -> bool {
        self.rebuild_target_sequence
            .is_some_and(|target| self.source_sequence >= target)
    }
}

fn row_to_event(row: sqlx::postgres::PgRow) -> Result<ProjectionEvent, CqrsSqlxError> {
    let event_id: Uuid = row.try_get("event_id")?;
    let command_id: Uuid = row.try_get("command_id")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let correlation_id: Uuid = row.try_get("correlation_id")?;
    let causation_id: Option<Uuid> = row.try_get("causation_id")?;
    let active_scope: Value = row.try_get("active_scope")?;
    let actor: Value = row.try_get("actor")?;
    Ok(ProjectionEvent {
        event_id: EventId::try_from_uuid(event_id)
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted event id is nil"))?,
        global_sequence: to_u64(row.try_get("global_sequence")?)?,
        context: row.try_get("context_name")?,
        partition: row.try_get("partition_key")?,
        command_id: CommandId::try_from_uuid(command_id)
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted command id is nil"))?,
        event_type: row.try_get("event_type")?,
        event_version: u32::try_from(row.try_get::<i32, _>("event_version")?)
            .map_err(|_| CqrsSqlxError::NumericRange)?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: AggregateId::try_from_uuid(aggregate_id)
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted aggregate id is nil"))?,
        aggregate_version: to_u64(row.try_get("aggregate_version")?)?,
        active_scope: serde_json::from_value(active_scope)?,
        actor: serde_json::from_value(actor)?,
        correlation_id: CorrelationId::try_from_uuid(correlation_id)
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted correlation id is nil"))?,
        causation_id: causation_id
            .map(EventId::try_from_uuid)
            .transpose()
            .map_err(|_| CqrsSqlxError::InvalidInput("persisted causation id is nil"))?,
        payload: row.try_get("payload")?,
        occurred_at: row.try_get("occurred_at")?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionGeneration {
    pub projection: String,
    pub generation: Uuid,
    pub active: bool,
    pub source_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildHandle {
    pub projection: String,
    pub generation: Uuid,
    pub replaces: Option<Uuid>,
}

#[derive(Debug)]
pub struct ProjectorContext<'a> {
    pub projection: &'a str,
    pub generation: Uuid,
}

#[async_trait]
pub trait Projector: Send + Sync {
    fn name(&self) -> &str;

    fn event_types(&self) -> &[String];

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure>;

    /// Delete or initialize only this projector's rows for `generation`.
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub maximum_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            base_delay: Duration::from_millis(250),
            maximum_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    pub fn validate(self) -> Result<Self, CqrsSqlxError> {
        if self.max_attempts == 0
            || self.base_delay.is_zero()
            || self.maximum_delay < self.base_delay
        {
            return Err(CqrsSqlxError::InvalidInput("retry policy is invalid"));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionRunnerConfig {
    pub page_size: u32,
    pub retry: RetryPolicy,
}

impl Default for ProjectionRunnerConfig {
    fn default() -> Self {
        Self {
            page_size: 250,
            retry: RetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDisposition {
    Applied,
    Duplicate,
    RetryScheduled { attempt: u32 },
    DeadLettered { attempts: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectionRun {
    pub fetched: u32,
    pub applied: u32,
    pub duplicates: u32,
    pub retry_scheduled: bool,
    pub dead_lettered: u32,
    pub cursor: u64,
}

#[derive(Debug, Clone)]
pub struct ProjectionStore {
    pool: ProjectionPool,
}

impl ProjectionStore {
    pub const fn new(pool: ProjectionPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_active(
        &self,
        projection: &str,
    ) -> Result<ProjectionGeneration, CqrsSqlxError> {
        validate_projection_name(projection)?;
        let mut transaction = self.pool.begin().await?;
        lock_rebuild_start(&mut transaction, projection).await?;
        let existing = sqlx::query(
            "SELECT generation, source_sequence
             FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(projection)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = existing {
            let active = ProjectionGeneration {
                projection: projection.into(),
                generation: row.try_get("generation")?,
                active: true,
                source_sequence: to_u64(row.try_get("source_sequence")?)?,
            };
            transaction.commit().await?;
            return Ok(active);
        }
        let rebuilding: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_cqrs_read.projection_generations
                 WHERE projection_name = $1 AND status = 'building'
             )",
        )
        .bind(projection)
        .fetch_one(&mut *transaction)
        .await?;
        if rebuilding {
            // Creating the first active generation here would change the
            // replacement fence of an already-running rebuild. Let that
            // rebuild finish (or be explicitly recovered) instead.
            return Err(CqrsSqlxError::MissingGeneration);
        }
        let generation = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO erpx_cqrs_read.projection_generations
                (projection_name, generation, status, source_sequence, created_at, activated_at)
             VALUES ($1, $2, 'active', 0, now(), now())",
        )
        .bind(projection)
        .bind(generation)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(ProjectionGeneration {
            projection: projection.into(),
            generation,
            active: true,
            source_sequence: 0,
        })
    }

    pub async fn active_generation(
        &self,
        projection: &str,
    ) -> Result<Option<ProjectionGeneration>, CqrsSqlxError> {
        validate_projection_name(projection)?;
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT generation, source_sequence
             FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active'",
        )
        .bind(projection)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        row.map(|row| {
            Ok(ProjectionGeneration {
                projection: projection.into(),
                generation: row.try_get("generation")?,
                active: true,
                source_sequence: to_u64(row.try_get("source_sequence")?)?,
            })
        })
        .transpose()
    }

    /// Read one active or building generation so a durable rebuild can resume
    /// from its persisted cursor instead of replaying from zero after retry.
    pub async fn generation(
        &self,
        projection: &str,
        generation: Uuid,
    ) -> Result<Option<ProjectionGeneration>, CqrsSqlxError> {
        validate_projection_name(projection)?;
        if generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection generation is required",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT status, source_sequence
             FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2
               AND status IN ('active', 'building')",
        )
        .bind(projection)
        .bind(generation)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        row.map(|row| {
            let status: String = row.try_get("status")?;
            Ok(ProjectionGeneration {
                projection: projection.into(),
                generation,
                active: status == "active",
                source_sequence: to_u64(row.try_get("source_sequence")?)?,
            })
        })
        .transpose()
    }

    /// Ensure an active generation has a cursor for one physical source.
    /// Building generations receive their complete, immutable source set from
    /// `begin_or_resume_multi_source_rebuild` instead.
    pub async fn ensure_source_checkpoint(
        &self,
        projection: &str,
        generation: Uuid,
        source_key: &ProjectionSourceKey,
    ) -> Result<ProjectionSourceCheckpoint, CqrsSqlxError> {
        validate_projection_name(projection)?;
        if generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection generation is required",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let statement = if self.pool.holding_id().is_some() {
            "INSERT INTO erpx_cqrs_read.projection_source_checkpoints
                (projection_name, generation, source_key, source_sequence,
                 rebuild_target_sequence, created_at, updated_at, holding_id)
             SELECT $1, $2, $3, 0, NULL, now(), now(), $4
             WHERE EXISTS (
                 SELECT 1 FROM erpx_cqrs_read.projection_generations
                 WHERE projection_name = $1 AND generation = $2 AND status = 'active'
                   AND holding_id = $4
             )
             ON CONFLICT (holding_id, projection_name, generation, source_key) DO NOTHING"
        } else {
            "INSERT INTO erpx_cqrs_read.projection_source_checkpoints
                (projection_name, generation, source_key, source_sequence,
                 rebuild_target_sequence, created_at, updated_at)
             SELECT $1, $2, $3, 0, NULL, now(), now()
             WHERE EXISTS (
                 SELECT 1 FROM erpx_cqrs_read.projection_generations
                 WHERE projection_name = $1 AND generation = $2 AND status = 'active'
             )
             ON CONFLICT (projection_name, generation, source_key) DO NOTHING"
        };
        let query = sqlx::query(statement)
            .bind(projection)
            .bind(generation)
            .bind(source_key.as_str());
        match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).execute(&mut *transaction).await?,
            None => query.execute(&mut *transaction).await?,
        };
        let statement = if self.pool.holding_id().is_some() {
            "SELECT checkpoint.source_sequence, checkpoint.rebuild_target_sequence
             FROM erpx_cqrs_read.projection_source_checkpoints checkpoint
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id = checkpoint.holding_id
              AND generation.projection_name = checkpoint.projection_name
              AND generation.generation = checkpoint.generation
              AND generation.status = 'active'
             WHERE checkpoint.projection_name = $1
               AND checkpoint.generation = $2
               AND checkpoint.source_key = $3 AND checkpoint.holding_id = $4"
        } else {
            "SELECT checkpoint.source_sequence, checkpoint.rebuild_target_sequence
             FROM erpx_cqrs_read.projection_source_checkpoints checkpoint
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = checkpoint.projection_name
              AND generation.generation = checkpoint.generation
              AND generation.status = 'active'
             WHERE checkpoint.projection_name = $1
               AND checkpoint.generation = $2
               AND checkpoint.source_key = $3"
        };
        let query = sqlx::query(statement)
            .bind(projection)
            .bind(generation)
            .bind(source_key.as_str());
        let row = match self.pool.holding_id() {
            Some(holding_id) => {
                query
                    .bind(holding_id)
                    .fetch_optional(&mut *transaction)
                    .await?
            }
            None => query.fetch_optional(&mut *transaction).await?,
        };
        let result = row
            .map(|row| source_checkpoint_from_row(projection, generation, source_key, &row))
            .transpose()?
            .ok_or(CqrsSqlxError::MissingGeneration);
        transaction.commit().await?;
        result
    }

    pub async fn source_checkpoint(
        &self,
        projection: &str,
        generation: Uuid,
        source_key: &ProjectionSourceKey,
    ) -> Result<Option<ProjectionSourceCheckpoint>, CqrsSqlxError> {
        validate_projection_name(projection)?;
        if generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection generation is required",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT source_sequence, rebuild_target_sequence
             FROM erpx_cqrs_read.projection_source_checkpoints
             WHERE projection_name = $1 AND generation = $2 AND source_key = $3",
        )
        .bind(projection)
        .bind(generation)
        .bind(source_key.as_str())
        .fetch_optional(&mut *transaction)
        .await?;
        let result = row
            .map(|row| source_checkpoint_from_row(projection, generation, source_key, &row))
            .transpose();
        transaction.commit().await?;
        result
    }

    /// Recover the immutable source fences already persisted for a building
    /// multi-source generation. An empty result means the generation has not
    /// been started yet; a partially populated row set fails closed.
    pub async fn source_rebuild_fences(
        &self,
        projection: &str,
        generation: Uuid,
    ) -> Result<Vec<ProjectionSourceFence>, CqrsSqlxError> {
        validate_projection_name(projection)?;
        if generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection generation is required",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT source_key, rebuild_target_sequence
             FROM erpx_cqrs_read.projection_source_checkpoints
             WHERE projection_name = $1 AND generation = $2
             ORDER BY source_key",
        )
        .bind(projection)
        .bind(generation)
        .fetch_all(&mut *transaction)
        .await?;
        let result = rows
            .into_iter()
            .map(|row| {
                let source_key =
                    ProjectionSourceKey::try_new(row.try_get::<String, _>("source_key")?)?;
                let target_sequence = row
                    .try_get::<Option<i64>, _>("rebuild_target_sequence")?
                    .ok_or(CqrsSqlxError::GenerationSwitchConflict)
                    .and_then(to_u64)?;
                Ok(ProjectionSourceFence {
                    source_key,
                    target_sequence,
                })
            })
            .collect();
        transaction.commit().await?;
        result
    }

    /// Advance exactly one physical source using compare-and-swap semantics.
    /// This prevents two schedulers from silently combining source-local
    /// cursors and refuses to cross a captured rebuild fence.
    pub async fn advance_source_checkpoint(
        &self,
        projection: &str,
        generation: Uuid,
        source_key: &ProjectionSourceKey,
        expected_after: u64,
        new_sequence: u64,
    ) -> Result<(), CqrsSqlxError> {
        validate_projection_name(projection)?;
        if generation.is_nil() || new_sequence < expected_after {
            return Err(CqrsSqlxError::InvalidInput(
                "projection source cursor is invalid",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR SHARE",
        )
        .bind(projection)
        .bind(generation)
        .fetch_optional(&mut *transaction)
        .await?;
        if !matches!(status.as_deref(), Some("active" | "building")) {
            return Err(CqrsSqlxError::MissingGeneration);
        }
        let updated = sqlx::query(
            "UPDATE erpx_cqrs_read.projection_source_checkpoints
             SET source_sequence = $5, updated_at = now()
             WHERE projection_name = $1
               AND generation = $2
               AND source_key = $3
               AND source_sequence = $4
               AND (rebuild_target_sequence IS NULL
                    OR $5 <= rebuild_target_sequence)",
        )
        .bind(projection)
        .bind(generation)
        .bind(source_key.as_str())
        .bind(to_i64(expected_after)?)
        .bind(to_i64(new_sequence)?)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(CqrsSqlxError::ProjectionPositionConflict);
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn begin_rebuild<P: Projector>(
        &self,
        projector: &P,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        self.begin_or_resume_rebuild(projector, Uuid::new_v4())
            .await
    }

    /// Begin a shadow rebuild using a durable, caller-owned generation, or
    /// resume that exact generation when a scheduler retries the same job.
    ///
    /// `rebuild_generation` should be the durable rebuild/job identifier. A
    /// different in-flight generation for the same projector is rejected so
    /// one scheduler job cannot accidentally adopt another job's failures or
    /// shadow rows. Resuming never calls [`Projector::reset`] again.
    pub async fn begin_or_resume_rebuild<P: Projector>(
        &self,
        projector: &P,
        rebuild_generation: Uuid,
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        validate_projection_name(projector.name())?;
        if rebuild_generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild generation is required",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        lock_rebuild_start(&mut transaction, projector.name()).await?;
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(projector.name())
        .fetch_optional(&mut *transaction)
        .await?;
        let requested_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR UPDATE",
        )
        .bind(projector.name())
        .bind(rebuild_generation)
        .fetch_optional(&mut *transaction)
        .await?;
        let building_generations = sqlx::query_scalar::<_, Uuid>(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'building'
             ORDER BY generation FOR UPDATE",
        )
        .bind(projector.name())
        .fetch_all(&mut *transaction)
        .await?;
        match rebuild_start_action(
            rebuild_generation,
            requested_status.as_deref(),
            &building_generations,
        )? {
            RebuildStartAction::Resume => {
                transaction.commit().await?;
                return Ok(RebuildHandle {
                    projection: projector.name().into(),
                    generation: rebuild_generation,
                    replaces: active,
                });
            }
            RebuildStartAction::Start => {}
        }
        sqlx::query(
            "INSERT INTO erpx_cqrs_read.projection_generations
                (projection_name, generation, status, source_sequence, created_at)
             VALUES ($1, $2, 'building', 0, now())",
        )
        .bind(projector.name())
        .bind(rebuild_generation)
        .execute(&mut *transaction)
        .await?;
        projector
            .reset(
                &mut transaction,
                &ProjectorContext {
                    projection: projector.name(),
                    generation: rebuild_generation,
                },
            )
            .await?;
        transaction.commit().await?;
        Ok(RebuildHandle {
            projection: projector.name().into(),
            generation: rebuild_generation,
            replaces: active,
        })
    }

    /// Begin or resume a rebuild whose source set and per-source fences are
    /// immutable for the lifetime of the durable rebuild generation.
    pub async fn begin_or_resume_multi_source_rebuild<P: Projector>(
        &self,
        projector: &P,
        rebuild_generation: Uuid,
        fences: &[ProjectionSourceFence],
    ) -> Result<RebuildHandle, CqrsSqlxError> {
        validate_projection_name(projector.name())?;
        if rebuild_generation.is_nil() {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild generation is required",
            ));
        }
        let fences = normalize_source_fences(fences)?;
        let mut transaction = self.pool.begin().await?;
        lock_rebuild_start(&mut transaction, projector.name()).await?;
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(projector.name())
        .fetch_optional(&mut *transaction)
        .await?;
        let requested_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR UPDATE",
        )
        .bind(projector.name())
        .bind(rebuild_generation)
        .fetch_optional(&mut *transaction)
        .await?;
        let building_generations = sqlx::query_scalar::<_, Uuid>(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'building'
             ORDER BY generation FOR UPDATE",
        )
        .bind(projector.name())
        .fetch_all(&mut *transaction)
        .await?;
        match rebuild_start_action(
            rebuild_generation,
            requested_status.as_deref(),
            &building_generations,
        )? {
            RebuildStartAction::Resume => {
                let rows = sqlx::query(
                    "SELECT source_key, rebuild_target_sequence
                     FROM erpx_cqrs_read.projection_source_checkpoints
                     WHERE projection_name = $1 AND generation = $2
                     ORDER BY source_key FOR UPDATE",
                )
                .bind(projector.name())
                .bind(rebuild_generation)
                .fetch_all(&mut *transaction)
                .await?;
                let stored = rows
                    .into_iter()
                    .map(|row| {
                        let source_key =
                            ProjectionSourceKey::try_new(row.try_get::<String, _>("source_key")?)?;
                        let target = row
                            .try_get::<Option<i64>, _>("rebuild_target_sequence")?
                            .ok_or(CqrsSqlxError::GenerationSwitchConflict)
                            .and_then(to_u64)?;
                        Ok(ProjectionSourceFence {
                            source_key,
                            target_sequence: target,
                        })
                    })
                    .collect::<Result<Vec<_>, CqrsSqlxError>>()?;
                if stored != fences {
                    return Err(CqrsSqlxError::GenerationSwitchConflict);
                }
                transaction.commit().await?;
                return Ok(RebuildHandle {
                    projection: projector.name().into(),
                    generation: rebuild_generation,
                    replaces: active,
                });
            }
            RebuildStartAction::Start => {}
        }
        sqlx::query(
            "INSERT INTO erpx_cqrs_read.projection_generations
                (projection_name, generation, status, source_sequence, created_at)
             VALUES ($1, $2, 'building', 0, now())",
        )
        .bind(projector.name())
        .bind(rebuild_generation)
        .execute(&mut *transaction)
        .await?;
        for fence in &fences {
            sqlx::query(
                "INSERT INTO erpx_cqrs_read.projection_source_checkpoints
                    (projection_name, generation, source_key, source_sequence,
                     rebuild_target_sequence, created_at, updated_at)
                 VALUES ($1, $2, $3, 0, $4, now(), now())",
            )
            .bind(projector.name())
            .bind(rebuild_generation)
            .bind(fence.source_key.as_str())
            .bind(to_i64(fence.target_sequence)?)
            .execute(&mut *transaction)
            .await?;
        }
        projector
            .reset(
                &mut transaction,
                &ProjectorContext {
                    projection: projector.name(),
                    generation: rebuild_generation,
                },
            )
            .await?;
        transaction.commit().await?;
        Ok(RebuildHandle {
            projection: projector.name().into(),
            generation: rebuild_generation,
            replaces: active,
        })
    }

    pub async fn activate_rebuild(&self, rebuild: &RebuildHandle) -> Result<(), CqrsSqlxError> {
        validate_rebuild_handle_shape(rebuild)?;
        let mut transaction = self.pool.begin().await?;
        lock_rebuild_start(&mut transaction, &rebuild.projection).await?;
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(&rebuild.projection)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != rebuild.replaces {
            return Err(CqrsSqlxError::GenerationSwitchConflict);
        }
        let building: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR UPDATE",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .fetch_optional(&mut *transaction)
        .await?;
        if building.as_deref() != Some("building") {
            return Err(CqrsSqlxError::MissingGeneration);
        }
        let has_dead_letters: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_cqrs_read.projection_failures
                 WHERE projection_name = $1 AND generation = $2
                   AND dead_lettered_at IS NOT NULL
             )",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .fetch_one(&mut *transaction)
        .await?;
        if has_dead_letters {
            return Err(CqrsSqlxError::InvalidInput(
                "projection rebuild has dead letters",
            ));
        }
        sqlx::query(
            "UPDATE erpx_cqrs_read.projection_generations
             SET status = 'retired', retired_at = now()
             WHERE projection_name = $1 AND status = 'active'",
        )
        .bind(&rebuild.projection)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_cqrs_read.projection_generations
             SET status = 'active', activated_at = now()
             WHERE projection_name = $1 AND generation = $2 AND status = 'building'",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Activate a multi-source shadow generation only after every captured
    /// physical source has reached its own rebuild fence.
    pub async fn activate_multi_source_rebuild(
        &self,
        rebuild: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        validate_rebuild_handle_shape(rebuild)?;
        let mut transaction = self.pool.begin().await?;
        lock_rebuild_start(&mut transaction, &rebuild.projection).await?;
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR UPDATE",
        )
        .bind(&rebuild.projection)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != rebuild.replaces {
            return Err(CqrsSqlxError::GenerationSwitchConflict);
        }
        let building: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR UPDATE",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .fetch_optional(&mut *transaction)
        .await?;
        if building.as_deref() != Some("building") {
            return Err(CqrsSqlxError::MissingGeneration);
        }
        let source_rows = sqlx::query(
            "SELECT source_sequence, rebuild_target_sequence
             FROM erpx_cqrs_read.projection_source_checkpoints
             WHERE projection_name = $1 AND generation = $2
             ORDER BY source_key FOR UPDATE",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .fetch_all(&mut *transaction)
        .await?;
        let mut ready = !source_rows.is_empty();
        for row in source_rows {
            let source = to_u64(row.try_get("source_sequence")?)?;
            let target = row
                .try_get::<Option<i64>, _>("rebuild_target_sequence")?
                .map(to_u64)
                .transpose()?;
            ready &= target.is_some_and(|target| source >= target);
        }
        let has_dead_letters: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                 SELECT 1 FROM erpx_cqrs_read.projection_failures
                 WHERE projection_name = $1 AND generation = $2
                   AND dead_lettered_at IS NOT NULL
             )",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .fetch_one(&mut *transaction)
        .await?;
        if !ready || has_dead_letters {
            return Err(CqrsSqlxError::InvalidInput(
                "multi-source rebuild is incomplete or has dead letters",
            ));
        }
        sqlx::query(
            "UPDATE erpx_cqrs_read.projection_generations
             SET status = 'retired', retired_at = now()
             WHERE projection_name = $1 AND status = 'active'",
        )
        .bind(&rebuild.projection)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_cqrs_read.projection_generations
             SET status = 'active', activated_at = now()
             WHERE projection_name = $1 AND generation = $2 AND status = 'building'",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE erpx_cqrs_read.projection_source_checkpoints
             SET rebuild_target_sequence = NULL, updated_at = now()
             WHERE projection_name = $1 AND generation = $2",
        )
        .bind(&rebuild.projection)
        .bind(rebuild.generation)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn require_building_handle(
        &self,
        projection: &str,
        rebuild: &RebuildHandle,
    ) -> Result<(), CqrsSqlxError> {
        validate_projection_name(projection)?;
        validate_rebuild_handle_shape(rebuild)?;
        if rebuild.projection != projection {
            return Err(CqrsSqlxError::InvalidInput(
                "rebuild handle belongs to another projector",
            ));
        }
        let mut transaction = self.pool.begin().await?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND generation = $2 FOR SHARE",
        )
        .bind(projection)
        .bind(rebuild.generation)
        .fetch_optional(&mut *transaction)
        .await?;
        if status.as_deref() != Some("building") {
            return Err(CqrsSqlxError::MissingGeneration);
        }
        let active: Option<Uuid> = sqlx::query_scalar(
            "SELECT generation FROM erpx_cqrs_read.projection_generations
             WHERE projection_name = $1 AND status = 'active' FOR SHARE",
        )
        .bind(projection)
        .fetch_optional(&mut *transaction)
        .await?;
        if active != rebuild.replaces {
            return Err(CqrsSqlxError::GenerationSwitchConflict);
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn apply<P: Projector>(
        &self,
        projector: &P,
        generation: Uuid,
        event: &ProjectionEvent,
        retry: RetryPolicy,
    ) -> Result<ApplyDisposition, CqrsSqlxError> {
        if let Some(disposition) = self
            .retry_state(projector.name(), generation, event)
            .await?
        {
            return Ok(disposition);
        }
        let mut transaction = self.pool.begin().await?;
        match apply_decision(
            &mut transaction,
            self.pool.holding_id(),
            projector.name(),
            generation,
            event,
        )
        .await?
        {
            Decision::Duplicate => {
                transaction.commit().await?;
                return Ok(ApplyDisposition::Duplicate);
            }
            Decision::PositionConflict => {
                return Err(CqrsSqlxError::ProjectionPositionConflict);
            }
            Decision::OutOfOrder => return Err(CqrsSqlxError::ProjectionOutOfOrder),
            Decision::Apply => {}
        }
        let context = ProjectorContext {
            projection: projector.name(),
            generation,
        };
        if let Err(error) = projector.apply(&mut transaction, &context, event).await {
            transaction.rollback().await?;
            return self
                .record_failure(projector.name(), generation, event, &error, retry)
                .await;
        }
        record_applied(
            &mut transaction,
            self.pool.holding_id(),
            projector.name(),
            generation,
            event,
        )
        .await?;
        transaction.commit().await?;
        Ok(ApplyDisposition::Applied)
    }

    async fn retry_state(
        &self,
        projection: &str,
        generation: Uuid,
        event: &ProjectionEvent,
    ) -> Result<Option<ApplyDisposition>, CqrsSqlxError> {
        let mut transaction = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT attempts, next_retry_at, dead_lettered_at
             FROM erpx_cqrs_read.projection_failures
             WHERE projection_name = $1 AND generation = $2
               AND partition_key = $3 AND event_id = $4",
        )
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let attempts = u32::try_from(row.try_get::<i32, _>("attempts")?)
            .map_err(|_| CqrsSqlxError::NumericRange)?;
        let dead_lettered_at: Option<DateTime<Utc>> = row.try_get("dead_lettered_at")?;
        if dead_lettered_at.is_some() {
            return Ok(Some(ApplyDisposition::DeadLettered { attempts }));
        }
        let next_retry_at: Option<DateTime<Utc>> = row.try_get("next_retry_at")?;
        if next_retry_at.is_some_and(|next| next > Utc::now()) {
            return Ok(Some(ApplyDisposition::RetryScheduled { attempt: attempts }));
        }
        Ok(None)
    }

    async fn record_failure(
        &self,
        projection: &str,
        generation: Uuid,
        event: &ProjectionEvent,
        failure: &ProjectionFailure,
        retry: RetryPolicy,
    ) -> Result<ApplyDisposition, CqrsSqlxError> {
        let mut transaction = self.pool.begin().await?;
        let previous: Option<i32> = sqlx::query_scalar(
            "SELECT attempts FROM erpx_cqrs_read.projection_failures
             WHERE projection_name = $1 AND generation = $2
               AND partition_key = $3 AND event_id = $4",
        )
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await?;
        let attempt = previous
            .map(|value| u32::try_from(value).map_err(|_| CqrsSqlxError::NumericRange))
            .transpose()?
            .unwrap_or(0)
            .saturating_add(1);
        let dead_lettered = attempt >= retry.max_attempts;
        let next_retry_at = if dead_lettered {
            None
        } else {
            chrono::Duration::from_std(retry.delay_for(attempt))
                .ok()
                .map(|delay| Utc::now() + delay)
        };
        let message = bounded_failure_message(failure.message());
        let statement = if self.pool.holding_id().is_some() {
            "INSERT INTO erpx_cqrs_read.projection_failures
                (projection_name, generation, partition_key, event_id, global_sequence,
                 attempts, next_retry_at, dead_lettered_at, last_error, updated_at, holding_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7,
                     CASE WHEN $8 THEN now() ELSE NULL END, $9, now(), $10)
             ON CONFLICT (holding_id, projection_name, generation, partition_key, event_id)
             DO UPDATE SET attempts = EXCLUDED.attempts,
                           next_retry_at = EXCLUDED.next_retry_at,
                           dead_lettered_at = EXCLUDED.dead_lettered_at,
                           last_error = EXCLUDED.last_error,
                           updated_at = now()"
        } else {
            "INSERT INTO erpx_cqrs_read.projection_failures
                (projection_name, generation, partition_key, event_id, global_sequence,
                 attempts, next_retry_at, dead_lettered_at, last_error, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7,
                     CASE WHEN $8 THEN now() ELSE NULL END, $9, now())
             ON CONFLICT (projection_name, generation, partition_key, event_id)
             DO UPDATE SET attempts = EXCLUDED.attempts,
                           next_retry_at = EXCLUDED.next_retry_at,
                           dead_lettered_at = EXCLUDED.dead_lettered_at,
                           last_error = EXCLUDED.last_error,
                           updated_at = now()"
        };
        let query = sqlx::query(statement)
            .bind(projection)
            .bind(generation)
            .bind(&event.partition)
            .bind(event.event_id.as_uuid())
            .bind(to_i64(event.global_sequence)?)
            .bind(i32::try_from(attempt).map_err(|_| CqrsSqlxError::NumericRange)?)
            .bind(next_retry_at)
            .bind(dead_lettered)
            .bind(message);
        match self.pool.holding_id() {
            Some(holding_id) => query.bind(holding_id).execute(&mut *transaction).await?,
            None => query.execute(&mut *transaction).await?,
        };
        transaction.commit().await?;
        if dead_lettered {
            Ok(ApplyDisposition::DeadLettered { attempts: attempt })
        } else {
            Ok(ApplyDisposition::RetryScheduled { attempt })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RebuildStartAction {
    Start,
    Resume,
}

fn rebuild_start_action(
    requested: Uuid,
    requested_status: Option<&str>,
    building_generations: &[Uuid],
) -> Result<RebuildStartAction, CqrsSqlxError> {
    match requested_status {
        Some("building")
            if building_generations.len() == 1 && building_generations[0] == requested =>
        {
            Ok(RebuildStartAction::Resume)
        }
        Some("building") => Err(CqrsSqlxError::GenerationSwitchConflict),
        Some("active" | "retired") => Err(CqrsSqlxError::MissingGeneration),
        Some(_) => Err(CqrsSqlxError::MissingGeneration),
        None if building_generations.is_empty() => Ok(RebuildStartAction::Start),
        None => Err(CqrsSqlxError::GenerationSwitchConflict),
    }
}

async fn lock_rebuild_start(
    transaction: &mut Transaction<'_, Postgres>,
    projection: &str,
) -> Result<(), CqrsSqlxError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0::bigint))")
        .bind(projection)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn validate_rebuild_handle_shape(rebuild: &RebuildHandle) -> Result<(), CqrsSqlxError> {
    validate_projection_name(&rebuild.projection)?;
    if rebuild.generation.is_nil() || rebuild.replaces == Some(rebuild.generation) {
        return Err(CqrsSqlxError::InvalidInput("rebuild handle is invalid"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProjectionRunner {
    source: ProjectionSource,
    store: ProjectionStore,
    config: ProjectionRunnerConfig,
}

impl ProjectionRunner {
    pub fn new(
        source: ProjectionSource,
        store: ProjectionStore,
        config: ProjectionRunnerConfig,
    ) -> Result<Self, CqrsSqlxError> {
        if config.page_size == 0 || config.page_size > MAX_PROJECTION_PAGE_SIZE {
            return Err(CqrsSqlxError::InvalidInput(
                "projection page size is outside the allowed range",
            ));
        }
        config.retry.validate()?;
        Ok(Self {
            source,
            store,
            config,
        })
    }

    pub async fn run_active<P: Projector>(
        &self,
        projector: &P,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        let generation = self.store.ensure_active(projector.name()).await?;
        self.run_generation(projector, generation.generation, generation.source_sequence)
            .await
    }

    pub async fn run_rebuild<P: Projector>(
        &self,
        projector: &P,
        rebuild: &RebuildHandle,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        self.store
            .require_building_handle(projector.name(), rebuild)
            .await?;
        self.run_generation(projector, rebuild.generation, after_sequence)
            .await
    }

    async fn run_generation<P: Projector>(
        &self,
        projector: &P,
        generation: Uuid,
        after_sequence: u64,
    ) -> Result<ProjectionRun, CqrsSqlxError> {
        let page = self
            .source
            .page_after(
                after_sequence,
                projector.event_types(),
                self.config.page_size,
            )
            .await?;
        let mut run = ProjectionRun {
            fetched: u32::try_from(page.events.len()).unwrap_or(u32::MAX),
            cursor: after_sequence,
            ..ProjectionRun::default()
        };
        for event in page.events {
            match self
                .store
                .apply(projector, generation, &event, self.config.retry)
                .await?
            {
                ApplyDisposition::Applied => run.applied += 1,
                ApplyDisposition::Duplicate => run.duplicates += 1,
                ApplyDisposition::DeadLettered { .. } => run.dead_lettered += 1,
                ApplyDisposition::RetryScheduled { .. } => {
                    run.retry_scheduled = true;
                    break;
                }
            }
            run.cursor = event.global_sequence;
            self.update_cursor(projector.name(), generation, run.cursor)
                .await?;
        }
        Ok(run)
    }

    async fn update_cursor(
        &self,
        projection: &str,
        generation: Uuid,
        cursor: u64,
    ) -> Result<(), CqrsSqlxError> {
        let mut transaction = self.store.pool.begin().await?;
        let updated = sqlx::query(
            "UPDATE erpx_cqrs_read.projection_generations
             SET source_sequence = GREATEST(source_sequence, $3)
             WHERE projection_name = $1 AND generation = $2
               AND status IN ('active', 'building')",
        )
        .bind(projection)
        .bind(generation)
        .bind(to_i64(cursor)?)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(CqrsSqlxError::MissingGeneration);
        }
        transaction.commit().await?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProjectionRun {
    pub source_key: ProjectionSourceKey,
    pub run: ProjectionRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MultiSourceProjectionRun {
    pub sources: Vec<SourceProjectionRun>,
}

#[derive(Debug, Clone)]
struct MultiSourceEntry {
    key: ProjectionSourceKey,
    source: ProjectionSource,
}

/// Fair, source-qualified projection runner for fan-in from physically
/// independent write databases. The existing `ProjectionRunner` remains the
/// single-source path.
#[derive(Debug, Clone)]
pub struct MultiSourceProjectionRunner {
    sources: Vec<MultiSourceEntry>,
    store: ProjectionStore,
    config: ProjectionRunnerConfig,
    next_source: Arc<AtomicUsize>,
}

impl MultiSourceProjectionRunner {
    pub fn new(
        sources: Vec<(ProjectionSourceKey, ProjectionSource)>,
        store: ProjectionStore,
        config: ProjectionRunnerConfig,
    ) -> Result<Self, CqrsSqlxError> {
        if sources.is_empty() {
            return Err(CqrsSqlxError::InvalidInput(
                "multi-source projection requires at least one source",
            ));
        }
        if config.page_size == 0 || config.page_size > MAX_PROJECTION_PAGE_SIZE {
            return Err(CqrsSqlxError::InvalidInput(
                "projection page size is outside the allowed range",
            ));
        }
        config.retry.validate()?;
        let mut seen = BTreeSet::new();
        let mut sources = sources
            .into_iter()
            .map(|(key, source)| {
                if !seen.insert(key.clone()) {
                    return Err(CqrsSqlxError::InvalidInput(
                        "projection source key is duplicated",
                    ));
                }
                Ok(MultiSourceEntry { key, source })
            })
            .collect::<Result<Vec<_>, _>>()?;
        sources.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(Self {
            sources,
            store,
            config,
            next_source: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn source_keys(&self) -> impl ExactSizeIterator<Item = &ProjectionSourceKey> {
        self.sources.iter().map(|entry| &entry.key)
    }

    /// Capture one reachable high-watermark per physical source. These fences
    /// are passed unchanged to `begin_or_resume_multi_source_rebuild`.
    pub async fn capture_source_fences<P: Projector>(
        &self,
        projector: &P,
    ) -> Result<Vec<ProjectionSourceFence>, CqrsSqlxError> {
        let mut fences = Vec::with_capacity(self.sources.len());
        for entry in &self.sources {
            fences.push(ProjectionSourceFence {
                source_key: entry.key.clone(),
                target_sequence: entry.source.high_watermark(projector.event_types()).await?,
            });
        }
        Ok(fences)
    }

    /// Run at most one page from every source, rotating the starting source on
    /// each invocation so a busy first source cannot consistently get priority.
    pub async fn run_active<P: Projector>(
        &self,
        projector: &P,
    ) -> Result<MultiSourceProjectionRun, CqrsSqlxError> {
        let generation = self.store.ensure_active(projector.name()).await?;
        let mut result = MultiSourceProjectionRun {
            sources: Vec::with_capacity(self.sources.len()),
        };
        for index in self.fair_source_indices() {
            let entry = &self.sources[index];
            let checkpoint = self
                .store
                .ensure_source_checkpoint(projector.name(), generation.generation, &entry.key)
                .await?;
            result.sources.push(
                self.run_source(
                    projector,
                    generation.generation,
                    entry,
                    checkpoint.source_sequence,
                    None,
                )
                .await?,
            );
        }
        Ok(result)
    }

    /// Run at most one bounded page per configured source for a building
    /// generation. Each source stops at its own captured high-watermark.
    pub async fn run_rebuild<P: Projector>(
        &self,
        projector: &P,
        rebuild: &RebuildHandle,
    ) -> Result<MultiSourceProjectionRun, CqrsSqlxError> {
        self.store
            .require_building_handle(projector.name(), rebuild)
            .await?;
        let mut result = MultiSourceProjectionRun {
            sources: Vec::with_capacity(self.sources.len()),
        };
        for index in self.fair_source_indices() {
            let entry = &self.sources[index];
            let checkpoint = self
                .store
                .source_checkpoint(projector.name(), rebuild.generation, &entry.key)
                .await?
                .ok_or(CqrsSqlxError::GenerationSwitchConflict)?;
            let target = checkpoint
                .rebuild_target_sequence
                .ok_or(CqrsSqlxError::GenerationSwitchConflict)?;
            result.sources.push(
                self.run_source(
                    projector,
                    rebuild.generation,
                    entry,
                    checkpoint.source_sequence,
                    Some(target),
                )
                .await?,
            );
        }
        Ok(result)
    }

    fn fair_source_indices(&self) -> Vec<usize> {
        let start = self.next_source.fetch_add(1, Ordering::Relaxed) % self.sources.len();
        rotated_indices(self.sources.len(), start)
    }

    async fn run_source<P: Projector>(
        &self,
        projector: &P,
        generation: Uuid,
        entry: &MultiSourceEntry,
        after_sequence: u64,
        through_sequence: Option<u64>,
    ) -> Result<SourceProjectionRun, CqrsSqlxError> {
        if through_sequence.is_some_and(|target| after_sequence >= target) {
            return Ok(SourceProjectionRun {
                source_key: entry.key.clone(),
                run: ProjectionRun {
                    cursor: after_sequence,
                    ..ProjectionRun::default()
                },
            });
        }
        let page = entry
            .source
            .page_between(
                after_sequence,
                through_sequence,
                projector.event_types(),
                self.config.page_size,
            )
            .await?;
        let mut run = ProjectionRun {
            fetched: u32::try_from(page.events.len()).unwrap_or(u32::MAX),
            cursor: after_sequence,
            ..ProjectionRun::default()
        };
        for mut event in page.events {
            event.partition = entry.key.qualify_partition(&event.partition)?;
            match self
                .store
                .apply(projector, generation, &event, self.config.retry)
                .await?
            {
                ApplyDisposition::Applied => run.applied += 1,
                ApplyDisposition::Duplicate => run.duplicates += 1,
                ApplyDisposition::DeadLettered { .. } => run.dead_lettered += 1,
                ApplyDisposition::RetryScheduled { .. } => {
                    run.retry_scheduled = true;
                    break;
                }
            }
            let previous = run.cursor;
            run.cursor = event.global_sequence;
            self.store
                .advance_source_checkpoint(
                    projector.name(),
                    generation,
                    &entry.key,
                    previous,
                    run.cursor,
                )
                .await?;
        }
        Ok(SourceProjectionRun {
            source_key: entry.key.clone(),
            run,
        })
    }
}

fn source_checkpoint_from_row(
    projection: &str,
    generation: Uuid,
    source_key: &ProjectionSourceKey,
    row: &sqlx::postgres::PgRow,
) -> Result<ProjectionSourceCheckpoint, CqrsSqlxError> {
    Ok(ProjectionSourceCheckpoint {
        projection: projection.into(),
        generation,
        source_key: source_key.clone(),
        source_sequence: to_u64(row.try_get("source_sequence")?)?,
        rebuild_target_sequence: row
            .try_get::<Option<i64>, _>("rebuild_target_sequence")?
            .map(to_u64)
            .transpose()?,
    })
}

fn normalize_source_fences(
    fences: &[ProjectionSourceFence],
) -> Result<Vec<ProjectionSourceFence>, CqrsSqlxError> {
    if fences.is_empty() {
        return Err(CqrsSqlxError::InvalidInput(
            "multi-source rebuild requires at least one source fence",
        ));
    }
    let mut normalized = BTreeMap::new();
    for fence in fences {
        if normalized
            .insert(fence.source_key.clone(), fence.target_sequence)
            .is_some()
        {
            return Err(CqrsSqlxError::InvalidInput(
                "projection source fence is duplicated",
            ));
        }
    }
    Ok(normalized
        .into_iter()
        .map(|(source_key, target_sequence)| ProjectionSourceFence {
            source_key,
            target_sequence,
        })
        .collect())
}

fn rotated_indices(length: usize, start: usize) -> Vec<usize> {
    if length == 0 {
        return Vec::new();
    }
    (0..length)
        .map(|offset| (start + offset) % length)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Apply,
    Duplicate,
    PositionConflict,
    OutOfOrder,
}

async fn apply_decision(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: Option<Uuid>,
    projection: &str,
    generation: Uuid,
    event: &ProjectionEvent,
) -> Result<Decision, CqrsSqlxError> {
    let statement = if holding_id.is_some() {
        "SELECT global_sequence, event_type, event_version
         FROM erpx_cqrs_read.projection_applied_events
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND event_id = $4 AND holding_id = $5"
    } else {
        "SELECT global_sequence, event_type, event_version
         FROM erpx_cqrs_read.projection_applied_events
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND event_id = $4"
    };
    let query = sqlx::query(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid());
    let duplicate = match holding_id {
        Some(holding_id) => {
            query
                .bind(holding_id)
                .fetch_optional(&mut **transaction)
                .await?
        }
        None => query.fetch_optional(&mut **transaction).await?,
    };
    if let Some(row) = duplicate {
        let same = to_u64(row.try_get("global_sequence")?)? == event.global_sequence
            && row.try_get::<String, _>("event_type")? == event.event_type
            && u32::try_from(row.try_get::<i32, _>("event_version")?)
                .map_err(|_| CqrsSqlxError::NumericRange)?
                == event.event_version;
        return Ok(if same {
            Decision::Duplicate
        } else {
            Decision::PositionConflict
        });
    }
    let statement = if holding_id.is_some() {
        "SELECT event_id FROM erpx_cqrs_read.projection_applied_events
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND global_sequence = $4 AND holding_id = $5"
    } else {
        "SELECT event_id FROM erpx_cqrs_read.projection_applied_events
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND global_sequence = $4"
    };
    let query = sqlx::query_scalar(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(to_i64(event.global_sequence)?);
    let occupied: Option<Uuid> = match holding_id {
        Some(holding_id) => {
            query
                .bind(holding_id)
                .fetch_optional(&mut **transaction)
                .await?
        }
        None => query.fetch_optional(&mut **transaction).await?,
    };
    if occupied.is_some() {
        return Ok(Decision::PositionConflict);
    }
    let statement = if holding_id.is_some() {
        "SELECT global_sequence FROM erpx_cqrs_read.projection_checkpoints
         WHERE projection_name = $1 AND generation = $2 AND partition_key = $3
           AND holding_id = $4 FOR UPDATE"
    } else {
        "SELECT global_sequence FROM erpx_cqrs_read.projection_checkpoints
         WHERE projection_name = $1 AND generation = $2 AND partition_key = $3
         FOR UPDATE"
    };
    let query = sqlx::query_scalar(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition);
    let checkpoint: Option<i64> = match holding_id {
        Some(holding_id) => {
            query
                .bind(holding_id)
                .fetch_optional(&mut **transaction)
                .await?
        }
        None => query.fetch_optional(&mut **transaction).await?,
    };
    match checkpoint.map(to_u64).transpose()? {
        Some(position) if event.global_sequence == position => Ok(Decision::PositionConflict),
        Some(position) if event.global_sequence < position => Ok(Decision::OutOfOrder),
        _ => Ok(Decision::Apply),
    }
}

async fn record_applied(
    transaction: &mut Transaction<'_, Postgres>,
    holding_id: Option<Uuid>,
    projection: &str,
    generation: Uuid,
    event: &ProjectionEvent,
) -> Result<(), CqrsSqlxError> {
    let statement = if holding_id.is_some() {
        "INSERT INTO erpx_cqrs_read.projection_applied_events
            (projection_name, generation, partition_key, event_id, global_sequence,
             event_type, event_version, applied_at, holding_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), $8)"
    } else {
        "INSERT INTO erpx_cqrs_read.projection_applied_events
            (projection_name, generation, partition_key, event_id, global_sequence,
             event_type, event_version, applied_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())"
    };
    let query = sqlx::query(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid())
        .bind(to_i64(event.global_sequence)?)
        .bind(&event.event_type)
        .bind(i32::try_from(event.event_version).map_err(|_| CqrsSqlxError::NumericRange)?);
    match holding_id {
        Some(holding_id) => query.bind(holding_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    let statement = if holding_id.is_some() {
        "INSERT INTO erpx_cqrs_read.projection_checkpoints
            (projection_name, generation, partition_key, event_id, global_sequence, updated_at,
             holding_id)
         VALUES ($1, $2, $3, $4, $5, now(), $6)
         ON CONFLICT (holding_id, projection_name, generation, partition_key)
         DO UPDATE SET event_id = EXCLUDED.event_id,
                       global_sequence = EXCLUDED.global_sequence,
                       updated_at = now()"
    } else {
        "INSERT INTO erpx_cqrs_read.projection_checkpoints
            (projection_name, generation, partition_key, event_id, global_sequence, updated_at)
         VALUES ($1, $2, $3, $4, $5, now())
         ON CONFLICT (projection_name, generation, partition_key)
         DO UPDATE SET event_id = EXCLUDED.event_id,
                       global_sequence = EXCLUDED.global_sequence,
                       updated_at = now()"
    };
    let query = sqlx::query(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid())
        .bind(to_i64(event.global_sequence)?);
    match holding_id {
        Some(holding_id) => query.bind(holding_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    let statement = if holding_id.is_some() {
        "INSERT INTO erpx_cqrs_read.projection_receipts
            (projection_name, generation, partition_key, command_id,
             correlation_id, event_id, global_sequence, projected_at, holding_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), $8)
         ON CONFLICT (holding_id, projection_name, generation, partition_key, command_id)
         DO UPDATE SET correlation_id = EXCLUDED.correlation_id,
                       event_id = EXCLUDED.event_id,
                       global_sequence = GREATEST(
                           erpx_cqrs_read.projection_receipts.global_sequence,
                           EXCLUDED.global_sequence
                       ),
                       projected_at = now()"
    } else {
        "INSERT INTO erpx_cqrs_read.projection_receipts
            (projection_name, generation, partition_key, command_id,
             correlation_id, event_id, global_sequence, projected_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, now())
         ON CONFLICT (projection_name, generation, partition_key, command_id)
         DO UPDATE SET correlation_id = EXCLUDED.correlation_id,
                       event_id = EXCLUDED.event_id,
                       global_sequence = GREATEST(
                           erpx_cqrs_read.projection_receipts.global_sequence,
                           EXCLUDED.global_sequence
                       ),
                       projected_at = now()"
    };
    let query = sqlx::query(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.command_id.as_uuid())
        .bind(event.correlation_id.as_uuid())
        .bind(event.event_id.as_uuid())
        .bind(to_i64(event.global_sequence)?);
    match holding_id {
        Some(holding_id) => query.bind(holding_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    let statement = if holding_id.is_some() {
        "DELETE FROM erpx_cqrs_read.projection_failures
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND event_id = $4 AND holding_id = $5"
    } else {
        "DELETE FROM erpx_cqrs_read.projection_failures
         WHERE projection_name = $1 AND generation = $2
           AND partition_key = $3 AND event_id = $4"
    };
    let query = sqlx::query(statement)
        .bind(projection)
        .bind(generation)
        .bind(&event.partition)
        .bind(event.event_id.as_uuid());
    match holding_id {
        Some(holding_id) => query.bind(holding_id).execute(&mut **transaction).await?,
        None => query.execute(&mut **transaction).await?,
    };
    Ok(())
}

fn validate_projection_name(value: &str) -> Result<(), CqrsSqlxError> {
    if value.trim().is_empty() || value.len() > 128 {
        return Err(CqrsSqlxError::InvalidInput("projection name is invalid"));
    }
    Ok(())
}

fn bounded_failure_message(value: &str) -> String {
    if value.len() <= MAX_FAILURE_MESSAGE_BYTES {
        return value.into();
    }
    let mut boundary = MAX_FAILURE_MESSAGE_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].into()
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
    fn retry_delay_is_bounded_exponential_backoff() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            maximum_delay: Duration::from_secs(1),
        };
        assert_eq!(policy.delay_for(1), Duration::from_millis(100));
        assert_eq!(policy.delay_for(4), Duration::from_millis(800));
        assert_eq!(policy.delay_for(10), Duration::from_secs(1));
    }

    #[test]
    fn invalid_retry_policies_are_rejected() {
        let policy = RetryPolicy {
            max_attempts: 0,
            ..RetryPolicy::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn persisted_failure_messages_are_utf8_safe_and_bounded() {
        let source = "ก".repeat(300);
        let bounded = bounded_failure_message(&source);
        assert!(bounded.len() <= MAX_FAILURE_MESSAGE_BYTES);
        assert!(source.starts_with(&bounded));
    }

    #[test]
    fn scheduler_retry_resumes_only_its_stable_building_generation() {
        let generation = Uuid::new_v4();
        assert_eq!(
            rebuild_start_action(generation, Some("building"), &[generation])
                .expect("same durable rebuild generation resumes"),
            RebuildStartAction::Resume
        );

        let other = Uuid::new_v4();
        assert!(matches!(
            rebuild_start_action(generation, None, &[other]),
            Err(CqrsSqlxError::GenerationSwitchConflict)
        ));
        assert!(matches!(
            rebuild_start_action(generation, Some("building"), &[generation, other]),
            Err(CqrsSqlxError::GenerationSwitchConflict)
        ));
    }

    #[test]
    fn completed_or_retired_generation_cannot_be_resumed_as_building() {
        let generation = Uuid::new_v4();
        for status in ["active", "retired"] {
            assert!(matches!(
                rebuild_start_action(generation, Some(status), &[]),
                Err(CqrsSqlxError::MissingGeneration)
            ));
        }
    }

    #[test]
    fn rebuild_handle_shape_rejects_nil_and_self_replacement() {
        let generation = Uuid::new_v4();
        let nil = RebuildHandle {
            projection: "quality.inspections".into(),
            generation: Uuid::nil(),
            replaces: None,
        };
        assert!(matches!(
            validate_rebuild_handle_shape(&nil),
            Err(CqrsSqlxError::InvalidInput(_))
        ));

        let self_replacing = RebuildHandle {
            projection: "quality.inspections".into(),
            generation,
            replaces: Some(generation),
        };
        assert!(matches!(
            validate_rebuild_handle_shape(&self_replacing),
            Err(CqrsSqlxError::InvalidInput(_))
        ));
    }

    #[test]
    fn incremental_runner_keeps_using_only_the_active_generation_path() {
        let source = include_str!("projection.rs");
        let run_active = source
            .split("pub async fn run_active")
            .nth(1)
            .and_then(|tail| tail.split("pub async fn run_rebuild").next())
            .expect("run_active source");
        assert!(run_active.contains("ensure_active"));
        assert!(!run_active.contains("begin_or_resume_rebuild"));
    }

    #[test]
    fn source_keys_are_stable_and_qualify_partitions() {
        let source = ProjectionSourceKey::try_new("holding:019fcda0-65b5-72e3-85cf-ecd0860f585d")
            .expect("holding route is a valid source key");
        assert_eq!(
            source
                .qualify_partition("customer:42")
                .expect("qualified partition"),
            "holding:019fcda0-65b5-72e3-85cf-ecd0860f585d/customer:42"
        );
        let (parsed_source, local_partition) = ProjectionSourceKey::parse_qualified_partition(
            "holding:019fcda0-65b5-72e3-85cf-ecd0860f585d/customer:42",
        )
        .expect("source provenance");
        assert_eq!(parsed_source, source);
        assert_eq!(local_partition, "customer:42");
        for invalid in ["", " holding:a", "holding:a ", "holding/a", "holding a"] {
            assert!(ProjectionSourceKey::try_new(invalid).is_err(), "{invalid}");
        }
        for invalid in ["holding:a", "holding:a/", "/customer:42"] {
            assert!(ProjectionSourceKey::parse_qualified_partition(invalid).is_err());
        }
    }

    #[test]
    fn qualified_partitions_keep_the_read_your_writes_identity_bounded() {
        let source = ProjectionSourceKey::try_new("holding:a").unwrap();
        let maximum_partition = "p".repeat(MAX_QUALIFIED_PARTITION_LENGTH);
        assert!(source.qualify_partition(&maximum_partition).is_err());
        assert!(source.qualify_partition(" ").is_err());
    }

    #[test]
    fn source_fences_are_sorted_and_duplicate_sources_are_rejected() {
        let a = ProjectionSourceKey::try_new("holding:a").unwrap();
        let b = ProjectionSourceKey::try_new("holding:b").unwrap();
        let normalized = normalize_source_fences(&[
            ProjectionSourceFence {
                source_key: b.clone(),
                target_sequence: 8,
            },
            ProjectionSourceFence {
                source_key: a.clone(),
                target_sequence: 13,
            },
        ])
        .expect("unique fences");
        assert_eq!(normalized[0].source_key, a);
        assert_eq!(normalized[1].source_key, b.clone());
        assert!(normalize_source_fences(&[]).is_err());
        assert!(
            normalize_source_fences(&[
                ProjectionSourceFence {
                    source_key: b.clone(),
                    target_sequence: 8,
                },
                ProjectionSourceFence {
                    source_key: b,
                    target_sequence: 9,
                },
            ])
            .is_err()
        );
    }

    #[test]
    fn fair_source_order_rotates_without_skipping_a_source() {
        assert_eq!(rotated_indices(3, 0), vec![0, 1, 2]);
        assert_eq!(rotated_indices(3, 1), vec![1, 2, 0]);
        assert_eq!(rotated_indices(3, 2), vec![2, 0, 1]);
        assert!(rotated_indices(0, 0).is_empty());
    }

    #[test]
    fn rebuild_readiness_is_source_local_and_bounded() {
        let checkpoint = |source_sequence, rebuild_target_sequence| ProjectionSourceCheckpoint {
            projection: "customer360.holding_activity".into(),
            generation: Uuid::new_v4(),
            source_key: ProjectionSourceKey::try_new("holding:a").unwrap(),
            source_sequence,
            rebuild_target_sequence,
        };
        assert!(checkpoint(11, Some(11)).rebuild_ready());
        assert!(checkpoint(12, Some(11)).rebuild_ready());
        assert!(!checkpoint(10, Some(11)).rebuild_ready());
        assert!(!checkpoint(11, None).rebuild_ready());
    }

    #[test]
    fn bounded_source_pages_have_an_explicit_upper_fence() {
        let source = include_str!("projection.rs");
        let page_between = source
            .split("pub async fn page_between")
            .nth(1)
            .and_then(|tail| tail.split("fn row_to_event").next())
            .expect("page_between source");
        assert!(page_between.contains("global_sequence <= $2"));
        assert!(page_between.contains("ORDER BY global_sequence ASC"));
    }

    #[test]
    fn durable_security_source_never_uses_delivery_availability_as_a_cursor_fence() {
        let source = include_str!("projection.rs");
        let durable_watermark = source
            .split("pub async fn durable_high_watermark")
            .nth(1)
            .and_then(|tail| tail.split("pub async fn page_between").next())
            .expect("durable watermark source");
        let durable_page = source
            .split("pub async fn page_durable_between")
            .nth(1)
            .and_then(|tail| tail.split("/// Stable identity").next())
            .expect("durable page source");
        for durable in [durable_watermark, durable_page] {
            assert!(!durable.contains("available_at"));
            assert!(durable.contains("event_type = ANY"));
        }
        assert!(durable_page.contains("ORDER BY global_sequence ASC"));
    }

    #[test]
    fn multi_source_activation_releases_the_rebuild_fence() {
        let source = include_str!("projection.rs");
        let activation = source
            .split("pub async fn activate_multi_source_rebuild")
            .nth(1)
            .and_then(|tail| tail.split("async fn require_building_handle").next())
            .expect("multi-source activation source");
        assert!(activation.contains("source >= target"));
        assert!(activation.contains("dead_lettered_at IS NOT NULL"));
        assert!(activation.contains("SET rebuild_target_sequence = NULL"));
    }

    #[test]
    fn source_cursor_advancement_is_compare_and_swap_and_fence_bounded() {
        let source = include_str!("projection.rs");
        let advancement = source
            .split("pub async fn advance_source_checkpoint")
            .nth(1)
            .and_then(|tail| tail.split("pub async fn begin_rebuild").next())
            .expect("source checkpoint advancement source");
        assert!(advancement.contains("source_sequence = $4"));
        assert!(advancement.contains("$5 <= rebuild_target_sequence"));
        assert!(advancement.contains("FOR SHARE"));
    }
}
