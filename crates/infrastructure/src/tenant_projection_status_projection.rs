//! Rebuildable control-read projection writer for tenant scheduler status.
//!
//! Scheduler coordination is authoritative on the control-write database.
//! This adapter copies only the payload-free status DTO to the physically
//! separate control-read database. Queries are served by the bounded
//! `TenantProjectionStatusReadRepository` in the scheduler module.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::tenant_projection_scheduler::redact_error;
use crate::{
    ControlPlanePools, MAX_PROJECTION_STATUS_PAGE_SIZE, PersistenceError,
    ProjectionSchedulerAuthorization, ProjectionStatusChangeCursor, SqlxControlPlane,
    SqlxTenantStoreRegistry, TenantProjectionSchedulerRepository, TenantProjectionStatus,
};

pub const TENANT_PROJECTION_STATUS_READ_PROJECTION: &str = "tenant_projection_status";
pub const TENANT_PROJECTION_STATUS_CHECKPOINT: &str = "tenant_projection_status";

#[derive(Debug, Clone)]
pub struct SqlxTenantProjectionStatusProjectionStore {
    source_pool: PgPool,
    read_pool: PgPool,
    control: Arc<SqlxControlPlane>,
}

impl SqlxTenantProjectionStatusProjectionStore {
    pub fn new(control: Arc<SqlxControlPlane>) -> Self {
        Self::from_pools(control.clone(), control.pools().clone())
    }

    pub fn from_pools(control: Arc<SqlxControlPlane>, pools: ControlPlanePools) -> Self {
        Self {
            source_pool: pools.write,
            read_pool: pools.read,
            control,
        }
    }

    pub async fn clear_read_model(&self) -> Result<(), PersistenceError> {
        let mut transaction = self.read_pool.begin().await?;
        sqlx::query("DELETE FROM erpx_control.tenant_projection_status")
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM erpx_control.tenant_projection_status_checkpoints
             WHERE projection_name = $1",
        )
        .bind(TENANT_PROJECTION_STATUS_CHECKPOINT)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn upsert_status(
        &self,
        status: &TenantProjectionStatus,
    ) -> Result<(), PersistenceError> {
        validate_status(status)?;
        execute_status_upsert(&self.read_pool, status).await?;
        Ok(())
    }

    /// Publish only scheduler rows newer than the durable source checkpoint.
    /// Each source page and its checkpoint advance commit atomically on the
    /// control-read database. A crash before commit therefore replays a safe
    /// idempotent page; a crash after commit cannot regress the checkpoint.
    pub async fn publish_read_model(&self) -> Result<u64, PersistenceError> {
        let registry = Arc::new(SqlxTenantStoreRegistry::new(self.control.clone()));
        let authorization = projection_status_authorization()?;
        let scheduler = TenantProjectionSchedulerRepository::new(
            self.source_pool.clone(),
            registry,
            authorization,
        );
        let mut after = self.read_checkpoint().await?;
        let mut written = 0_u64;
        loop {
            let page = scheduler
                .list_status_changes_page(after.as_ref(), MAX_PROJECTION_STATUS_PAGE_SIZE)
                .await
                .map_err(|_| {
                    PersistenceError::InvalidData("projection status source read failed".into())
                })?;
            for status in &page.statuses {
                validate_status(status)?;
            }
            if page.statuses.is_empty() {
                break;
            }

            let mut transaction = self.read_pool.begin().await?;
            for status in &page.statuses {
                execute_status_upsert(&mut *transaction, status).await?;
            }
            let checkpoint = page.next_cursor.clone().or_else(|| {
                page.statuses
                    .last()
                    .map(|status| ProjectionStatusChangeCursor {
                        updated_at: status.updated_at,
                        job_id: status.job_id,
                    })
            });
            if let Some(checkpoint) = checkpoint.as_ref() {
                write_checkpoint(&mut transaction, checkpoint).await?;
            }
            transaction.commit().await?;
            written = written.saturating_add(page.statuses.len() as u64);
            after = checkpoint;
            if !page.has_more {
                break;
            }
            if after.is_none() {
                return Err(PersistenceError::InvalidData(
                    "projection status source returned an incomplete checkpoint".into(),
                ));
            }
        }
        if self.read_model_requires_rebuild().await? {
            let rebuilt = self.rebuild_read_model().await?;
            return Ok(written.saturating_add(rebuilt));
        }
        Ok(written)
    }

    /// The source checkpoint only fences source changes. It cannot detect a
    /// read row deleted after the checkpoint advanced, so compare the two
    /// control-plane cardinalities before acknowledging publication. A
    /// mismatch takes the existing clear-and-copy rebuild path, which also
    /// removes orphaned read rows instead of silently leaving drift behind.
    async fn read_model_requires_rebuild(&self) -> Result<bool, PersistenceError> {
        let source_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM erpx_control.tenant_projection_jobs")
                .fetch_one(&self.source_pool)
                .await?;
        let read_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM erpx_control.tenant_projection_status")
                .fetch_one(&self.read_pool)
                .await?;
        if read_model_cardinality_requires_rebuild(source_count, read_count)? {
            return Ok(true);
        }

        let mut source_after = None;
        let mut read_after = None;
        loop {
            let source_ids =
                projection_status_job_id_page(&self.source_pool, source_after, true).await?;
            let read_ids =
                projection_status_job_id_page(&self.read_pool, read_after, false).await?;
            if read_model_identity_requires_rebuild(&source_ids, &read_ids) {
                return Ok(true);
            }
            let Some(source_last) = source_ids.last().copied() else {
                return Ok(false);
            };
            let Some(read_last) = read_ids.last().copied() else {
                return Ok(true);
            };
            source_after = Some(source_last);
            read_after = Some(read_last);
        }
    }

    /// Rebuild from the authoritative control-write scheduler. Replacement
    /// database switching remains an explicit composition-root operation.
    pub async fn rebuild_read_model(&self) -> Result<u64, PersistenceError> {
        let registry = Arc::new(SqlxTenantStoreRegistry::new(self.control.clone()));
        let authorization = projection_status_authorization()?;
        let scheduler = TenantProjectionSchedulerRepository::new(
            self.source_pool.clone(),
            registry,
            authorization,
        );
        let mut transaction = self.read_pool.begin().await?;
        sqlx::query("DELETE FROM erpx_control.tenant_projection_status")
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM erpx_control.tenant_projection_status_checkpoints
             WHERE projection_name = $1",
        )
        .bind(TENANT_PROJECTION_STATUS_CHECKPOINT)
        .execute(&mut *transaction)
        .await?;

        // Keep deletion, replacement rows, and the source checkpoint in one
        // read-side transaction. A source read or validation failure must
        // roll back the replacement and leave the last known-good projection
        // available to Operations.
        let written = Self::copy_source_statuses(&scheduler, &mut transaction).await?;
        transaction.commit().await?;
        Ok(written)
    }

    async fn read_checkpoint(
        &self,
    ) -> Result<Option<ProjectionStatusChangeCursor>, PersistenceError> {
        let row = sqlx::query(
            "SELECT source_updated_at, source_job_id
             FROM erpx_control.tenant_projection_status_checkpoints
             WHERE projection_name = $1",
        )
        .bind(TENANT_PROJECTION_STATUS_CHECKPOINT)
        .fetch_optional(&self.read_pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let updated_at: Option<DateTime<Utc>> = row.try_get("source_updated_at")?;
        let job_id: Option<Uuid> = row.try_get("source_job_id")?;
        match (updated_at, job_id) {
            (None, None) => Ok(None),
            (Some(updated_at), Some(job_id)) => {
                ProjectionStatusChangeCursor::new(updated_at, job_id)
                    .map(Some)
                    .map_err(|_| {
                        PersistenceError::InvalidData(
                            "projection status checkpoint is invalid".into(),
                        )
                    })
            }
            _ => Err(PersistenceError::InvalidData(
                "projection status checkpoint is incomplete".into(),
            )),
        }
    }

    /// Full rebuild is intentionally separate from steady-state publication.
    /// It copies all source rows and then advances the durable source cursor to
    /// the greatest source tuple observed during the rebuild.
    async fn copy_source_statuses(
        scheduler: &TenantProjectionSchedulerRepository,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<u64, PersistenceError> {
        let mut after = None;
        let mut latest_change: Option<ProjectionStatusChangeCursor> = None;
        let mut written = 0_u64;
        loop {
            let page = scheduler
                .list_status_page(
                    None,
                    None,
                    None,
                    after.as_ref(),
                    MAX_PROJECTION_STATUS_PAGE_SIZE,
                )
                .await
                .map_err(|_| {
                    PersistenceError::InvalidData("projection status source read failed".into())
                })?;
            for status in &page.statuses {
                validate_status(status)?;
                execute_status_upsert(&mut **transaction, status).await?;
                let cursor = ProjectionStatusChangeCursor::new(status.updated_at, status.job_id)
                    .map_err(|_| {
                        PersistenceError::InvalidData(
                            "projection status source cursor is invalid".into(),
                        )
                    })?;
                if latest_change.as_ref().is_none_or(|current| {
                    (cursor.updated_at, cursor.job_id) > (current.updated_at, current.job_id)
                }) {
                    latest_change = Some(cursor);
                }
                written = written.saturating_add(1);
            }
            if !page.has_more {
                break;
            }
            after = page.next_cursor;
            if after.is_none() {
                return Err(PersistenceError::InvalidData(
                    "projection status source returned an incomplete cursor".into(),
                ));
            }
        }
        if let Some(checkpoint) = latest_change.as_ref() {
            write_checkpoint(transaction, checkpoint).await?;
        }
        Ok(written)
    }
}

fn projection_status_authorization() -> Result<ProjectionSchedulerAuthorization, PersistenceError> {
    ProjectionSchedulerAuthorization::new(
        "system/projection-status-publisher",
        TENANT_PROJECTION_STATUS_READ_PROJECTION,
        Uuid::new_v4(),
    )
    .map_err(|_| {
        PersistenceError::InvalidData(
            "projection status publication authorization is invalid".into(),
        )
    })
}

async fn execute_status_upsert<'a, E>(
    executor: E,
    status: &TenantProjectionStatus,
) -> Result<(), PersistenceError>
where
    E: Executor<'a, Database = Postgres>,
{
    sqlx::query(
        "INSERT INTO erpx_control.tenant_projection_status
            (tenant_id, holding_ref, projection_name, job_key, job_id,
             kind, state, available_at, attempts, max_attempts, lease_until,
             last_error, dead_lettered_at, rebuild_id, version, created_at,
             updated_at, published_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
         ON CONFLICT (job_id) DO UPDATE SET
            tenant_id = EXCLUDED.tenant_id,
            holding_ref = EXCLUDED.holding_ref,
            projection_name = EXCLUDED.projection_name,
            job_key = EXCLUDED.job_key,
            kind = EXCLUDED.kind,
            state = EXCLUDED.state,
            available_at = EXCLUDED.available_at,
            attempts = EXCLUDED.attempts,
            max_attempts = EXCLUDED.max_attempts,
            lease_until = EXCLUDED.lease_until,
            last_error = EXCLUDED.last_error,
            dead_lettered_at = EXCLUDED.dead_lettered_at,
            rebuild_id = EXCLUDED.rebuild_id,
            version = EXCLUDED.version,
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at,
            published_at = EXCLUDED.published_at
         WHERE erpx_control.tenant_projection_status.version
                   <= EXCLUDED.version",
    )
    .bind(&status.tenant_id)
    .bind(&status.holding_ref)
    .bind(&status.projection_name)
    .bind(&status.job_key)
    .bind(status.job_id)
    .bind(status.kind.as_str())
    .bind(status.state.as_str())
    .bind(status.available_at)
    .bind(i32::from(status.attempts))
    .bind(i32::from(status.max_attempts))
    .bind(status.lease_until)
    .bind(status.last_error.as_deref().map(redact_error))
    .bind(status.dead_lettered_at)
    .bind(status.rebuild_id)
    .bind(i64::try_from(status.version).map_err(|_| {
        PersistenceError::InvalidData("projection status version exceeds database range".into())
    })?)
    .bind(status.created_at)
    .bind(status.updated_at)
    .bind(Utc::now())
    .execute(executor)
    .await?;
    Ok(())
}

async fn write_checkpoint(
    transaction: &mut Transaction<'_, Postgres>,
    cursor: &ProjectionStatusChangeCursor,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_control.tenant_projection_status_checkpoints
            (projection_name, source_updated_at, source_job_id, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (projection_name) DO UPDATE SET
            source_updated_at = EXCLUDED.source_updated_at,
            source_job_id = EXCLUDED.source_job_id,
            updated_at = now()
         WHERE erpx_control.tenant_projection_status_checkpoints.source_updated_at IS NULL
            OR (erpx_control.tenant_projection_status_checkpoints.source_updated_at,
                erpx_control.tenant_projection_status_checkpoints.source_job_id)
               < (EXCLUDED.source_updated_at, EXCLUDED.source_job_id)",
    )
    .bind(TENANT_PROJECTION_STATUS_CHECKPOINT)
    .bind(cursor.updated_at)
    .bind(cursor.job_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn read_model_cardinality_requires_rebuild(
    source_count: i64,
    read_count: i64,
) -> Result<bool, PersistenceError> {
    if source_count < 0 || read_count < 0 {
        return Err(PersistenceError::InvalidData(
            "projection status row count is invalid".into(),
        ));
    }
    Ok(source_count != read_count)
}

async fn projection_status_job_id_page(
    pool: &PgPool,
    after: Option<Uuid>,
    source: bool,
) -> Result<Vec<Uuid>, PersistenceError> {
    let limit = i64::from(MAX_PROJECTION_STATUS_PAGE_SIZE);
    let rows = if source {
        if let Some(after) = after {
            sqlx::query(
                "SELECT id AS job_id
                   FROM erpx_control.tenant_projection_jobs
                  WHERE id > $1
                  ORDER BY id
                  LIMIT $2",
            )
            .bind(after)
            .bind(limit)
            .fetch_all(pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id AS job_id
                   FROM erpx_control.tenant_projection_jobs
                  ORDER BY id
                  LIMIT $1",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    } else if let Some(after) = after {
        sqlx::query(
            "SELECT job_id
               FROM erpx_control.tenant_projection_status
              WHERE job_id > $1
              ORDER BY job_id
              LIMIT $2",
        )
        .bind(after)
        .bind(limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT job_id
               FROM erpx_control.tenant_projection_status
              ORDER BY job_id
              LIMIT $1",
        )
        .bind(limit)
        .fetch_all(pool)
        .await?
    };
    rows.into_iter()
        .map(|row| row.try_get("job_id").map_err(PersistenceError::from))
        .collect()
}

fn read_model_identity_requires_rebuild(source_ids: &[Uuid], read_ids: &[Uuid]) -> bool {
    source_ids != read_ids
}

fn validate_status(status: &TenantProjectionStatus) -> Result<(), PersistenceError> {
    if status.version == 0
        || status.max_attempts == 0
        || status.max_attempts > 25
        || status.attempts > status.max_attempts
    {
        return Err(PersistenceError::InvalidData(
            "projection status counters are invalid".into(),
        ));
    }
    if status.job_id.is_nil()
        || status.tenant_id.trim().is_empty()
        || status.holding_ref.trim().is_empty()
        || status.projection_name.trim().is_empty()
        || status.job_key.trim().is_empty()
    {
        return Err(PersistenceError::InvalidData(
            "projection status identity is invalid".into(),
        ));
    }
    if status.tenant_id.len() > 200
        || status.holding_ref.len() > 200
        || status.job_key.len() > 200
        || status
            .tenant_id
            .chars()
            .chain(status.holding_ref.chars())
            .chain(status.job_key.chars())
            .any(char::is_control)
        || status.projection_name.len() > 120
        || status.projection_name.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
        })
    {
        return Err(PersistenceError::InvalidData(
            "projection status references are invalid".into(),
        ));
    }
    if matches!(status.kind, crate::ProjectionJobKind::Incremental)
        && matches!(
            status.state,
            crate::ProjectionJobState::RebuildPending | crate::ProjectionJobState::RebuildSucceeded
        )
    {
        return Err(PersistenceError::InvalidData(
            "projection status kind and state are inconsistent".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectionJobKind, ProjectionJobState};
    use chrono::Utc;

    #[test]
    fn status_validation_rejects_invalid_counters_without_database_access() {
        let status = TenantProjectionStatus {
            tenant_id: "tenant".into(),
            holding_ref: "holding".into(),
            projection_name: "operations".into(),
            job_key: "job".into(),
            job_id: Uuid::new_v4(),
            kind: ProjectionJobKind::Incremental,
            state: ProjectionJobState::Pending,
            available_at: Utc::now(),
            attempts: 2,
            max_attempts: 1,
            lease_until: None,
            last_error: None,
            dead_lettered_at: None,
            rebuild_id: None,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            published_at: Utc::now(),
        };
        assert!(validate_status(&status).is_err());
    }

    #[test]
    fn read_model_cardinality_fence_detects_deleted_or_orphaned_rows() {
        assert!(read_model_cardinality_requires_rebuild(7, 6).expect("deleted row drift"));
        assert!(read_model_cardinality_requires_rebuild(6, 7).expect("orphan row drift"));
        assert!(!read_model_cardinality_requires_rebuild(7, 7).expect("matching cardinality"));
        assert!(read_model_cardinality_requires_rebuild(-1, 7).is_err());
    }

    #[test]
    fn read_model_identity_fence_detects_equal_count_replacement() {
        let source = [Uuid::from_u128(1), Uuid::from_u128(2)];
        let matching = [Uuid::from_u128(1), Uuid::from_u128(2)];
        let replaced = [Uuid::from_u128(1), Uuid::from_u128(3)];
        assert!(!read_model_identity_requires_rebuild(&source, &matching));
        assert!(read_model_identity_requires_rebuild(&source, &replaced));
    }
}
