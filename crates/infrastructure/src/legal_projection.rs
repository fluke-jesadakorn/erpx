//! Tenant-read Legal/document projection and typed read adapter.
//!
//! The write database is read only at the source boundary.  The projection
//! writes only the rebuildable read database and accepts only the complete
//! shared event envelope plus one of the closed, typed Legal payloads.  Raw
//! payload JSON, document bytes, URLs, credentials, and tokens never cross
//! the public read-model boundary.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{
    ActorAttribution, ActorKind, CONTRACT_VERSION, EventEnvelope, TenantScope,
};
use erpx_domain::legal::{
    ContractStatus, LegalProjectionReconciliation, MAX_DOCUMENT_VERSIONS, MAX_LEGAL_PARTIES,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;
use crate::legal_persistence::{
    LEGAL_ACCESS_HISTORY_AGGREGATE, LEGAL_ACCESS_RECORDED_EVENT, LEGAL_CONTRACT_AGGREGATE,
    LEGAL_CONTRACT_SAVED_EVENT, LEGAL_DOCUMENT_AGGREGATE, LEGAL_DOCUMENT_SAVED_EVENT,
    LegalAccessHistoryEvent, LegalAggregateType, LegalContractEvent, LegalDocumentEvent,
    LegalEventType, LegalOutboxPayload,
};

pub const LEGAL_PROJECTION_NAME: &str = "legal_documents";
pub const MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_LEGAL_READ_PAGE_SIZE: u32 = 100;
pub const MAX_LEGAL_READ_CHILD_ROWS: u32 = 128;
/// Maximum source-event history one Legal projection invocation may drain.
pub const MAX_LEGAL_PROJECTION_EVENTS: u64 = 1_000_000;
pub const LEGAL_READ_STALE_AFTER_SECONDS: u64 = 300;

const MAX_LEGAL_RETENTION_HORIZON: Duration = Duration::days(36_525);
const MAX_LEGAL_CLOCK_SKEW: Duration = Duration::minutes(5);

const LEGAL_AGGREGATES: [&str; 3] = [
    LEGAL_CONTRACT_AGGREGATE,
    LEGAL_DOCUMENT_AGGREGATE,
    LEGAL_ACCESS_HISTORY_AGGREGATE,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

/// The persisted event metadata retained by the normalized read rows.  The
/// tenant scope itself is validated as a typed `TenantScope` at ingestion;
/// read rows retain only the already-normalized holding/company/branch scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalSourceMetadata {
    pub contract_version: u16,
    pub event_version: u16,
    pub actor_ref: String,
    pub actor_kind: ActorKind,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegalSourceEvent {
    pub event_id: Uuid,
    pub contract_version: u16,
    pub event_version: u16,
    pub event_type: LegalEventType,
    pub aggregate_type: LegalAggregateType,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub occurred_at: DateTime<Utc>,
    pub tenant_scope: TenantScope,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: String,
    pub payload: LegalOutboxPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegalSourceEventPage {
    pub events: Vec<LegalSourceEvent>,
    pub next_cursor: Option<LegalSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: LegalAggregateType,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub receipts_cleared: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalContractReadCursor {
    pub contract_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalDocumentReadCursor {
    pub document_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalAccessReadCursor {
    pub observed_at: DateTime<Utc>,
    pub entry_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalReadFreshness {
    pub last_event_id: Option<Uuid>,
    pub last_projected_at: Option<DateTime<Utc>>,
    pub source_aggregate_version: Option<u64>,
    pub lag_seconds: u64,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractPartyReadRow {
    pub party_id: Uuid,
    pub holding_id: Uuid,
    pub party_role: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractApprovalReadRow {
    pub holding_id: Uuid,
    pub approved_by: String,
    pub approved_at: DateTime<Utc>,
    pub contract_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractRetentionReadRow {
    pub holding_id: Uuid,
    pub retention_until: DateTime<Utc>,
    pub retention_state: String,
    pub contract_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractReadRow {
    pub contract_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub reference: String,
    pub title: String,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: DateTime<Utc>,
    pub status: String,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
    pub parties: Vec<LegalContractPartyReadRow>,
    pub approval: Option<LegalContractApprovalReadRow>,
    pub retention: LegalContractRetentionReadRow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractReadPage {
    pub rows: Vec<LegalContractReadRow>,
    pub next_cursor: Option<LegalContractReadCursor>,
    pub has_more: bool,
    pub freshness: LegalReadFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalDocumentVersionReadRow {
    pub version: u64,
    pub object_ref: String,
    pub content_sha256: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalDocumentReadRow {
    pub document_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub title: String,
    pub current_version: u64,
    pub archived: bool,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
    pub versions: Vec<LegalDocumentVersionReadRow>,
    pub versions_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalDocumentReadPage {
    pub rows: Vec<LegalDocumentReadRow>,
    pub next_cursor: Option<LegalDocumentReadCursor>,
    pub has_more: bool,
    pub freshness: LegalReadFreshness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalAccessHistoryReadRow {
    pub entry_id: Uuid,
    pub document_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub document_version: u64,
    pub actor_ref: String,
    pub action: String,
    pub purpose: String,
    pub observed_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: LegalSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalAccessHistoryReadPage {
    pub rows: Vec<LegalAccessHistoryReadRow>,
    pub next_cursor: Option<LegalAccessReadCursor>,
    pub has_more: bool,
    pub freshness: LegalReadFreshness,
}

#[derive(Debug, Clone)]
pub struct SqlxLegalProjectionStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxLegalProjectionStore {
    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool: write_pool.clone(),
            read_pool: read_pool.clone(),
            holding_ref,
        })
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Read only the complete shared envelope and closed typed Legal events
    /// from the authoritative tenant-write outbox.  Legacy rows with a NULL
    /// envelope field or an untyped payload fail closed.
    pub async fn read_source_event_page(
        &self,
        cursor: Option<&LegalSourceEventCursor>,
        page_size: u32,
    ) -> Result<LegalSourceEventPage, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let page =
            read_source_event_page_in_transaction(&mut tx, &self.holding_ref, cursor, page_size)
                .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Return the holding-scoped count of authoritative Legal source events.
    /// This is a preflight bound for replacement projection work and is not a
    /// read-model authority.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let count = source_event_high_watermark_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(count)
    }

    /// Compare the authoritative Legal/document outbox and contract retention
    /// rows with the disposable projection. This operation is read-only and
    /// scoped to the routed holding; mismatches are rebuild/review signals.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<LegalProjectionReconciliation, PersistenceError> {
        // Reconciliation is also a source-integrity check. Walk the typed
        // source pages before comparing counts so legacy/untyped or forged
        // outbox rows cannot be reported as a green projection merely because
        // their aggregate keys happen to line up.
        let mut source_cursor = None;
        loop {
            let page = self
                .read_source_event_page(source_cursor.as_ref(), MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE)
                .await?;
            if !page.has_more {
                break;
            }
            source_cursor = page.next_cursor;
            if source_cursor.is_none() {
                return Err(invalid(
                    "Legal source reconciliation page omitted its continuation cursor",
                ));
            }
        }
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let source_rows = sqlx::query(
            "SELECT events.aggregate_type, events.aggregate_id,
                    MIN(events.aggregate_version) AS first_version,
                    MAX(events.aggregate_version) AS latest_version,
                    COUNT(*) AS event_count,
                    COUNT(DISTINCT events.aggregate_version) AS distinct_event_version_count,
                    (SELECT latest.event_id
                     FROM erpx_tenant.outbox_events AS latest
                     WHERE latest.holding_ref=$1
                       AND latest.aggregate_type=events.aggregate_type
                       AND latest.aggregate_id=events.aggregate_id
                     ORDER BY latest.aggregate_version DESC, latest.event_id DESC
                     LIMIT 1) AS latest_event_id
             FROM erpx_tenant.outbox_events AS events
             WHERE events.holding_ref=$1 AND events.aggregate_type = ANY($2)
             GROUP BY events.aggregate_type, events.aggregate_id",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_AGGREGATES)
        .fetch_all(&mut *source_tx)
        .await?;
        let source_retention_rows = sqlx::query(
            "SELECT contracts.contract_id, contracts.holding_id, contracts.retention_until,
                    contracts.version AS contract_version,
                    retention.holding_id AS stored_holding_id,
                    retention.retention_until AS stored_retention_until,
                    retention.retention_state,
                    retention.contract_version AS stored_contract_version
             FROM erpx_tenant.legal_contracts AS contracts
             LEFT JOIN erpx_tenant.legal_contract_retention AS retention
               ON retention.holding_ref=contracts.holding_ref
              AND retention.contract_id=contracts.contract_id
             WHERE contracts.holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source = HashMap::with_capacity(source_rows.len());
        let mut source_event_count = 0_u64;
        let mut source_history_mismatch_count = 0_u64;
        for row in source_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let first_version =
                as_u64(row.try_get("first_version")?, "Legal first source version")?;
            let latest_version = as_u64(row.try_get("latest_version")?, "Legal source version")?;
            let latest_event_id: Uuid = row.try_get("latest_event_id")?;
            let event_count = as_u64(row.try_get("event_count")?, "Legal source event count")?;
            let distinct_event_version_count = as_u64(
                row.try_get("distinct_event_version_count")?,
                "Legal distinct source event version count",
            )?;
            source_event_count = source_event_count
                .checked_add(event_count)
                .ok_or_else(|| invalid("Legal event count overflow"))?;
            if !source_history_is_contiguous(
                first_version,
                event_count,
                distinct_event_version_count,
                latest_version,
            ) {
                source_history_mismatch_count = source_history_mismatch_count.saturating_add(1);
            }
            source.insert(
                (aggregate_type, aggregate_id),
                (latest_version, latest_event_id),
            );
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, source_version, last_event_id
             FROM erpx_tenant.legal_projection_checkpoints
             WHERE holding_ref=$1 AND projection_name=$2",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_PROJECTION_NAME)
        .fetch_all(&mut *read_tx)
        .await?;
        let projected = read_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    (row.try_get("aggregate_type")?, row.try_get("aggregate_id")?),
                    (
                        as_u64(row.try_get("source_version")?, "Legal projected version")?,
                        row.try_get("last_event_id")?,
                    ),
                ))
            })
            .collect::<Result<HashMap<(String, Uuid), (u64, Uuid)>, _>>()?;

        let read_model_rows = sqlx::query(
            "SELECT 'legal_contract'::TEXT AS aggregate_type, contract_id AS aggregate_id
             FROM erpx_tenant.legal_contract_read_rows
             WHERE holding_ref=$1
             UNION ALL
             SELECT 'legal_document'::TEXT AS aggregate_type, document_id AS aggregate_id
             FROM erpx_tenant.legal_document_read_rows
             WHERE holding_ref=$1
             UNION ALL
             SELECT 'legal_access_history'::TEXT AS aggregate_type, entry_id AS aggregate_id
             FROM erpx_tenant.legal_access_history_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let read_models = read_model_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    row.try_get("aggregate_type")?,
                    row.try_get("aggregate_id")?,
                ))
            })
            .collect::<Result<HashSet<(String, Uuid)>, _>>()?;

        let retention_read_rows = sqlx::query(
            "SELECT contract_id, retention_until, retention_state, contract_version
             FROM erpx_tenant.legal_contract_retention_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;

        let source_retention = source_retention_rows
            .into_iter()
            .map(|row| {
                let contract_id: Uuid = row.try_get("contract_id")?;
                let contract_holding_id: Uuid = row.try_get("holding_id")?;
                let contract_version = as_u64(
                    row.try_get("contract_version")?,
                    "Legal contract retention source version",
                )?;
                let stored_holding_id = row.try_get::<Option<Uuid>, _>("stored_holding_id")?;
                let stored_contract_version = row
                    .try_get::<Option<i64>, _>("stored_contract_version")?
                    .map(|value| as_u64(value, "Legal stored retention version"))
                    .transpose()?;
                Ok::<_, PersistenceError>((
                    contract_id,
                    (
                        row.try_get::<DateTime<Utc>, _>("retention_until")?,
                        contract_version,
                        contract_holding_id,
                        stored_holding_id,
                        row.try_get::<Option<DateTime<Utc>>, _>("stored_retention_until")?,
                        row.try_get::<Option<String>, _>("retention_state")?,
                        stored_contract_version,
                    ),
                ))
            })
            .collect::<Result<HashMap<Uuid, _>, _>>()?;
        let projected_retention = retention_read_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    row.try_get("contract_id")?,
                    (
                        row.try_get::<DateTime<Utc>, _>("retention_until")?,
                        row.try_get::<String, _>("retention_state")?,
                        as_u64(
                            row.try_get("contract_version")?,
                            "Legal projected retention version",
                        )?,
                    ),
                ))
            })
            .collect::<Result<HashMap<Uuid, _>, _>>()?;
        let mut retention_mismatch_count = 0_u64;
        for (
            contract_id,
            (
                retention_until,
                contract_version,
                contract_holding_id,
                stored_holding_id,
                stored_until,
                stored_state,
                stored_version,
            ),
        ) in &source_retention
        {
            let expected_retention_state = retention_state(*retention_until);
            let source_retention_is_consistent = stored_holding_id == &Some(*contract_holding_id)
                && stored_until.as_ref() == Some(retention_until)
                && stored_version == &Some(*contract_version)
                && stored_state.as_deref() == Some(expected_retention_state);
            let read_retention_is_consistent = projected_retention.get(contract_id).is_some_and(
                |(read_until, read_state, read_version)| {
                    read_until == retention_until
                        && read_state == expected_retention_state
                        && *read_version == *contract_version
                },
            );
            if !source_retention_is_consistent || !read_retention_is_consistent {
                retention_mismatch_count = retention_mismatch_count.saturating_add(1);
            }
        }
        retention_mismatch_count = retention_mismatch_count.saturating_add(
            projected_retention
                .keys()
                .filter(|contract_id| !source_retention.contains_key(contract_id))
                .count() as u64,
        );

        let missing_aggregate_count = source
            .keys()
            .filter(|key| !projected.contains_key(*key))
            .count() as u64;
        let stale_aggregate_count = source_history_mismatch_count
            .saturating_add(
                source
                    .iter()
                    .filter(|(key, (version, event_id))| {
                        projected
                            .get(*key)
                            .is_some_and(|(current_version, current_event_id)| {
                                current_version != version || current_event_id != event_id
                            })
                    })
                    .count() as u64,
            )
            .saturating_add(
                projected
                    .keys()
                    .filter(|key| !read_models.contains(*key))
                    .count() as u64,
            );
        let orphan_aggregate_count = (projected
            .keys()
            .filter(|key| !source.contains_key(*key))
            .count() as u64)
            .saturating_add(
                read_models
                    .iter()
                    .filter(|key| !source.contains_key(*key))
                    .count() as u64,
            );
        Ok(LegalProjectionReconciliation {
            source_event_count,
            source_aggregate_count: source.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
            retention_mismatch_count,
        })
    }

    /// Apply one event atomically to the read database.  Receipt insertion,
    /// normalized rows, and the contiguous aggregate checkpoint share one
    /// read-side transaction.
    pub async fn project_event(
        &self,
        event: &LegalSourceEvent,
    ) -> Result<LegalProjectionApplication, PersistenceError> {
        validate_source_event(event)?;
        self.verify_authoritative_source_event(event).await?;
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let application = self.project_event_in_transaction(&mut tx, event).await?;
        tx.commit().await?;
        Ok(application)
    }

    /// Atomically replace the disposable read model from a stable source
    /// snapshot. The source share lock prevents outbox writes while source
    /// pages are read; the read transaction is committed only after every
    /// event has been validated and replayed. Any source/decode/version/read
    /// failure drops the read transaction and preserves the prior projection.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_event_count: Option<u64>,
        page_size: u32,
    ) -> Result<(LegalProjectionRebuildPreparation, u64), PersistenceError> {
        validate_source_page_size(page_size)?;
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_event_count =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if expected_source_event_count.is_some_and(|expected| expected != source_event_count) {
            return Err(invalid(
                "Legal source fence changed before replacement rebuild",
            ));
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation = clear_projection_in_transaction(&mut read_tx, &self.holding_ref).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                verify_authoritative_source_event_in_transaction(
                    &mut source_tx,
                    &self.holding_ref,
                    event,
                )
                .await?;
                if self
                    .project_event_in_transaction(&mut read_tx, event)
                    .await?
                    .applied
                {
                    events_replayed = events_replayed
                        .checked_add(1)
                        .ok_or_else(|| invalid("Legal replay event count overflow"))?;
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(invalid("Legal source page omitted its continuation cursor"));
            }
        }
        let final_source_event_count =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_event_count != source_event_count {
            return Err(invalid(
                "Legal source fence changed during replacement rebuild",
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    async fn project_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: &LegalSourceEvent,
    ) -> Result<LegalProjectionApplication, PersistenceError> {
        validate_source_event(event)?;
        if let Some(receipt) = self.receipt(tx, event).await? {
            return Ok(LegalProjectionApplication {
                event_id: event.event_id,
                aggregate_type: event.aggregate_type,
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
                applied: receipt,
            });
        }
        self.validate_checkpoint(tx, event).await?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.legal_projection_event_receipts
                (holding_ref, projection_name, event_id, aggregate_type,
                 aggregate_id, aggregate_version, event_type,
                 source_contract_version, source_event_version, source_actor_ref,
                 source_actor_kind, source_correlation_id, source_causation_id,
                 source_idempotency_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (holding_ref, projection_name, event_id) DO NOTHING",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_PROJECTION_NAME)
        .bind(event.event_id)
        .bind(event.aggregate_type.as_str())
        .bind(event.aggregate_id)
        .bind(as_i64(event.aggregate_version, "Legal source version")?)
        .bind(event.event_type.as_str())
        .bind(i32::from(event.contract_version))
        .bind(i32::from(event.event_version))
        .bind(&event.actor.principal_ref)
        .bind(actor_kind_name(&event.actor.kind))
        .bind(event.correlation_id)
        .bind(event.causation_id)
        .bind(&event.idempotency_key)
        .execute(&mut **tx)
        .await?;
        if inserted.rows_affected() == 0 {
            return Ok(LegalProjectionApplication {
                event_id: event.event_id,
                aggregate_type: event.aggregate_type,
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
                applied: false,
            });
        }

        match event.payload.clone() {
            LegalOutboxPayload::ContractSaved(contract) => {
                project_contract(tx, &self.holding_ref, event, &contract).await?
            }
            LegalOutboxPayload::DocumentSaved(document) => {
                project_document(tx, &self.holding_ref, event, &document).await?
            }
            LegalOutboxPayload::AccessRecorded(access) => {
                validate_access_source_document(&self.write_pool, &self.holding_ref, &access)
                    .await?;
                project_access(tx, &self.holding_ref, event, &access).await?
            }
        }
        let (holding_id, company_id, branch_id) = event_scope(event)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.legal_projection_checkpoints
                (holding_ref, projection_name, aggregate_type, aggregate_id,
                 holding_id, company_id, branch_id, last_event_id, source_version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
             ON CONFLICT (holding_ref, projection_name, aggregate_type, aggregate_id)
             DO UPDATE SET holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
                 branch_id=EXCLUDED.branch_id, last_event_id=EXCLUDED.last_event_id,
                 source_version=EXCLUDED.source_version, projected_at=now()",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_PROJECTION_NAME)
        .bind(event.aggregate_type.as_str())
        .bind(event.aggregate_id)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(event.event_id)
        .bind(as_i64(event.aggregate_version, "Legal source version")?)
        .execute(&mut **tx)
        .await?;
        Ok(LegalProjectionApplication {
            event_id: event.event_id,
            aggregate_type: event.aggregate_type,
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
            applied: true,
        })
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<LegalProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(preparation)
    }

    async fn receipt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: &LegalSourceEvent,
    ) -> Result<Option<bool>, PersistenceError> {
        let row = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version, event_type,
                    source_contract_version, source_event_version, source_actor_ref,
                    source_actor_kind, source_correlation_id, source_causation_id,
                    source_idempotency_key
             FROM erpx_tenant.legal_projection_event_receipts
             WHERE holding_ref=$1 AND projection_name=$2 AND event_id=$3
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_PROJECTION_NAME)
        .bind(event.event_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(row) = row else { return Ok(None) };
        if row.try_get::<String, _>("aggregate_type")? != event.aggregate_type.as_str()
            || row.try_get::<Uuid, _>("aggregate_id")? != event.aggregate_id
            || as_u64(row.try_get("aggregate_version")?, "receipt version")?
                != event.aggregate_version
            || row.try_get::<String, _>("event_type")? != event.event_type.as_str()
            || row.try_get::<i32, _>("source_contract_version")?
                != i32::from(event.contract_version)
            || row.try_get::<i32, _>("source_event_version")? != i32::from(event.event_version)
            || row.try_get::<String, _>("source_actor_ref")? != event.actor.principal_ref
            || row.try_get::<String, _>("source_actor_kind")? != actor_kind_name(&event.actor.kind)
            || row.try_get::<Uuid, _>("source_correlation_id")? != event.correlation_id
            || row.try_get::<Option<Uuid>, _>("source_causation_id")? != event.causation_id
            || row.try_get::<String, _>("source_idempotency_key")? != event.idempotency_key
        {
            return Err(invalid("Legal projection receipt metadata conflicts"));
        }
        Ok(Some(false))
    }

    async fn validate_checkpoint(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: &LegalSourceEvent,
    ) -> Result<(), PersistenceError> {
        let row = sqlx::query(
            "SELECT source_version, last_event_id
             FROM erpx_tenant.legal_projection_checkpoints
             WHERE holding_ref=$1 AND projection_name=$2
               AND aggregate_type=$3 AND aggregate_id=$4
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(LEGAL_PROJECTION_NAME)
        .bind(event.aggregate_type.as_str())
        .bind(event.aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = row {
            let current = as_u64(row.try_get("source_version")?, "Legal checkpoint version")?;
            let last_event_id: Uuid = row.try_get("last_event_id")?;
            if event.aggregate_version <= current {
                if event.aggregate_version == current && event.event_id != last_event_id {
                    return Err(invalid(
                        "Legal checkpoint version is bound to another event",
                    ));
                }
                return Err(invalid("Legal checkpoint is missing its event receipt"));
            }
            if event.aggregate_version != current.saturating_add(1) {
                return Err(invalid("Legal aggregate checkpoint is not contiguous"));
            }
        } else if event.aggregate_version != 1 {
            return Err(invalid(
                "Legal projection must start at aggregate version one",
            ));
        }
        Ok(())
    }

    /// The typed source event is supplied across a worker boundary. Compare
    /// its complete envelope and payload with the immutable outbox row before
    /// inserting a read-side receipt or any normalized legal row.
    async fn verify_authoritative_source_event(
        &self,
        event: &LegalSourceEvent,
    ) -> Result<(), PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        verify_authoritative_source_event_in_transaction(&mut tx, &self.holding_ref, event).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn clear_projection_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<LegalProjectionRebuildPreparation, PersistenceError> {
    let mut rows_cleared = 0_u64;
    for table in [
        "legal_access_history_read_rows",
        "legal_document_version_read_rows",
        "legal_document_read_rows",
        "legal_contract_approval_read_rows",
        "legal_contract_retention_read_rows",
        "legal_contract_party_read_rows",
        "legal_contract_read_rows",
    ] {
        rows_cleared = rows_cleared.saturating_add(
            sqlx::query(&format!(
                "DELETE FROM erpx_tenant.{table} WHERE holding_ref=$1"
            ))
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected(),
        );
    }
    let checkpoints_cleared =
        sqlx::query("DELETE FROM erpx_tenant.legal_projection_checkpoints WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let receipts_cleared =
        sqlx::query("DELETE FROM erpx_tenant.legal_projection_event_receipts WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    Ok(LegalProjectionRebuildPreparation {
        holding_ref: holding_ref.into(),
        rows_cleared,
        checkpoints_cleared,
        receipts_cleared,
    })
}

async fn read_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&LegalSourceEventCursor>,
    page_size: u32,
) -> Result<LegalSourceEventPage, PersistenceError> {
    validate_source_page_size(page_size)?;
    if let Some(cursor) = cursor {
        validate_source_cursor(cursor)?;
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, occurred_at, contract_version,
                    event_version, tenant_scope, actor, correlation_id,
                    causation_id, idempotency_key, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
               AND (aggregate_type > $3
                    OR (aggregate_type=$3 AND aggregate_id > $4)
                    OR (aggregate_type=$3 AND aggregate_id=$4
                        AND aggregate_version > $5))
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $6",
        )
        .bind(holding_ref)
        .bind(LEGAL_AGGREGATES)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Legal source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, occurred_at, contract_version,
                    event_version, tenant_scope, actor, correlation_id,
                    causation_id, idempotency_key, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type = ANY($2)
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $3",
        )
        .bind(holding_ref)
        .bind(LEGAL_AGGREGATES)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size)
        .map_err(|_| invalid("Legal source page size is not representable"))?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(decode_source_event)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| LegalSourceEventCursor {
            aggregate_type: event.aggregate_type.as_str().into(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(LegalSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type = ANY($2)",
    )
    .bind(holding_ref)
    .bind(LEGAL_AGGREGATES)
    .fetch_one(&mut **tx)
    .await?;
    let count = as_u64(count, "Legal source event count")?;
    if count > MAX_LEGAL_PROJECTION_EVENTS {
        return Err(invalid("Legal projection source event bound exceeded"));
    }
    Ok(count)
}

async fn verify_authoritative_source_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &LegalSourceEvent,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                occurred_at, contract_version, event_version, tenant_scope,
                actor, correlation_id, causation_id, idempotency_key, payload
         FROM erpx_tenant.outbox_events
         WHERE event_id=$1 AND holding_ref=$2
         FOR SHARE",
    )
    .bind(event.event_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(invalid("Legal projection event is not authoritative"));
    };
    let source_event_type: String = row.try_get("event_type")?;
    let source_aggregate_type: String = row.try_get("aggregate_type")?;
    let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let source_version = as_u64(row.try_get("aggregate_version")?, "Legal source version")?;
    let source_occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
    let source_contract_version: Option<i32> = row.try_get("contract_version")?;
    let source_event_version: Option<i32> = row.try_get("event_version")?;
    let source_tenant_scope: Option<Value> = row.try_get("tenant_scope")?;
    let source_actor: Option<Value> = row.try_get("actor")?;
    let source_correlation_id: Option<Uuid> = row.try_get("correlation_id")?;
    let source_causation_id: Option<Uuid> = row.try_get("causation_id")?;
    let source_idempotency_key: Option<String> = row.try_get("idempotency_key")?;
    let source_payload: Value = row.try_get("payload")?;
    if source_event_type != event.event_type.as_str()
        || source_aggregate_type != event.aggregate_type.as_str()
        || source_aggregate_id != event.aggregate_id
        || source_version != event.aggregate_version
        || source_occurred_at != event.occurred_at
        || source_contract_version != Some(i32::from(event.contract_version))
        || source_event_version != Some(i32::from(event.event_version))
        || source_tenant_scope != Some(serde_json::to_value(&event.tenant_scope)?)
        || source_actor != Some(serde_json::to_value(&event.actor)?)
        || source_correlation_id != Some(event.correlation_id)
        || source_causation_id != event.causation_id
        || source_idempotency_key != Some(event.idempotency_key.clone())
        || source_payload != serde_json::to_value(&event.payload)?
    {
        return Err(invalid(
            "Legal projection event does not match the authoritative outbox",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SqlxLegalReadStore {
    read_pool: PgPool,
    source_pool: Option<PgPool>,
    holding_ref: String,
}

impl SqlxLegalReadStore {
    pub fn new(
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            read_pool,
            source_pool: None,
            holding_ref,
        })
    }

    /// Construct a read adapter with access to the authoritative source pool.
    /// Freshness is fail-closed when the source pool is not supplied; callers
    /// that need a positive freshness claim must use this constructor.
    pub fn from_pools(
        read_pool: &PgPool,
        source_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            read_pool: read_pool.clone(),
            source_pool: Some(source_pool.clone()),
            holding_ref,
        })
    }

    pub async fn list_contracts(
        &self,
        scope: LegalReadScope,
        page_size: u32,
        before: Option<LegalContractReadCursor>,
    ) -> Result<LegalContractReadPage, PersistenceError> {
        validate_scope(scope)?;
        validate_read_page_size(page_size)?;
        if before.is_some_and(|cursor| cursor.contract_id.is_nil()) {
            return Err(invalid("Legal contract read cursor is invalid"));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT contract_id, holding_id, company_id, branch_id, reference, title,
                    object_ref, content_sha256, retention_until, status, created_by,
                    approved_by, source_version, source_event_id,
                    source_contract_version, source_event_version, source_actor_ref,
                    source_actor_kind, source_correlation_id, source_causation_id,
                    source_idempotency_key, projected_at
             FROM erpx_tenant.legal_contract_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::UUID IS NULL OR contract_id < $5)
             ORDER BY contract_id DESC LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.contract_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = usize::try_from(page_size)
            .map_err(|_| invalid("Legal contract page size is not representable"))?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut result = Vec::with_capacity(selected.len());
        for row in selected {
            result.push(load_contract_row(&mut tx, &self.holding_ref, row).await?);
        }
        let freshness =
            read_freshness(&mut tx, &self.holding_ref, self.source_pool.as_ref()).await?;
        tx.commit().await?;
        let next_cursor = if has_more {
            result.last().map(|row| LegalContractReadCursor {
                contract_id: row.contract_id,
            })
        } else {
            None
        };
        Ok(LegalContractReadPage {
            rows: result,
            next_cursor,
            has_more,
            freshness,
        })
    }

    pub async fn list_documents(
        &self,
        scope: LegalReadScope,
        page_size: u32,
        before: Option<LegalDocumentReadCursor>,
    ) -> Result<LegalDocumentReadPage, PersistenceError> {
        validate_scope(scope)?;
        validate_read_page_size(page_size)?;
        if before.is_some_and(|cursor| cursor.document_id.is_nil()) {
            return Err(invalid("Legal document read cursor is invalid"));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT document_id, holding_id, company_id, branch_id, title,
                    current_version, archived, source_version, source_event_id,
                    source_contract_version, source_event_version, source_actor_ref,
                    source_actor_kind, source_correlation_id, source_causation_id,
                    source_idempotency_key, projected_at
             FROM erpx_tenant.legal_document_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::UUID IS NULL OR document_id < $5)
             ORDER BY document_id DESC LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.document_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = usize::try_from(page_size)
            .map_err(|_| invalid("Legal document page size is not representable"))?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut result = Vec::with_capacity(selected.len());
        for row in selected {
            result.push(load_document_row(&mut tx, &self.holding_ref, row).await?);
        }
        let freshness =
            read_freshness(&mut tx, &self.holding_ref, self.source_pool.as_ref()).await?;
        tx.commit().await?;
        let next_cursor = if has_more {
            result.last().map(|row| LegalDocumentReadCursor {
                document_id: row.document_id,
            })
        } else {
            None
        };
        Ok(LegalDocumentReadPage {
            rows: result,
            next_cursor,
            has_more,
            freshness,
        })
    }

    pub async fn list_access_history(
        &self,
        scope: LegalReadScope,
        document_id: Option<Uuid>,
        page_size: u32,
        before: Option<LegalAccessReadCursor>,
    ) -> Result<LegalAccessHistoryReadPage, PersistenceError> {
        validate_scope(scope)?;
        validate_read_page_size(page_size)?;
        if document_id.is_some_and(|value| value.is_nil()) {
            return Err(invalid("Legal access document ID is invalid"));
        }
        if before.is_some_and(|cursor| cursor.entry_id.is_nil()) {
            return Err(invalid("Legal access read cursor is invalid"));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT entry_id, document_id, holding_id, company_id, branch_id,
                    document_version, actor_ref, action, purpose, observed_at,
                    source_version, source_event_id, source_contract_version,
                    source_event_version, source_actor_ref, source_actor_kind,
                    source_correlation_id, source_causation_id, source_idempotency_key,
                    projected_at
             FROM erpx_tenant.legal_access_history_read_rows
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::UUID IS NULL OR document_id=$5)
               AND ($6::TIMESTAMPTZ IS NULL OR (observed_at, entry_id) < ($6, $7))
             ORDER BY observed_at DESC, entry_id DESC LIMIT $8",
        )
        .bind(&self.holding_ref)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(document_id)
        .bind(before.map(|cursor| cursor.observed_at))
        .bind(before.map(|cursor| cursor.entry_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = usize::try_from(page_size)
            .map_err(|_| invalid("Legal access page size is not representable"))?;
        let has_more = rows.len() > page_len;
        let mut result = Vec::with_capacity(page_len.min(rows.len()));
        for row in rows.into_iter().take(page_len) {
            let row = decode_access_read_row(row)?;
            validate_access_read_row(&mut tx, &self.holding_ref, &row).await?;
            result.push(row);
        }
        let freshness =
            read_freshness(&mut tx, &self.holding_ref, self.source_pool.as_ref()).await?;
        tx.commit().await?;
        let next_cursor = if has_more {
            result.last().map(|row| LegalAccessReadCursor {
                observed_at: row.observed_at,
                entry_id: row.entry_id,
            })
        } else {
            None
        };
        Ok(LegalAccessHistoryReadPage {
            rows: result,
            next_cursor,
            has_more,
            freshness,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegalWatermark {
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LegalSourceWatermark {
    event_count: u64,
    latest: Option<LegalWatermark>,
}

async fn read_freshness(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    source_pool: Option<&PgPool>,
) -> Result<LegalReadFreshness, PersistenceError> {
    let row = sqlx::query(
        "SELECT aggregate_type, aggregate_id, last_event_id, projected_at, source_version
         FROM erpx_tenant.legal_projection_checkpoints
         WHERE holding_ref=$1 AND projection_name=$2
         ORDER BY aggregate_type DESC, aggregate_id DESC, source_version DESC,
                  last_event_id DESC
         LIMIT 1",
    )
    .bind(holding_ref)
    .bind(LEGAL_PROJECTION_NAME)
    .fetch_optional(&mut **tx)
    .await?;
    let projected_latest = row
        .as_ref()
        .map(|row| {
            Ok::<LegalWatermark, PersistenceError>(LegalWatermark {
                aggregate_type: row.try_get("aggregate_type")?,
                aggregate_id: row.try_get("aggregate_id")?,
                aggregate_version: as_u64(
                    row.try_get("source_version")?,
                    "Legal freshness source version",
                )?,
                event_id: row.try_get("last_event_id")?,
            })
        })
        .transpose()?;
    let last_event_id = row
        .as_ref()
        .map(|value| value.try_get("last_event_id"))
        .transpose()?;
    let last_projected_at = row
        .as_ref()
        .map(|value| value.try_get("projected_at"))
        .transpose()?;
    let lag_seconds = last_projected_at
        .map(|projected_at: DateTime<Utc>| {
            Utc::now()
                .signed_duration_since(projected_at)
                .num_seconds()
                .max(0) as u64
        })
        .unwrap_or(0);
    let projected_event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_tenant.legal_projection_event_receipts
         WHERE holding_ref=$1 AND projection_name=$2",
    )
    .bind(holding_ref)
    .bind(LEGAL_PROJECTION_NAME)
    .fetch_one(&mut **tx)
    .await?;
    let projected_event_count = as_u64(projected_event_count, "Legal projected event count")?;

    let Some(source_pool) = source_pool else {
        return Ok(LegalReadFreshness {
            last_event_id,
            last_projected_at,
            source_aggregate_version: projected_latest.map(|value| value.aggregate_version),
            lag_seconds,
            stale: true,
        });
    };
    let source_watermark = read_source_watermark(source_pool, holding_ref).await?;
    let watermark_matches = source_watermark.event_count == projected_event_count
        && source_watermark.latest.as_ref() == projected_latest.as_ref();
    Ok(LegalReadFreshness {
        last_event_id,
        last_projected_at,
        source_aggregate_version: source_watermark
            .latest
            .as_ref()
            .map(|value| value.aggregate_version),
        lag_seconds,
        stale: lag_seconds > LEGAL_READ_STALE_AFTER_SECONDS || !watermark_matches,
    })
}

async fn read_source_watermark(
    source_pool: &PgPool,
    holding_ref: &str,
) -> Result<LegalSourceWatermark, PersistenceError> {
    let mut tx = begin_scoped(source_pool, holding_ref).await?;
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type = ANY($2)",
    )
    .bind(holding_ref)
    .bind(LEGAL_AGGREGATES)
    .fetch_one(&mut *tx)
    .await?;
    let latest = sqlx::query(
        "SELECT aggregate_type, aggregate_id, aggregate_version, event_id
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type = ANY($2)
         ORDER BY aggregate_type DESC, aggregate_id DESC, aggregate_version DESC,
                  event_id DESC
         LIMIT 1",
    )
    .bind(holding_ref)
    .bind(LEGAL_AGGREGATES)
    .fetch_optional(&mut *tx)
    .await?
    .map(|row| {
        Ok::<LegalWatermark, PersistenceError>(LegalWatermark {
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            aggregate_version: as_u64(
                row.try_get("aggregate_version")?,
                "Legal source watermark version",
            )?,
            event_id: row.try_get("event_id")?,
        })
    })
    .transpose()?;
    tx.commit().await?;
    let event_count = as_u64(event_count, "Legal source event count")?;
    if event_count > MAX_LEGAL_PROJECTION_EVENTS {
        return Err(invalid("Legal projection source event bound exceeded"));
    }
    Ok(LegalSourceWatermark {
        event_count,
        latest,
    })
}

async fn project_contract(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &LegalSourceEvent,
    contract: &LegalContractEvent,
) -> Result<(), PersistenceError> {
    validate_contract_payload(event, contract)?;
    let source = event_source(event);
    validate_existing_scope(
        tx,
        "legal_contract_read_rows",
        "contract_id",
        contract.contract_id,
        holding_ref,
        contract.holding_id,
        contract.company_id,
        contract.branch_id,
        "Legal contract",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.legal_contract_read_rows
            (holding_ref, contract_id, holding_id, company_id, branch_id, reference,
             title, object_ref, content_sha256, retention_until, status, created_by,
             approved_by, source_version, source_event_id, source_contract_version,
             source_event_version, source_actor_ref, source_actor_kind,
             source_correlation_id, source_causation_id, source_idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22)
         ON CONFLICT (holding_ref, contract_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, reference=EXCLUDED.reference,
             title=EXCLUDED.title, object_ref=EXCLUDED.object_ref,
             content_sha256=EXCLUDED.content_sha256,
             retention_until=EXCLUDED.retention_until, status=EXCLUDED.status,
             created_by=EXCLUDED.created_by, approved_by=EXCLUDED.approved_by,
             source_version=EXCLUDED.source_version, source_event_id=EXCLUDED.source_event_id,
             source_contract_version=EXCLUDED.source_contract_version,
             source_event_version=EXCLUDED.source_event_version,
             source_actor_ref=EXCLUDED.source_actor_ref, source_actor_kind=EXCLUDED.source_actor_kind,
             source_correlation_id=EXCLUDED.source_correlation_id,
             source_causation_id=EXCLUDED.source_causation_id,
             source_idempotency_key=EXCLUDED.source_idempotency_key, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(contract.contract_id)
    .bind(contract.holding_id)
    .bind(contract.company_id)
    .bind(contract.branch_id)
    .bind(&contract.reference)
    .bind(&contract.title)
    .bind(&contract.object_ref)
    .bind(&contract.content_sha256)
    .bind(contract.retention_until)
    .bind(contract_status(contract.status))
    .bind(&contract.created_by)
    .bind(&contract.approved_by)
    .bind(as_i64(event.aggregate_version, "Legal source version")?)
    .bind(event.event_id)
    .bind(i32::from(source.contract_version))
    .bind(i32::from(source.event_version))
    .bind(&source.actor_ref)
    .bind(actor_kind_name(&source.actor_kind))
    .bind(source.correlation_id)
    .bind(source.causation_id)
    .bind(&source.idempotency_key)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "DELETE FROM erpx_tenant.legal_contract_party_read_rows
         WHERE holding_ref=$1 AND contract_id=$2",
    )
    .bind(holding_ref)
    .bind(contract.contract_id)
    .execute(&mut **tx)
    .await?;
    for party in &contract.parties {
        sqlx::query(
            "INSERT INTO erpx_tenant.legal_contract_party_read_rows
                (holding_ref, contract_id, holding_id, party_id, party_role,
                 source_version, source_event_id, source_contract_version,
                 source_event_version, source_actor_ref, source_actor_kind,
                 source_correlation_id, source_causation_id, source_idempotency_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(holding_ref)
        .bind(contract.contract_id)
        .bind(contract.holding_id)
        .bind(party.party_id)
        .bind(&party.role)
        .bind(as_i64(event.aggregate_version, "Legal source version")?)
        .bind(event.event_id)
        .bind(i32::from(source.contract_version))
        .bind(i32::from(source.event_version))
        .bind(&source.actor_ref)
        .bind(actor_kind_name(&source.actor_kind))
        .bind(source.correlation_id)
        .bind(source.causation_id)
        .bind(&source.idempotency_key)
        .execute(&mut **tx)
        .await?;
    }
    if let Some(approved_by) = &contract.approved_by {
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.legal_contract_approval_read_rows
                (holding_ref, contract_id, holding_id, approved_by, approved_at,
                 contract_version, source_event_id, source_contract_version,
                 source_event_version, source_actor_ref, source_actor_kind,
                 source_correlation_id, source_causation_id, source_idempotency_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (holding_ref, contract_id) DO NOTHING",
        )
        .bind(holding_ref)
        .bind(contract.contract_id)
        .bind(contract.holding_id)
        .bind(approved_by)
        .bind(event.occurred_at)
        .bind(as_i64(event.aggregate_version, "Legal contract version")?)
        .bind(event.event_id)
        .bind(i32::from(source.contract_version))
        .bind(i32::from(source.event_version))
        .bind(&source.actor_ref)
        .bind(actor_kind_name(&source.actor_kind))
        .bind(source.correlation_id)
        .bind(source.causation_id)
        .bind(&source.idempotency_key)
        .execute(&mut **tx)
        .await?;
        if inserted.rows_affected() == 0 {
            let row = sqlx::query(
                "SELECT holding_id, approved_by
                 FROM erpx_tenant.legal_contract_approval_read_rows
                 WHERE holding_ref=$1 AND contract_id=$2",
            )
            .bind(holding_ref)
            .bind(contract.contract_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| invalid("Legal contract approval read row disappeared"))?;
            if row.try_get::<Uuid, _>("holding_id")? != contract.holding_id
                || row.try_get::<String, _>("approved_by")? != *approved_by
            {
                return Err(invalid("Legal contract approval cannot be reassigned"));
            }
        }
    } else {
        let approval_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_tenant.legal_contract_approval_read_rows
                 WHERE holding_ref=$1 AND contract_id=$2
             )",
        )
        .bind(holding_ref)
        .bind(contract.contract_id)
        .fetch_one(&mut **tx)
        .await?;
        if approval_exists {
            return Err(invalid("Legal contract approval cannot be removed"));
        }
    }
    sqlx::query(
        "INSERT INTO erpx_tenant.legal_contract_retention_read_rows
            (holding_ref, contract_id, holding_id, retention_until, retention_state,
             contract_version, source_event_id, source_contract_version,
             source_event_version, source_actor_ref, source_actor_kind,
             source_correlation_id, source_causation_id, source_idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
         ON CONFLICT (holding_ref, contract_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, retention_until=EXCLUDED.retention_until,
             retention_state=EXCLUDED.retention_state, contract_version=EXCLUDED.contract_version,
             source_event_id=EXCLUDED.source_event_id,
             source_contract_version=EXCLUDED.source_contract_version,
             source_event_version=EXCLUDED.source_event_version,
             source_actor_ref=EXCLUDED.source_actor_ref, source_actor_kind=EXCLUDED.source_actor_kind,
             source_correlation_id=EXCLUDED.source_correlation_id,
             source_causation_id=EXCLUDED.source_causation_id,
             source_idempotency_key=EXCLUDED.source_idempotency_key, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(contract.contract_id)
    .bind(contract.holding_id)
    .bind(contract.retention_until)
    .bind(retention_state(contract.retention_until))
    .bind(as_i64(event.aggregate_version, "Legal contract version")?)
    .bind(event.event_id)
    .bind(i32::from(source.contract_version))
    .bind(i32::from(source.event_version))
    .bind(&source.actor_ref)
    .bind(actor_kind_name(&source.actor_kind))
    .bind(source.correlation_id)
    .bind(source.causation_id)
    .bind(&source.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn project_document(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &LegalSourceEvent,
    document: &LegalDocumentEvent,
) -> Result<(), PersistenceError> {
    validate_document_payload(event, document)?;
    let source = event_source(event);
    validate_existing_scope(
        tx,
        "legal_document_read_rows",
        "document_id",
        document.document_id,
        holding_ref,
        document.holding_id,
        document.company_id,
        document.branch_id,
        "Legal document",
    )
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.legal_document_read_rows
            (holding_ref, document_id, holding_id, company_id, branch_id, title,
             current_version, archived, source_version, source_event_id,
             source_contract_version, source_event_version, source_actor_ref,
             source_actor_kind, source_correlation_id, source_causation_id,
             source_idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
         ON CONFLICT (holding_ref, document_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, title=EXCLUDED.title,
             current_version=EXCLUDED.current_version, archived=EXCLUDED.archived,
             source_version=EXCLUDED.source_version, source_event_id=EXCLUDED.source_event_id,
             source_contract_version=EXCLUDED.source_contract_version,
             source_event_version=EXCLUDED.source_event_version,
             source_actor_ref=EXCLUDED.source_actor_ref, source_actor_kind=EXCLUDED.source_actor_kind,
             source_correlation_id=EXCLUDED.source_correlation_id,
             source_causation_id=EXCLUDED.source_causation_id,
             source_idempotency_key=EXCLUDED.source_idempotency_key, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(document.document_id)
    .bind(document.holding_id)
    .bind(document.company_id)
    .bind(document.branch_id)
    .bind(&document.title)
    .bind(as_i64(document.current_version, "Legal document current version")?)
    .bind(document.archived)
    .bind(as_i64(event.aggregate_version, "Legal source version")?)
    .bind(event.event_id)
    .bind(i32::from(source.contract_version))
    .bind(i32::from(source.event_version))
    .bind(&source.actor_ref)
    .bind(actor_kind_name(&source.actor_kind))
    .bind(source.correlation_id)
    .bind(source.causation_id)
    .bind(&source.idempotency_key)
    .execute(&mut **tx)
    .await?;

    for version in &document.versions {
        sqlx::query(
            "INSERT INTO erpx_tenant.legal_document_version_read_rows
                (holding_ref, document_id, holding_id, version, object_ref,
                 content_sha256, created_at, created_by, source_version,
                 source_event_id, source_contract_version, source_event_version,
                 source_actor_ref, source_actor_kind, source_correlation_id,
                 source_causation_id, source_idempotency_key)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
             ON CONFLICT (holding_ref, document_id, version) DO NOTHING",
        )
        .bind(holding_ref)
        .bind(document.document_id)
        .bind(document.holding_id)
        .bind(as_i64(version.version, "Legal document version")?)
        .bind(&version.object_ref)
        .bind(&version.content_sha256)
        .bind(version.created_at)
        .bind(&version.created_by)
        .bind(as_i64(event.aggregate_version, "Legal source version")?)
        .bind(event.event_id)
        .bind(i32::from(source.contract_version))
        .bind(i32::from(source.event_version))
        .bind(&source.actor_ref)
        .bind(actor_kind_name(&source.actor_kind))
        .bind(source.correlation_id)
        .bind(source.causation_id)
        .bind(&source.idempotency_key)
        .execute(&mut **tx)
        .await?;
        let row = sqlx::query(
            "SELECT holding_id, object_ref, content_sha256, created_at, created_by
             FROM erpx_tenant.legal_document_version_read_rows
             WHERE holding_ref=$1 AND document_id=$2 AND version=$3",
        )
        .bind(holding_ref)
        .bind(document.document_id)
        .bind(as_i64(version.version, "Legal document version")?)
        .fetch_one(&mut **tx)
        .await?;
        if row.try_get::<Uuid, _>("holding_id")? != document.holding_id
            || row.try_get::<String, _>("object_ref")? != version.object_ref
            || row.try_get::<String, _>("content_sha256")? != version.content_sha256
            || row.try_get::<DateTime<Utc>, _>("created_at")? != version.created_at
            || row.try_get::<String, _>("created_by")? != version.created_by
        {
            return Err(invalid("Legal document version is immutable"));
        }
    }
    Ok(())
}

async fn project_access(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &LegalSourceEvent,
    access: &LegalAccessHistoryEvent,
) -> Result<(), PersistenceError> {
    validate_access_payload(event, access)?;
    let source = event_source(event);
    sqlx::query(
        "INSERT INTO erpx_tenant.legal_access_history_read_rows
            (holding_ref, entry_id, document_id, holding_id, company_id, branch_id,
             document_version, actor_ref, action, purpose, observed_at,
             source_version, source_event_id, source_contract_version,
             source_event_version, source_actor_ref, source_actor_kind,
             source_correlation_id, source_causation_id, source_idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
         ON CONFLICT (holding_ref, entry_id) DO NOTHING",
    )
    .bind(holding_ref)
    .bind(access.entry_id)
    .bind(access.document_id)
    .bind(access.holding_id)
    .bind(access.company_id)
    .bind(access.branch_id)
    .bind(as_i64(access.version, "Legal access document version")?)
    .bind(&access.actor_ref)
    .bind(&access.action)
    .bind(&access.purpose)
    .bind(access.observed_at)
    .bind(as_i64(event.aggregate_version, "Legal source version")?)
    .bind(event.event_id)
    .bind(i32::from(source.contract_version))
    .bind(i32::from(source.event_version))
    .bind(&source.actor_ref)
    .bind(actor_kind_name(&source.actor_kind))
    .bind(source.correlation_id)
    .bind(source.causation_id)
    .bind(&source.idempotency_key)
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(
        "SELECT document_id, holding_id, company_id, branch_id, document_version,
                actor_ref, action, purpose, observed_at
         FROM erpx_tenant.legal_access_history_read_rows
         WHERE holding_ref=$1 AND entry_id=$2",
    )
    .bind(holding_ref)
    .bind(access.entry_id)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<Uuid, _>("document_id")? != access.document_id
        || row.try_get::<Uuid, _>("holding_id")? != access.holding_id
        || row.try_get::<Uuid, _>("company_id")? != access.company_id
        || row.try_get::<Option<Uuid>, _>("branch_id")? != access.branch_id
        || as_u64(row.try_get("document_version")?, "Legal access version")? != access.version
        || row.try_get::<String, _>("actor_ref")? != access.actor_ref
        || row.try_get::<String, _>("action")? != access.action
        || row.try_get::<String, _>("purpose")? != access.purpose
        || row.try_get::<DateTime<Utc>, _>("observed_at")? != access.observed_at
    {
        return Err(invalid("Legal access history is immutable"));
    }
    Ok(())
}

/// Access events may sort before document events because source cursors are
/// aggregate-key ordered. Validate the immutable reference against the
/// authoritative write model before materializing the access fact; the read
/// API separately refuses to serve it until the referenced document is also
/// present in the disposable read model.
async fn validate_access_source_document(
    write_pool: &PgPool,
    holding_ref: &str,
    access: &LegalAccessHistoryEvent,
) -> Result<(), PersistenceError> {
    let mut source_tx = begin_scoped(write_pool, holding_ref).await?;
    let document = sqlx::query(
        "SELECT holding_id, company_id, branch_id, current_version
         FROM erpx_tenant.legal_documents
         WHERE holding_ref=$1 AND document_id=$2 FOR SHARE",
    )
    .bind(holding_ref)
    .bind(access.document_id)
    .fetch_optional(&mut *source_tx)
    .await?
    .ok_or_else(|| invalid("Legal access source document is missing"))?;
    if document.try_get::<Uuid, _>("holding_id")? != access.holding_id
        || document.try_get::<Uuid, _>("company_id")? != access.company_id
        || document.try_get::<Option<Uuid>, _>("branch_id")? != access.branch_id
        || access.version
            > as_u64(
                document.try_get("current_version")?,
                "Legal access source document version",
            )?
    {
        return Err(invalid(
            "Legal access source document scope or version is inconsistent",
        ));
    }
    let version_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_tenant.legal_document_versions
             WHERE holding_ref=$1 AND document_id=$2 AND version=$3
         )",
    )
    .bind(holding_ref)
    .bind(access.document_id)
    .bind(as_i64(
        access.version,
        "Legal access source document version",
    )?)
    .fetch_one(&mut *source_tx)
    .await?;
    if !version_exists {
        return Err(invalid("Legal access source document version is missing"));
    }
    source_tx.commit().await?;
    Ok(())
}

fn ensure_source_metadata_match(
    expected: &LegalSourceMetadata,
    actual: &LegalSourceMetadata,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    if expected != actual {
        return Err(invalid(format!("{entity} source metadata is inconsistent")));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn validate_existing_scope(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    id_column: &str,
    aggregate_id: Uuid,
    holding_ref: &str,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    let query = format!(
        "SELECT holding_id, company_id, branch_id
         FROM erpx_tenant.{table}
         WHERE holding_ref=$1 AND {id_column}=$2 FOR SHARE"
    );
    let row = sqlx::query(&query)
        .bind(holding_ref)
        .bind(aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = row
        && (row.try_get::<Uuid, _>("holding_id")? != holding_id
            || row.try_get::<Uuid, _>("company_id")? != company_id
            || row.try_get::<Option<Uuid>, _>("branch_id")? != branch_id)
    {
        return Err(invalid(format!("{entity} scope cannot be rebound")));
    }
    Ok(())
}

async fn validate_access_read_row(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    access: &LegalAccessHistoryReadRow,
) -> Result<(), PersistenceError> {
    validate_uuid(access.entry_id, "Legal access read entry ID")?;
    validate_uuid(access.document_id, "Legal access read document ID")?;
    validate_uuid(access.holding_id, "Legal access read holding ID")?;
    validate_uuid(access.company_id, "Legal access read company ID")?;
    if access.branch_id.is_some_and(|value| value.is_nil()) {
        return Err(invalid("Legal access read branch ID is invalid"));
    }
    validate_bounded(&access.actor_ref, 255, "Legal access read actor")?;
    validate_bounded(&access.action, 255, "Legal access read action")?;
    validate_bounded(&access.purpose, 512, "Legal access read purpose")?;
    if access.source.actor_ref != access.actor_ref {
        return Err(invalid(
            "Legal access read actor source binding is inconsistent",
        ));
    }
    let document = sqlx::query(
        "SELECT holding_id, company_id, branch_id, current_version
         FROM erpx_tenant.legal_document_read_rows
         WHERE holding_ref=$1 AND document_id=$2",
    )
    .bind(holding_ref)
    .bind(access.document_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Legal access read document is missing"))?;
    if document.try_get::<Uuid, _>("holding_id")? != access.holding_id
        || document.try_get::<Uuid, _>("company_id")? != access.company_id
        || document.try_get::<Option<Uuid>, _>("branch_id")? != access.branch_id
        || access.document_version
            > as_u64(
                document.try_get("current_version")?,
                "Legal access read document version",
            )?
    {
        return Err(invalid("Legal access read document scope is inconsistent"));
    }
    let version_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_tenant.legal_document_version_read_rows
             WHERE holding_ref=$1 AND document_id=$2 AND version=$3
         )",
    )
    .bind(holding_ref)
    .bind(access.document_id)
    .bind(as_i64(
        access.document_version,
        "Legal access read document version",
    )?)
    .fetch_one(&mut **tx)
    .await?;
    if !version_exists {
        return Err(invalid("Legal access read document version is missing"));
    }
    Ok(())
}

async fn load_contract_row(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    row: sqlx::postgres::PgRow,
) -> Result<LegalContractReadRow, PersistenceError> {
    let contract_id: Uuid = row.try_get("contract_id")?;
    let holding_id: Uuid = row.try_get("holding_id")?;
    let company_id: Uuid = row.try_get("company_id")?;
    let branch_id: Option<Uuid> = row.try_get("branch_id")?;
    validate_uuid(holding_id, "Legal contract read holding ID")?;
    validate_uuid(company_id, "Legal contract read company ID")?;
    if branch_id.is_some_and(|value| value.is_nil()) {
        return Err(invalid("Legal contract read branch ID is invalid"));
    }
    let source_version = as_u64(
        row.try_get("source_version")?,
        "Legal contract source version",
    )?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    validate_uuid(source_event_id, "Legal contract source event ID")?;
    let source = decode_source_metadata(&row)?;
    let parties = sqlx::query(
        "SELECT party_id, holding_id, party_role, source_version, source_event_id,
                source_contract_version, source_event_version, source_actor_ref,
                source_actor_kind, source_correlation_id, source_causation_id,
                source_idempotency_key, projected_at
         FROM erpx_tenant.legal_contract_party_read_rows
         WHERE holding_ref=$1 AND contract_id=$2
         ORDER BY party_id LIMIT $3",
    )
    .bind(holding_ref)
    .bind(contract_id)
    .bind(i64::from(MAX_LEGAL_READ_CHILD_ROWS) + 1)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(decode_party_read_row)
    .collect::<Result<Vec<_>, _>>()?;
    if parties.len() > MAX_LEGAL_READ_CHILD_ROWS as usize {
        return Err(invalid("Legal contract party read limit exceeded"));
    }
    for party in &parties {
        if party.holding_id != holding_id || party.source_version != source_version {
            return Err(invalid(
                "Legal contract party scope or version is inconsistent",
            ));
        }
        if party.source_event_id != source_event_id {
            return Err(invalid("Legal contract party source event is inconsistent"));
        }
        ensure_source_metadata_match(&source, &party.source, "Legal contract party")?;
    }
    let approval = sqlx::query(
        "SELECT holding_id, approved_by, approved_at, contract_version, source_event_id,
                source_contract_version, source_event_version, source_actor_ref,
                source_actor_kind, source_correlation_id, source_causation_id,
                source_idempotency_key, projected_at
         FROM erpx_tenant.legal_contract_approval_read_rows
         WHERE holding_ref=$1 AND contract_id=$2",
    )
    .bind(holding_ref)
    .bind(contract_id)
    .fetch_optional(&mut **tx)
    .await?
    .map(decode_approval_read_row)
    .transpose()?;
    if let Some(approval) = &approval {
        if approval.holding_id != holding_id
            || approval.approved_by
                != row
                    .try_get::<Option<String>, _>("approved_by")?
                    .as_deref()
                    .unwrap_or_default()
            || approval.contract_version == 0
            || approval.contract_version > source_version
            || approval.source_event_id.is_nil()
            || approval.source.actor_ref != approval.approved_by
        {
            return Err(invalid("Legal contract approval binding is inconsistent"));
        }
    } else if row.try_get::<Option<String>, _>("approved_by")?.is_some() {
        return Err(invalid("Legal contract approval read row is missing"));
    }
    let retention = sqlx::query(
        "SELECT holding_id, retention_until, retention_state, contract_version, source_event_id,
                source_contract_version, source_event_version, source_actor_ref,
                source_actor_kind, source_correlation_id, source_causation_id,
                source_idempotency_key, projected_at
         FROM erpx_tenant.legal_contract_retention_read_rows
         WHERE holding_ref=$1 AND contract_id=$2",
    )
    .bind(holding_ref)
    .bind(contract_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(PersistenceError::from)
    .and_then(decode_retention_read_row)?;
    let source_retention_until: DateTime<Utc> = row.try_get("retention_until")?;
    if retention.holding_id != holding_id
        || retention.retention_until != source_retention_until
        || retention.contract_version != source_version
        || retention.source_event_id != source_event_id
    {
        return Err(invalid("Legal contract retention binding is inconsistent"));
    }
    ensure_source_metadata_match(&source, &retention.source, "Legal contract retention")?;
    Ok(LegalContractReadRow {
        contract_id,
        holding_id,
        company_id,
        branch_id,
        reference: row.try_get("reference")?,
        title: row.try_get("title")?,
        object_ref: row.try_get("object_ref")?,
        content_sha256: row.try_get("content_sha256")?,
        retention_until: row.try_get("retention_until")?,
        status: row.try_get("status")?,
        created_by: row.try_get("created_by")?,
        approved_by: row.try_get("approved_by")?,
        source_version,
        source_event_id,
        source,
        projected_at: row.try_get("projected_at")?,
        parties,
        approval,
        retention,
    })
}

async fn load_document_row(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    row: sqlx::postgres::PgRow,
) -> Result<LegalDocumentReadRow, PersistenceError> {
    let document_id: Uuid = row.try_get("document_id")?;
    let document_holding_id: Uuid = row.try_get("holding_id")?;
    let document_company_id: Uuid = row.try_get("company_id")?;
    let document_branch_id: Option<Uuid> = row.try_get("branch_id")?;
    let current_version = as_u64(
        row.try_get("current_version")?,
        "Legal document current version",
    )?;
    let source_version = as_u64(
        row.try_get("source_version")?,
        "Legal document source version",
    )?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    let source = decode_source_metadata(&row)?;
    validate_uuid(document_holding_id, "Legal document read holding ID")?;
    validate_uuid(document_company_id, "Legal document read company ID")?;
    validate_uuid(source_event_id, "Legal document source event ID")?;
    if document_branch_id.is_some_and(|value| value.is_nil()) {
        return Err(invalid("Legal document read branch ID is invalid"));
    }
    if current_version > MAX_DOCUMENT_VERSIONS as u64 {
        return Err(invalid("Legal document current version exceeds its bound"));
    }
    let version_rows = sqlx::query(
        "SELECT holding_id, version, object_ref, content_sha256, created_at, created_by,
                source_version, source_event_id, source_contract_version,
                source_event_version, source_actor_ref, source_actor_kind,
                source_correlation_id, source_causation_id, source_idempotency_key,
                projected_at
         FROM erpx_tenant.legal_document_version_read_rows
         WHERE holding_ref=$1 AND document_id=$2
         ORDER BY version LIMIT $3",
    )
    .bind(holding_ref)
    .bind(document_id)
    .bind(i64::from(MAX_DOCUMENT_VERSIONS as u32) + 1)
    .fetch_all(&mut **tx)
    .await?;
    let mut versions = Vec::with_capacity(version_rows.len());
    for version_row in version_rows {
        if version_row.try_get::<Uuid, _>("holding_id")? != document_holding_id {
            return Err(invalid(
                "Legal document version holding binding is inconsistent",
            ));
        }
        let version = decode_document_version_read_row(version_row)?;
        if version.source_version > source_version {
            return Err(invalid(
                "Legal document version source is ahead of its document",
            ));
        }
        versions.push(version);
    }
    if versions.len() != current_version as usize {
        return Err(invalid(
            "Legal document read versions do not match the current version",
        ));
    }
    if versions
        .iter()
        .enumerate()
        .any(|(index, version)| version.version != (index + 1) as u64)
    {
        return Err(invalid("Legal document read versions are not contiguous"));
    }
    Ok(LegalDocumentReadRow {
        document_id,
        holding_id: row.try_get("holding_id")?,
        company_id: document_company_id,
        branch_id: document_branch_id,
        title: row.try_get("title")?,
        current_version,
        archived: row.try_get("archived")?,
        source_version,
        source_event_id,
        source,
        projected_at: row.try_get("projected_at")?,
        versions,
        versions_truncated: false,
    })
}

fn decode_source_event(row: sqlx::postgres::PgRow) -> Result<LegalSourceEvent, PersistenceError> {
    let event_id: Uuid = row.try_get("event_id")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let aggregate_type_text: String = row.try_get("aggregate_type")?;
    let event_type_text: String = row.try_get("event_type")?;
    let contract_version =
        required_positive_u16(row.try_get("contract_version")?, "contract version")?;
    let event_version = required_positive_u16(row.try_get("event_version")?, "event version")?;
    let tenant_value: Value = row.try_get("tenant_scope")?;
    let actor_value: Value = row.try_get("actor")?;
    let tenant_scope: TenantScope = serde_json::from_value(tenant_value)
        .map_err(|_| invalid("Legal source tenant scope is missing or malformed"))?;
    let actor: ActorAttribution = serde_json::from_value(actor_value)
        .map_err(|_| invalid("Legal source actor attribution is missing or malformed"))?;
    let correlation_id: Uuid = row.try_get("correlation_id")?;
    let causation_id: Option<Uuid> = row.try_get("causation_id")?;
    let idempotency_key: String = row
        .try_get::<Option<String>, _>("idempotency_key")?
        .ok_or_else(|| invalid("Legal source idempotency key is missing"))?;
    let payload_value: Value = row.try_get("payload")?;
    let payload: LegalOutboxPayload = serde_json::from_value(payload_value.clone())
        .map_err(|_| invalid("Legal source payload is legacy or untyped"))?;
    let aggregate_type = parse_aggregate_type(&aggregate_type_text)?;
    let event_type = parse_event_type(&event_type_text)?;
    let event = LegalSourceEvent {
        event_id,
        contract_version,
        event_version,
        event_type,
        aggregate_type,
        aggregate_id,
        aggregate_version: as_u64(row.try_get("aggregate_version")?, "Legal source version")?,
        occurred_at: row.try_get("occurred_at")?,
        tenant_scope,
        actor,
        correlation_id,
        causation_id,
        idempotency_key,
        payload,
    };
    // Reconciliation reads the source pages without projecting them. Apply
    // the same semantic legal validation here so a typed-but-corrupt outbox
    // payload cannot be counted as authoritative merely because its envelope
    // is well formed.
    validate_source_event(&event)?;
    Ok(event)
}

fn validate_source_event(event: &LegalSourceEvent) -> Result<(), PersistenceError> {
    let value = serde_json::to_value(&event.payload)?;
    let envelope = EventEnvelope {
        contract_version: event.contract_version,
        event_id: event.event_id,
        event_type: event.event_type.as_str().into(),
        event_version: event.event_version,
        occurred_at: event.occurred_at,
        aggregate_type: event.aggregate_type.as_str().into(),
        aggregate_id: event.aggregate_id,
        aggregate_version: event.aggregate_version,
        tenant_scope: event.tenant_scope.clone(),
        actor: event.actor.clone(),
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        idempotency_key: Some(event.idempotency_key.clone()),
        payload: value,
    };
    validate_envelope(
        &envelope,
        &event.event_type,
        &event.aggregate_type,
        &event.payload,
    )?;
    match &event.payload {
        LegalOutboxPayload::ContractSaved(contract) => validate_contract_payload(event, contract),
        LegalOutboxPayload::DocumentSaved(document) => validate_document_payload(event, document),
        LegalOutboxPayload::AccessRecorded(access) => validate_access_payload(event, access),
    }
}

fn validate_envelope(
    envelope: &EventEnvelope,
    event_type: &LegalEventType,
    aggregate_type: &LegalAggregateType,
    payload: &LegalOutboxPayload,
) -> Result<(), PersistenceError> {
    if envelope.contract_version != CONTRACT_VERSION
        || envelope.event_version == 0
        || envelope.event_version != 1
        || envelope.event_id.is_nil()
        || envelope.aggregate_id.is_nil()
        || envelope.aggregate_version == 0
        || envelope.correlation_id.is_nil()
        || envelope
            .idempotency_key
            .as_deref()
            .is_none_or(|key| key.is_empty())
        || matches!(envelope.tenant_scope, TenantScope::Platform)
        || envelope.actor.principal_ref.trim().is_empty()
        || envelope.actor.principal_ref.len() > 255
        || envelope.actor.principal_ref.chars().any(char::is_control)
    {
        return Err(invalid(
            "Legal source event envelope is incomplete or invalid",
        ));
    }
    if envelope.causation_id.is_some_and(|value| value.is_nil()) {
        return Err(invalid("Legal source causation ID cannot be nil"));
    }
    if envelope.idempotency_key.as_ref().is_some_and(|key| {
        key.trim().is_empty() || key.len() > 512 || key.chars().any(char::is_control)
    }) {
        return Err(invalid("Legal source idempotency key is invalid"));
    }
    match (aggregate_type, event_type, payload) {
        (
            LegalAggregateType::Contract,
            LegalEventType::ContractSaved,
            LegalOutboxPayload::ContractSaved(value),
        ) if envelope.aggregate_id == value.contract_id => validate_scope_containment(
            &envelope.tenant_scope,
            value.holding_id,
            value.company_id,
            value.branch_id,
        ),
        (
            LegalAggregateType::Document,
            LegalEventType::DocumentSaved,
            LegalOutboxPayload::DocumentSaved(value),
        ) if envelope.aggregate_id == value.document_id => validate_scope_containment(
            &envelope.tenant_scope,
            value.holding_id,
            value.company_id,
            value.branch_id,
        ),
        (
            LegalAggregateType::AccessHistory,
            LegalEventType::AccessRecorded,
            LegalOutboxPayload::AccessRecorded(value),
        ) if envelope.aggregate_id == value.entry_id => {
            if value.actor_ref != envelope.actor.principal_ref {
                Err(invalid(
                    "Legal access actor does not match the source event actor",
                ))
            } else {
                validate_scope_containment(
                    &envelope.tenant_scope,
                    value.holding_id,
                    value.company_id,
                    value.branch_id,
                )
            }
        }
        _ => Err(invalid(
            "Legal source event type, aggregate, and payload disagree",
        )),
    }
}

fn validate_contract_payload(
    event: &LegalSourceEvent,
    contract: &LegalContractEvent,
) -> Result<(), PersistenceError> {
    validate_scope_containment(
        &event.tenant_scope,
        contract.holding_id,
        contract.company_id,
        contract.branch_id,
    )?;
    validate_contract_actor_binding(
        &event.actor.principal_ref,
        event.aggregate_version,
        contract,
    )?;
    validate_uuid(contract.contract_id, "Legal contract ID")?;
    validate_bounded(&contract.reference, 255, "Legal contract reference")?;
    validate_bounded(&contract.title, 255, "Legal contract title")?;
    validate_object_ref(&contract.object_ref, "Legal contract object reference")?;
    validate_digest(&contract.content_sha256, "Legal contract digest")?;
    validate_bounded(&contract.created_by, 255, "Legal contract creator")?;
    validate_retention_deadline(contract.retention_until)?;
    if let Some(approved_by) = &contract.approved_by {
        validate_bounded(approved_by, 255, "Legal contract approver")?;
        if approved_by == &contract.created_by {
            return Err(invalid(
                "Legal contract approval requires a different principal",
            ));
        }
    }
    if matches!(
        contract.status,
        ContractStatus::Draft | ContractStatus::Review
    ) && contract.approved_by.is_some()
    {
        return Err(invalid(
            "Draft or review Legal contracts cannot contain approval",
        ));
    }
    if matches!(
        contract.status,
        ContractStatus::Approved
            | ContractStatus::Active
            | ContractStatus::Expired
            | ContractStatus::Terminated
            | ContractStatus::Archived
    ) && contract.approved_by.is_none()
    {
        return Err(invalid(
            "Approved Legal lifecycle states require an approver",
        ));
    }
    if contract.parties.len() > MAX_LEGAL_PARTIES {
        return Err(invalid("Legal contract party limit exceeded"));
    }
    let mut party_ids = HashSet::with_capacity(contract.parties.len());
    for party in &contract.parties {
        validate_uuid(party.party_id, "Legal contract party ID")?;
        validate_bounded(&party.role, 255, "Legal contract party role")?;
        if !party_ids.insert(party.party_id) {
            return Err(invalid("Legal contract parties are not unique"));
        }
    }
    Ok(())
}

fn validate_contract_actor_binding(
    actor_ref: &str,
    aggregate_version: u64,
    contract: &LegalContractEvent,
) -> Result<(), PersistenceError> {
    if aggregate_version == 1 && contract.created_by != actor_ref {
        return Err(invalid(
            "Legal initial contract actor does not match its creator",
        ));
    }
    if contract.status == ContractStatus::Approved
        && contract.approved_by.as_deref() != Some(actor_ref)
    {
        return Err(invalid(
            "Legal approval actor does not match the approved-by principal",
        ));
    }
    Ok(())
}

fn validate_document_payload(
    event: &LegalSourceEvent,
    document: &LegalDocumentEvent,
) -> Result<(), PersistenceError> {
    validate_scope_containment(
        &event.tenant_scope,
        document.holding_id,
        document.company_id,
        document.branch_id,
    )?;
    validate_uuid(document.document_id, "Legal document ID")?;
    validate_bounded(&document.title, 255, "Legal document title")?;
    if document.versions.len() > MAX_DOCUMENT_VERSIONS
        || document.current_version
            != document
                .versions
                .last()
                .map_or(0, |version| version.version)
    {
        return Err(invalid("Legal document versions are invalid"));
    }
    for (index, version) in document.versions.iter().enumerate() {
        let expected = u64::try_from(index + 1)
            .map_err(|_| invalid("Legal document version index overflow"))?;
        if version.version != expected {
            return Err(invalid("Legal document versions are not contiguous"));
        }
        validate_object_ref(&version.object_ref, "Legal document object reference")?;
        validate_digest(&version.content_sha256, "Legal document digest")?;
        validate_bounded(&version.created_by, 255, "Legal document version creator")?;
        if version.created_at > Utc::now() + MAX_LEGAL_CLOCK_SKEW {
            return Err(invalid(
                "Legal document version timestamp is too far in the future",
            ));
        }
    }
    Ok(())
}

fn validate_access_payload(
    event: &LegalSourceEvent,
    access: &LegalAccessHistoryEvent,
) -> Result<(), PersistenceError> {
    validate_scope_containment(
        &event.tenant_scope,
        access.holding_id,
        access.company_id,
        access.branch_id,
    )?;
    validate_uuid(access.entry_id, "Legal access entry ID")?;
    validate_uuid(access.document_id, "Legal access document ID")?;
    validate_uuid(access.holding_id, "Legal access holding ID")?;
    validate_uuid(access.company_id, "Legal access company ID")?;
    validate_bounded(&access.actor_ref, 255, "Legal access actor")?;
    if access.actor_ref != event.actor.principal_ref {
        return Err(invalid(
            "Legal access actor does not match the source event actor",
        ));
    }
    validate_bounded(&access.action, 255, "Legal access action")?;
    validate_bounded(&access.purpose, 512, "Legal access purpose")?;
    if access.observed_at > Utc::now() + MAX_LEGAL_CLOCK_SKEW {
        return Err(invalid("Legal access timestamp is too far in the future"));
    }
    if access.version == 0 {
        return Err(invalid("Legal access document version is invalid"));
    }
    Ok(())
}

fn validate_scope_containment(
    scope: &TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<(), PersistenceError> {
    let record_scope = match branch_id {
        Some(branch_id) => TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        None => TenantScope::Company {
            holding_id,
            company_id,
        },
    };
    if !scope.contains(&record_scope) {
        return Err(invalid(
            "Legal event tenant scope does not contain record scope",
        ));
    }
    Ok(())
}

fn validate_retention_deadline(retention_until: DateTime<Utc>) -> Result<(), PersistenceError> {
    let latest_allowed = Utc::now()
        .checked_add_signed(MAX_LEGAL_RETENTION_HORIZON)
        .ok_or_else(|| invalid("Legal retention horizon overflow"))?;
    if retention_until > latest_allowed {
        return Err(invalid("Legal retention exceeds the bounded horizon"));
    }
    Ok(())
}

fn event_scope(event: &LegalSourceEvent) -> Result<(Uuid, Uuid, Option<Uuid>), PersistenceError> {
    match &event.payload {
        LegalOutboxPayload::ContractSaved(value) => {
            Ok((value.holding_id, value.company_id, value.branch_id))
        }
        LegalOutboxPayload::DocumentSaved(value) => {
            Ok((value.holding_id, value.company_id, value.branch_id))
        }
        LegalOutboxPayload::AccessRecorded(value) => {
            Ok((value.holding_id, value.company_id, value.branch_id))
        }
    }
}

fn event_source(event: &LegalSourceEvent) -> LegalSourceMetadata {
    LegalSourceMetadata {
        contract_version: event.contract_version,
        event_version: event.event_version,
        actor_ref: event.actor.principal_ref.clone(),
        actor_kind: event.actor.kind.clone(),
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        idempotency_key: event.idempotency_key.clone(),
    }
}

fn parse_aggregate_type(value: &str) -> Result<LegalAggregateType, PersistenceError> {
    match value {
        LEGAL_CONTRACT_AGGREGATE => Ok(LegalAggregateType::Contract),
        LEGAL_DOCUMENT_AGGREGATE => Ok(LegalAggregateType::Document),
        LEGAL_ACCESS_HISTORY_AGGREGATE => Ok(LegalAggregateType::AccessHistory),
        _ => Err(invalid("Legal source aggregate type is unsupported")),
    }
}

fn parse_event_type(value: &str) -> Result<LegalEventType, PersistenceError> {
    match value {
        LEGAL_CONTRACT_SAVED_EVENT => Ok(LegalEventType::ContractSaved),
        LEGAL_DOCUMENT_SAVED_EVENT => Ok(LegalEventType::DocumentSaved),
        LEGAL_ACCESS_RECORDED_EVENT => Ok(LegalEventType::AccessRecorded),
        _ => Err(invalid("Legal source event type is unsupported")),
    }
}

fn contract_status(value: ContractStatus) -> &'static str {
    match value {
        ContractStatus::Draft => "draft",
        ContractStatus::Review => "review",
        ContractStatus::Approved => "approved",
        ContractStatus::Active => "active",
        ContractStatus::Expired => "expired",
        ContractStatus::Terminated => "terminated",
        ContractStatus::Archived => "archived",
    }
}

fn retention_state(until: DateTime<Utc>) -> &'static str {
    if until <= Utc::now() {
        "ended"
    } else {
        "active"
    }
}

fn decode_source_metadata(
    row: &sqlx::postgres::PgRow,
) -> Result<LegalSourceMetadata, PersistenceError> {
    let contract_version = required_positive_u16(
        row.try_get("source_contract_version")?,
        "read contract version",
    )?;
    let event_version =
        required_positive_u16(row.try_get("source_event_version")?, "read event version")?;
    if contract_version != CONTRACT_VERSION || event_version != 1 {
        return Err(invalid("Legal read source envelope version is unsupported"));
    }
    let actor_ref: String = row.try_get("source_actor_ref")?;
    let idempotency_key: String = row.try_get("source_idempotency_key")?;
    let correlation_id: Uuid = row.try_get("source_correlation_id")?;
    validate_bounded(&actor_ref, 255, "Legal read source actor")?;
    validate_bounded(&idempotency_key, 512, "Legal read source idempotency key")?;
    validate_uuid(correlation_id, "Legal read source correlation ID")?;
    if row
        .try_get::<Option<Uuid>, _>("source_causation_id")?
        .is_some_and(|value| value.is_nil())
    {
        return Err(invalid("Legal read source causation ID is invalid"));
    }
    Ok(LegalSourceMetadata {
        contract_version,
        event_version,
        actor_ref,
        actor_kind: parse_actor_kind(&row.try_get::<String, _>("source_actor_kind")?)?,
        correlation_id,
        causation_id: row.try_get("source_causation_id")?,
        idempotency_key,
    })
}

fn decode_party_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<LegalContractPartyReadRow, PersistenceError> {
    let party_id: Uuid = row.try_get("party_id")?;
    let holding_id: Uuid = row.try_get("holding_id")?;
    let party_role: String = row.try_get("party_role")?;
    let source_version = as_u64(row.try_get("source_version")?, "Legal party source version")?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    validate_uuid(party_id, "Legal party ID")?;
    validate_uuid(holding_id, "Legal party holding ID")?;
    validate_uuid(source_event_id, "Legal party source event ID")?;
    validate_bounded(&party_role, 255, "Legal party role")?;
    Ok(LegalContractPartyReadRow {
        party_id,
        holding_id,
        party_role,
        source_version,
        source_event_id,
        source: decode_source_metadata(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_approval_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<LegalContractApprovalReadRow, PersistenceError> {
    let holding_id: Uuid = row.try_get("holding_id")?;
    let approved_by: String = row.try_get("approved_by")?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    validate_uuid(holding_id, "Legal approval holding ID")?;
    validate_uuid(source_event_id, "Legal approval source event ID")?;
    validate_bounded(&approved_by, 255, "Legal approval actor")?;
    Ok(LegalContractApprovalReadRow {
        holding_id,
        approved_by,
        approved_at: row.try_get("approved_at")?,
        contract_version: as_u64(row.try_get("contract_version")?, "Legal approval version")?,
        source_event_id,
        source: decode_source_metadata(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_retention_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<LegalContractRetentionReadRow, PersistenceError> {
    let holding_id: Uuid = row.try_get("holding_id")?;
    let retention_state: String = row.try_get("retention_state")?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    validate_uuid(holding_id, "Legal retention holding ID")?;
    validate_uuid(source_event_id, "Legal retention source event ID")?;
    if !matches!(retention_state.as_str(), "active" | "ended") {
        return Err(invalid("Legal retention state is invalid"));
    }
    Ok(LegalContractRetentionReadRow {
        holding_id,
        retention_until: row.try_get("retention_until")?,
        retention_state,
        contract_version: as_u64(row.try_get("contract_version")?, "Legal retention version")?,
        source_event_id,
        source: decode_source_metadata(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_document_version_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<LegalDocumentVersionReadRow, PersistenceError> {
    let version = as_u64(row.try_get("version")?, "Legal document version")?;
    let object_ref: String = row.try_get("object_ref")?;
    let content_sha256: String = row.try_get("content_sha256")?;
    let created_by: String = row.try_get("created_by")?;
    let source_version = as_u64(
        row.try_get("source_version")?,
        "Legal version source version",
    )?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    if version == 0 {
        return Err(invalid("Legal document version is invalid"));
    }
    validate_object_ref(&object_ref, "Legal document version object reference")?;
    validate_digest(&content_sha256, "Legal document version digest")?;
    validate_bounded(&created_by, 255, "Legal document version creator")?;
    validate_uuid(source_event_id, "Legal document version source event ID")?;
    Ok(LegalDocumentVersionReadRow {
        version,
        object_ref,
        content_sha256,
        created_at: row.try_get("created_at")?,
        created_by,
        source_version,
        source_event_id,
        source: decode_source_metadata(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_access_read_row(
    row: sqlx::postgres::PgRow,
) -> Result<LegalAccessHistoryReadRow, PersistenceError> {
    let entry_id: Uuid = row.try_get("entry_id")?;
    let document_id: Uuid = row.try_get("document_id")?;
    let holding_id: Uuid = row.try_get("holding_id")?;
    let company_id: Uuid = row.try_get("company_id")?;
    let branch_id: Option<Uuid> = row.try_get("branch_id")?;
    let document_version = as_u64(row.try_get("document_version")?, "Legal access version")?;
    let actor_ref: String = row.try_get("actor_ref")?;
    let action: String = row.try_get("action")?;
    let purpose: String = row.try_get("purpose")?;
    let source_version = as_u64(
        row.try_get("source_version")?,
        "Legal access source version",
    )?;
    let source_event_id: Uuid = row.try_get("source_event_id")?;
    validate_uuid(entry_id, "Legal access entry ID")?;
    validate_uuid(document_id, "Legal access document ID")?;
    validate_uuid(holding_id, "Legal access holding ID")?;
    validate_uuid(company_id, "Legal access company ID")?;
    if branch_id.is_some_and(|value| value.is_nil()) || document_version == 0 {
        return Err(invalid("Legal access scope or version is invalid"));
    }
    validate_bounded(&actor_ref, 255, "Legal access actor")?;
    validate_bounded(&action, 255, "Legal access action")?;
    validate_bounded(&purpose, 512, "Legal access purpose")?;
    validate_uuid(source_event_id, "Legal access source event ID")?;
    Ok(LegalAccessHistoryReadRow {
        entry_id,
        document_id,
        holding_id,
        company_id,
        branch_id,
        document_version,
        actor_ref,
        action,
        purpose,
        observed_at: row.try_get("observed_at")?,
        source_version,
        source_event_id,
        source: decode_source_metadata(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn parse_actor_kind(value: &str) -> Result<ActorKind, PersistenceError> {
    match value {
        "platform_user" => Ok(ActorKind::PlatformUser),
        "tenant_user" => Ok(ActorKind::TenantUser),
        "service" => Ok(ActorKind::Service),
        "system_worker" => Ok(ActorKind::SystemWorker),
        _ => Err(invalid("Legal read actor kind is invalid")),
    }
}

fn actor_kind_name(value: &ActorKind) -> &'static str {
    match value {
        ActorKind::PlatformUser => "platform_user",
        ActorKind::TenantUser => "tenant_user",
        ActorKind::Service => "service",
        ActorKind::SystemWorker => "system_worker",
    }
}

fn validate_scope(scope: LegalReadScope) -> Result<(), PersistenceError> {
    validate_uuid(scope.holding_id, "Legal read holding ID")?;
    validate_uuid(scope.company_id, "Legal read company ID")?;
    if let Some(branch_id) = scope.branch_id {
        validate_uuid(branch_id, "Legal read branch ID")?;
    }
    Ok(())
}

fn validate_source_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE {
        return Err(invalid(format!(
            "Legal source event page size must be 1..={MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn validate_read_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_LEGAL_READ_PAGE_SIZE {
        return Err(invalid(format!(
            "Legal read page size must be 1..={MAX_LEGAL_READ_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn validate_source_cursor(cursor: &LegalSourceEventCursor) -> Result<(), PersistenceError> {
    if parse_aggregate_type(&cursor.aggregate_type).is_err()
        || cursor.aggregate_id.is_nil()
        || cursor.aggregate_version == 0
    {
        return Err(invalid("Legal source event cursor is invalid"));
    }
    Ok(())
}

fn source_history_is_contiguous(
    first_version: u64,
    event_count: u64,
    distinct_event_version_count: u64,
    latest_version: u64,
) -> bool {
    first_version == 1
        && event_count == distinct_event_version_count
        && event_count == latest_version
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn validate_bounded(value: &str, max: usize, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_object_ref(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    validate_bounded(value, 255, field)?;
    let lower = value.to_ascii_lowercase();
    if value.starts_with('/')
        || value.contains("..")
        || lower.contains("://")
        || lower.contains("data:")
        || lower.contains("base64")
        || value.bytes().any(|character| b"?#%=".contains(&character))
        || !value
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || b"/._-".contains(&character))
    {
        return Err(invalid(format!(
            "{field} contains forbidden reference material"
        )));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.len() != 71
        || !value.starts_with("sha256:")
        || value[7..]
            .chars()
            .any(|character| !character.is_ascii_hexdigit())
    {
        return Err(invalid(format!(
            "{field} must be sha256 followed by 64 hex characters"
        )));
    }
    Ok(())
}

fn required_positive_u16(value: Option<i32>, field: &'static str) -> Result<u16, PersistenceError> {
    let value = value.ok_or_else(|| invalid(format!("{field} is missing")))?;
    let value = u16::try_from(value).map_err(|_| invalid(format!("{field} is invalid")))?;
    if value == 0 {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(value)
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is invalid")))
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LegalContractPartyEvent;

    #[test]
    fn exact_digest_and_opaque_reference_validation_is_fail_closed() {
        assert!(
            validate_digest(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "digest"
            )
            .is_ok()
        );
        assert!(validate_digest("sha256:abc123", "digest").is_err());
        assert!(validate_object_ref("https://example.invalid/object", "ref").is_err());
        assert!(validate_object_ref("opaque/../object", "ref").is_err());
        assert!(validate_object_ref("opaque/object?download=1", "ref").is_err());
        assert!(validate_object_ref("opaque/contracts/001-v1", "ref").is_ok());
    }

    #[test]
    fn source_cursor_and_scope_are_bounded() {
        assert!(
            validate_source_cursor(&LegalSourceEventCursor {
                aggregate_type: LEGAL_CONTRACT_AGGREGATE.into(),
                aggregate_id: Uuid::new_v4(),
                aggregate_version: 1,
            })
            .is_ok()
        );
        assert!(
            validate_source_cursor(&LegalSourceEventCursor {
                aggregate_type: "legacy".into(),
                aggregate_id: Uuid::new_v4(),
                aggregate_version: 1,
            })
            .is_err()
        );
        assert!(
            validate_scope_containment(
                &TenantScope::Company {
                    holding_id: Uuid::new_v4(),
                    company_id: Uuid::new_v4(),
                },
                Uuid::new_v4(),
                Uuid::new_v4(),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn source_history_reconciliation_rejects_missing_and_duplicate_versions() {
        assert!(source_history_is_contiguous(1, 1, 1, 1));
        assert!(source_history_is_contiguous(1, 3, 3, 3));
        assert!(!source_history_is_contiguous(2, 1, 1, 2));
        assert!(!source_history_is_contiguous(1, 2, 2, 3));
        assert!(!source_history_is_contiguous(1, 3, 2, 3));
        assert!(!source_history_is_contiguous(1, 2, 1, 2));
    }

    #[test]
    fn access_history_payload_actor_must_match_source_actor() {
        let event_id = Uuid::from_u128(31);
        let mut event = LegalSourceEvent {
            event_id,
            contract_version: CONTRACT_VERSION,
            event_version: 1,
            event_type: LegalEventType::AccessRecorded,
            aggregate_type: LegalAggregateType::AccessHistory,
            aggregate_id: Uuid::from_u128(32),
            aggregate_version: 1,
            occurred_at: Utc::now(),
            tenant_scope: TenantScope::Company {
                holding_id: Uuid::from_u128(33),
                company_id: Uuid::from_u128(34),
            },
            actor: ActorAttribution {
                principal_ref: "source-actor".into(),
                kind: ActorKind::TenantUser,
                session_id: Some(Uuid::from_u128(35)),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::from_u128(36),
            causation_id: None,
            idempotency_key: "legal-access-actor-binding".into(),
            payload: LegalOutboxPayload::AccessRecorded(LegalAccessHistoryEvent {
                entry_id: Uuid::from_u128(32),
                document_id: Uuid::from_u128(37),
                version: 1,
                actor_ref: "source-actor".into(),
                holding_id: Uuid::from_u128(33),
                company_id: Uuid::from_u128(34),
                branch_id: None,
                action: "download".into(),
                purpose: "legal review".into(),
                observed_at: Utc::now(),
            }),
        };
        validate_source_event(&event).expect("matching access actor");
        if let LegalOutboxPayload::AccessRecorded(access) = &mut event.payload {
            access.actor_ref = "different-actor".into();
        }
        assert!(validate_source_event(&event).is_err());
    }

    #[test]
    fn contract_lifecycle_actor_bindings_are_rejected_at_projection_boundary() {
        let contract = LegalContractEvent {
            contract_id: Uuid::from_u128(51),
            holding_id: Uuid::from_u128(52),
            company_id: Uuid::from_u128(53),
            branch_id: None,
            reference: "contract-actor-binding".into(),
            title: "Actor binding contract".into(),
            object_ref: "opaque/contracts/actor-binding".into(),
            content_sha256:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            retention_until: Utc::now(),
            status: ContractStatus::Draft,
            created_by: "creator".into(),
            approved_by: None,
            parties: Vec::new(),
        };
        validate_contract_actor_binding("creator", 1, &contract)
            .expect("initial creator actor binding");
        assert!(validate_contract_actor_binding("other", 1, &contract).is_err());

        let mut approved = contract;
        approved.status = ContractStatus::Approved;
        approved.approved_by = Some("reviewer".into());
        validate_contract_actor_binding("reviewer", 2, &approved).expect("approval actor binding");
        assert!(validate_contract_actor_binding("other", 2, &approved).is_err());
    }

    #[test]
    fn typed_source_pages_apply_semantic_retention_and_child_invariants() {
        let holding_id = Uuid::from_u128(61);
        let company_id = Uuid::from_u128(62);
        let actor = ActorAttribution {
            principal_ref: "creator".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(63)),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let party_id = Uuid::from_u128(64);
        let mut event = LegalSourceEvent {
            event_id: Uuid::from_u128(65),
            contract_version: CONTRACT_VERSION,
            event_version: 1,
            event_type: LegalEventType::ContractSaved,
            aggregate_type: LegalAggregateType::Contract,
            aggregate_id: Uuid::from_u128(66),
            aggregate_version: 1,
            occurred_at: Utc::now(),
            tenant_scope: scope,
            actor,
            correlation_id: Uuid::from_u128(67),
            causation_id: None,
            idempotency_key: "legal-semantic-validation".into(),
            payload: LegalOutboxPayload::ContractSaved(LegalContractEvent {
                contract_id: Uuid::from_u128(66),
                holding_id,
                company_id,
                branch_id: None,
                reference: "semantic-contract".into(),
                title: "Semantic contract".into(),
                object_ref: "opaque/contracts/semantic".into(),
                content_sha256:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                retention_until: Utc::now(),
                status: ContractStatus::Draft,
                created_by: "creator".into(),
                approved_by: None,
                parties: vec![LegalContractPartyEvent {
                    party_id,
                    role: "supplier".into(),
                }],
            }),
        };
        validate_source_event(&event).expect("valid semantic source event");

        if let LegalOutboxPayload::ContractSaved(contract) = &mut event.payload {
            contract.retention_until = Utc::now() + Duration::days(36_526);
        }
        assert!(
            validate_source_event(&event)
                .expect_err("retention horizon must be enforced during source decoding")
                .to_string()
                .contains("retention exceeds")
        );

        if let LegalOutboxPayload::ContractSaved(contract) = &mut event.payload {
            contract.retention_until = Utc::now();
            contract.parties.push(LegalContractPartyEvent {
                party_id,
                role: "duplicate".into(),
            });
        }
        assert!(
            validate_source_event(&event)
                .expect_err("duplicate parties must be rejected during source decoding")
                .to_string()
                .contains("not unique")
        );
    }

    #[test]
    fn bounded_legal_source_and_read_pages_reject_zero_and_oversized_requests() {
        assert!(validate_source_page_size(0).is_err());
        assert!(validate_source_page_size(MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE).is_ok());
        assert!(validate_source_page_size(MAX_LEGAL_SOURCE_EVENT_PAGE_SIZE + 1).is_err());
        assert!(validate_read_page_size(0).is_err());
        assert!(validate_read_page_size(MAX_LEGAL_READ_PAGE_SIZE).is_ok());
        assert!(validate_read_page_size(MAX_LEGAL_READ_PAGE_SIZE + 1).is_err());
    }
}
