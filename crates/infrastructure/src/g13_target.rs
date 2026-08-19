//! Authoritative G13 target-write execution.
//!
//! The Legacy bridge stores only bounded, PII-free source evidence. This
//! adapter consumes that evidence through an immutable source fence and writes
//! a holding-scoped import ledger plus target outbox events. It is deliberately
//! independent from command ownership: importing and rehearsing rollback do
//! not advance a migration phase or activate customer traffic.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use erpx_migration::{LegacySourceSnapshot, ReconciliationDifference, ReconciliationReport};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

const IMPORT_AGGREGATE_TYPE: &str = "g13_migration_import";
const IMPORT_EVENT_TYPE: &str = "migration.imported";
const FINAL_DELTA_EVENT_TYPE: &str = "migration.final_delta_imported";
const ROLLBACK_REPLAY_EVENT_TYPE: &str = "migration.import_rollback_replayed";
const SHADOW_REPLAY_EVENT_TYPE: &str = "migration.shadow_replay_validated";
const RECONCILIATION_EVENT_TYPE: &str = "migration.target_reconciled";
const READ_PROJECTION_EVENT_TYPE: &str = "migration.read_projection_rebuilt";
const RETENTION_EVENT_TYPE: &str = "migration.rehearsal_retention_expired";
const SHADOW_REPLAY_AGGREGATE_TYPE: &str = "g13_migration_shadow_replay";
const RECONCILIATION_AGGREGATE_TYPE: &str = "g13_migration_reconciliation";
const READ_PROJECTION_AGGREGATE_TYPE: &str = "g13_migration_read_projection";
const RETENTION_AGGREGATE_TYPE: &str = "g13_migration_retention";

#[derive(Debug, Error)]
pub enum G13TargetImportError {
    #[error("G13 target database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("G13 target import payload could not be serialized")]
    Serialization,
    #[error("G13 target import source fence conflicted with an immutable row")]
    SourceFenceConflict,
    #[error("G13 target import reference is invalid: {0}")]
    InvalidReference(&'static str),
    #[error("G13 target read projection is unavailable")]
    ReadProjectionUnavailable,
    #[error("G13 retention expiry requires a complete rollback fence")]
    RollbackFenceRequired,
    #[error("G13 final delta requires a complete source identity mapping")]
    IncompleteSourceSnapshot,
    #[error("G13 final delta requires a recently validated Legacy write fence")]
    WriteFenceRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13TargetImportResult {
    pub source_kind: String,
    pub source_count: u64,
    pub rows_seen: u64,
    pub rows_applied: u64,
    pub target_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13TargetReconciliation {
    pub report: ReconciliationReport,
    pub source_counts: BTreeMap<String, u64>,
    pub target_counts: BTreeMap<String, u64>,
    pub source_identity_digest: String,
    pub target_identity_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13TargetRollbackResult {
    pub rows_captured: u64,
    pub rows_restored: u64,
    pub outbox_events_replayed: u64,
    pub read_model_rebuilt: bool,
    pub restored_state_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13ShadowReplayResult {
    pub source_kind: String,
    pub source_count: u64,
    pub expected_rows: u64,
    pub target_rows: u64,
    pub mapping_complete: bool,
    pub missing_source_keys: Vec<String>,
    pub extra_source_keys: Vec<String>,
    pub conflicting_target_ids: Vec<String>,
}

impl G13ShadowReplayResult {
    pub fn matched(&self) -> bool {
        self.missing_source_keys.is_empty()
            && self.extra_source_keys.is_empty()
            && self.conflicting_target_ids.is_empty()
            && self.mapping_complete
            && self.expected_rows == self.target_rows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13ReadProjectionResult {
    pub rows_projected: u64,
    pub projection_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G13RetentionResult {
    pub rows_expired: u64,
    pub read_rows_expired: u64,
    pub retained_rollback_rows: bool,
}

#[derive(Debug, Clone)]
pub struct G13TargetImporter {
    target: PgPool,
    read: PgPool,
    rollback: PgPool,
    data_profile_key: String,
    read_projection_enabled: bool,
}

impl G13TargetImporter {
    pub fn new(
        target: PgPool,
        rollback: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, G13TargetImportError> {
        let data_profile_key = data_profile_key.into();
        if data_profile_key.trim().is_empty() || data_profile_key.len() > 200 {
            return Err(G13TargetImportError::InvalidReference("data_profile_key"));
        }
        Ok(Self {
            read: target.clone(),
            target,
            rollback,
            data_profile_key,
            read_projection_enabled: false,
        })
    }

    /// Construct an importer with physically separate target-write, target-read,
    /// and rollback databases. The three pools are deliberately explicit so a
    /// caller cannot mistake a write-ledger query for replacement-read evidence.
    pub fn new_with_read_pool(
        target: PgPool,
        read: PgPool,
        rollback: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, G13TargetImportError> {
        let data_profile_key = data_profile_key.into();
        if data_profile_key.trim().is_empty() || data_profile_key.len() > 200 {
            return Err(G13TargetImportError::InvalidReference("data_profile_key"));
        }
        Ok(Self {
            target,
            read,
            rollback,
            data_profile_key,
            read_projection_enabled: true,
        })
    }

    /// Import one source identity scope in a transaction. Replays are safe:
    /// an existing row must carry the same source fence and target identity,
    /// while the immutable target outbox event is deduplicated by event ID.
    pub async fn import_scope(
        &self,
        run_id: Uuid,
        snapshot: &LegacySourceSnapshot,
        source_kind: &str,
        source_fence: &str,
    ) -> Result<G13TargetImportResult, G13TargetImportError> {
        self.import_scope_with_event(
            run_id,
            snapshot,
            source_kind,
            source_fence,
            IMPORT_EVENT_TYPE,
            None,
        )
        .await
    }

    /// Apply only the missing rows for the final-delta work item. The source
    /// fence remains immutable and an existing source key with a different
    /// target identity is rejected by the same path as backfill.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_final_delta(
        &self,
        run_id: Uuid,
        snapshot: &LegacySourceSnapshot,
        source_kind: &str,
        source_fence: &str,
        legacy_write_fence_ref: &str,
        legacy_writes_blocked: bool,
        legacy_fence_validated_at: DateTime<Utc>,
    ) -> Result<G13TargetImportResult, G13TargetImportError> {
        validate_write_fence(
            legacy_write_fence_ref,
            legacy_writes_blocked,
            legacy_fence_validated_at,
        )?;
        let mapping_len = snapshot
            .id_mappings
            .get(source_kind)
            .map_or(0, BTreeMap::len);
        let source_count = snapshot.counts.get(source_kind).copied().unwrap_or(0);
        if source_count != u64::try_from(mapping_len).unwrap_or(u64::MAX) {
            return Err(G13TargetImportError::IncompleteSourceSnapshot);
        }
        self.import_scope_with_event(
            run_id,
            snapshot,
            source_kind,
            source_fence,
            FINAL_DELTA_EVENT_TYPE,
            Some((legacy_write_fence_ref, legacy_fence_validated_at)),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_scope_with_event(
        &self,
        run_id: Uuid,
        snapshot: &LegacySourceSnapshot,
        source_kind: &str,
        source_fence: &str,
        event_type: &str,
        operation_evidence: Option<(&str, DateTime<Utc>)>,
    ) -> Result<G13TargetImportResult, G13TargetImportError> {
        validate_scope(run_id, source_kind, source_fence)?;
        if snapshot.source_contract_hash != source_fence {
            return Err(G13TargetImportError::SourceFenceConflict);
        }
        let mapping = snapshot
            .id_mappings
            .get(source_kind)
            .cloned()
            .unwrap_or_default();
        let source_count = snapshot.counts.get(source_kind).copied().unwrap_or(0);
        if source_count != u64::try_from(mapping.len()).unwrap_or(u64::MAX) {
            return Err(G13TargetImportError::IncompleteSourceSnapshot);
        }
        let mut tx = self.target.begin().await.map_err(db)?;
        set_holding(&mut tx, &self.data_profile_key).await?;
        let mut rows_applied = 0_u64;
        for (source_key, target_id) in &mapping {
            let payload = json!({
                "run_id": run_id,
                "source_kind": source_kind,
                "source_key": source_key,
                "target_id": target_id,
                "source_contract_hash": source_fence,
            });
            let inserted = sqlx::query(
                "INSERT INTO erpx_holding.g13_migration_import_rows
                    (data_profile_key, run_id, source_contract_hash, source_fence,
                     source_kind, source_key, target_id, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT (data_profile_key, run_id, source_kind, source_key)
                 DO NOTHING",
            )
            .bind(&self.data_profile_key)
            .bind(run_id)
            .bind(source_fence)
            .bind(source_fence)
            .bind(source_kind)
            .bind(source_key)
            .bind(target_id)
            .bind(&payload)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
            if inserted == 0 {
                let existing = sqlx::query(
                    "SELECT source_contract_hash, source_fence, target_id, payload
                     FROM erpx_holding.g13_migration_import_rows
                     WHERE data_profile_key=$1 AND run_id=$2 AND source_kind=$3 AND source_key=$4",
                )
                .bind(&self.data_profile_key)
                .bind(run_id)
                .bind(source_kind)
                .bind(source_key)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                let existing_hash: String = existing.try_get("source_contract_hash").map_err(db)?;
                let existing_fence: String = existing.try_get("source_fence").map_err(db)?;
                let existing_target: Uuid = existing.try_get("target_id").map_err(db)?;
                let existing_payload: Value = existing.try_get("payload").map_err(db)?;
                let same_identity = existing_target == *target_id && existing_payload == payload;
                if existing_target != *target_id
                    || existing_hash != source_fence
                    || existing_fence != source_fence
                    || !same_identity
                {
                    return Err(G13TargetImportError::SourceFenceConflict);
                }
            } else {
                rows_applied = rows_applied.saturating_add(1);
            }
            let event_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("g13-{event_type}:{run_id}:{source_kind}:{source_key}").as_bytes(),
            );
            sqlx::query(
                "INSERT INTO erpx_holding.outbox_events
                    (event_id, aggregate_type, aggregate_id, data_profile_key,
                     aggregate_version, event_type, payload, occurred_at,
                     contract_version, event_version, active_scope, actor,
                     correlation_id, causation_id, idempotency_key)
                 VALUES ($1,$2,$3,$4,1,$5,$6,now(),1,1,$7,$8,$9,NULL,$10)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(event_id)
            .bind(IMPORT_AGGREGATE_TYPE)
            .bind(target_id)
            .bind(&self.data_profile_key)
            .bind(event_type)
            .bind(&payload)
            .bind(json!({ "data_profile_key": self.data_profile_key }))
            .bind(json!({ "kind": "system", "principal_ref": "system:g13-target" }))
            .bind(Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("g13-correlation:{event_id}").as_bytes(),
            ))
            .bind(format!("g13:{event_type}:{event_id}"))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            ensure_outbox_event_matches(
                &mut tx,
                event_id,
                IMPORT_AGGREGATE_TYPE,
                *target_id,
                &self.data_profile_key,
                event_type,
                &payload,
            )
            .await?;
        }
        let target_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2 AND source_kind=$3",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_kind)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if let Some((write_fence_ref, validated_at)) = operation_evidence {
            append_operation_event_in_transaction(
                &mut tx,
                &self.data_profile_key,
                FINAL_DELTA_EVENT_TYPE,
                IMPORT_AGGREGATE_TYPE,
                operation_id("final-delta", run_id, source_kind, source_fence),
                json!({
                    "run_id": run_id,
                    "source_kind": source_kind,
                    "source_fence": source_fence,
                    "source_count": source_count,
                    "target_count": target_count,
                    "legacy_write_fence_ref": write_fence_ref,
                    "legacy_fence_validated_at": validated_at,
                    "writes_blocked": true,
                }),
            )
            .await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(G13TargetImportResult {
            source_kind: source_kind.to_owned(),
            source_count,
            rows_seen: u64::try_from(mapping.len())
                .map_err(|_| G13TargetImportError::InvalidReference("mapping size"))?,
            rows_applied,
            target_count: u64::try_from(target_count)
                .map_err(|_| G13TargetImportError::InvalidReference("target count"))?,
        })
    }

    /// Compare the immutable source identity mapping with the target ledger
    /// without mutating either database. This is the shadow-replay gate: every
    /// missing, extra, or conflicting identity remains explicit evidence.
    pub async fn shadow_replay(
        &self,
        run_id: Uuid,
        snapshot: &LegacySourceSnapshot,
        source_kind: &str,
        source_fence: &str,
    ) -> Result<G13ShadowReplayResult, G13TargetImportError> {
        validate_scope(run_id, source_kind, source_fence)?;
        if snapshot.source_contract_hash != source_fence {
            return Err(G13TargetImportError::SourceFenceConflict);
        }
        let expected = snapshot
            .id_mappings
            .get(source_kind)
            .cloned()
            .unwrap_or_default();
        let rows = self.load_target_rows(run_id).await?;
        let target = rows
            .into_iter()
            .filter(|row| row.source_kind == source_kind)
            .map(|row| (row.source_key.clone(), row))
            .collect::<BTreeMap<_, _>>();
        let mut missing_source_keys = Vec::new();
        let mut conflicting_target_ids = Vec::new();
        for (source_key, target_id) in &expected {
            match target.get(source_key) {
                None => missing_source_keys.push(source_key.clone()),
                Some(row) if row.target_id != *target_id || row.source_fence != source_fence => {
                    conflicting_target_ids.push(source_key.clone())
                }
                Some(_) => {}
            }
        }
        let extra_source_keys = target
            .keys()
            .filter(|source_key| !expected.contains_key(*source_key))
            .cloned()
            .collect::<Vec<_>>();
        let source_count = snapshot.counts.get(source_kind).copied().unwrap_or(0);
        let expected_rows = u64::try_from(expected.len())
            .map_err(|_| G13TargetImportError::InvalidReference("mapping size"))?;
        let result = G13ShadowReplayResult {
            source_kind: source_kind.to_owned(),
            source_count,
            expected_rows,
            target_rows: u64::try_from(target.len())
                .map_err(|_| G13TargetImportError::InvalidReference("target count"))?,
            mapping_complete: source_count == expected_rows,
            missing_source_keys,
            extra_source_keys,
            conflicting_target_ids,
        };
        let matched = result.matched();
        append_operation_event(
            &self.target,
            &self.data_profile_key,
            SHADOW_REPLAY_EVENT_TYPE,
            SHADOW_REPLAY_AGGREGATE_TYPE,
            operation_id("shadow-replay", run_id, source_kind, source_fence),
            json!({
                "run_id": run_id,
                "source_kind": source_kind,
                "source_fence": source_fence,
                "source_count": result.source_count,
                "expected_rows": result.expected_rows,
                "target_rows": result.target_rows,
                "mapping_complete": result.mapping_complete,
                "missing_source_keys": &result.missing_source_keys,
                "extra_source_keys": &result.extra_source_keys,
                "conflicting_target_ids": &result.conflicting_target_ids,
                "matched": matched,
            }),
        )
        .await?;
        Ok(result)
    }

    /// Rebuild a physically separate read model from the target write ledger.
    /// The delete, ordered replay, and read-model checkpoint are one read-side
    /// transaction; the write ledger and its outbox remain authoritative.
    pub async fn project_read_model(
        &self,
        run_id: Uuid,
    ) -> Result<G13ReadProjectionResult, G13TargetImportError> {
        if !self.read_projection_enabled {
            return Err(G13TargetImportError::ReadProjectionUnavailable);
        }
        if run_id.is_nil() {
            return Err(G13TargetImportError::InvalidReference("run_id"));
        }
        let rows = self.load_target_rows(run_id).await?;
        let mut tx = self.read.begin().await.map_err(db)?;
        set_holding(&mut tx, &self.data_profile_key).await?;
        sqlx::query(
            "DELETE FROM erpx_holding.g13_migration_import_read_rows
             WHERE data_profile_key=$1 AND run_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for row in &rows {
            let source_key_hash = digest_bytes(row.source_key.as_bytes());
            let payload_hash = digest_bytes(
                &serde_json::to_vec(&row.payload)
                    .map_err(|_| G13TargetImportError::Serialization)?,
            );
            sqlx::query(
                "INSERT INTO erpx_holding.g13_migration_import_read_rows
                    (data_profile_key, run_id, source_kind, source_key_hash, target_id,
                     source_contract_hash, source_fence, payload_hash, projected_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())",
            )
            .bind(&self.data_profile_key)
            .bind(run_id)
            .bind(&row.source_kind)
            .bind(source_key_hash)
            .bind(row.target_id)
            .bind(&row.source_contract_hash)
            .bind(&row.source_fence)
            .bind(payload_hash)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        let projection_digest = digest_rows(&rows)?;
        append_operation_event(
            &self.target,
            &self.data_profile_key,
            READ_PROJECTION_EVENT_TYPE,
            READ_PROJECTION_AGGREGATE_TYPE,
            operation_id("read-projection", run_id, "all", &projection_digest),
            json!({
                "run_id": run_id,
                "rows_projected": rows.len(),
                "projection_digest": projection_digest,
                "read_database_is_separate": true,
            }),
        )
        .await?;
        Ok(G13ReadProjectionResult {
            rows_projected: u64::try_from(rows.len())
                .map_err(|_| G13TargetImportError::InvalidReference("row count"))?,
            projection_digest,
        })
    }

    /// Expire only the bounded rehearsal target rows after the retention
    /// horizon. Rollback rows are intentionally preserved so the evidence can
    /// still be inspected; production retention requires an explicit operator
    /// decision outside this worker.
    pub async fn expire_rehearsal(
        &self,
        run_id: Uuid,
        retention_until: DateTime<Utc>,
        now: DateTime<Utc>,
        source_fence: &str,
    ) -> Result<G13RetentionResult, G13TargetImportError> {
        if !self.read_projection_enabled {
            return Err(G13TargetImportError::ReadProjectionUnavailable);
        }
        validate_scope(run_id, "retention", source_fence)?;
        if retention_until > now {
            return Err(G13TargetImportError::InvalidReference("retention fence"));
        }

        // The rollback database is the durable safety fence for expiry. It is
        // checked before either target or read rows can be removed, and every
        // retained row must carry the exact immutable source fence.
        let mut rollback_tx = self.rollback.begin().await.map_err(db)?;
        set_holding(&mut rollback_tx, &self.data_profile_key).await?;
        let rollback_counts = sqlx::query(
            "SELECT count(*) AS total,
                    count(*) FILTER (
                        WHERE source_contract_hash=$3 AND source_fence=$3
                    ) AS fenced
             FROM erpx_holding.g13_migration_rollback_rows
             WHERE data_profile_key=$1 AND run_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_fence)
        .fetch_one(&mut *rollback_tx)
        .await
        .map_err(db)?;
        let rollback_total: i64 = rollback_counts.try_get("total").map_err(db)?;
        let rollback_fenced: i64 = rollback_counts.try_get("fenced").map_err(db)?;
        rollback_tx.commit().await.map_err(db)?;
        if rollback_total != rollback_fenced {
            return Err(G13TargetImportError::RollbackFenceRequired);
        }

        let mut target_tx = self.target.begin().await.map_err(db)?;
        set_holding(&mut target_tx, &self.data_profile_key).await?;
        let target_counts = sqlx::query(
            "SELECT count(*) AS total,
                    count(*) FILTER (
                        WHERE source_contract_hash=$3 AND source_fence=$3
                    ) AS fenced
             FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_fence)
        .fetch_one(&mut *target_tx)
        .await
        .map_err(db)?;
        let target_total: i64 = target_counts.try_get("total").map_err(db)?;
        let target_fenced: i64 = target_counts.try_get("fenced").map_err(db)?;
        let retention_aggregate_id = operation_id("retention", run_id, "all", source_fence);
        let retention_event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("g13-event:{RETENTION_EVENT_TYPE}:{retention_aggregate_id}").as_bytes(),
        );
        let existing_retention_payload: Option<Value> =
            sqlx::query_scalar("SELECT payload FROM erpx_holding.outbox_events WHERE event_id=$1")
                .bind(retention_event_id)
                .fetch_optional(&mut *target_tx)
                .await
                .map_err(db)?;
        if let Some(payload) = &existing_retention_payload
            && (payload.get("run_id") != Some(&json!(run_id))
                || payload.get("source_fence") != Some(&json!(source_fence))
                || payload.get("retention_until") != Some(&json!(retention_until)))
        {
            return Err(G13TargetImportError::SourceFenceConflict);
        }
        let retention_event_present = existing_retention_payload.is_some();
        if target_total != target_fenced
            || (target_total != 0 && target_total != rollback_total)
            || (target_total == 0 && rollback_total != 0 && !retention_event_present)
        {
            return Err(G13TargetImportError::RollbackFenceRequired);
        }
        let target_deleted = sqlx::query(
            "DELETE FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2
               AND source_contract_hash=$3 AND source_fence=$3
               AND imported_at <= $4",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_fence)
        .bind(retention_until)
        .execute(&mut *target_tx)
        .await
        .map_err(db)?
        .rows_affected();
        let retention_payload = existing_retention_payload.unwrap_or_else(|| {
            json!({
                "run_id": run_id,
                "source_fence": source_fence,
                "retention_until": retention_until,
                "expired_at": now,
                "rows_expired": target_deleted,
                "rollback_rows_preserved": true,
                "immutable_outbox_preserved": true,
            })
        });
        append_operation_event_in_transaction(
            &mut target_tx,
            &self.data_profile_key,
            RETENTION_EVENT_TYPE,
            RETENTION_AGGREGATE_TYPE,
            retention_aggregate_id,
            retention_payload,
        )
        .await?;
        target_tx.commit().await.map_err(db)?;

        let mut read_tx = self.read.begin().await.map_err(db)?;
        set_holding(&mut read_tx, &self.data_profile_key).await?;
        let read_deleted = sqlx::query(
            "DELETE FROM erpx_holding.g13_migration_import_read_rows
             WHERE data_profile_key=$1 AND run_id=$2 AND source_fence=$3
               AND projected_at <= $4",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_fence)
        .bind(retention_until)
        .execute(&mut *read_tx)
        .await
        .map_err(db)?
        .rows_affected();
        let read_remaining: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM erpx_holding.g13_migration_import_read_rows
             WHERE data_profile_key=$1 AND run_id=$2 AND source_fence=$3
               AND projected_at <= $4",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .bind(source_fence)
        .bind(retention_until)
        .fetch_one(&mut *read_tx)
        .await
        .map_err(db)?;
        if read_remaining != 0 {
            return Err(G13TargetImportError::ReadProjectionUnavailable);
        }
        read_tx.commit().await.map_err(db)?;
        Ok(G13RetentionResult {
            rows_expired: target_deleted,
            read_rows_expired: read_deleted,
            retained_rollback_rows: true,
        })
    }

    /// Compare every source count and immutable identity mapping with the
    /// target import ledger. Counts alone are insufficient: a target can have
    /// the right cardinality while a source key points at a different target
    /// identity, fence, or forged payload. A deterministic reconciliation
    /// event is appended for every distinct source/target evidence pair, so a
    /// retry is durable and idempotent without collapsing a later drift report
    /// into an earlier success.
    pub async fn reconcile_run(
        &self,
        run_id: Uuid,
        snapshot: &LegacySourceSnapshot,
    ) -> Result<G13TargetReconciliation, G13TargetImportError> {
        // Reconciliation is itself durable source evidence. Enforce the same
        // immutable fence shape as every write, shadow-replay, rollback, and
        // retention path before reading or appending any target evidence.
        validate_scope(run_id, "reconcile", &snapshot.source_contract_hash)?;
        let rows = self.load_target_rows(run_id).await?;
        let mut target_counts = BTreeMap::new();
        let mut target_by_identity = BTreeMap::new();
        for row in &rows {
            *target_counts.entry(row.source_kind.clone()).or_insert(0) += 1;
            target_by_identity.insert((row.source_kind.clone(), row.source_key.clone()), row);
        }

        let source_identity_digest = digest_source_identity(snapshot);
        let target_identity_digest = digest_target_identity(&rows);
        let source_snapshot_digest = digest_source_snapshot(snapshot)?;
        let mut source_kinds = snapshot.counts.keys().cloned().collect::<BTreeSet<_>>();
        source_kinds.extend(snapshot.id_mappings.keys().cloned());
        source_kinds.extend(target_counts.keys().cloned());
        let mut differences = Vec::new();
        for source_kind in source_kinds {
            let source_count = snapshot.counts.get(&source_kind).copied();
            let target_count = target_counts.get(&source_kind).copied().unwrap_or(0);
            let expected_mapping = snapshot
                .id_mappings
                .get(&source_kind)
                .cloned()
                .unwrap_or_default();
            let mut identity_mismatch = false;
            for (source_key, target_id) in &expected_mapping {
                let Some(row) = target_by_identity.get(&(source_kind.clone(), source_key.clone()))
                else {
                    identity_mismatch = true;
                    continue;
                };
                let expected_payload = json!({
                    "run_id": run_id,
                    "source_kind": source_kind,
                    "source_key": source_key,
                    "target_id": target_id,
                    "source_contract_hash": snapshot.source_contract_hash,
                });
                if row.target_id != *target_id
                    || row.source_contract_hash != snapshot.source_contract_hash
                    || row.source_fence != snapshot.source_contract_hash
                    || row.payload != expected_payload
                {
                    identity_mismatch = true;
                }
            }
            if rows.iter().any(|row| {
                row.source_kind == source_kind
                    && (!expected_mapping.contains_key(&row.source_key)
                        || row.source_contract_hash != snapshot.source_contract_hash
                        || row.source_fence != snapshot.source_contract_hash)
            }) {
                identity_mismatch = true;
            }
            if source_count != Some(target_count) || identity_mismatch {
                let mut explanations = Vec::new();
                if source_count != Some(target_count) {
                    explanations.push(format!(
                        "source count {:?} and immutable target count {} differ",
                        source_count, target_count
                    ));
                }
                if identity_mismatch {
                    explanations.push(
                        "source key, target identity, source fence, or canonical payload differs"
                            .to_owned(),
                    );
                }
                differences.push(ReconciliationDifference {
                    key: source_kind.clone(),
                    source_count,
                    target_count: Some(target_count),
                    source_total_minor: snapshot.totals_minor.get(&source_kind).copied(),
                    target_total_minor: None,
                    explanation: Some(explanations.join("; ")),
                });
            }
        }
        let report = ReconciliationReport {
            differences,
            unexplained: Vec::new(),
        };
        let event_fence = format!("{source_snapshot_digest}:{target_identity_digest}");
        append_operation_event(
            &self.target,
            &self.data_profile_key,
            RECONCILIATION_EVENT_TYPE,
            RECONCILIATION_AGGREGATE_TYPE,
            operation_id("reconcile", run_id, "all", &event_fence),
            json!({
                "run_id": run_id,
                "source_contract_hash": snapshot.source_contract_hash,
                "source_snapshot_digest": source_snapshot_digest,
                "source_identity_digest": source_identity_digest,
                "target_identity_digest": target_identity_digest,
                "source_counts": snapshot.counts,
                "target_counts": target_counts,
                "difference_count": report.differences.len(),
                "reconciled": report.is_reconciled(),
            }),
        )
        .await?;
        Ok(G13TargetReconciliation {
            report,
            source_counts: snapshot.counts.clone(),
            target_counts,
            source_identity_digest,
            target_identity_digest,
        })
    }

    /// Capture target rows in a separate rollback database, delete the target
    /// import rows, restore them, and append explicit rollback-replay events.
    /// The caller must gate this operation with an authenticated write fence.
    pub async fn rehearse_rollback(
        &self,
        run_id: Uuid,
        source_fence: &str,
    ) -> Result<G13TargetRollbackResult, G13TargetImportError> {
        validate_scope(run_id, "rollback", source_fence)?;
        let target_rows = self.load_target_rows(run_id).await?;
        if target_rows
            .iter()
            .any(|row| row.source_contract_hash != source_fence || row.source_fence != source_fence)
        {
            return Err(G13TargetImportError::SourceFenceConflict);
        }
        // A rollback snapshot must never become the new authority by copying
        // a target row whose canonical payload has already drifted.
        for row in &target_rows {
            validate_loaded_row(run_id, row)?;
        }

        // The rollback database is the durable recovery boundary. A worker
        // may crash after this snapshot commits but before the target restore
        // commits (or after only part of the target/read projection is
        // restored). Retrying from the current target rows would silently
        // preserve that partial state, so an existing snapshot must always
        // win over the disposable target projection.
        let mut rollback_rows = self.load_rollback_rows(run_id).await?;
        if rollback_rows
            .iter()
            .any(|row| row.source_contract_hash != source_fence || row.source_fence != source_fence)
        {
            return Err(G13TargetImportError::SourceFenceConflict);
        }
        if rollback_rows.is_empty() {
            if target_rows.is_empty() {
                let read_model_rebuilt = if self.read_projection_enabled {
                    self.project_read_model(run_id).await?;
                    true
                } else {
                    false
                };
                return Ok(G13TargetRollbackResult {
                    rows_captured: 0,
                    rows_restored: 0,
                    outbox_events_replayed: 0,
                    read_model_rebuilt,
                    restored_state_hash: digest_rows(&target_rows)?,
                });
            }

            let mut rollback_tx = self.rollback.begin().await.map_err(db)?;
            set_holding(&mut rollback_tx, &self.data_profile_key).await?;
            for row in &target_rows {
                sqlx::query(
                    "INSERT INTO erpx_holding.g13_migration_rollback_rows
                        (data_profile_key, run_id, source_contract_hash, source_fence,
                         source_kind, source_key, target_id, payload)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                     ON CONFLICT (data_profile_key, run_id, source_kind, source_key)
                     DO NOTHING",
                )
                .bind(&self.data_profile_key)
                .bind(run_id)
                .bind(&row.source_contract_hash)
                .bind(&row.source_fence)
                .bind(&row.source_kind)
                .bind(&row.source_key)
                .bind(row.target_id)
                .bind(&row.payload)
                .execute(&mut *rollback_tx)
                .await
                .map_err(db)?;
            }
            rollback_tx.commit().await.map_err(db)?;
            rollback_rows = self.load_rollback_rows(run_id).await?;
        }

        let rows = rollback_rows;
        let rows_captured = u64::try_from(rows.len())
            .map_err(|_| G13TargetImportError::InvalidReference("row count"))?;

        let mut target_tx = self.target.begin().await.map_err(db)?;
        set_holding(&mut target_tx, &self.data_profile_key).await?;
        sqlx::query(
            "DELETE FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .execute(&mut *target_tx)
        .await
        .map_err(db)?;
        let mut outbox_events_replayed = 0_u64;
        for row in &rows {
            sqlx::query(
                "INSERT INTO erpx_holding.g13_migration_import_rows
                    (data_profile_key, run_id, source_contract_hash, source_fence,
                     source_kind, source_key, target_id, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                 ON CONFLICT (data_profile_key, run_id, source_kind, source_key)
                 DO NOTHING",
            )
            .bind(&self.data_profile_key)
            .bind(run_id)
            .bind(&row.source_contract_hash)
            .bind(&row.source_fence)
            .bind(&row.source_kind)
            .bind(&row.source_key)
            .bind(row.target_id)
            .bind(&row.payload)
            .execute(&mut *target_tx)
            .await
            .map_err(db)?;
            let event_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "g13-rollback:{run_id}:{}:{}",
                    row.source_kind, row.source_key
                )
                .as_bytes(),
            );
            let inserted = sqlx::query(
                "INSERT INTO erpx_holding.outbox_events
                    (event_id, aggregate_type, aggregate_id, data_profile_key,
                     aggregate_version, event_type, payload, occurred_at,
                     contract_version, event_version, active_scope, actor,
                     correlation_id, causation_id, idempotency_key)
                 VALUES ($1,$2,$3,$4,1,$5,$6,now(),1,1,$7,$8,$9,NULL,$10)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(event_id)
            .bind(IMPORT_AGGREGATE_TYPE)
            .bind(row.target_id)
            .bind(&self.data_profile_key)
            .bind(ROLLBACK_REPLAY_EVENT_TYPE)
            .bind(json!({
                "run_id": run_id,
                "source_kind": row.source_kind,
                "source_key": row.source_key,
                "target_id": row.target_id,
            }))
            .bind(json!({ "data_profile_key": self.data_profile_key }))
            .bind(json!({ "kind": "system", "principal_ref": "system:g13-target" }))
            .bind(Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("g13-correlation:{event_id}").as_bytes(),
            ))
            .bind(format!("g13:{ROLLBACK_REPLAY_EVENT_TYPE}:{event_id}"))
            .execute(&mut *target_tx)
            .await
            .map_err(db)?
            .rows_affected();
            if inserted == 0 {
                let event_is_valid: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                         SELECT 1 FROM erpx_holding.outbox_events
                         WHERE event_id=$1 AND aggregate_type=$2 AND aggregate_id=$3
                           AND data_profile_key=$4 AND event_type=$5
                     )",
                )
                .bind(event_id)
                .bind(IMPORT_AGGREGATE_TYPE)
                .bind(row.target_id)
                .bind(&self.data_profile_key)
                .bind(ROLLBACK_REPLAY_EVENT_TYPE)
                .fetch_one(&mut *target_tx)
                .await
                .map_err(db)?;
                if !event_is_valid {
                    return Err(G13TargetImportError::SourceFenceConflict);
                }
            }
            // Count both newly inserted and already durable events. A worker
            // retry after the target transaction committed must remain a
            // successful, idempotent rollback replay.
            outbox_events_replayed = outbox_events_replayed.saturating_add(1);
        }
        target_tx.commit().await.map_err(db)?;
        let read_model_rebuilt = if self.read_projection_enabled {
            self.project_read_model(run_id).await?;
            true
        } else {
            false
        };
        let restored = self.load_target_rows(run_id).await?;
        let restored_state_hash = digest_rows(&restored)?;
        Ok(G13TargetRollbackResult {
            rows_captured,
            rows_restored: u64::try_from(restored.len())
                .map_err(|_| G13TargetImportError::InvalidReference("row count"))?,
            outbox_events_replayed,
            read_model_rebuilt,
            restored_state_hash,
        })
    }

    async fn load_target_rows(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<G13ImportRow>, G13TargetImportError> {
        let mut tx = self.target.begin().await.map_err(db)?;
        set_holding(&mut tx, &self.data_profile_key).await?;
        let rows = sqlx::query(
            "SELECT source_contract_hash, source_fence, source_kind,
                    source_key, target_id, payload
             FROM erpx_holding.g13_migration_import_rows
             WHERE data_profile_key=$1 AND run_id=$2
             ORDER BY source_kind, source_key",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?
        .into_iter()
        .map(|row| {
            Ok(G13ImportRow {
                source_contract_hash: row.try_get("source_contract_hash").map_err(db)?,
                source_fence: row.try_get("source_fence").map_err(db)?,
                source_kind: row.try_get("source_kind").map_err(db)?,
                source_key: row.try_get("source_key").map_err(db)?,
                target_id: row.try_get("target_id").map_err(db)?,
                payload: row.try_get("payload").map_err(db)?,
            })
        })
        .collect::<Result<Vec<_>, G13TargetImportError>>()?;
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }

    async fn load_rollback_rows(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<G13ImportRow>, G13TargetImportError> {
        let mut tx = self.rollback.begin().await.map_err(db)?;
        set_holding(&mut tx, &self.data_profile_key).await?;
        let rows = sqlx::query(
            "SELECT source_contract_hash, source_fence, source_kind,
                    source_key, target_id, payload
             FROM erpx_holding.g13_migration_rollback_rows
             WHERE data_profile_key=$1 AND run_id=$2
             ORDER BY source_kind, source_key",
        )
        .bind(&self.data_profile_key)
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?
        .into_iter()
        .map(|row| {
            Ok(G13ImportRow {
                source_contract_hash: row.try_get("source_contract_hash").map_err(db)?,
                source_fence: row.try_get("source_fence").map_err(db)?,
                source_kind: row.try_get("source_kind").map_err(db)?,
                source_key: row.try_get("source_key").map_err(db)?,
                target_id: row.try_get("target_id").map_err(db)?,
                payload: row.try_get("payload").map_err(db)?,
            })
        })
        .collect::<Result<Vec<_>, G13TargetImportError>>()?;
        for row in &rows {
            validate_loaded_row(run_id, row)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(rows)
    }
}

#[derive(Debug, Clone)]
struct G13ImportRow {
    source_contract_hash: String,
    source_fence: String,
    source_kind: String,
    source_key: String,
    target_id: Uuid,
    payload: Value,
}

fn validate_loaded_row(run_id: Uuid, row: &G13ImportRow) -> Result<(), G13TargetImportError> {
    if run_id.is_nil() || row.target_id.is_nil() {
        return Err(G13TargetImportError::InvalidReference("row identity"));
    }
    if row.source_contract_hash != row.source_fence
        || row.source_fence.len() != 64
        || !row.source_fence.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(G13TargetImportError::SourceFenceConflict);
    }
    if row.source_kind.trim().is_empty()
        || row.source_kind.len() > 128
        || row.source_kind.chars().any(char::is_control)
    {
        return Err(G13TargetImportError::InvalidReference("source_kind"));
    }
    if row.source_key.trim().is_empty()
        || row.source_key.len() > 512
        || row.source_key.chars().any(char::is_control)
    {
        return Err(G13TargetImportError::InvalidReference("source_key"));
    }
    let expected_payload = json!({
        "run_id": run_id,
        "source_kind": &row.source_kind,
        "source_key": &row.source_key,
        "target_id": row.target_id,
        "source_contract_hash": &row.source_fence,
    });
    if row.payload != expected_payload {
        return Err(G13TargetImportError::SourceFenceConflict);
    }
    Ok(())
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
) -> Result<(), G13TargetImportError> {
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    Ok(())
}

fn validate_scope(
    run_id: Uuid,
    source_kind: &str,
    source_fence: &str,
) -> Result<(), G13TargetImportError> {
    if run_id.is_nil() {
        return Err(G13TargetImportError::InvalidReference("run_id"));
    }
    if source_kind.trim().is_empty() || source_kind.len() > 128 {
        return Err(G13TargetImportError::InvalidReference("source_kind"));
    }
    if source_fence.len() != 64 || !source_fence.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(G13TargetImportError::InvalidReference("source_fence"));
    }
    Ok(())
}

fn digest_rows(rows: &[G13ImportRow]) -> Result<String, G13TargetImportError> {
    let mut digest = Sha256::new();
    for row in rows {
        digest.update(row.source_contract_hash.as_bytes());
        digest.update([0]);
        digest.update(row.source_fence.as_bytes());
        digest.update([0]);
        digest.update(row.source_kind.as_bytes());
        digest.update([0]);
        digest.update(row.source_key.as_bytes());
        digest.update([0]);
        digest.update(row.target_id.as_bytes());
        digest.update([0]);
        digest.update(
            serde_json::to_vec(&row.payload).map_err(|_| G13TargetImportError::Serialization)?,
        );
        digest.update([0]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn digest_source_snapshot(snapshot: &LegacySourceSnapshot) -> Result<String, G13TargetImportError> {
    let encoded = serde_json::to_vec(snapshot).map_err(|_| G13TargetImportError::Serialization)?;
    Ok(digest_bytes(&encoded))
}

fn digest_source_identity(snapshot: &LegacySourceSnapshot) -> String {
    let mut digest = Sha256::new();
    for (source_kind, mapping) in &snapshot.id_mappings {
        for (source_key, target_id) in mapping {
            digest.update(source_kind.as_bytes());
            digest.update([0]);
            digest.update(source_key.as_bytes());
            digest.update([0]);
            digest.update(target_id.as_bytes());
            digest.update([0]);
        }
    }
    hex::encode(digest.finalize())
}

fn digest_target_identity(rows: &[G13ImportRow]) -> String {
    let mut digest = Sha256::new();
    for row in rows {
        digest.update(row.source_kind.as_bytes());
        digest.update([0]);
        digest.update(row.source_key.as_bytes());
        digest.update([0]);
        digest.update(row.target_id.as_bytes());
        digest.update([0]);
    }
    hex::encode(digest.finalize())
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn operation_id(kind: &str, run_id: Uuid, source_kind: &str, source_fence: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("g13-operation:{kind}:{run_id}:{source_kind}:{source_fence}").as_bytes(),
    )
}

async fn append_operation_event(
    pool: &PgPool,
    data_profile_key: &str,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    payload: Value,
) -> Result<(), G13TargetImportError> {
    let mut tx = pool.begin().await.map_err(db)?;
    set_holding(&mut tx, data_profile_key).await?;
    append_operation_event_in_transaction(
        &mut tx,
        data_profile_key,
        event_type,
        aggregate_type,
        aggregate_id,
        payload,
    )
    .await?;
    tx.commit().await.map_err(db)
}

async fn append_operation_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    payload: Value,
) -> Result<(), G13TargetImportError> {
    let event_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("g13-event:{event_type}:{aggregate_id}").as_bytes(),
    );
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, active_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,1,$5,$6,now(),1,1,$7,$8,$9,NULL,$10)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(event_type)
    .bind(&payload)
    .bind(json!({ "data_profile_key": data_profile_key }))
    .bind(json!({ "kind": "system", "principal_ref": "system:g13-target" }))
    .bind(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("g13-correlation:{event_id}").as_bytes(),
    ))
    .bind(format!("g13:{event_type}:{event_id}"))
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    ensure_outbox_event_matches(
        tx,
        event_id,
        aggregate_type,
        aggregate_id,
        data_profile_key,
        event_type,
        &payload,
    )
    .await
}

async fn ensure_outbox_event_matches(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    aggregate_type: &str,
    aggregate_id: Uuid,
    data_profile_key: &str,
    event_type: &str,
    payload: &Value,
) -> Result<(), G13TargetImportError> {
    let stored = sqlx::query(
        "SELECT aggregate_type, aggregate_id, data_profile_key, aggregate_version,
                event_type, payload, contract_version, event_version,
                active_scope, actor, correlation_id, causation_id, idempotency_key
         FROM erpx_holding.outbox_events
         WHERE event_id=$1",
    )
    .bind(event_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(db)?;
    let expected_holding_scope = json!({ "data_profile_key": data_profile_key });
    let expected_actor = json!({ "kind": "system", "principal_ref": "system:g13-target" });
    let expected_correlation = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("g13-correlation:{event_id}").as_bytes(),
    );
    let expected_idempotency_key = format!("g13:{event_type}:{event_id}");
    let matches = stored.try_get::<String, _>("aggregate_type").map_err(db)? == aggregate_type
        && stored.try_get::<Uuid, _>("aggregate_id").map_err(db)? == aggregate_id
        && stored
            .try_get::<String, _>("data_profile_key")
            .map_err(db)?
            == data_profile_key
        && stored.try_get::<i64, _>("aggregate_version").map_err(db)? == 1
        && stored.try_get::<String, _>("event_type").map_err(db)? == event_type
        && stored.try_get::<Value, _>("payload").map_err(db)? == *payload
        && stored
            .try_get::<Option<i32>, _>("contract_version")
            .map_err(db)?
            == Some(1)
        && stored
            .try_get::<Option<i32>, _>("event_version")
            .map_err(db)?
            == Some(1)
        && stored
            .try_get::<Option<Value>, _>("active_scope")
            .map_err(db)?
            == Some(expected_holding_scope)
        && stored.try_get::<Option<Value>, _>("actor").map_err(db)? == Some(expected_actor)
        && stored
            .try_get::<Option<Uuid>, _>("correlation_id")
            .map_err(db)?
            == Some(expected_correlation)
        && stored
            .try_get::<Option<Uuid>, _>("causation_id")
            .map_err(db)?
            .is_none()
        && stored
            .try_get::<Option<String>, _>("idempotency_key")
            .map_err(db)?
            == Some(expected_idempotency_key);
    if !matches {
        return Err(G13TargetImportError::SourceFenceConflict);
    }
    Ok(())
}

fn validate_write_fence(
    reference: &str,
    writes_blocked: bool,
    validated_at: DateTime<Utc>,
) -> Result<(), G13TargetImportError> {
    let now = Utc::now();
    if reference.trim().is_empty()
        || reference.len() > 200
        || !writes_blocked
        || validated_at > now + Duration::seconds(30)
        || now.signed_duration_since(validated_at) > Duration::minutes(5)
    {
        return Err(G13TargetImportError::WriteFenceRequired);
    }
    Ok(())
}

fn db(error: sqlx::Error) -> G13TargetImportError {
    G13TargetImportError::Database(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reconcile_rejects_invalid_source_fence_before_database_access() {
        let importer = G13TargetImporter::new(
            PgPool::connect_lazy("postgres://unused").expect("lazy target pool"),
            PgPool::connect_lazy("postgres://unused-rollback").expect("lazy rollback pool"),
            "holding-g13-fence-test",
        )
        .expect("valid holding reference");
        let snapshot = LegacySourceSnapshot {
            extractor_version: "g13-test".into(),
            generated_at: Utc::now(),
            source_contract_hash: "not-a-source-fence".into(),
            missing_tables: Vec::new(),
            counts: BTreeMap::new(),
            statuses: BTreeMap::new(),
            totals_minor: BTreeMap::new(),
            object_bytes: BTreeMap::new(),
            id_mappings: BTreeMap::new(),
        };

        let result = importer.reconcile_run(Uuid::new_v4(), &snapshot).await;

        assert!(matches!(
            result,
            Err(G13TargetImportError::InvalidReference("source_fence"))
        ));
    }

    #[test]
    fn loaded_rows_require_the_canonical_immutable_payload() {
        let run_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let fence = "a".repeat(64);
        let row = G13ImportRow {
            source_contract_hash: fence.clone(),
            source_fence: fence.clone(),
            source_kind: "customers".into(),
            source_key: "customer-1".into(),
            target_id,
            payload: json!({
                "run_id": run_id,
                "source_kind": "customers",
                "source_key": "customer-1",
                "target_id": target_id,
                "source_contract_hash": fence,
            }),
        };
        assert!(validate_loaded_row(run_id, &row).is_ok());

        let mut forged = row;
        forged.payload["source_key"] = json!("customer-forged");
        assert!(matches!(
            validate_loaded_row(run_id, &forged),
            Err(G13TargetImportError::SourceFenceConflict)
        ));
    }
}
