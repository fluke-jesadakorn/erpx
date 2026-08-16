//! Tenant-routed SQLx persistence for Quality inspections.
//!
//! The inspection aggregate is authoritative on the tenant write database.
//! Every accepted command commits the source row, immutable snapshot, complete
//! tenant outbox envelope, and idempotency receipt together. The read model is
//! disposable and can only be populated from an authoritative outbox event.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use erpx_domain::contracts::{ActorAttribution, CommandContext, TenantScope};
use erpx_domain::quality::{QualityInspection, QualityProjectionReconciliation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const MAX_QUALITY_SOURCE_PAGE_SIZE: u32 = 500;
pub const MAX_QUALITY_READ_PAGE_SIZE: u32 = 100;
/// Maximum source-event history one Quality projection invocation may drain.
pub const MAX_QUALITY_PROJECTION_EVENTS: u64 = 1_000_000;
const QUALITY_EVENT_VERSION: i32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityWriteReceipt {
    pub aggregate_id: Uuid,
    pub version: u64,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualitySourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualitySourceEvent {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualitySourceEventPage {
    pub events: Vec<QualitySourceEvent>,
    pub next_cursor: Option<QualitySourceEventCursor>,
    pub has_more: bool,
}

/// Explicit tenant scope accepted by the Quality read gateway. Company scope
/// includes its branches; a branch scope is exact. Platform and subtree
/// scopes are rejected by the transport before this type is constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityInspectionReadCursor {
    pub projected_at: DateTime<Utc>,
    pub inspection_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityInspectionReadRow {
    pub inspection: QualityInspection,
    pub source_version: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QualityInspectionReadPage {
    pub rows: Vec<QualityInspectionReadRow>,
    pub next_cursor: Option<QualityInspectionReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone)]
pub struct SqlxQualityStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

#[derive(Debug, Clone, PartialEq)]
struct QualitySourceProjectionFact {
    company_id: Uuid,
    branch_id: Option<Uuid>,
    version: u64,
    payload: Value,
    event_count: u64,
    max_event_version: Option<u64>,
    latest_event_id: Option<Uuid>,
    latest_event_payload: Option<Value>,
    event_versions: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq)]
struct QualityProjectedFact {
    holding_ref: String,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    source_version: u64,
    payload: Value,
    checkpoint: Option<(u64, Uuid)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualityProjectionDecision {
    Apply,
    Skip,
}

/// Decide whether an authoritative event can advance a disposable Quality
/// row. Older events are safe no-ops because the row could only have reached
/// its current version through contiguous projection. Same-version payloads
/// must match exactly; gaps remain permanent projection errors.
fn projection_decision(
    current: Option<(u64, &Value)>,
    aggregate_version: u64,
    aggregate_payload: &Value,
) -> Result<QualityProjectionDecision, PersistenceError> {
    let Some((current_version, current_payload)) = current else {
        return if aggregate_version == 1 {
            Ok(QualityProjectionDecision::Apply)
        } else {
            Err(PersistenceError::InvalidData(
                "quality projection cannot start above version one".into(),
            ))
        };
    };
    if current_version > aggregate_version {
        return Ok(QualityProjectionDecision::Skip);
    }
    if current_version == aggregate_version {
        return if current_payload == aggregate_payload {
            Ok(QualityProjectionDecision::Skip)
        } else {
            Err(PersistenceError::InvalidData(
                "quality projection same-version payload conflict".into(),
            ))
        };
    }
    if current_version.saturating_add(1) == aggregate_version {
        Ok(QualityProjectionDecision::Apply)
    } else {
        Err(PersistenceError::InvalidData(
            "quality projection version is not contiguous".into(),
        ))
    }
}

fn quality_idempotency_lock_name(command_key: &str) -> String {
    format!("quality:idempotency:{command_key}")
}

fn quality_projection_lock_name(holding_ref: &str) -> String {
    format!("quality:projection:{holding_ref}")
}

async fn lock_quality_transaction(
    tx: &mut Transaction<'_, Postgres>,
    lock_name: String,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_name)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

impl SqlxQualityStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_text(&holding_ref, "holding_ref", 255)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), holding_ref)
    }

    pub async fn save(
        &self,
        inspection: &QualityInspection,
        expected_version: Option<u64>,
        context: &CommandContext,
    ) -> Result<QualityWriteReceipt, PersistenceError> {
        inspection
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        context
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if !context.tenant_scope.contains(&inspection.scope) {
            return Err(PersistenceError::InvalidData(
                "quality inspection is outside command scope".into(),
            ));
        }
        let command_key = format!("quality/inspection/{}", context.idempotency_key);
        validate_text(&command_key, "quality command idempotency key", 512)?;
        let payload = serde_json::to_value(inspection)?;
        let request_hash = quality_request_hash(&payload, expected_version, context)?;
        let mut tx = self.begin_write().await?;
        lock_quality_transaction(&mut tx, quality_idempotency_lock_name(&command_key)).await?;
        if let Some(row) = sqlx::query(
            "SELECT request_hash, response FROM erpx_tenant.quality_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&command_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: command_key.clone(),
                });
            }
            let mut receipt: QualityWriteReceipt =
                serde_json::from_value(row.try_get("response")?)?;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }
        let current = sqlx::query(
            "SELECT version, payload FROM erpx_tenant.quality_inspections
             WHERE holding_ref=$1 AND inspection_id=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(inspection.id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(row.try_get("version")?, "quality inspection version")?,
                row.try_get::<Value, _>("payload")?,
            ))
        })
        .transpose()?;
        let current_version = current.as_ref().map(|(version, _)| *version);
        if expected_version.is_some_and(|expected| expected != current_version.unwrap_or(0)) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "quality_inspection".into(),
                aggregate_id: inspection.id,
                expected: expected_version,
                actual: current_version,
            });
        }
        if inspection.version != current_version.unwrap_or(0).saturating_add(1) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "quality_inspection".into(),
                aggregate_id: inspection.id,
                expected: Some(current_version.unwrap_or(0).saturating_add(1)),
                actual: Some(inspection.version),
            });
        }
        if let Some((_, persisted_payload)) = current.as_ref() {
            let persisted: QualityInspection = serde_json::from_value(persisted_payload.clone())?;
            persisted
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            if persisted.id != inspection.id
                || persisted.scope != inspection.scope
                || persisted.holding_id != inspection.holding_id
                || persisted.company_id != inspection.company_id
                || persisted.branch_id != inspection.branch_id
            {
                return Err(PersistenceError::InvalidData(
                    "quality inspection scope is immutable".into(),
                ));
            }
        }
        if current.is_some() {
            sqlx::query(
                "UPDATE erpx_tenant.quality_inspections
                 SET company_id=$3, branch_id=$4, reference=$5, version=$6,
                     payload=$7, updated_at=$8
                 WHERE holding_ref=$1 AND inspection_id=$2",
            )
            .bind(&self.holding_ref)
            .bind(inspection.id)
            .bind(inspection.company_id)
            .bind(inspection.branch_id)
            .bind(&inspection.reference)
            .bind(as_i64(inspection.version, "quality inspection version")?)
            .bind(&payload)
            .bind(inspection.updated_at)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.quality_inspections
                    (inspection_id, holding_ref, company_id, branch_id, reference,
                     version, idempotency_key, payload, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(inspection.id)
            .bind(&self.holding_ref)
            .bind(inspection.company_id)
            .bind(inspection.branch_id)
            .bind(&inspection.reference)
            .bind(as_i64(inspection.version, "quality inspection version")?)
            .bind(&inspection.idempotency_key)
            .bind(&payload)
            .bind(inspection.updated_at)
            .execute(&mut *tx)
            .await?;
        }
        let event_id = Uuid::new_v4();
        let event_payload = serde_json::json!({
            "aggregate_type": "quality_inspection",
            "aggregate": payload,
            "version": inspection.version,
        });
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             VALUES ($1,'quality_inspection',$2,$3,$4,'quality.inspection.v1',$5,$6,
                     1,1,$7,$8,$9,$10,$11)",
        )
        .bind(event_id)
        .bind(inspection.id)
        .bind(&self.holding_ref)
        .bind(as_i64(inspection.version, "quality inspection version")?)
        .bind(event_payload)
        .bind(inspection.updated_at)
        .bind(serde_json::to_value(&context.tenant_scope)?)
        .bind(serde_json::to_value(&context.actor)?)
        .bind(context.correlation_id)
        .bind(context.causation_id)
        .bind(&command_key)
        .execute(&mut *tx)
        .await?;
        let receipt = QualityWriteReceipt {
            aggregate_id: inspection.id,
            version: inspection.version,
            event_id,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.quality_command_idempotency
                (idempotency_key, holding_ref, request_hash, response)
             VALUES ($1,$2,$3,$4)",
        )
        .bind(&command_key)
        .bind(&self.holding_ref)
        .bind(request_hash)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&QualitySourceEventCursor>,
        page_size: u32,
    ) -> Result<QualitySourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_QUALITY_SOURCE_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "quality source page size is out of bounds".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let page =
            read_source_event_page_in_transaction(&mut tx, &self.holding_ref, cursor, page_size)
                .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Return the holding-scoped count of authoritative Quality source events.
    /// Replacement rebuilds use this count before and after replay as a
    /// source fence; the predicate is intentionally identical to the source
    /// page query below.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count = source_event_high_watermark_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(count)
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<QualityProjectionApplication, PersistenceError> {
        validate_event_payload(event_id, aggregate_id, aggregate_version, payload)?;
        let mut source_tx = self.begin_write().await?;
        let source = sqlx::query(
            "SELECT e.event_type, e.aggregate_id, e.aggregate_version, e.payload AS payload,
                    e.contract_version, e.event_version, e.tenant_scope, e.actor, e.correlation_id,
                    e.causation_id, e.idempotency_key,
                    q.version AS source_version, q.payload AS source_payload
             FROM erpx_tenant.outbox_events e
             JOIN erpx_tenant.quality_inspections q
               ON q.holding_ref=e.holding_ref AND q.inspection_id=e.aggregate_id
             WHERE e.event_id=$1 AND e.holding_ref=$2",
        )
        .bind(event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *source_tx)
        .await?;
        source_tx.commit().await?;
        let Some(source) = source else {
            return Err(PersistenceError::InvalidData(
                "quality projection event is not authoritative".into(),
            ));
        };
        let inspection = quality_source_inspection_from_row(
            &source,
            event_id,
            aggregate_id,
            aggregate_version,
            payload,
        )?;
        let source_version = as_u64(
            source.try_get("source_version")?,
            "quality source snapshot version",
        )?;
        let source_payload: Value = source.try_get("source_payload")?;
        validate_quality_source_snapshot(
            aggregate_id,
            source_version,
            &source_payload,
            aggregate_version,
            &inspection,
        )?;
        let mut read_tx = self.begin_read().await?;
        lock_quality_transaction(
            &mut read_tx,
            quality_projection_lock_name(&self.holding_ref),
        )
        .await?;
        let result = self
            .project_event_in_transaction(
                &mut read_tx,
                event_id,
                aggregate_id,
                aggregate_version,
                payload,
                &inspection,
            )
            .await;
        match result {
            Ok(result) => {
                read_tx.commit().await?;
                Ok(result)
            }
            Err(error) => Err(error),
        }
    }

    async fn project_event_in_transaction(
        &self,
        read_tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
        inspection: &QualityInspection,
    ) -> Result<QualityProjectionApplication, PersistenceError> {
        validate_event_payload(event_id, aggregate_id, aggregate_version, payload)?;
        let projected_payload = serde_json::to_value(inspection)?;
        if projected_payload
            != payload.get("aggregate").cloned().ok_or_else(|| {
                PersistenceError::InvalidData("quality event aggregate is missing".into())
            })?
        {
            return Err(PersistenceError::InvalidData(
                "quality projection payload does not match aggregate".into(),
            ));
        }
        let existing = sqlx::query(
            "SELECT company_id, branch_id, source_version, payload
             FROM erpx_tenant.quality_inspection_read_rows
             WHERE holding_ref=$1 AND inspection_id=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(aggregate_id)
        .fetch_optional(&mut **read_tx)
        .await?;
        if let Some(existing) = existing {
            let current_company_id: Uuid = existing.try_get("company_id")?;
            let current_branch_id: Option<Uuid> = existing.try_get("branch_id")?;
            if current_company_id != inspection.company_id
                || current_branch_id != inspection.branch_id
            {
                return Err(PersistenceError::InvalidData(
                    "quality projection row scope mismatch".into(),
                ));
            }
            let current = as_u64(existing.try_get("source_version")?, "quality read version")?;
            let current_payload: Value = existing.try_get("payload")?;
            let checkpoint = sqlx::query(
                "SELECT source_version, last_event_id
                 FROM erpx_tenant.quality_projection_checkpoints
                 WHERE holding_ref=$1 AND inspection_id=$2 FOR UPDATE",
            )
            .bind(&self.holding_ref)
            .bind(aggregate_id)
            .fetch_optional(&mut **read_tx)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData("quality projection checkpoint is missing".into())
            })?;
            let checkpoint_version = as_u64(
                checkpoint.try_get("source_version")?,
                "quality checkpoint version",
            )?;
            let checkpoint_event_id: Uuid = checkpoint.try_get("last_event_id")?;
            if checkpoint_event_id.is_nil() || checkpoint_version != current {
                return Err(PersistenceError::InvalidData(
                    "quality projection row and checkpoint are inconsistent".into(),
                ));
            }
            if current == aggregate_version && checkpoint_event_id != event_id {
                return Err(PersistenceError::InvalidData(
                    "quality projection same-version event conflict".into(),
                ));
            }
            if projection_decision(
                Some((current, &current_payload)),
                aggregate_version,
                &projected_payload,
            )? == QualityProjectionDecision::Skip
            {
                // A restarted worker may replay from an older durable cursor.
                // The contiguous projector invariant means every lower version
                // was already applied before this row could reach `current`.
                return Ok(QualityProjectionApplication {
                    event_id,
                    aggregate_id,
                    aggregate_version,
                    applied: false,
                });
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "quality projection cannot start above version one".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.quality_inspection_read_rows
                (inspection_id, holding_ref, company_id, branch_id, reference,
                 source_version, payload, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,now())
             ON CONFLICT (inspection_id) DO UPDATE SET
                company_id=$3, branch_id=$4, reference=$5, source_version=$6,
                payload=$7, projected_at=now()",
        )
        .bind(inspection.id)
        .bind(&self.holding_ref)
        .bind(inspection.company_id)
        .bind(inspection.branch_id)
        .bind(&inspection.reference)
        .bind(as_i64(inspection.version, "quality projection version")?)
        .bind(serde_json::to_value(inspection)?)
        .execute(&mut **read_tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.quality_projection_checkpoints
                (inspection_id, holding_ref, last_event_id, source_version, projected_at)
             VALUES ($1,$2,$3,$4,now())
             ON CONFLICT (inspection_id) DO UPDATE SET
                last_event_id=$3, source_version=$4, projected_at=now()",
        )
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(event_id)
        .bind(as_i64(aggregate_version, "quality checkpoint version")?)
        .execute(&mut **read_tx)
        .await?;
        Ok(QualityProjectionApplication {
            event_id,
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    pub async fn get(
        &self,
        inspection_id: Uuid,
    ) -> Result<Option<(QualityInspection, u64)>, PersistenceError> {
        if inspection_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "quality inspection ID cannot be nil".into(),
            ));
        }
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT company_id, branch_id, payload, source_version
             FROM erpx_tenant.quality_inspection_read_rows
             WHERE holding_ref=$1 AND inspection_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(inspection_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else { return Ok(None) };
        let inspection: QualityInspection = serde_json::from_value(row.try_get("payload")?)?;
        inspection
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let row_company_id: Uuid = row.try_get("company_id")?;
        let row_branch_id: Option<Uuid> = row.try_get("branch_id")?;
        if inspection.company_id != row_company_id || inspection.branch_id != row_branch_id {
            return Err(PersistenceError::InvalidData(
                "quality read row scope mismatch".into(),
            ));
        }
        let source_version = as_u64(row.try_get("source_version")?, "quality read version")?;
        if inspection.id != inspection_id || inspection.version != source_version {
            return Err(PersistenceError::InvalidData(
                "quality read identity/version mismatch".into(),
            ));
        }
        Ok(Some((inspection, source_version)))
    }

    /// Return a read row only when an explicit company/branch authorization
    /// scope contains the immutable scope carried by the projected aggregate.
    /// Holding routing alone is not an authorization decision.
    pub async fn get_for_scope(
        &self,
        inspection_id: Uuid,
        authorizing_scope: &TenantScope,
    ) -> Result<Option<(QualityInspection, u64)>, PersistenceError> {
        validate_authorizing_quality_scope(authorizing_scope)?;
        let Some((inspection, source_version)) = self.get(inspection_id).await? else {
            return Ok(None);
        };
        if !authorizing_scope.contains(&inspection.scope) {
            return Err(PersistenceError::InvalidData(
                "quality read scope is not authorized".into(),
            ));
        }
        Ok(Some((inspection, source_version)))
    }

    /// Read typed Quality facts from the physically separate tenant read pool.
    /// The source aggregate and command history never participate in this
    /// query. Rows are bounded and keyset-paginated by projection time plus
    /// immutable inspection identity.
    pub async fn list_read_rows(
        &self,
        scope: QualityReadScope,
        page_size: u32,
        before: Option<&QualityInspectionReadCursor>,
    ) -> Result<QualityInspectionReadPage, PersistenceError> {
        validate_quality_read_scope(scope)?;
        if page_size == 0 || page_size > MAX_QUALITY_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "quality read page size must be 1..={MAX_QUALITY_READ_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = before
            && cursor.inspection_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "quality read cursor inspection ID is invalid".into(),
            ));
        }
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT inspection_id, company_id, branch_id, payload, source_version,
                    projected_at
             FROM erpx_tenant.quality_inspection_read_rows
             WHERE holding_ref=$1 AND company_id=$2
               AND ($3::UUID IS NULL OR branch_id=$3)
               AND ($4::TIMESTAMPTZ IS NULL OR
                    (projected_at, inspection_id) < ($4, $5))
             ORDER BY projected_at DESC, inspection_id DESC
             LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.projected_at))
        .bind(before.map(|cursor| cursor.inspection_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let page_len = usize::try_from(page_size)
            .map_err(|_| PersistenceError::InvalidData("quality read page size overflow".into()))?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut decoded = Vec::with_capacity(selected.len());
        for row in selected {
            let inspection_id: Uuid = row.try_get("inspection_id")?;
            let inspection: QualityInspection = serde_json::from_value(row.try_get("payload")?)?;
            inspection
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            if inspection.id != inspection_id
                || inspection.holding_id != scope.holding_id
                || inspection.company_id != scope.company_id
                || inspection.branch_id != row.try_get("branch_id")?
            {
                return Err(PersistenceError::InvalidData(
                    "quality read row scope or identity mismatch".into(),
                ));
            }
            let source_version = as_u64(row.try_get("source_version")?, "quality read version")?;
            if inspection.version != source_version {
                return Err(PersistenceError::InvalidData(
                    "quality read row version mismatch".into(),
                ));
            }
            decoded.push(QualityInspectionReadRow {
                inspection,
                source_version,
                projected_at: row.try_get("projected_at")?,
            });
        }
        let next_cursor = has_more
            .then(|| {
                decoded.last().map(|row| QualityInspectionReadCursor {
                    projected_at: row.projected_at,
                    inspection_id: row.inspection.id,
                })
            })
            .flatten();
        Ok(QualityInspectionReadPage {
            rows: decoded,
            next_cursor,
            has_more,
        })
    }

    pub async fn get_authoritative(
        &self,
        inspection_id: Uuid,
    ) -> Result<Option<QualityInspection>, PersistenceError> {
        if inspection_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "quality inspection ID cannot be nil".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT payload FROM erpx_tenant.quality_inspections
             WHERE holding_ref=$1 AND inspection_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(inspection_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else { return Ok(None) };
        let inspection: QualityInspection = serde_json::from_value(row.try_get("payload")?)?;
        inspection
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if inspection.id != inspection_id {
            return Err(PersistenceError::InvalidData(
                "quality source identity mismatch".into(),
            ));
        }
        Ok(Some(inspection))
    }

    pub async fn replay_receipt(
        &self,
        idempotency_key: &str,
        request_hash: &str,
    ) -> Result<Option<QualityWriteReceipt>, PersistenceError> {
        validate_text(idempotency_key, "idempotency_key", 512)?;
        validate_text(request_hash, "request_hash", 64)?;
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT request_hash, response FROM erpx_tenant.quality_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else { return Ok(None) };
        let stored_hash: String = row.try_get("request_hash")?;
        if stored_hash != request_hash {
            return Err(PersistenceError::IdempotencyConflict {
                key: idempotency_key.into(),
            });
        }
        let mut receipt: QualityWriteReceipt = serde_json::from_value(row.try_get("response")?)?;
        receipt.replayed = true;
        Ok(Some(receipt))
    }

    pub async fn reconcile(&self) -> Result<QualityProjectionReconciliation, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let source_rows = sqlx::query(
            "SELECT q.inspection_id, q.company_id, q.branch_id, q.version, q.payload,
                    (SELECT count(*)
                     FROM erpx_tenant.outbox_events o
                     WHERE o.holding_ref=q.holding_ref
                       AND o.aggregate_type='quality_inspection'
                       AND o.aggregate_id=q.inspection_id) AS event_count,
                    (SELECT max(o.aggregate_version)
                     FROM erpx_tenant.outbox_events o
                     WHERE o.holding_ref=q.holding_ref
                       AND o.aggregate_type='quality_inspection'
                       AND o.aggregate_id=q.inspection_id) AS max_event_version,
                    (SELECT o.event_id
                     FROM erpx_tenant.outbox_events o
                     WHERE o.holding_ref=q.holding_ref
                       AND o.aggregate_type='quality_inspection'
                       AND o.aggregate_id=q.inspection_id
                     ORDER BY o.aggregate_version DESC, o.occurred_at DESC, o.event_id DESC
                     LIMIT 1) AS latest_event_id,
                    (SELECT o.payload
                     FROM erpx_tenant.outbox_events o
                     WHERE o.holding_ref=q.holding_ref
                       AND o.aggregate_type='quality_inspection'
                       AND o.aggregate_id=q.inspection_id
                     ORDER BY o.aggregate_version DESC, o.occurred_at DESC, o.event_id DESC
                     LIMIT 1) AS latest_event_payload
             FROM erpx_tenant.quality_inspections q
             WHERE q.holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *tx)
        .await?;
        let source_events: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='quality_inspection'",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        let source_facts = source_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    row.try_get("inspection_id")?,
                    QualitySourceProjectionFact {
                        company_id: row.try_get("company_id")?,
                        branch_id: row.try_get("branch_id")?,
                        version: as_u64(row.try_get("version")?, "quality source version")?,
                        payload: row.try_get("payload")?,
                        event_count: as_u64(
                            row.try_get("event_count")?,
                            "quality source event count",
                        )?,
                        max_event_version: row
                            .try_get::<Option<i64>, _>("max_event_version")?
                            .map(|value| as_u64(value, "quality source event version"))
                            .transpose()?,
                        latest_event_id: row.try_get("latest_event_id")?,
                        latest_event_payload: row.try_get("latest_event_payload")?,
                        event_versions: Vec::new(),
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut read_tx = self.begin_read().await?;
        let checkpoint_rows = sqlx::query(
            "SELECT inspection_id, source_version, last_event_id
             FROM erpx_tenant.quality_projection_checkpoints
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let checkpoints = checkpoint_rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>((
                    row.try_get("inspection_id")?,
                    (
                        as_u64(row.try_get("source_version")?, "quality checkpoint version")?,
                        row.try_get("last_event_id")?,
                    ),
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let projected_rows = sqlx::query(
            "SELECT inspection_id, holding_ref, company_id, branch_id, source_version, payload
             FROM erpx_tenant.quality_inspection_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let projected_facts = projected_rows
            .into_iter()
            .map(|row| {
                let inspection_id: Uuid = row.try_get("inspection_id")?;
                Ok::<_, PersistenceError>((
                    inspection_id,
                    QualityProjectedFact {
                        holding_ref: row.try_get("holding_ref")?,
                        company_id: row.try_get("company_id")?,
                        branch_id: row.try_get("branch_id")?,
                        source_version: as_u64(
                            row.try_get("source_version")?,
                            "quality read version",
                        )?,
                        payload: row.try_get("payload")?,
                        checkpoint: checkpoints.get(&inspection_id).copied(),
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(reconcile_projection_facts(
            &self.holding_ref,
            &source_facts,
            &projected_facts,
            &checkpoints,
            as_u64(source_events, "quality source event count")?,
        ))
    }

    pub async fn reconcile_projection(
        &self,
    ) -> Result<QualityProjectionReconciliation, PersistenceError> {
        self.reconcile().await
    }

    /// Atomically replace the disposable Quality projection from a fenced
    /// authoritative source snapshot.
    ///
    /// The source transaction holds a SHARE lock on the outbox for the whole
    /// replay and the read transaction owns the clear, every projection write,
    /// and checkpoint update. Any source movement, malformed event, version
    /// conflict, or read-side failure drops the read transaction and preserves
    /// the previously committed projection. Callers must not emulate a
    /// replacement rebuild with `clear_projection` followed by individual
    /// `project_event` calls.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_event_count: Option<u64>,
        page_size: u32,
    ) -> Result<(QualityProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_QUALITY_SOURCE_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "quality source event page size must be 1..={MAX_QUALITY_SOURCE_PAGE_SIZE}"
            )));
        }

        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_event_count =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if expected_source_event_count.is_some_and(|expected| expected != actual_source_event_count)
        {
            return Err(PersistenceError::InvalidData(
                "quality source fence changed before replacement rebuild".into(),
            ));
        }
        if actual_source_event_count > MAX_QUALITY_PROJECTION_EVENTS {
            return Err(PersistenceError::InvalidData(
                "quality projection source event bound exceeded".into(),
            ));
        }
        ensure_no_duplicate_source_versions(&mut source_tx, &self.holding_ref).await?;

        let mut read_tx = self.begin_read().await?;
        lock_quality_transaction(
            &mut read_tx,
            quality_projection_lock_name(&self.holding_ref),
        )
        .await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.quality_inspection_read_rows WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.quality_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let preparation = QualityProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        };

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
                let inspection = quality_inspection_from_payload(
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )?;
                if self
                    .project_event_in_transaction(
                        &mut read_tx,
                        event.event_id,
                        event.aggregate_id,
                        event.aggregate_version,
                        &event.payload,
                        &inspection,
                    )
                    .await?
                    .applied
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "quality source event cursor is missing".into(),
                ));
            }
        }

        let final_source_event_count =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_event_count != actual_source_event_count {
            return Err(PersistenceError::InvalidData(
                "quality source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<QualityProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        lock_quality_transaction(&mut tx, quality_projection_lock_name(&self.holding_ref)).await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.quality_inspection_read_rows WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.quality_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(QualityProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        })
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref).await
    }
}

async fn read_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&QualitySourceEventCursor>,
    page_size: u32,
) -> Result<QualitySourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_QUALITY_SOURCE_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(
            "quality source event page size is out of bounds".into(),
        ));
    }
    ensure_no_duplicate_source_versions(tx, holding_ref).await?;
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        if cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "quality source cursor is invalid".into(),
            ));
        }
        sqlx::query(
            "SELECT event_id, aggregate_id, aggregate_version, payload, occurred_at,
                    event_type, contract_version, event_version, tenant_scope, actor,
                    correlation_id, idempotency_key
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='quality_inspection'
               AND (aggregate_id>$2 OR (aggregate_id=$2 AND aggregate_version>$3))
             ORDER BY aggregate_id, aggregate_version LIMIT $4",
        )
        .bind(holding_ref)
        .bind(cursor.aggregate_id)
        .bind(as_i64(cursor.aggregate_version, "quality cursor version")?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, aggregate_id, aggregate_version, payload, occurred_at,
                    event_type, contract_version, event_version, tenant_scope, actor,
                    correlation_id, idempotency_key
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='quality_inspection'
             ORDER BY aggregate_id, aggregate_version LIMIT $2",
        )
        .bind(holding_ref)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size)
        .map_err(|_| PersistenceError::InvalidData("quality page size overflow".into()))?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "quality source event version",
            )?;
            let payload: Value = row.try_get("payload")?;
            quality_source_inspection_from_row(
                &row,
                event_id,
                aggregate_id,
                aggregate_version,
                &payload,
            )?;
            Ok(QualitySourceEvent {
                event_id,
                aggregate_id,
                aggregate_version,
                payload,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = has_more
        .then(|| {
            events.last().map(|event| QualitySourceEventCursor {
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
            })
        })
        .flatten();
    Ok(QualitySourceEventPage {
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
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type='quality_inspection'",
    )
    .bind(holding_ref)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "quality source event high watermark")
}

async fn ensure_no_duplicate_source_versions(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let duplicate = sqlx::query(
        "SELECT aggregate_id, aggregate_version
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type='quality_inspection'
         GROUP BY aggregate_id, aggregate_version
         HAVING COUNT(*) > 1
         LIMIT 1",
    )
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if duplicate.is_some() {
        return Err(PersistenceError::InvalidData(
            "quality source stream contains a duplicate aggregate version".into(),
        ));
    }
    Ok(())
}

fn validate_event_payload(
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    if event_id.is_nil() || aggregate_id.is_nil() || aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "quality event identity is invalid".into(),
        ));
    }
    if payload.get("aggregate_type") != Some(&Value::String("quality_inspection".into()))
        || payload.get("version") != Some(&Value::from(aggregate_version))
        || !payload.get("aggregate").is_some_and(Value::is_object)
    {
        return Err(PersistenceError::InvalidData(
            "quality event envelope is invalid".into(),
        ));
    }
    Ok(())
}

fn quality_inspection_from_payload(
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<QualityInspection, PersistenceError> {
    validate_event_payload(event_id, aggregate_id, aggregate_version, payload)?;
    let inspection: QualityInspection =
        serde_json::from_value(payload.get("aggregate").cloned().ok_or_else(|| {
            PersistenceError::InvalidData("quality event aggregate is missing".into())
        })?)?;
    inspection
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if inspection.id != aggregate_id || inspection.version != aggregate_version {
        return Err(PersistenceError::InvalidData(
            "quality event identity/version mismatch".into(),
        ));
    }
    Ok(inspection)
}

fn quality_source_inspection_from_row(
    row: &PgRow,
    event_id: Uuid,
    expected_aggregate_id: Uuid,
    expected_aggregate_version: u64,
    expected_payload: &Value,
) -> Result<QualityInspection, PersistenceError> {
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let aggregate_version = as_u64(
        row.try_get("aggregate_version")?,
        "quality source event version",
    )?;
    let payload: Value = row.try_get("payload")?;
    if aggregate_id != expected_aggregate_id
        || aggregate_version != expected_aggregate_version
        || payload != *expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "quality source event does not match requested identity or payload".into(),
        ));
    }
    validate_quality_source_envelope(QualitySourceEnvelope {
        event_id,
        aggregate_id,
        aggregate_version,
        payload,
        event_type: row.try_get("event_type")?,
        contract_version: row.try_get("contract_version")?,
        event_version: row.try_get("event_version")?,
        tenant_scope: row.try_get("tenant_scope")?,
        actor: row.try_get("actor")?,
        correlation_id: row.try_get("correlation_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
    })
}

struct QualitySourceEnvelope {
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
    event_type: String,
    contract_version: Option<i32>,
    event_version: Option<i32>,
    tenant_scope: Option<Value>,
    actor: Option<Value>,
    correlation_id: Option<Uuid>,
    idempotency_key: Option<String>,
}

fn validate_quality_source_envelope(
    envelope: QualitySourceEnvelope,
) -> Result<QualityInspection, PersistenceError> {
    validate_event_payload(
        envelope.event_id,
        envelope.aggregate_id,
        envelope.aggregate_version,
        &envelope.payload,
    )?;
    if envelope.event_type != "quality.inspection.v1"
        || envelope.contract_version != Some(1)
        || envelope.event_version != Some(QUALITY_EVENT_VERSION)
    {
        return Err(PersistenceError::InvalidData(
            "quality source event envelope version or type is invalid".into(),
        ));
    }
    let tenant_scope: TenantScope =
        serde_json::from_value(envelope.tenant_scope.ok_or_else(|| {
            PersistenceError::InvalidData("quality source event tenant scope is missing".into())
        })?)?;
    let actor: ActorAttribution = serde_json::from_value(envelope.actor.ok_or_else(|| {
        PersistenceError::InvalidData("quality source event actor is missing".into())
    })?)?;
    if actor.principal_ref.trim().is_empty() {
        return Err(PersistenceError::InvalidData(
            "quality source event actor is invalid".into(),
        ));
    }
    if envelope.correlation_id.is_none_or(|value| value.is_nil()) {
        return Err(PersistenceError::InvalidData(
            "quality source event correlation ID is invalid".into(),
        ));
    }
    let idempotency_key = envelope.idempotency_key.ok_or_else(|| {
        PersistenceError::InvalidData("quality source event idempotency key is missing".into())
    })?;
    validate_text(
        &idempotency_key,
        "quality source event idempotency key",
        512,
    )?;
    let inspection: QualityInspection =
        serde_json::from_value(envelope.payload.get("aggregate").cloned().ok_or_else(|| {
            PersistenceError::InvalidData("quality event aggregate is missing".into())
        })?)?;
    inspection
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if inspection.id != envelope.aggregate_id || inspection.version != envelope.aggregate_version {
        return Err(PersistenceError::InvalidData(
            "quality source event identity/version mismatch".into(),
        ));
    }
    if !tenant_scope.contains(&inspection.scope) {
        return Err(PersistenceError::InvalidData(
            "quality source event scope does not contain aggregate scope".into(),
        ));
    }
    Ok(inspection)
}

pub fn quality_request_hash(
    payload: &Value,
    expected_version: Option<u64>,
    context: &CommandContext,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        payload,
        expected_version,
        context.command_id,
        context.idempotency_key.as_str(),
    ))?)))
}

fn validate_text(value: &str, field: &'static str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn validate_quality_read_scope(scope: QualityReadScope) -> Result<(), PersistenceError> {
    for (value, field) in [
        (scope.holding_id, "quality read holding ID"),
        (scope.company_id, "quality read company ID"),
    ] {
        if value.is_nil() {
            return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
        }
    }
    if scope.branch_id.is_some_and(|branch_id| branch_id.is_nil()) {
        return Err(PersistenceError::InvalidData(
            "quality read branch ID is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_authorizing_quality_scope(scope: &TenantScope) -> Result<(), PersistenceError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } if !holding_id.is_nil() && !company_id.is_nil() => Ok(()),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil() => Ok(()),
        _ => Err(PersistenceError::InvalidData(
            "quality read authorization requires an explicit company or branch scope".into(),
        )),
    }
}

fn validate_quality_source_snapshot(
    aggregate_id: Uuid,
    source_version: u64,
    source_payload: &Value,
    event_version: u64,
    event_inspection: &QualityInspection,
) -> Result<(), PersistenceError> {
    let source: QualityInspection = serde_json::from_value(source_payload.clone())?;
    source
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if source.id != aggregate_id
        || source.version != source_version
        || source_version < event_version
    {
        return Err(PersistenceError::InvalidData(
            "quality source snapshot identity or version is invalid".into(),
        ));
    }
    if source_version == event_version && source_payload != &serde_json::to_value(event_inspection)?
    {
        return Err(PersistenceError::InvalidData(
            "quality source snapshot does not match its authoritative outbox event".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
fn reconcile_projection_versions(
    source_versions: &HashMap<Uuid, u64>,
    projected_versions: &HashMap<Uuid, u64>,
    source_event_count: u64,
    checkpoint_count: u64,
) -> QualityProjectionReconciliation {
    let mut missing_aggregate_count = 0;
    let mut stale_aggregate_count = 0;
    let mut ahead_aggregate_count = 0;
    for (inspection_id, source_version) in source_versions {
        match projected_versions.get(inspection_id) {
            None => missing_aggregate_count += 1,
            Some(projected_version) if projected_version < source_version => {
                stale_aggregate_count += 1;
            }
            Some(projected_version) if projected_version > source_version => {
                ahead_aggregate_count += 1;
            }
            Some(_) => {}
        }
    }
    let orphan_aggregate_count = projected_versions
        .keys()
        .filter(|inspection_id| !source_versions.contains_key(inspection_id))
        .count() as u64;
    let source_aggregate_count = source_versions.len() as u64;
    let projected_aggregate_count = projected_versions.len() as u64;
    QualityProjectionReconciliation {
        source_event_count,
        source_aggregate_count,
        projected_aggregate_count,
        missing_aggregate_count,
        stale_aggregate_count,
        ahead_aggregate_count,
        orphan_aggregate_count,
        missing_checkpoint_count: projected_aggregate_count.saturating_sub(checkpoint_count),
        orphan_checkpoint_count: checkpoint_count.saturating_sub(projected_aggregate_count),
        invalid_projection_count: 0,
    }
}

fn reconcile_projection_facts(
    holding_ref: &str,
    source_facts: &HashMap<Uuid, QualitySourceProjectionFact>,
    projected_facts: &HashMap<Uuid, QualityProjectedFact>,
    checkpoints: &HashMap<Uuid, (u64, Uuid)>,
    source_event_count: u64,
) -> QualityProjectionReconciliation {
    let mut reconciliation = QualityProjectionReconciliation {
        source_event_count,
        source_aggregate_count: source_facts.len() as u64,
        projected_aggregate_count: projected_facts.len() as u64,
        ..QualityProjectionReconciliation::default()
    };

    let source_fact_event_count = source_facts
        .values()
        .fold(0_u64, |total, fact| total.saturating_add(fact.event_count));
    if source_fact_event_count != source_event_count {
        reconciliation.invalid_projection_count =
            reconciliation.invalid_projection_count.saturating_add(1);
    }

    for (inspection_id, source) in source_facts {
        let Some(projected) = projected_facts.get(inspection_id) else {
            reconciliation.missing_aggregate_count =
                reconciliation.missing_aggregate_count.saturating_add(1);
            continue;
        };

        if projected.source_version < source.version {
            reconciliation.stale_aggregate_count =
                reconciliation.stale_aggregate_count.saturating_add(1);
        } else if projected.source_version > source.version {
            reconciliation.ahead_aggregate_count =
                reconciliation.ahead_aggregate_count.saturating_add(1);
        }

        if projected.holding_ref != holding_ref
            || projected.company_id != source.company_id
            || projected.branch_id != source.branch_id
            || projected.payload != source.payload
        {
            reconciliation.invalid_projection_count =
                reconciliation.invalid_projection_count.saturating_add(1);
        }

        let source_stream_is_contiguous = source.version > 0
            && source.event_count == source.version
            && source.max_event_version == Some(source.version)
            && source.latest_event_id.is_some()
            && source.latest_event_payload.as_ref().is_some_and(|payload| {
                payload.get("aggregate_type") == Some(&Value::String("quality_inspection".into()))
                    && payload.get("version") == Some(&Value::from(source.version))
                    && payload.get("aggregate") == Some(&source.payload)
            });
        if !source_stream_is_contiguous {
            reconciliation.invalid_projection_count =
                reconciliation.invalid_projection_count.saturating_add(1);
        }

        match projected.checkpoint {
            None => {
                reconciliation.missing_checkpoint_count =
                    reconciliation.missing_checkpoint_count.saturating_add(1);
            }
            Some((checkpoint_version, checkpoint_event_id)) => {
                let checkpoint_matches_projection =
                    checkpoint_version == projected.source_version && !checkpoint_event_id.is_nil();
                let checkpoint_matches_source = projected.source_version == source.version
                    && Some(checkpoint_event_id) == source.latest_event_id;
                if !checkpoint_matches_projection || !checkpoint_matches_source {
                    reconciliation.invalid_projection_count =
                        reconciliation.invalid_projection_count.saturating_add(1);
                }
            }
        }
    }

    reconciliation.orphan_aggregate_count = projected_facts
        .keys()
        .filter(|inspection_id| !source_facts.contains_key(*inspection_id))
        .count() as u64;
    reconciliation.orphan_checkpoint_count = checkpoints
        .keys()
        .filter(|inspection_id| !projected_facts.contains_key(*inspection_id))
        .count() as u64;
    reconciliation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_projection_replay_is_monotonic_and_conflicts_fail_closed() {
        let current_payload = serde_json::json!({"version": 2});
        let same_payload = serde_json::json!({"version": 2});
        let different_payload = serde_json::json!({"version": 3});

        assert_eq!(
            projection_decision(
                Some((2, &current_payload)),
                1,
                &serde_json::json!({"version": 1})
            )
            .expect("older replay is a no-op"),
            QualityProjectionDecision::Skip
        );
        assert_eq!(
            projection_decision(Some((2, &current_payload)), 2, &same_payload)
                .expect("same event is a no-op"),
            QualityProjectionDecision::Skip
        );
        assert!(projection_decision(Some((2, &current_payload)), 2, &different_payload).is_err());
        assert!(projection_decision(Some((2, &current_payload)), 4, &different_payload).is_err());
        assert_eq!(
            projection_decision(None, 1, &serde_json::json!({"version": 1}))
                .expect("first event applies"),
            QualityProjectionDecision::Apply
        );
        assert!(projection_decision(None, 2, &same_payload).is_err());
    }

    #[test]
    fn reconciliation_detects_version_drift_when_row_counts_match() {
        let stale = Uuid::new_v4();
        let ahead = Uuid::new_v4();
        let source_only = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        let source_versions = HashMap::from([(stale, 2), (ahead, 1), (source_only, 1)]);
        let projected_versions = HashMap::from([(stale, 1), (ahead, 2), (orphan, 1)]);

        let reconciliation =
            reconcile_projection_versions(&source_versions, &projected_versions, 3, 3);

        assert_eq!(reconciliation.source_aggregate_count, 3);
        assert_eq!(reconciliation.projected_aggregate_count, 3);
        assert_eq!(reconciliation.missing_aggregate_count, 1);
        assert_eq!(reconciliation.stale_aggregate_count, 1);
        assert_eq!(reconciliation.ahead_aggregate_count, 1);
        assert_eq!(reconciliation.orphan_aggregate_count, 1);
        assert!(!reconciliation.is_consistent());
    }

    #[test]
    fn reconciliation_detects_same_count_payload_scope_and_checkpoint_drift() {
        let inspection_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();
        let source_payload = serde_json::json!({
            "id": inspection_id,
            "status": "in_progress",
            "version": 2,
        });
        let source = HashMap::from([(
            inspection_id,
            QualitySourceProjectionFact {
                company_id,
                branch_id: Some(branch_id),
                version: 2,
                payload: source_payload.clone(),
                event_count: 2,
                event_versions: vec![1, 2],
                max_event_version: Some(2),
                latest_event_id: Some(event_id),
                latest_event_payload: Some(serde_json::json!({
                    "aggregate_type": "quality_inspection",
                    "aggregate": source_payload,
                    "version": 2,
                })),
            },
        )]);
        let projected = HashMap::from([(
            inspection_id,
            QualityProjectedFact {
                holding_ref: "holding-quality".into(),
                company_id: Uuid::new_v4(),
                branch_id: Some(branch_id),
                source_version: 2,
                payload: serde_json::json!({
                    "id": inspection_id,
                    "status": "passed",
                    "version": 2,
                }),
                checkpoint: Some((2, Uuid::new_v4())),
            },
        )]);
        let checkpoints = HashMap::from([(inspection_id, (2, Uuid::new_v4()))]);

        let reconciliation =
            reconcile_projection_facts("holding-quality", &source, &projected, &checkpoints, 2);

        assert_eq!(reconciliation.source_aggregate_count, 1);
        assert_eq!(reconciliation.projected_aggregate_count, 1);
        assert_eq!(reconciliation.invalid_projection_count, 2);
        assert!(!reconciliation.is_consistent());
    }

    #[tokio::test]
    async fn store_rejects_invalid_configuration_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxQualityStore::new(pool.clone(), pool, " ").is_err());
    }

    #[tokio::test]
    async fn source_page_rejects_unbounded_page_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        let store = SqlxQualityStore::new(pool.clone(), pool, "quality-test").expect("store");
        assert!(store.read_source_event_page(None, 0).await.is_err());
    }
}
