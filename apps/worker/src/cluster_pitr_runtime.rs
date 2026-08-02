//! Durable cluster-scoped PITR inventory worker.
//!
//! PostgreSQL's archive helper uploads and reads back each immutable WAL
//! object before writing its private local spool copy. This worker turns those
//! verified copies into bounded, durable cluster-chain inventory records.

use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const CLUSTER_PITR_CONTROLLER_HANDLER_NAME: &str = "cluster_pitr_controller";
const MAX_WAL_SEGMENTS_PER_PASS: usize = 128;

#[derive(Debug, Clone)]
pub struct ClusterPitrControllerRuntimeHandler {
    pool: PgPool,
    wal_spool: PathBuf,
    cluster_ref: String,
    chain_ref: String,
}

impl ClusterPitrControllerRuntimeHandler {
    pub fn from_env(pool: PgPool) -> Result<Self, WorkerRuntimeError> {
        let wal_spool = required_env("ERPX_PITR_WAL_SPOOL")?;
        let wal_spool = PathBuf::from(wal_spool);
        validate_spool_root(&wal_spool)?;
        let cluster_ref = required_reference("ERPX_PITR_CLUSTER_REF")?;
        let chain_ref = required_reference("ERPX_PITR_CHAIN_REF")?;
        Ok(Self {
            pool,
            wal_spool,
            cluster_ref,
            chain_ref,
        })
    }

    async fn inventory_verified_wal(&self) -> Result<usize, WorkerRuntimeError> {
        validate_spool_root(&self.wal_spool)?;
        let chain_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM erpx_control.platform_pitr_chains
             WHERE cluster_ref=$1 AND chain_ref=$2
               AND state IN ('continuous','degraded')",
        )
        .bind(&self.cluster_ref)
        .bind(&self.chain_ref)
        .fetch_optional(&self.pool)
        .await?;
        let Some(chain_id) = chain_id else {
            return Ok(0);
        };
        let existing: BTreeSet<String> = sqlx::query_scalar(
            "SELECT segment_name FROM erpx_control.platform_pitr_wal_segments
             WHERE chain_id=$1",
        )
        .bind(chain_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect();
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&self.wal_spool).map_err(configuration_error)? {
            let entry = entry.map_err(configuration_error)?;
            let segment_name = entry.file_name().to_string_lossy().into_owned();
            if !valid_wal_segment(&segment_name) || existing.contains(&segment_name) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(configuration_error)?;
            if !metadata.file_type().is_file()
                || metadata.file_type().is_symlink()
                || metadata.permissions().mode() & 0o777 != 0o600
                || metadata.len() == 0
            {
                continue;
            }
            candidates.push((segment_name, entry.path(), metadata.len()));
        }
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        candidates.truncate(MAX_WAL_SEGMENTS_PER_PASS);
        let mut recorded = 0_usize;
        for (segment_name, path, size_bytes) in candidates {
            let body = fs::read(path).map_err(configuration_error)?;
            if u64::try_from(body.len()).ok() != Some(size_bytes) {
                return Err(WorkerRuntimeError::Configuration(
                    "verified WAL spool segment changed while being inventoried".into(),
                ));
            }
            let checksum = format!("{:x}", Sha256::digest(&body));
            let object_key = format!(
                "platform/{}/chains/{}/wal/{segment_name}",
                self.cluster_ref, self.chain_ref
            );
            let result = sqlx::query(
                "INSERT INTO erpx_control.platform_pitr_wal_segments
                    (chain_id, segment_name, object_key, size_bytes, sha256,
                     archived_at, verified_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$6)
                 ON CONFLICT (chain_id, segment_name) DO UPDATE
                 SET verified_at=EXCLUDED.verified_at
                 WHERE erpx_control.platform_pitr_wal_segments.object_key=EXCLUDED.object_key
                   AND erpx_control.platform_pitr_wal_segments.size_bytes=EXCLUDED.size_bytes
                   AND erpx_control.platform_pitr_wal_segments.sha256=EXCLUDED.sha256",
            )
            .bind(chain_id)
            .bind(segment_name)
            .bind(object_key)
            .bind(i64::try_from(size_bytes).map_err(|_| {
                WorkerRuntimeError::Configuration("WAL segment size is invalid".into())
            })?)
            .bind(checksum)
            .bind(Utc::now())
            .execute(&self.pool)
            .await?;
            if result.rows_affected() != 1 {
                return Err(WorkerRuntimeError::Configuration(
                    "immutable WAL inventory conflicts with the verified spool".into(),
                ));
            }
            recorded = recorded.saturating_add(1);
        }
        let now = Utc::now();
        sqlx::query(
            "UPDATE erpx_control.platform_pitr_chains SET updated_at=$3
             WHERE cluster_ref=$1 AND chain_ref=$2
               AND state IN ('continuous','degraded')",
        )
        .bind(&self.cluster_ref)
        .bind(&self.chain_ref)
        .bind(now)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "UPDATE erpx_control.platform_pitr_clusters
             SET state='active', updated_at=$2 WHERE cluster_ref=$1",
        )
        .bind(&self.cluster_ref)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(recorded)
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for ClusterPitrControllerRuntimeHandler {
    fn name(&self) -> &str {
        CLUSTER_PITR_CONTROLLER_HANDLER_NAME
    }

    async fn run_once(&self, _worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let recorded = self.inventory_verified_wal().await?;
        let recorded = u64::try_from(recorded).map_err(|_| {
            WorkerRuntimeError::Configuration("WAL inventory count is invalid".into())
        })?;
        Ok(WorkerHandlerReport {
            claimed: recorded,
            processed: recorded,
            published: recorded,
            retried: 0,
            dead_lettered: 0,
            has_work: recorded != 0,
        })
    }
}

fn required_env(name: &str) -> Result<String, WorkerRuntimeError> {
    std::env::var(name)
        .map_err(|_| WorkerRuntimeError::Configuration(format!("{name} is required")))
}

fn required_reference(name: &str) -> Result<String, WorkerRuntimeError> {
    let value = required_env(name)?;
    if value.is_empty()
        || value.len() > 120
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WorkerRuntimeError::Configuration(format!(
            "{name} is invalid"
        )));
    }
    Ok(value)
}

fn validate_spool_root(path: &Path) -> Result<(), WorkerRuntimeError> {
    if !path.is_absolute() || path.parent().is_none() || path.is_symlink() {
        return Err(WorkerRuntimeError::Configuration(
            "ERPX_PITR_WAL_SPOOL is unsafe".into(),
        ));
    }
    let metadata = fs::symlink_metadata(path).map_err(configuration_error)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(WorkerRuntimeError::Configuration(
            "ERPX_PITR_WAL_SPOOL must be a private directory".into(),
        ));
    }
    Ok(())
}

fn valid_wal_segment(value: &str) -> bool {
    value.len() == 24
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
}

fn configuration_error(error: std::io::Error) -> WorkerRuntimeError {
    tracing::error!(error_kind = ?error.kind(), "cluster PITR WAL spool access failed");
    WorkerRuntimeError::Configuration("cluster PITR WAL spool is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_inventory_accepts_only_canonical_segments() {
        assert!(valid_wal_segment("000000010000000000000001"));
        assert!(!valid_wal_segment("0000000100000000000000af"));
        assert!(!valid_wal_segment("000000010000000000000001.backup"));
        assert!(!valid_wal_segment("../0000000100000000000001"));
    }
}
