//! Secret-free, cluster-scoped physical PITR coordination.

use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_CLUSTER_PITR_PAGE_SIZE: u32 = 500;
const MAX_REFERENCE_BYTES: usize = 200;

#[derive(Debug, Error)]
pub enum ClusterPitrError {
    #[error("cluster PITR request is invalid")]
    Invalid,
    #[error("cluster PITR state conflicts with the request")]
    Conflict,
    #[error("cluster PITR persistence is unavailable")]
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterPitrChain {
    pub id: Uuid,
    pub cluster_ref: String,
    pub chain_ref: String,
    pub state: String,
    pub retention_until: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRecoveryPoint {
    pub id: Uuid,
    pub chain_id: Uuid,
    pub label: String,
    pub target_time: Option<DateTime<Utc>>,
    pub target_lsn: Option<String>,
    pub timeline_id: u64,
    pub wal_continuity_verified: bool,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRestoreRehearsal {
    pub id: Uuid,
    pub recovery_point_id: Uuid,
    pub restore_target_ref: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRestoreRehearsalResult {
    pub id: Uuid,
    pub state: String,
    pub verification_checks: Vec<String>,
    pub failure_category: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ClusterPitrRepository {
    pool: PgPool,
}

impl ClusterPitrRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn schedule_base_backup(
        &self,
        cluster_ref: &str,
        storage_namespace: &str,
        chain_ref: &str,
        retention_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<ClusterPitrChain, ClusterPitrError> {
        validate_ref(cluster_ref)?;
        validate_ref(chain_ref)?;
        validate_namespace(storage_namespace)?;
        if retention_until <= at + Duration::hours(1) {
            return Err(ClusterPitrError::Invalid);
        }
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;
        let cluster = sqlx::query(
            "INSERT INTO erpx_control.platform_pitr_clusters
                (cluster_ref, storage_namespace, state, created_at, updated_at)
             VALUES ($1,$2,'configured',$3,$3)
             ON CONFLICT (cluster_ref) DO UPDATE SET updated_at=EXCLUDED.updated_at
             WHERE erpx_control.platform_pitr_clusters.storage_namespace=EXCLUDED.storage_namespace",
        )
        .bind(cluster_ref)
        .bind(storage_namespace)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if cluster.rows_affected() != 1 {
            return Err(ClusterPitrError::Conflict);
        }
        let id = Uuid::new_v4();
        let chain = sqlx::query(
            "INSERT INTO erpx_control.platform_pitr_chains
                (id, cluster_ref, chain_ref, state, retention_until, created_at, updated_at)
             VALUES ($1,$2,$3,'scheduled',$4,$5,$5)
             ON CONFLICT (cluster_ref, chain_ref) DO NOTHING",
        )
        .bind(id)
        .bind(cluster_ref)
        .bind(chain_ref)
        .bind(retention_until)
        .bind(at)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;
        if chain.rows_affected() != 1 {
            return Err(ClusterPitrError::Conflict);
        }
        tx.commit().await.map_err(map_sqlx)?;
        Ok(ClusterPitrChain {
            id,
            cluster_ref: cluster_ref.into(),
            chain_ref: chain_ref.into(),
            state: "scheduled".into(),
            retention_until,
            created_at: at,
        })
    }

    pub async fn list_recovery_points(
        &self,
        cluster_ref: &str,
        page_size: u32,
    ) -> Result<Vec<ClusterRecoveryPoint>, ClusterPitrError> {
        validate_ref(cluster_ref)?;
        if page_size == 0 || page_size > MAX_CLUSTER_PITR_PAGE_SIZE {
            return Err(ClusterPitrError::Invalid);
        }
        sqlx::query(
            "SELECT point.id, point.chain_id, point.label, point.target_time,
                    point.target_lsn, point.timeline_id,
                    point.wal_continuity_verified, point.state, point.created_at
             FROM erpx_control.platform_pitr_recovery_points point
             JOIN erpx_control.platform_pitr_chains chain ON chain.id=point.chain_id
             WHERE chain.cluster_ref=$1
             ORDER BY point.created_at DESC, point.id DESC LIMIT $2",
        )
        .bind(cluster_ref)
        .bind(i64::from(page_size))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?
        .into_iter()
        .map(recovery_point_from_row)
        .collect()
    }

    pub async fn schedule_restore_rehearsal(
        &self,
        recovery_point_id: Uuid,
        restore_target_ref: &str,
        requested_by: &str,
        correlation_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<ClusterRestoreRehearsal, ClusterPitrError> {
        validate_ref(restore_target_ref)?;
        validate_ref(requested_by)?;
        if recovery_point_id.is_nil() || correlation_id.is_nil() {
            return Err(ClusterPitrError::Invalid);
        }
        let id = Uuid::new_v4();
        let result = sqlx::query(
            "INSERT INTO erpx_control.platform_pitr_restore_rehearsals
                (id, recovery_point_id, restore_target_ref, state, requested_by,
                 correlation_id, created_at, updated_at)
             SELECT $1,$2,$3,'pending',$4,$5,$6,$6
             WHERE EXISTS(
                 SELECT 1 FROM erpx_control.platform_pitr_recovery_points
                 WHERE id=$2 AND state='available' AND wal_continuity_verified
             )",
        )
        .bind(id)
        .bind(recovery_point_id)
        .bind(restore_target_ref)
        .bind(requested_by)
        .bind(correlation_id)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if result.rows_affected() != 1 {
            return Err(ClusterPitrError::Conflict);
        }
        Ok(ClusterRestoreRehearsal {
            id,
            recovery_point_id,
            restore_target_ref: restore_target_ref.into(),
            state: "pending".into(),
            created_at: at,
        })
    }

    pub async fn record_wal_segment(
        &self,
        chain_id: Uuid,
        segment_name: &str,
        object_key: &str,
        size_bytes: u64,
        sha256: &str,
        at: DateTime<Utc>,
    ) -> Result<(), ClusterPitrError> {
        if chain_id.is_nil()
            || !valid_wal_segment(segment_name)
            || !object_key.starts_with("platform/")
            || !object_key.contains("/chains/")
            || !object_key.contains("/wal/")
            || size_bytes == 0
            || !valid_sha256(sha256)
        {
            return Err(ClusterPitrError::Invalid);
        }
        let result = sqlx::query(
            "INSERT INTO erpx_control.platform_pitr_wal_segments
                (chain_id, segment_name, object_key, size_bytes, sha256, archived_at, verified_at)
             VALUES ($1,$2,$3,$4,$5,$6,$6)
             ON CONFLICT (chain_id, segment_name) DO UPDATE SET verified_at=EXCLUDED.verified_at
             WHERE erpx_control.platform_pitr_wal_segments.object_key=EXCLUDED.object_key
               AND erpx_control.platform_pitr_wal_segments.size_bytes=EXCLUDED.size_bytes
               AND erpx_control.platform_pitr_wal_segments.sha256=EXCLUDED.sha256",
        )
        .bind(chain_id)
        .bind(segment_name)
        .bind(object_key)
        .bind(i64::try_from(size_bytes).map_err(|_| ClusterPitrError::Invalid)?)
        .bind(sha256)
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        if result.rows_affected() != 1 {
            return Err(ClusterPitrError::Conflict);
        }
        Ok(())
    }

    pub async fn read_restore_rehearsal(
        &self,
        rehearsal_id: Uuid,
    ) -> Result<ClusterRestoreRehearsalResult, ClusterPitrError> {
        if rehearsal_id.is_nil() {
            return Err(ClusterPitrError::Invalid);
        }
        let row = sqlx::query(
            "SELECT id, state, verification_summary, failure_category, updated_at
             FROM erpx_control.platform_pitr_restore_rehearsals WHERE id=$1",
        )
        .bind(rehearsal_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?
        .ok_or(ClusterPitrError::Conflict)?;
        let summary: serde_json::Value = row.try_get("verification_summary").map_err(map_sqlx)?;
        let verification_checks = summary
            .as_object()
            .into_iter()
            .flat_map(|object| object.iter())
            .filter_map(|(key, value)| {
                value
                    .as_bool()
                    .filter(|passed| *passed)
                    .map(|_| key.clone())
            })
            .take(64)
            .collect();
        Ok(ClusterRestoreRehearsalResult {
            id: row.try_get("id").map_err(map_sqlx)?,
            state: row.try_get("state").map_err(map_sqlx)?,
            verification_checks,
            failure_category: row.try_get("failure_category").map_err(map_sqlx)?,
            updated_at: row.try_get("updated_at").map_err(map_sqlx)?,
        })
    }
}

fn recovery_point_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<ClusterRecoveryPoint, ClusterPitrError> {
    Ok(ClusterRecoveryPoint {
        id: row.try_get("id").map_err(map_sqlx)?,
        chain_id: row.try_get("chain_id").map_err(map_sqlx)?,
        label: row.try_get("label").map_err(map_sqlx)?,
        target_time: row.try_get("target_time").map_err(map_sqlx)?,
        target_lsn: row.try_get("target_lsn").map_err(map_sqlx)?,
        timeline_id: u64::try_from(row.try_get::<i64, _>("timeline_id").map_err(map_sqlx)?)
            .map_err(|_| ClusterPitrError::Unavailable)?,
        wal_continuity_verified: row.try_get("wal_continuity_verified").map_err(map_sqlx)?,
        state: row.try_get("state").map_err(map_sqlx)?,
        created_at: row.try_get("created_at").map_err(map_sqlx)?,
    })
}

fn validate_ref(value: &str) -> Result<(), ClusterPitrError> {
    if value.trim() != value
        || value.is_empty()
        || value.len() > MAX_REFERENCE_BYTES
        || value.chars().any(char::is_control)
        || value
            .chars()
            .any(|value| matches!(value, '/' | '\\' | ':' | ' '))
    {
        return Err(ClusterPitrError::Invalid);
    }
    Ok(())
}

fn validate_namespace(value: &str) -> Result<(), ClusterPitrError> {
    if !(3..=63).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || value.starts_with('-')
        || value.ends_with('-')
    {
        return Err(ClusterPitrError::Invalid);
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_wal_segment(value: &str) -> bool {
    let base = value.split('.').next().unwrap_or_default();
    base.len() == 24 && base.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn map_sqlx(_error: sqlx::Error) -> ClusterPitrError {
    ClusterPitrError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callers_cannot_supply_paths_urls_or_ports() {
        for invalid in [
            "/tmp/restore",
            "postgres://local/db",
            "127.0.0.1:55440",
            "a b",
        ] {
            assert!(validate_ref(invalid).is_err());
        }
        assert!(validate_ref("restore-slot-01").is_ok());
    }

    #[test]
    fn wal_identity_is_bounded() {
        assert!(valid_wal_segment("000000010000000000000001"));
        assert!(!valid_wal_segment("../000000010000000000000001"));
    }
}
