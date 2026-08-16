use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{ProjectionCheckpoint, ProjectionNotReady, ProjectionWaiter, ReadYourWrites};
use erpx_kernel::{CommandId, CorrelationId, EventId};
use sqlx::Row;
use thiserror::Error;

use crate::{CqrsSqlxError, ReadPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaiterConfig {
    pub poll_interval: Duration,
}

impl Default for WaiterConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(25),
        }
    }
}

#[derive(Debug, Error)]
pub enum WaitError {
    #[error(transparent)]
    Database(#[from] CqrsSqlxError),
    #[error("projection did not reach the requested receipt before the deadline")]
    NotReady(ProjectionNotReady),
}

#[derive(Debug, Clone)]
pub struct SqlxProjectionWaiter {
    pool: ReadPool,
    config: WaiterConfig,
}

impl SqlxProjectionWaiter {
    /// This constructor deliberately accepts only `ReadPool`; there is no
    /// conversion from `WritePool` and therefore no write-store fallback path.
    pub fn new(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        if config.poll_interval.is_zero() {
            return Err(CqrsSqlxError::InvalidInput(
                "projection poll interval must be greater than zero",
            ));
        }
        Ok(Self { pool, config })
    }

    pub async fn wait_until_checked(
        &self,
        requirement: &ReadYourWrites,
    ) -> Result<ProjectionCheckpoint, WaitError> {
        requirement
            .validate()
            .map_err(|_| CqrsSqlxError::InvalidInput("read-your-writes requirement is invalid"))?;
        let deadline = tokio::time::Instant::now() + requirement.timeout();
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::NotReady(not_ready(requirement, None)));
            }
            let remaining = deadline - now;
            let checkpoint = match tokio::time::timeout(
                remaining,
                load_active_checkpoint(&self.pool, &requirement.projection, &requirement.partition),
            )
            .await
            {
                Ok(result) => result?,
                Err(_) => return Err(WaitError::NotReady(not_ready(requirement, None))),
            };
            if let Some(value) = checkpoint.as_ref()
                && value.satisfies(requirement.target)
            {
                return Ok(value.clone());
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(WaitError::NotReady(not_ready(requirement, checkpoint)));
            }
            tokio::time::sleep(self.config.poll_interval.min(deadline - now)).await;
        }
    }
}

#[async_trait]
impl ProjectionWaiter for SqlxProjectionWaiter {
    async fn wait_until(
        &self,
        requirement: &ReadYourWrites,
    ) -> Result<ProjectionCheckpoint, ProjectionNotReady> {
        match self.wait_until_checked(requirement).await {
            Ok(checkpoint) => Ok(checkpoint),
            Err(WaitError::NotReady(not_ready)) => Err(not_ready),
            Err(WaitError::Database(_)) => Err(not_ready(requirement, None)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSideReceipt {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub event_id: EventId,
    pub global_sequence: u64,
    pub projected_at: DateTime<Utc>,
}

/// Read-only access to projection receipts and checkpoints for query adapters.
#[derive(Debug, Clone)]
pub struct ReadSideStore {
    pool: ReadPool,
}

impl ReadSideStore {
    pub const fn new(pool: ReadPool) -> Self {
        Self { pool }
    }

    pub async fn checkpoint(
        &self,
        projection: &str,
        partition: &str,
    ) -> Result<Option<ProjectionCheckpoint>, CqrsSqlxError> {
        load_active_checkpoint(&self.pool, projection, partition).await
    }

    pub async fn receipt(
        &self,
        projection: &str,
        partition: &str,
        command_id: CommandId,
    ) -> Result<Option<ReadSideReceipt>, CqrsSqlxError> {
        let statement = if self.pool.tenant_id().is_some() {
            "SELECT receipt.command_id, receipt.correlation_id, receipt.event_id,
                    receipt.global_sequence, receipt.projected_at
             FROM erpx_cqrs_read.projection_receipts receipt
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.tenant_id = receipt.tenant_id
              AND generation.projection_name = receipt.projection_name
              AND generation.generation = receipt.generation
              AND generation.status = 'active'
             WHERE receipt.projection_name = $1 AND receipt.partition_key = $2
               AND receipt.command_id = $3 AND receipt.tenant_id = $4"
        } else {
            "SELECT receipt.command_id, receipt.correlation_id, receipt.event_id,
                    receipt.global_sequence, receipt.projected_at
             FROM erpx_cqrs_read.projection_receipts receipt
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name = receipt.projection_name
              AND generation.generation = receipt.generation
              AND generation.status = 'active'
             WHERE receipt.projection_name = $1 AND receipt.partition_key = $2
               AND receipt.command_id = $3"
        };
        let mut transaction = self.pool.begin().await?;
        let query = sqlx::query(statement)
            .bind(projection)
            .bind(partition)
            .bind(command_id.as_uuid());
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
        row.map(|row| {
            let command_id = CommandId::try_from_uuid(row.try_get("command_id")?)
                .map_err(|_| CqrsSqlxError::InvalidInput("read receipt command id is nil"))?;
            let correlation_id = CorrelationId::try_from_uuid(row.try_get("correlation_id")?)
                .map_err(|_| CqrsSqlxError::InvalidInput("read receipt correlation id is nil"))?;
            let event_id = EventId::try_from_uuid(row.try_get("event_id")?)
                .map_err(|_| CqrsSqlxError::InvalidInput("read receipt event id is nil"))?;
            Ok(ReadSideReceipt {
                command_id,
                correlation_id,
                event_id,
                global_sequence: to_u64(row.try_get("global_sequence")?)?,
                projected_at: row.try_get("projected_at")?,
            })
        })
        .transpose()
    }
}

async fn load_active_checkpoint(
    pool: &ReadPool,
    projection: &str,
    partition: &str,
) -> Result<Option<ProjectionCheckpoint>, CqrsSqlxError> {
    let statement = if pool.tenant_id().is_some() {
        "SELECT checkpoint.event_id, checkpoint.global_sequence, checkpoint.updated_at
         FROM erpx_cqrs_read.projection_checkpoints checkpoint
         JOIN erpx_cqrs_read.projection_generations generation
           ON generation.tenant_id = checkpoint.tenant_id
          AND generation.projection_name = checkpoint.projection_name
          AND generation.generation = checkpoint.generation
          AND generation.status = 'active'
         WHERE checkpoint.projection_name = $1 AND checkpoint.partition_key = $2
           AND checkpoint.tenant_id = $3"
    } else {
        "SELECT checkpoint.event_id, checkpoint.global_sequence, checkpoint.updated_at
         FROM erpx_cqrs_read.projection_checkpoints checkpoint
         JOIN erpx_cqrs_read.projection_generations generation
           ON generation.projection_name = checkpoint.projection_name
          AND generation.generation = checkpoint.generation
          AND generation.status = 'active'
         WHERE checkpoint.projection_name = $1 AND checkpoint.partition_key = $2"
    };
    let mut transaction = pool.begin().await?;
    let query = sqlx::query(statement).bind(projection).bind(partition);
    let row = match pool.tenant_id() {
        Some(tenant_id) => {
            query
                .bind(tenant_id)
                .fetch_optional(&mut *transaction)
                .await?
        }
        None => query.fetch_optional(&mut *transaction).await?,
    };
    transaction.commit().await?;
    row.map(|row| {
        let event_id = EventId::try_from_uuid(row.try_get("event_id")?)
            .map_err(|_| CqrsSqlxError::InvalidInput("checkpoint event id is nil"))?;
        Ok(ProjectionCheckpoint {
            projection: projection.into(),
            partition: partition.into(),
            event_id,
            global_sequence: to_u64(row.try_get("global_sequence")?)?,
            updated_at: row.try_get("updated_at")?,
        })
    })
    .transpose()
}

fn not_ready(
    requirement: &ReadYourWrites,
    observed: Option<ProjectionCheckpoint>,
) -> ProjectionNotReady {
    ProjectionNotReady {
        projection: requirement.projection.clone(),
        partition: requirement.partition.clone(),
        target: requirement.target,
        observed: observed.map(Box::new),
    }
}

fn to_u64(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn polling_must_be_bounded_and_nonzero() {
        let pool = ReadPool::from_control_pg_pool(
            sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap(),
        );
        assert!(
            SqlxProjectionWaiter::new(
                pool,
                WaiterConfig {
                    poll_interval: Duration::ZERO,
                }
            )
            .is_err()
        );
    }
}
