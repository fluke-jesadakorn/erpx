//! Tenant-routed SQLx persistence for Assets and Maintenance.
//!
//! Asset and maintenance rows are source-owned in the tenant write database.
//! Read rows and custody projections are disposable. Every aggregate mutation
//! validates its domain payload, binds the holding RLS setting, and commits
//! source state, shared CQRS snapshot, outbox event, and idempotency receipt in
//! one transaction.

use chrono::{DateTime, Utc};
use erpx_domain::asset_maintenance::{
    ASSET_MAINTENANCE_CONTRACT_VERSION, Asset, AssetMaintenanceError,
    AssetMaintenanceProjectionReconciliation, AssetScope, CustodyTransfer, MaintenanceWorkOrder,
    OperationsCapacityImpactHandoff,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::PersistenceError;

pub const MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE: u32 = 500;
/// Maximum source-event history one Asset/Maintenance projection invocation
/// may drain. Exactly one million events is supported.
pub const MAX_ASSET_MAINTENANCE_PROJECTION_EVENTS: u64 = 1_000_000;
const ASSET_MAINTENANCE_EVENT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMaintenanceWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub custody_rows_cleared: u64,
    pub handoff_rows_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityHandoffReceipt {
    pub source_event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMaintenanceSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetMaintenanceSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetMaintenanceSourceEventPage {
    pub events: Vec<AssetMaintenanceSourceEvent>,
    pub next_cursor: Option<AssetMaintenanceSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetMaintenanceReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl AssetMaintenanceReadScope {
    pub fn contains(&self, scope: &AssetScope) -> bool {
        self.holding_id == scope.holding_id
            && self.company_id == scope.company_id
            && self
                .branch_id
                .is_none_or(|branch_id| scope.branch_id == Some(branch_id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetMaintenanceReadCursor {
    pub projected_at: DateTime<Utc>,
    pub aggregate_type: &'static str,
    pub aggregate_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetMaintenanceReadListRow {
    pub aggregate: AssetMaintenanceAggregate,
    pub reference: String,
    pub source_version: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetMaintenanceReadPage {
    pub rows: Vec<AssetMaintenanceReadListRow>,
    pub next_cursor: Option<AssetMaintenanceReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum AssetMaintenanceAggregate {
    Asset(Asset),
    MaintenanceWorkOrder(MaintenanceWorkOrder),
}

impl AssetMaintenanceAggregate {
    pub fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Asset(_) => "asset",
            Self::MaintenanceWorkOrder(_) => "maintenance_work_order",
        }
    }

    pub fn aggregate_id(&self) -> Uuid {
        match self {
            Self::Asset(value) => value.id,
            Self::MaintenanceWorkOrder(value) => value.id,
        }
    }

    pub fn version(&self) -> u64 {
        match self {
            Self::Asset(value) => value.version,
            Self::MaintenanceWorkOrder(value) => value.version,
        }
    }

    fn scope(&self) -> &AssetScope {
        match self {
            Self::Asset(value) => &value.scope,
            Self::MaintenanceWorkOrder(value) => &value.scope,
        }
    }

    fn reference(&self) -> &str {
        match self {
            Self::Asset(value) => &value.reference,
            Self::MaintenanceWorkOrder(value) => &value.reference,
        }
    }

    fn company_id(&self) -> Uuid {
        self.scope().company_id
    }

    fn branch_id(&self) -> Option<Uuid> {
        self.scope().branch_id
    }

    pub fn validate(&self) -> Result<(), PersistenceError> {
        match self {
            Self::Asset(value) => validate_asset(value),
            Self::MaintenanceWorkOrder(value) => validate_work_order(value),
        }
    }

    fn payload(&self) -> Result<Value, PersistenceError> {
        Ok(match self {
            Self::Asset(value) => serde_json::to_value(value)?,
            Self::MaintenanceWorkOrder(value) => serde_json::to_value(value)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SqlxAssetMaintenanceStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxAssetMaintenanceStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
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

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    pub async fn save(
        &self,
        aggregate: &AssetMaintenanceAggregate,
        idempotency_key: &str,
        expected_version: Option<u64>,
    ) -> Result<AssetMaintenanceWriteReceipt, PersistenceError> {
        aggregate.validate()?;
        validate_text(idempotency_key, "idempotency_key", 512)?;
        let version = aggregate.version();
        if version == 0 {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance aggregate version must be positive".into(),
            ));
        }
        let aggregate_type = aggregate.aggregate_type();
        let aggregate_id = aggregate.aggregate_id();
        let payload = aggregate.payload()?;
        let request_hash =
            hash_request(aggregate_type, &payload, expected_version, idempotency_key)?;
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_hash, response
             FROM erpx_tenant.asset_maintenance_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.into(),
                });
            }
            let mut receipt: AssetMaintenanceWriteReceipt =
                serde_json::from_value(row.try_get("response")?)?;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }
        if let AssetMaintenanceAggregate::MaintenanceWorkOrder(work_order) = aggregate {
            validate_work_order_asset_link(&mut tx, &self.holding_ref, work_order).await?;
        }
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM erpx_tenant.asset_maintenance_aggregates
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| as_u64(value, "Asset/Maintenance aggregate version"))
        .transpose()?;
        // A missing source row is the authoritative version-zero state.  The
        // command API expresses that state as `Some(0)` for an optimistic
        // create, while `None` continues to mean that no version constraint
        // was requested.  Compare a supplied expectation to the normalized
        // persisted version, but retain the raw `current` value in a
        // concurrency error so diagnostics still distinguish absence from a
        // stored row.
        if expected_version.is_some_and(|expected| expected != current.unwrap_or(0)) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                expected: expected_version,
                actual: current,
            });
        }
        if version != current.unwrap_or(0).saturating_add(1) {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance aggregate version is not contiguous".into(),
            ));
        }
        if current.is_some() {
            sqlx::query(
                "UPDATE erpx_tenant.asset_maintenance_aggregates
                 SET company_id=$3, branch_id=$4, reference=$5, version=$6,
                     idempotency_key=$7, payload=$8, updated_at=now()
                 WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$9",
            )
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(aggregate.company_id())
            .bind(aggregate.branch_id())
            .bind(aggregate.reference())
            .bind(as_i64(version, "Asset/Maintenance version")?)
            .bind(idempotency_key)
            .bind(&payload)
            .bind(&self.holding_ref)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.asset_maintenance_aggregates
                    (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
                     reference, version, idempotency_key, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(aggregate_id)
            .bind(aggregate_type)
            .bind(&self.holding_ref)
            .bind(aggregate.company_id())
            .bind(aggregate.branch_id())
            .bind(aggregate.reference())
            .bind(as_i64(version, "Asset/Maintenance version")?)
            .bind(idempotency_key)
            .bind(&payload)
            .execute(&mut *tx)
            .await?;
        }
        if let AssetMaintenanceAggregate::Asset(asset) = aggregate {
            persist_custody_history(&mut tx, &self.holding_ref, asset).await?;
        }
        persist_shared_outbox(
            &mut tx,
            &self.holding_ref,
            aggregate_type,
            aggregate_id,
            version,
            idempotency_key,
            aggregate.scope(),
            &payload,
        )
        .await?;
        let receipt = AssetMaintenanceWriteReceipt {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            version,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_maintenance_command_idempotency
                (idempotency_key, holding_ref, operation, aggregate_type,
                 aggregate_id, request_hash, response)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(idempotency_key)
        .bind(&self.holding_ref)
        .bind(aggregate_type)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(request_hash)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn save_capacity_handoff(
        &self,
        handoff: &OperationsCapacityImpactHandoff,
    ) -> Result<CapacityHandoffReceipt, PersistenceError> {
        validate_handoff(handoff)?;
        let payload = serde_json::to_value(handoff)?;
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(
            "SELECT source_event_id, payload FROM erpx_tenant.asset_operations_capacity_handoffs
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&handoff.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let source_event_id: Uuid = row.try_get("source_event_id")?;
            let stored_payload: Value = row.try_get("payload")?;
            // An idempotency key binds the complete handoff command, not only
            // the source event identity.  Returning a replay for a changed
            // downtime, scope, or provenance payload would silently accept a
            // conflicting capacity impact and make the outbox non-auditable.
            if source_event_id != handoff.source.source_event_id || stored_payload != payload {
                return Err(PersistenceError::IdempotencyConflict {
                    key: handoff.idempotency_key.clone(),
                });
            }
            tx.commit().await?;
            return Ok(CapacityHandoffReceipt {
                source_event_id,
                replayed: true,
            });
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_operations_capacity_handoffs
                (source_event_id, source_id, holding_ref, company_id, branch_id,
                 reference, source_domain, source_type, asset_id, downtime_minutes,
                 occurred_at, idempotency_key, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        )
        .bind(handoff.source.source_event_id)
        .bind(handoff.source.source_id)
        .bind(&self.holding_ref)
        .bind(handoff.scope.company_id)
        .bind(handoff.scope.branch_id)
        .bind(&handoff.reference)
        .bind(&handoff.source.source_domain)
        .bind(&handoff.source.source_type)
        .bind(handoff.asset_id)
        .bind(handoff.downtime_minutes)
        .bind(handoff.occurred_at)
        .bind(&handoff.idempotency_key)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(handoff.source.source_event_id)
        .bind("operations_capacity_handoff")
        .bind(handoff.source.source_id)
        .bind(&self.holding_ref)
        .bind("assets.operations_capacity_handoff.v1")
        .bind(&payload)
        .bind(handoff.occurred_at)
        .bind(i32::from(ASSET_MAINTENANCE_CONTRACT_VERSION))
        .bind(i32::from(ASSET_MAINTENANCE_EVENT_VERSION))
        .bind(serde_json::to_value(handoff.scope.tenant_scope())?)
        .bind(Option::<Value>::None)
        .bind(handoff.source.source_event_id)
        .bind(Option::<Uuid>::None)
        .bind(&handoff.idempotency_key)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CapacityHandoffReceipt {
            source_event_id: handoff.source.source_event_id,
            replayed: false,
        })
    }

    /// Project one Operations capacity handoff only from its immutable
    /// source row and matching outbox envelope.  The read row is disposable;
    /// a conflicting existing row is rejected rather than overwritten.
    pub async fn project_capacity_handoff(
        &self,
        source_event_id: Uuid,
    ) -> Result<bool, PersistenceError> {
        validate_uuid(
            source_event_id,
            "Asset/Maintenance capacity handoff event ID",
        )?;
        let mut source_tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT h.source_event_id, h.source_id, h.holding_ref, h.company_id,
                    h.branch_id, h.reference, h.source_domain, h.source_type,
                    h.asset_id, h.downtime_minutes, h.occurred_at,
                    h.idempotency_key, h.payload,
                    o.aggregate_type, o.aggregate_id, o.aggregate_version,
                    o.event_type, o.payload AS outbox_payload,
                    o.contract_version, o.event_version, o.tenant_scope,
                    o.actor, o.correlation_id, o.causation_id,
                    o.idempotency_key AS outbox_idempotency_key
             FROM erpx_tenant.asset_operations_capacity_handoffs h
             JOIN erpx_tenant.outbox_events o
               ON o.event_id=h.source_event_id AND o.holding_ref=h.holding_ref
             WHERE h.source_event_id=$1 AND h.holding_ref=$2
             FOR UPDATE",
        )
        .bind(source_event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *source_tx)
        .await?;
        source_tx.commit().await?;
        let Some(row) = row else {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance capacity handoff is not an authoritative source event".into(),
            ));
        };
        let source_id: Uuid = row.try_get("source_id")?;
        let holding_ref: String = row.try_get("holding_ref")?;
        let company_id: Uuid = row.try_get("company_id")?;
        let branch_id: Option<Uuid> = row.try_get("branch_id")?;
        let reference: String = row.try_get("reference")?;
        let source_domain: String = row.try_get("source_domain")?;
        let source_type: String = row.try_get("source_type")?;
        let asset_id: Uuid = row.try_get("asset_id")?;
        let downtime_minutes: i64 = row.try_get("downtime_minutes")?;
        let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
        let idempotency_key: String = row.try_get("idempotency_key")?;
        let payload: Value = row.try_get("payload")?;
        let handoff = parse_capacity_handoff_payload(&payload)?;
        validate_capacity_handoff_columns(
            &handoff,
            source_event_id,
            &holding_ref,
            source_id,
            company_id,
            branch_id,
            &reference,
            &source_domain,
            &source_type,
            asset_id,
            downtime_minutes,
            occurred_at,
            &idempotency_key,
            &self.holding_ref,
        )?;
        validate_capacity_handoff_outbox(
            source_event_id,
            row.try_get("aggregate_type")?,
            row.try_get("aggregate_id")?,
            row.try_get("aggregate_version")?,
            row.try_get("event_type")?,
            row.try_get("outbox_payload")?,
            row.try_get("contract_version")?,
            row.try_get("event_version")?,
            row.try_get("tenant_scope")?,
            row.try_get("actor")?,
            row.try_get("correlation_id")?,
            row.try_get("causation_id")?,
            row.try_get("outbox_idempotency_key")?,
            &handoff,
        )?;

        let mut read_tx = self.begin_read().await?;
        if let Some(existing) = sqlx::query(
            "SELECT source_id, holding_ref, company_id, branch_id, reference,
                    source_version, payload
             FROM erpx_tenant.asset_capacity_handoff_read_rows
             WHERE source_event_id=$1 AND holding_ref=$2
             FOR UPDATE",
        )
        .bind(source_event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *read_tx)
        .await?
        {
            let existing_source_id: Uuid = existing.try_get("source_id")?;
            let existing_holding_ref: String = existing.try_get("holding_ref")?;
            let existing_company_id: Uuid = existing.try_get("company_id")?;
            let existing_branch_id: Option<Uuid> = existing.try_get("branch_id")?;
            let existing_reference: String = existing.try_get("reference")?;
            let existing_version = as_u64(
                existing.try_get("source_version")?,
                "Asset/Maintenance capacity handoff projection version",
            )?;
            let existing_payload: Value = existing.try_get("payload")?;
            if existing_version != 1
                || existing_source_id != source_id
                || existing_holding_ref != holding_ref
                || existing_company_id != company_id
                || existing_branch_id != branch_id
                || existing_reference != reference
                || existing_payload != payload
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance capacity handoff read row conflicts with the authoritative source".into(),
                ));
            }
            read_tx.commit().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_capacity_handoff_read_rows
                (source_event_id, source_id, holding_ref, company_id, branch_id,
                 reference, source_version, payload, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,1,$7,now())",
        )
        .bind(source_event_id)
        .bind(source_id)
        .bind(&holding_ref)
        .bind(company_id)
        .bind(branch_id)
        .bind(&reference)
        .bind(&payload)
        .execute(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        Ok(true)
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&AssetMaintenanceSourceEventCursor>,
        page_size: u32,
    ) -> Result<AssetMaintenanceSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Asset/Maintenance source page size must be 1..={MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE}"
            )));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = if let Some(cursor) = cursor {
            validate_aggregate_type(&cursor.aggregate_type)?;
            validate_uuid(cursor.aggregate_id, "Asset/Maintenance source cursor ID")?;
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at,
                        contract_version, event_version, tenant_scope, actor,
                        correlation_id, causation_id, idempotency_key
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN ('asset', 'maintenance_work_order')
                   AND (aggregate_type > $2
                        OR (aggregate_type = $2 AND aggregate_id > $3)
                        OR (aggregate_type = $2 AND aggregate_id = $3
                            AND aggregate_version > $4))
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $5",
            )
            .bind(&self.holding_ref)
            .bind(&cursor.aggregate_type)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "Asset/Maintenance cursor version",
            )?)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at,
                        contract_version, event_version, tenant_scope, actor,
                        correlation_id, causation_id, idempotency_key
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN ('asset', 'maintenance_work_order')
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $2",
            )
            .bind(&self.holding_ref)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        validate_source_event_page_versions(&rows)?;
        tx.commit().await?;
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Asset/Maintenance page size is not representable".into())
        })?;
        let has_more = rows.len() > page_len;
        let events = rows
            .into_iter()
            .take(page_len)
            .map(|row| {
                let event_id: Uuid = row.try_get("event_id")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                let aggregate_type: String = row.try_get("aggregate_type")?;
                let event_type: String = row.try_get("event_type")?;
                let aggregate_version = as_u64(
                    row.try_get("aggregate_version")?,
                    "Asset/Maintenance source event version",
                )?;
                let payload: Value = row.try_get("payload")?;
                let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
                validate_uuid(event_id, "Asset/Maintenance source event ID")?;
                validate_uuid(aggregate_id, "Asset/Maintenance source aggregate ID")?;
                validate_aggregate_type(&aggregate_type)?;
                validate_outbox_event_envelope(
                    event_id,
                    &event_type,
                    &aggregate_type,
                    aggregate_id,
                    aggregate_version,
                    &payload,
                    row.try_get("contract_version")?,
                    row.try_get("event_version")?,
                    row.try_get("tenant_scope")?,
                    row.try_get("actor")?,
                    row.try_get("correlation_id")?,
                    row.try_get("causation_id")?,
                    row.try_get("idempotency_key")?,
                )?;
                Ok(AssetMaintenanceSourceEvent {
                    event_id,
                    event_type,
                    aggregate_type,
                    aggregate_id,
                    aggregate_version,
                    payload,
                    occurred_at,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = if has_more {
            events
                .last()
                .map(|event| AssetMaintenanceSourceEventCursor {
                    aggregate_type: event.aggregate_type.clone(),
                    aggregate_id: event.aggregate_id,
                    aggregate_version: event.aggregate_version,
                })
        } else {
            None
        };
        Ok(AssetMaintenanceSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    /// Return the holding-scoped count of authoritative Asset/Maintenance
    /// source events. Workers use it as a preflight bound before clearing a
    /// replaceable read model.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN ('asset', 'maintenance_work_order')",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        let count = as_u64(count, "Asset/Maintenance source event count")?;
        if count > MAX_ASSET_MAINTENANCE_PROJECTION_EVENTS {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance projection source event bound exceeded".into(),
            ));
        }
        Ok(count)
    }

    pub async fn get(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<(AssetMaintenanceAggregate, u64)>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Asset/Maintenance aggregate ID")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT payload, source_version
             FROM erpx_tenant.asset_maintenance_read_rows
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        let result = if let Some(row) = row {
            let payload: Value = row.try_get("payload")?;
            let aggregate: AssetMaintenanceAggregate = serde_json::from_value(payload)?;
            aggregate.validate()?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance read identity does not match key".into(),
                ));
            }
            let source_version = as_u64(
                row.try_get("source_version")?,
                "Asset/Maintenance source version",
            )?;
            if aggregate.version() != source_version {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance read payload version does not match source version".into(),
                ));
            }
            if let AssetMaintenanceAggregate::Asset(asset) = &aggregate {
                let custody_rows = sqlx::query(
                    "SELECT aggregate_version, transfer
                     FROM erpx_tenant.asset_custody_transfer_read_rows
                     WHERE asset_id=$1 AND holding_ref=$2
                     ORDER BY aggregate_version",
                )
                .bind(asset.id)
                .bind(&self.holding_ref)
                .fetch_all(&mut *tx)
                .await?;
                let projected_history = custody_rows
                    .into_iter()
                    .map(|row| {
                        let row_version = as_u64(
                            row.try_get("aggregate_version")?,
                            "Asset/Maintenance custody read version",
                        )?;
                        let transfer: CustodyTransfer =
                            serde_json::from_value(row.try_get("transfer")?)?;
                        validate_custody_transfer(&transfer)?;
                        if transfer.aggregate_version != row_version {
                            return Err(PersistenceError::InvalidData(
                                "Asset/Maintenance custody read version does not match transfer"
                                    .into(),
                            ));
                        }
                        Ok(transfer)
                    })
                    .collect::<Result<Vec<_>, PersistenceError>>()?;
                if projected_history != asset.custody_history {
                    return Err(PersistenceError::InvalidData(
                        "Asset/Maintenance custody read history does not match aggregate".into(),
                    ));
                }
            }
            Some(Ok((aggregate, source_version)))
        } else {
            None
        };
        tx.commit().await?;
        result.transpose()
    }

    pub async fn list_for_scope(
        &self,
        scope: AssetMaintenanceReadScope,
        page_size: u32,
        before: Option<&AssetMaintenanceReadCursor>,
    ) -> Result<AssetMaintenanceReadPage, PersistenceError> {
        validate_read_scope(scope)?;
        validate_read_page_size(page_size)?;
        if let Some(cursor) = before {
            validate_uuid(
                cursor.aggregate_id,
                "Asset/Maintenance read cursor aggregate ID",
            )?;
            validate_aggregate_type(cursor.aggregate_type)?;
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT r.aggregate_type, r.aggregate_id, r.company_id, r.branch_id,
                    r.reference, r.source_version, r.payload, r.projected_at,
                    c.source_version AS checkpoint_version,
                    c.last_event_id AS checkpoint_event_id
             FROM erpx_tenant.asset_maintenance_read_rows r
             LEFT JOIN erpx_tenant.asset_maintenance_projection_checkpoints c
               ON c.aggregate_type=r.aggregate_type
              AND c.aggregate_id=r.aggregate_id
              AND c.holding_ref=r.holding_ref
             WHERE r.holding_ref=$1 AND r.company_id=$2
               AND ($3::UUID IS NULL OR r.branch_id=$3)
               AND ($4::TIMESTAMPTZ IS NULL OR
                    (r.projected_at, r.aggregate_type, r.aggregate_id)
                    < ($4, $5, $6))
             ORDER BY r.projected_at DESC, r.aggregate_type DESC, r.aggregate_id DESC
             LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.projected_at))
        .bind(before.map(|cursor| cursor.aggregate_type))
        .bind(before.map(|cursor| cursor.aggregate_id))
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Asset/Maintenance page size is invalid".into())
        })?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut decoded = Vec::with_capacity(selected.len());
        for row in selected {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_aggregate_type(&aggregate_type)?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(aggregate_id, "Asset/Maintenance read aggregate ID")?;
            let source_version = as_u64(
                row.try_get("source_version")?,
                "Asset/Maintenance read source version",
            )?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|value| as_u64(value, "Asset/Maintenance checkpoint version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("checkpoint_event_id")?;
            if checkpoint_version != Some(source_version)
                || checkpoint_event_id.is_none_or(|event_id| event_id.is_nil())
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance list row checkpoint is missing or inconsistent".into(),
                ));
            }
            let aggregate: AssetMaintenanceAggregate =
                serde_json::from_value(row.try_get("payload")?)?;
            aggregate.validate()?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
                || aggregate.version() != source_version
                || !scope.contains(aggregate.scope())
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance list row identity, version, or scope is invalid".into(),
                ));
            }
            let persisted_company_id: Uuid = row.try_get("company_id")?;
            let persisted_branch_id: Option<Uuid> = row.try_get("branch_id")?;
            if persisted_company_id != aggregate.company_id()
                || persisted_branch_id != aggregate.branch_id()
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance list row typed scope columns are inconsistent".into(),
                ));
            }
            if let AssetMaintenanceAggregate::Asset(asset) = &aggregate {
                let custody_rows = sqlx::query(
                    "SELECT aggregate_version, transfer
                     FROM erpx_tenant.asset_custody_transfer_read_rows
                     WHERE asset_id=$1 AND holding_ref=$2
                     ORDER BY aggregate_version",
                )
                .bind(asset.id)
                .bind(&self.holding_ref)
                .fetch_all(&mut *tx)
                .await?;
                let projected_history = custody_rows
                    .into_iter()
                    .map(|custody_row| {
                        let row_version = as_u64(
                            custody_row.try_get("aggregate_version")?,
                            "Asset/Maintenance custody read version",
                        )?;
                        let transfer: CustodyTransfer =
                            serde_json::from_value(custody_row.try_get("transfer")?)?;
                        validate_custody_transfer(&transfer)?;
                        if transfer.aggregate_version != row_version {
                            return Err(PersistenceError::InvalidData(
                                "Asset/Maintenance custody read version does not match transfer"
                                    .into(),
                            ));
                        }
                        Ok(transfer)
                    })
                    .collect::<Result<Vec<_>, PersistenceError>>()?;
                if projected_history != asset.custody_history {
                    return Err(PersistenceError::InvalidData(
                        "Asset/Maintenance list custody history does not match aggregate".into(),
                    ));
                }
            }
            let reference: String = row.try_get("reference")?;
            validate_text(&reference, "Asset/Maintenance read reference", 255)?;
            decoded.push(AssetMaintenanceReadListRow {
                aggregate,
                reference,
                source_version,
                projected_at: row.try_get("projected_at")?,
            });
        }
        tx.commit().await?;
        let next_cursor = has_more
            .then(|| {
                decoded.last().map(|row| AssetMaintenanceReadCursor {
                    projected_at: row.projected_at,
                    aggregate_type: match row.aggregate.aggregate_type() {
                        "asset" => "asset",
                        "maintenance_work_order" => "maintenance_work_order",
                        _ => unreachable!("validated Asset/Maintenance aggregate type"),
                    },
                    aggregate_id: row.aggregate.aggregate_id(),
                })
            })
            .flatten();
        Ok(AssetMaintenanceReadPage {
            rows: decoded,
            next_cursor,
            has_more,
        })
    }

    /// Compare the authoritative Asset/Maintenance outbox with the
    /// disposable aggregate read rows. Custody and Operations handoff rows
    /// remain separately rebuildable and are not treated as source authority.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<AssetMaintenanceProjectionReconciliation, PersistenceError> {
        let mut source_tx = self.begin_write().await?;
        let source_rows = sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, contract_version, event_version,
                    tenant_scope, actor, correlation_id, causation_id, idempotency_key
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN ('asset', 'maintenance_work_order')
             ORDER BY aggregate_type, aggregate_id, aggregate_version",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        let source_handoff_rows = sqlx::query(
            "SELECT h.source_event_id, h.source_id, h.holding_ref, h.company_id,
                    h.branch_id, h.reference, h.source_domain, h.source_type,
                    h.asset_id, h.downtime_minutes, h.occurred_at,
                    h.idempotency_key, h.payload,
                    o.aggregate_type, o.aggregate_id, o.aggregate_version,
                    o.event_type, o.payload AS outbox_payload,
                    o.contract_version, o.event_version, o.tenant_scope,
                    o.actor, o.correlation_id, o.causation_id,
                    o.idempotency_key AS outbox_idempotency_key
             FROM erpx_tenant.asset_operations_capacity_handoffs h
             LEFT JOIN erpx_tenant.outbox_events o
               ON o.event_id=h.source_event_id AND o.holding_ref=h.holding_ref
             WHERE h.holding_ref=$1
             ORDER BY h.source_event_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source_evidence = HashMap::<(String, Uuid), SourceVersionEvidence>::new();
        let mut source_event_count = 0_u64;
        for row in source_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "Asset/Maintenance source version",
            )?;
            let payload: Value = row.try_get("payload")?;
            validate_outbox_event_envelope(
                event_id,
                &event_type,
                &aggregate_type,
                aggregate_id,
                aggregate_version,
                &payload,
                row.try_get("contract_version")?,
                row.try_get("event_version")?,
                row.try_get("tenant_scope")?,
                row.try_get("actor")?,
                row.try_get("correlation_id")?,
                row.try_get("causation_id")?,
                row.try_get("idempotency_key")?,
            )?;
            source_event_count = source_event_count.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("Asset/Maintenance event count overflow".into())
            })?;
            let evidence = source_evidence
                .entry((aggregate_type, aggregate_id))
                .or_default();
            evidence.event_count = evidence.event_count.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("Asset/Maintenance event count overflow".into())
            })?;
            evidence.latest_version = evidence.latest_version.max(aggregate_version);
            evidence.versions.insert(aggregate_version);
        }

        let mut source = HashMap::with_capacity(source_evidence.len());
        let mut source_version_gap_count = 0_u64;
        for (key, evidence) in source_evidence {
            let contiguous = evidence.latest_version > 0
                && evidence.event_count == evidence.latest_version
                && u64::try_from(evidence.versions.len()).is_ok_and(|count| {
                    count == evidence.latest_version && evidence.versions.contains(&1)
                });
            if !contiguous {
                source_version_gap_count =
                    source_version_gap_count.checked_add(1).ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "Asset/Maintenance source version gap count overflow".into(),
                        )
                    })?;
            }
            source.insert(key, evidence.latest_version);
        }

        let mut read_tx = self.begin_read().await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, source_version, payload
             FROM erpx_tenant.asset_maintenance_read_rows
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let custody_rows = sqlx::query(
            "SELECT asset_id, aggregate_version, transfer
             FROM erpx_tenant.asset_custody_transfer_read_rows
             WHERE holding_ref=$1
             ORDER BY asset_id, aggregate_version",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let handoff_read_rows = sqlx::query(
            "SELECT source_event_id, source_id, holding_ref, company_id,
                    branch_id, reference, source_version, payload
             FROM erpx_tenant.asset_capacity_handoff_read_rows
             WHERE holding_ref=$1
             ORDER BY source_event_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let mut projected = HashMap::with_capacity(read_rows.len());
        let mut projected_assets = HashMap::<Uuid, Vec<CustodyTransfer>>::new();
        for row in read_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_aggregate_type(&aggregate_type)?;
            validate_uuid(aggregate_id, "Asset/Maintenance projected aggregate ID")?;
            let source_version = as_u64(
                row.try_get("source_version")?,
                "Asset/Maintenance projected version",
            )?;
            let payload: Value = row.try_get("payload")?;
            let aggregate: AssetMaintenanceAggregate = serde_json::from_value(payload)?;
            aggregate.validate()?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
            {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance projected read identity is invalid".into(),
                ));
            }
            if aggregate.version() > source_version {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance projected read version regresses from payload".into(),
                ));
            }
            if let AssetMaintenanceAggregate::Asset(asset) = aggregate {
                projected_assets.insert(asset.id, asset.custody_history);
            }
            projected.insert((aggregate_type, aggregate_id), source_version);
        }

        let mut projected_custody = HashMap::<Uuid, Vec<CustodyTransfer>>::new();
        for row in custody_rows {
            let asset_id: Uuid = row.try_get("asset_id")?;
            validate_uuid(asset_id, "Asset/Maintenance custody read asset ID")?;
            let row_version = as_u64(
                row.try_get("aggregate_version")?,
                "Asset/Maintenance custody read version",
            )?;
            let transfer: CustodyTransfer = serde_json::from_value(row.try_get("transfer")?)?;
            validate_custody_transfer(&transfer)?;
            if transfer.aggregate_version != row_version {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance custody read version does not match transfer".into(),
                ));
            }
            projected_custody
                .entry(asset_id)
                .or_default()
                .push(transfer);
        }
        let mut custody_history_mismatch_count = 0_u64;
        for (asset_id, expected_history) in projected_assets {
            let actual_history = projected_custody.remove(&asset_id).unwrap_or_default();
            if actual_history != expected_history {
                custody_history_mismatch_count = custody_history_mismatch_count
                    .checked_add(1)
                    .ok_or_else(|| {
                        PersistenceError::InvalidData(
                            "Asset/Maintenance custody mismatch count overflow".into(),
                        )
                    })?;
            }
        }
        custody_history_mismatch_count = custody_history_mismatch_count
            .checked_add(u64::try_from(projected_custody.len()).map_err(|_| {
                PersistenceError::InvalidData(
                    "Asset/Maintenance custody mismatch count is not representable".into(),
                )
            })?)
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Asset/Maintenance custody mismatch count overflow".into(),
                )
            })?;

        let source_handoffs = source_handoff_rows
            .into_iter()
            .map(|row| {
                let source_event_id: Uuid = row.try_get("source_event_id")?;
                let source_id: Uuid = row.try_get("source_id")?;
                let holding_ref: String = row.try_get("holding_ref")?;
                let company_id: Uuid = row.try_get("company_id")?;
                let branch_id: Option<Uuid> = row.try_get("branch_id")?;
                let reference: String = row.try_get("reference")?;
                let source_domain: String = row.try_get("source_domain")?;
                let source_type: String = row.try_get("source_type")?;
                let asset_id: Uuid = row.try_get("asset_id")?;
                let downtime_minutes: i64 = row.try_get("downtime_minutes")?;
                let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
                let idempotency_key: String = row.try_get("idempotency_key")?;
                let payload: Value = row.try_get("payload")?;
                let handoff = parse_capacity_handoff_payload(&payload)?;
                validate_capacity_handoff_columns(
                    &handoff,
                    source_event_id,
                    &holding_ref,
                    source_id,
                    company_id,
                    branch_id,
                    &reference,
                    &source_domain,
                    &source_type,
                    asset_id,
                    downtime_minutes,
                    occurred_at,
                    &idempotency_key,
                    &self.holding_ref,
                )?;
                validate_capacity_handoff_outbox(
                    source_event_id,
                    row.try_get("aggregate_type")?,
                    row.try_get("aggregate_id")?,
                    row.try_get("aggregate_version")?,
                    row.try_get("event_type")?,
                    row.try_get("outbox_payload")?,
                    row.try_get("contract_version")?,
                    row.try_get("event_version")?,
                    row.try_get("tenant_scope")?,
                    row.try_get("actor")?,
                    row.try_get("correlation_id")?,
                    row.try_get("causation_id")?,
                    row.try_get("outbox_idempotency_key")?,
                    &handoff,
                )?;
                Ok((
                    source_event_id,
                    CapacityHandoffEvidence {
                        source_id,
                        company_id,
                        branch_id,
                        reference,
                        source_version: 1,
                        payload,
                    },
                ))
            })
            .collect::<Result<HashMap<Uuid, CapacityHandoffEvidence>, PersistenceError>>()?;
        let projected_handoffs = handoff_read_rows
            .into_iter()
            .map(|row| {
                let source_event_id: Uuid = row.try_get("source_event_id")?;
                let source_id: Uuid = row.try_get("source_id")?;
                let holding_ref: String = row.try_get("holding_ref")?;
                let company_id: Uuid = row.try_get("company_id")?;
                let branch_id: Option<Uuid> = row.try_get("branch_id")?;
                let reference: String = row.try_get("reference")?;
                let source_version = as_u64(
                    row.try_get("source_version")?,
                    "Asset/Maintenance capacity handoff projection version",
                )?;
                let payload: Value = row.try_get("payload")?;
                let handoff = parse_capacity_handoff_payload(&payload)?;
                validate_capacity_handoff_columns(
                    &handoff,
                    source_event_id,
                    &holding_ref,
                    source_id,
                    company_id,
                    branch_id,
                    &reference,
                    &handoff.source.source_domain,
                    &handoff.source.source_type,
                    handoff.asset_id,
                    handoff.downtime_minutes,
                    handoff.occurred_at,
                    &handoff.idempotency_key,
                    &self.holding_ref,
                )?;
                if source_version != 1 {
                    return Err(PersistenceError::InvalidData(
                        "Asset/Maintenance capacity handoff projection version is invalid".into(),
                    ));
                }
                Ok((
                    source_event_id,
                    CapacityHandoffEvidence {
                        source_id,
                        company_id,
                        branch_id,
                        reference,
                        source_version,
                        payload,
                    },
                ))
            })
            .collect::<Result<HashMap<Uuid, CapacityHandoffEvidence>, PersistenceError>>()?;
        if source_handoffs != projected_handoffs {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance capacity handoff projection reconciliation mismatch".into(),
            ));
        }

        let missing_aggregate_count = source
            .keys()
            .filter(|key| !projected.contains_key(*key))
            .count() as u64;
        let stale_aggregate_count = source
            .iter()
            .filter(|(key, version)| {
                projected
                    .get(*key)
                    .is_some_and(|current| current < *version)
            })
            .count() as u64;
        let orphan_aggregate_count = projected
            .keys()
            .filter(|key| !source.contains_key(*key))
            .count() as u64;
        let projected_ahead_aggregate_count = projected
            .iter()
            .filter(|(key, version)| source.get(*key).is_some_and(|latest| *version > latest))
            .count() as u64;
        Ok(AssetMaintenanceProjectionReconciliation {
            source_event_count,
            source_aggregate_count: source.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
            source_version_gap_count,
            projected_ahead_aggregate_count,
            custody_history_mismatch_count,
        })
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<AssetMaintenanceProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "Asset/Maintenance event ID")?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Asset/Maintenance aggregate ID")?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance projection version must be positive".into(),
            ));
        }
        let event = parse_event_payload(payload, aggregate_type, aggregate_id, aggregate_version)?;
        self.verify_authoritative_source_event(
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let mut tx = self.begin_read().await?;
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT source_version FROM erpx_tenant.asset_maintenance_read_rows
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| as_u64(value, "Asset/Maintenance projection source version"))
        .transpose()?;
        if let Some(current) = current {
            if current >= aggregate_version {
                tx.commit().await?;
                return Ok(AssetMaintenanceProjectionApplication {
                    event_id,
                    aggregate_type: aggregate_type.into(),
                    aggregate_id,
                    aggregate_version,
                    applied: false,
                });
            }
            if current.saturating_add(1) != aggregate_version {
                return Err(PersistenceError::InvalidData(
                    "Asset/Maintenance projection event is out of order".into(),
                ));
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance projection must start at version one".into(),
            ));
        }
        let read_payload = serde_json::to_value(&event.aggregate)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_maintenance_read_rows
                (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
                 reference, source_version, payload, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                 company_id=$4, branch_id=$5, reference=$6, source_version=$7,
                 payload=$8, projected_at=now()",
        )
        .bind(aggregate_id)
        .bind(aggregate_type)
        .bind(&self.holding_ref)
        .bind(event.aggregate.company_id())
        .bind(event.aggregate.branch_id())
        .bind(event.aggregate.reference())
        .bind(as_i64(
            aggregate_version,
            "Asset/Maintenance projection version",
        )?)
        .bind(read_payload)
        .execute(&mut *tx)
        .await?;
        if let AssetMaintenanceAggregate::Asset(asset) = &event.aggregate {
            sqlx::query(
                "DELETE FROM erpx_tenant.asset_custody_transfer_read_rows
                 WHERE asset_id=$1 AND holding_ref=$2",
            )
            .bind(asset.id)
            .bind(&self.holding_ref)
            .execute(&mut *tx)
            .await?;
            for transfer in &asset.custody_history {
                insert_read_custody_transfer(&mut tx, &self.holding_ref, asset.id, transfer)
                    .await?;
            }
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_maintenance_projection_checkpoints
                (aggregate_type, aggregate_id, holding_ref, last_event_id,
                 source_version, projected_at)
             VALUES ($1,$2,$3,$4,$5,now())
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                 holding_ref=$3, last_event_id=$4, source_version=$5, projected_at=now()",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(event_id)
        .bind(as_i64(
            aggregate_version,
            "Asset/Maintenance projection version",
        )?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(AssetMaintenanceProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    /// A projector may only apply an event that is present, correctly scoped,
    /// and semantically equivalent to the authoritative write-side outbox.
    /// This prevents a caller with read-database access from manufacturing a
    /// fresh-looking read row by submitting an otherwise valid aggregate
    /// payload.
    async fn verify_authoritative_source_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT event_type, aggregate_type, aggregate_id, aggregate_version, payload,
                    contract_version, event_version, tenant_scope, actor,
                    correlation_id, causation_id, idempotency_key
             FROM erpx_tenant.outbox_events
             WHERE event_id=$1 AND holding_ref=$2",
        )
        .bind(event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let Some(row) = row else {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance projector event is not an authoritative source event".into(),
            ));
        };
        let event_type: String = row.try_get("event_type")?;
        let actual_aggregate_type: String = row.try_get("aggregate_type")?;
        let actual_aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let actual_aggregate_version = as_u64(
            row.try_get("aggregate_version")?,
            "Asset/Maintenance source event version",
        )?;
        let actual_payload: Value = row.try_get("payload")?;
        validate_outbox_event_envelope(
            event_id,
            &event_type,
            &actual_aggregate_type,
            actual_aggregate_id,
            actual_aggregate_version,
            &actual_payload,
            row.try_get("contract_version")?,
            row.try_get("event_version")?,
            row.try_get("tenant_scope")?,
            row.try_get("actor")?,
            row.try_get("correlation_id")?,
            row.try_get("causation_id")?,
            row.try_get("idempotency_key")?,
        )?;
        if actual_aggregate_type != aggregate_type
            || actual_aggregate_id != aggregate_id
            || actual_aggregate_version != aggregate_version
            || actual_payload != *payload
        {
            return Err(PersistenceError::InvalidData(
                "Asset/Maintenance projector event does not match the authoritative outbox".into(),
            ));
        }
        Ok(())
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<AssetMaintenanceProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.asset_maintenance_read_rows WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.asset_maintenance_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let custody_rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.asset_custody_transfer_read_rows WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let handoff_rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.asset_capacity_handoff_read_rows WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(AssetMaintenanceProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
            custody_rows_cleared,
            handoff_rows_cleared,
        })
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref).await
    }
}

/// A maintenance work order is only authoritative when its referenced asset
/// is present in the same routed holding and its typed scope still matches the
/// work order. This check belongs in the source transaction: validating only
/// the serialized work-order shape would allow an orphan or cross-company
/// work order to become a durable source row and emit a misleading outbox
/// event. Idempotency replays are checked before this relation so a replay
/// remains a replay even if the asset has since been disposed.
async fn validate_work_order_asset_link(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    work_order: &MaintenanceWorkOrder,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT company_id, branch_id, payload
         FROM erpx_tenant.asset_maintenance_aggregates
         WHERE aggregate_type='asset' AND aggregate_id=$1 AND holding_ref=$2
         FOR SHARE",
    )
    .bind(work_order.asset_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Err(PersistenceError::InvalidData(
            "maintenance work order references an asset that is not authoritative in this holding"
                .into(),
        ));
    };
    let company_id: Uuid = row.try_get("company_id")?;
    let branch_id: Option<Uuid> = row.try_get("branch_id")?;
    if company_id != work_order.scope.company_id || branch_id != work_order.scope.branch_id {
        return Err(PersistenceError::InvalidData(
            "maintenance work order scope does not match its authoritative asset".into(),
        ));
    }
    let payload: Value = row.try_get("payload")?;
    let asset: Asset = serde_json::from_value(payload)?;
    validate_asset(&asset)?;
    if asset.scope != work_order.scope {
        return Err(PersistenceError::InvalidData(
            "maintenance work order scope does not match its authoritative asset payload".into(),
        ));
    }
    if matches!(
        asset.status,
        erpx_domain::asset_maintenance::AssetStatus::Disposed
    ) {
        return Err(PersistenceError::InvalidData(
            "maintenance work order cannot reference a disposed asset".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Default)]
struct SourceVersionEvidence {
    event_count: u64,
    latest_version: u64,
    versions: HashSet<u64>,
}

#[derive(Debug, Clone, PartialEq)]
struct CapacityHandoffEvidence {
    source_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    reference: String,
    source_version: u64,
    payload: Value,
}

fn parse_capacity_handoff_payload(
    payload: &Value,
) -> Result<OperationsCapacityImpactHandoff, PersistenceError> {
    let handoff: OperationsCapacityImpactHandoff = serde_json::from_value(payload.clone())?;
    validate_handoff(&handoff)?;
    Ok(handoff)
}

#[allow(clippy::too_many_arguments)]
fn validate_capacity_handoff_columns(
    handoff: &OperationsCapacityImpactHandoff,
    source_event_id: Uuid,
    holding_ref: &str,
    source_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    reference: &str,
    source_domain: &str,
    source_type: &str,
    asset_id: Uuid,
    downtime_minutes: i64,
    occurred_at: DateTime<Utc>,
    idempotency_key: &str,
    expected_holding_ref: &str,
) -> Result<(), PersistenceError> {
    if holding_ref != expected_holding_ref
        || handoff.source.source_event_id != source_event_id
        || handoff.source.source_id != source_id
        || handoff.scope.company_id != company_id
        || handoff.scope.branch_id != branch_id
        || handoff.reference != reference
        || handoff.source.source_domain != source_domain
        || handoff.source.source_type != source_type
        || handoff.asset_id != asset_id
        || handoff.downtime_minutes != downtime_minutes
        || handoff.occurred_at != occurred_at
        || handoff.idempotency_key != idempotency_key
    {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance capacity handoff provenance columns do not match payload".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_capacity_handoff_outbox(
    event_id: Uuid,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: i64,
    event_type: String,
    outbox_payload: Value,
    contract_version: Option<i32>,
    event_version: Option<i32>,
    tenant_scope: Option<Value>,
    actor: Option<Value>,
    correlation_id: Option<Uuid>,
    causation_id: Option<Uuid>,
    outbox_idempotency_key: Option<String>,
    handoff: &OperationsCapacityImpactHandoff,
) -> Result<(), PersistenceError> {
    let expected_payload = serde_json::to_value(handoff)?;
    if aggregate_type != "operations_capacity_handoff"
        || aggregate_id != handoff.source.source_id
        || as_u64(
            aggregate_version,
            "Asset/Maintenance capacity handoff version",
        )? != 1
        || event_type != "assets.operations_capacity_handoff.v1"
        || outbox_payload != expected_payload
        || contract_version != Some(i32::from(ASSET_MAINTENANCE_CONTRACT_VERSION))
        || event_version != Some(i32::from(ASSET_MAINTENANCE_EVENT_VERSION))
        || tenant_scope != Some(serde_json::to_value(handoff.scope.tenant_scope())?)
        || actor.is_some()
        || correlation_id != Some(event_id)
        || causation_id.is_some()
        || outbox_idempotency_key.as_deref() != Some(handoff.idempotency_key.as_str())
    {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance capacity handoff outbox provenance is invalid".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetMaintenanceEventPayload {
    aggregate: AssetMaintenanceAggregate,
    version: u64,
}

fn parse_event_payload(
    payload: &Value,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
) -> Result<AssetMaintenanceEventPayload, PersistenceError> {
    validate_event_payload_shape(payload)?;
    let event: AssetMaintenanceEventPayload = serde_json::from_value(payload.clone())?;
    event.aggregate.validate()?;
    if event.version != aggregate_version
        || event.aggregate.aggregate_type() != aggregate_type
        || event.aggregate.aggregate_id() != aggregate_id
    {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance projection identity or version mismatch".into(),
        ));
    }
    Ok(event)
}

fn validate_event_payload_shape(payload: &Value) -> Result<(), PersistenceError> {
    let Some(object) = payload.as_object() else {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance event envelope must be an object".into(),
        ));
    };
    if object.len() != 2 || !object.contains_key("aggregate") || !object.contains_key("version") {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance event envelope has unexpected fields".into(),
        ));
    }
    let Some(aggregate) = object.get("aggregate").and_then(Value::as_object) else {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance event aggregate envelope must be an object".into(),
        ));
    };
    if aggregate.len() != 2
        || !aggregate.contains_key("aggregate_type")
        || !aggregate.contains_key("aggregate")
    {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance event aggregate envelope has unexpected fields".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_outbox_event_envelope(
    event_id: Uuid,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
    contract_version: Option<i32>,
    event_version: Option<i32>,
    tenant_scope: Option<Value>,
    actor: Option<Value>,
    correlation_id: Option<Uuid>,
    causation_id: Option<Uuid>,
    idempotency_key: Option<String>,
) -> Result<AssetMaintenanceAggregate, PersistenceError> {
    validate_uuid(event_id, "Asset/Maintenance source event ID")?;
    validate_aggregate_type(aggregate_type)?;
    validate_uuid(aggregate_id, "Asset/Maintenance source aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance source event version must be positive".into(),
        ));
    }
    let expected_event_type =
        format!("asset_maintenance.{aggregate_type}.v{ASSET_MAINTENANCE_EVENT_VERSION}");
    if event_type != expected_event_type {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance outbox event type is invalid".into(),
        ));
    }
    if contract_version != Some(i32::from(ASSET_MAINTENANCE_CONTRACT_VERSION))
        || event_version != Some(i32::from(ASSET_MAINTENANCE_EVENT_VERSION))
    {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance outbox envelope version is invalid".into(),
        ));
    }
    if actor.is_some() || correlation_id != Some(event_id) || causation_id.is_some() {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance outbox provenance envelope is invalid".into(),
        ));
    }
    let idempotency_key = idempotency_key.ok_or_else(|| {
        PersistenceError::InvalidData("Asset/Maintenance outbox idempotency key is missing".into())
    })?;
    validate_text(
        &idempotency_key,
        "Asset/Maintenance outbox idempotency key",
        512,
    )?;
    let event = parse_event_payload(payload, aggregate_type, aggregate_id, aggregate_version)?;
    let expected_scope = serde_json::to_value(event.aggregate.scope().tenant_scope())?;
    if tenant_scope != Some(expected_scope) {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance outbox tenant scope does not match aggregate".into(),
        ));
    }
    Ok(event.aggregate)
}

async fn persist_custody_history(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    asset: &Asset,
) -> Result<(), PersistenceError> {
    for transfer in &asset.custody_history {
        let transfer_payload = serde_json::to_value(transfer)?;
        let existing = sqlx::query_scalar::<_, Value>(
            "SELECT transfer
             FROM erpx_tenant.asset_custody_transfers
             WHERE asset_id=$1 AND holding_ref=$2 AND aggregate_version=$3",
        )
        .bind(asset.id)
        .bind(holding_ref)
        .bind(as_i64(
            transfer.aggregate_version,
            "custody transfer version",
        )?)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(existing) = existing {
            if existing != transfer_payload {
                return Err(PersistenceError::InvalidData(
                    "immutable asset custody history conflicts with stored transfer".into(),
                ));
            }
            continue;
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.asset_custody_transfers
                (asset_id, holding_ref, aggregate_version, from_scope, to_scope,
                 from_custodian_ref, to_custodian_ref, from_location_ref,
                 to_location_ref, transferred_at, actor_ref, reason, command_id, transfer)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)",
        )
        .bind(asset.id)
        .bind(holding_ref)
        .bind(as_i64(
            transfer.aggregate_version,
            "custody transfer version",
        )?)
        .bind(serde_json::to_value(&transfer.from_scope)?)
        .bind(serde_json::to_value(&transfer.to_scope)?)
        .bind(&transfer.from_custodian_ref)
        .bind(&transfer.to_custodian_ref)
        .bind(&transfer.from_location_ref)
        .bind(&transfer.to_location_ref)
        .bind(transfer.transferred_at)
        .bind(&transfer.actor_ref)
        .bind(&transfer.reason)
        .bind(transfer.command_id)
        .bind(transfer_payload)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_read_custody_transfer(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    asset_id: Uuid,
    transfer: &CustodyTransfer,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.asset_custody_transfer_read_rows
            (asset_id, aggregate_version, holding_ref, transfer, projected_at)
         VALUES ($1,$2,$3,$4,now())",
    )
    .bind(asset_id)
    .bind(as_i64(transfer.aggregate_version, "custody read version")?)
    .bind(holding_ref)
    .bind(serde_json::to_value(transfer)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn persist_shared_outbox(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    idempotency_key: &str,
    scope: &AssetScope,
    payload: &Value,
) -> Result<(), PersistenceError> {
    // Keep the event aggregate in the same tagged representation consumed by
    // `project_event`. The normalized source row remains authoritative, and
    // the event must carry the aggregate envelope so the
    // projector can validate type, identity, and version before writing the
    // disposable read model.
    let event_aggregate = serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
    });
    let event_payload = serde_json::json!({
        "aggregate": event_aggregate,
        "version": version,
    });
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now(),$8,$9,$10,$11,$12,$13,$14)",
    )
    .bind(event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(version, "Asset/Maintenance version")?)
    .bind(format!(
        "asset_maintenance.{aggregate_type}.v{ASSET_MAINTENANCE_EVENT_VERSION}"
    ))
    .bind(event_payload)
    .bind(i32::from(ASSET_MAINTENANCE_CONTRACT_VERSION))
    .bind(i32::from(ASSET_MAINTENANCE_EVENT_VERSION))
    .bind(serde_json::to_value(scope.tenant_scope())?)
    .bind(Option::<Value>::None)
    .bind(event_id)
    .bind(Option::<Uuid>::None)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_asset(asset: &Asset) -> Result<(), PersistenceError> {
    asset.scope.validate().map_err(domain_error)?;
    validate_uuid(asset.id, "asset ID")?;
    if asset.version == 0 {
        return Err(PersistenceError::InvalidData(
            "asset version must be positive".into(),
        ));
    }
    validate_text(&asset.reference, "asset reference", 64)?;
    if asset.reference != format!("AST-{}", asset.id.simple()) {
        return Err(PersistenceError::InvalidData(
            "asset reference is not deterministic".into(),
        ));
    }
    validate_text(&asset.name, "asset name", 255)?;
    validate_text(&asset.category, "asset category", 128)?;
    validate_text(&asset.custodian_ref, "custodian reference", 255)?;
    validate_text(&asset.location_ref, "location reference", 255)?;
    if asset.acquisition_cost_minor < 0
        || asset.residual_value_minor < 0
        || asset.residual_value_minor > asset.acquisition_cost_minor
        || asset.useful_life_months == 0
    {
        return Err(PersistenceError::InvalidData(
            "asset accounting bounds are invalid".into(),
        ));
    }
    asset.book_value_minor().map_err(domain_error)?;
    validate_custody_history(asset)?;
    Ok(())
}

fn validate_work_order(work_order: &MaintenanceWorkOrder) -> Result<(), PersistenceError> {
    work_order.scope.validate().map_err(domain_error)?;
    validate_uuid(work_order.id, "maintenance work order ID")?;
    validate_uuid(work_order.asset_id, "maintenance asset ID")?;
    if work_order.version == 0 {
        return Err(PersistenceError::InvalidData(
            "maintenance work order version must be positive".into(),
        ));
    }
    validate_text(&work_order.reference, "maintenance reference", 64)?;
    if work_order.reference != format!("MWO-{}", work_order.id.simple()) {
        return Err(PersistenceError::InvalidData(
            "maintenance reference is not deterministic".into(),
        ));
    }
    validate_text(&work_order.title, "maintenance title", 255)?;
    let mut part_ids = HashSet::new();
    for part in &work_order.parts {
        validate_uuid(part.id, "maintenance part ID")?;
        validate_text(&part.part_ref, "maintenance part reference", 128)?;
        if !part_ids.insert(part.id) {
            return Err(PersistenceError::InvalidData(
                "maintenance part history contains a duplicate ID".into(),
            ));
        }
        part.total_minor().map_err(domain_error)?;
    }
    let mut labor_ids = HashSet::new();
    for labor in &work_order.labor {
        validate_uuid(labor.id, "maintenance labor ID")?;
        validate_text(&labor.worker_ref, "maintenance labor worker", 255)?;
        if !labor_ids.insert(labor.id) {
            return Err(PersistenceError::InvalidData(
                "maintenance labor history contains a duplicate ID".into(),
            ));
        }
        labor.total_minor().map_err(domain_error)?;
    }
    let mut downtime_ids = HashSet::new();
    for downtime in &work_order.downtime {
        if !downtime_ids.insert(downtime.id) {
            return Err(PersistenceError::InvalidData(
                "maintenance downtime history contains a duplicate ID".into(),
            ));
        }
        downtime.validate().map_err(domain_error)?;
        if work_order.downtime.iter().any(|other| {
            other.id != downtime.id
                && downtime.started_at < other.ended_at
                && other.started_at < downtime.ended_at
        }) {
            return Err(PersistenceError::InvalidData(
                "maintenance downtime history contains overlapping intervals".into(),
            ));
        }
    }
    let mut reliability_ids = HashSet::new();
    for reliability in &work_order.reliability {
        if !reliability_ids.insert(reliability.id) {
            return Err(PersistenceError::InvalidData(
                "maintenance reliability history contains a duplicate ID".into(),
            ));
        }
        reliability.validate().map_err(domain_error)?;
    }
    Ok(())
}

fn validate_custody_history(asset: &Asset) -> Result<(), PersistenceError> {
    let acquired_at = asset
        .acquired_on
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| PersistenceError::InvalidData("asset acquisition date is invalid".into()))?
        .and_utc();
    let mut previous: Option<&CustodyTransfer> = None;
    for transfer in &asset.custody_history {
        validate_custody_transfer(transfer)?;
        if transfer.aggregate_version > asset.version || transfer.transferred_at < acquired_at {
            return Err(PersistenceError::InvalidData(
                "asset custody history is outside aggregate bounds".into(),
            ));
        }
        if transfer.from_scope == transfer.to_scope
            && transfer.from_custodian_ref == transfer.to_custodian_ref
            && transfer.from_location_ref == transfer.to_location_ref
        {
            return Err(PersistenceError::InvalidData(
                "asset custody history contains a no-op transfer".into(),
            ));
        }
        if let Some(previous) = previous
            && (previous.aggregate_version >= transfer.aggregate_version
                || previous.to_scope != transfer.from_scope
                || previous.to_custodian_ref != transfer.from_custodian_ref
                || previous.to_location_ref != transfer.from_location_ref)
        {
            return Err(PersistenceError::InvalidData(
                "asset custody history is not a contiguous chain".into(),
            ));
        }
        previous = Some(transfer);
    }
    if let Some(last) = previous
        && (last.to_scope != asset.scope
            || last.to_custodian_ref != asset.custodian_ref
            || last.to_location_ref != asset.location_ref)
    {
        return Err(PersistenceError::InvalidData(
            "asset custody history does not end at current custody".into(),
        ));
    }
    Ok(())
}

fn validate_custody_transfer(transfer: &CustodyTransfer) -> Result<(), PersistenceError> {
    transfer.from_scope.validate().map_err(domain_error)?;
    transfer.to_scope.validate().map_err(domain_error)?;
    validate_uuid(transfer.command_id, "custody command ID")?;
    if transfer.aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "custody aggregate version must be positive".into(),
        ));
    }
    for (value, field) in [
        (&transfer.from_custodian_ref, "from custodian"),
        (&transfer.to_custodian_ref, "to custodian"),
        (&transfer.from_location_ref, "from location"),
        (&transfer.to_location_ref, "to location"),
        (&transfer.actor_ref, "transfer actor"),
        (&transfer.reason, "transfer reason"),
    ] {
        validate_text(value, field, 1_024)?;
    }
    Ok(())
}

fn validate_handoff(handoff: &OperationsCapacityImpactHandoff) -> Result<(), PersistenceError> {
    handoff.scope.validate().map_err(domain_error)?;
    validate_uuid(handoff.asset_id, "capacity handoff asset ID")?;
    validate_uuid(handoff.source.source_id, "capacity handoff source ID")?;
    validate_uuid(handoff.source.source_event_id, "capacity handoff event ID")?;
    validate_text(&handoff.reference, "capacity handoff reference", 255)?;
    validate_text(
        &handoff.idempotency_key,
        "capacity handoff idempotency key",
        255,
    )?;
    if handoff.source.source_domain != "assets"
        || handoff.source.source_type != "maintenance_work_order"
        || handoff.downtime_minutes <= 0
    {
        return Err(PersistenceError::InvalidData(
            "capacity handoff provenance is invalid".into(),
        ));
    }
    Ok(())
}

fn domain_error(error: AssetMaintenanceError) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if matches!(value, "asset" | "maintenance_work_order") {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unknown Asset/Maintenance aggregate type".into(),
        ))
    }
}

/// The source cursor is version-based for compatibility with the projection
/// worker contract. Fetching one extra row lets this boundary reject a
/// duplicate `(aggregate_type, aggregate_id, aggregate_version)` before a
/// continuation cursor could skip it with `aggregate_version > cursor_version`.
fn validate_source_event_page_versions(
    rows: &[sqlx::postgres::PgRow],
) -> Result<(), PersistenceError> {
    let mut seen = HashSet::with_capacity(rows.len());
    for row in rows {
        let aggregate_type: String = row.try_get("aggregate_type")?;
        let aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let aggregate_version = as_u64(
            row.try_get("aggregate_version")?,
            "Asset/Maintenance source event version",
        )?;
        insert_source_event_version_key(
            &mut seen,
            &aggregate_type,
            aggregate_id,
            aggregate_version,
        )?;
    }
    Ok(())
}

fn insert_source_event_version_key(
    seen: &mut HashSet<(String, Uuid, u64)>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
) -> Result<(), PersistenceError> {
    if !seen.insert((aggregate_type.to_owned(), aggregate_id, aggregate_version)) {
        return Err(PersistenceError::InvalidData(
            "Asset/Maintenance source page contains a duplicate aggregate version".into(),
        ));
    }
    Ok(())
}

fn validate_read_scope(scope: AssetMaintenanceReadScope) -> Result<(), PersistenceError> {
    validate_uuid(scope.holding_id, "Asset/Maintenance read holding ID")?;
    validate_uuid(scope.company_id, "Asset/Maintenance read company ID")?;
    if let Some(branch_id) = scope.branch_id {
        validate_uuid(branch_id, "Asset/Maintenance read branch ID")?;
    }
    Ok(())
}

fn validate_read_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "Asset/Maintenance read page size must be 1..={MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    validate_text(value, "holding reference", 255)
}

fn validate_text(value: &str, field: &'static str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn hash_request(
    aggregate_type: &str,
    payload: &Value,
    expected_version: Option<u64>,
    idempotency_key: &str,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        aggregate_type,
        payload,
        expected_version,
        idempotency_key,
    ))?)))
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    holding_ref: &str,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[tokio::test]
    async fn store_rejects_unsafe_holding_references_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxAssetMaintenanceStore::new(pool.clone(), pool, " ").is_err());
    }

    #[tokio::test]
    async fn source_event_page_rejects_unbounded_pages_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        let store =
            SqlxAssetMaintenanceStore::new(pool.clone(), pool, "holding-asset-maintenance-test")
                .expect("valid store");
        assert!(store.read_source_event_page(None, 0).await.is_err());
        assert!(
            store
                .read_source_event_page(None, MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE + 1)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn read_list_scope_and_page_bounds_fail_closed_before_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        let store =
            SqlxAssetMaintenanceStore::new(pool.clone(), pool, "holding-asset-maintenance-test")
                .expect("valid store");
        let scope = AssetMaintenanceReadScope {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: None,
        };
        assert!(store.list_for_scope(scope, 0, None).await.is_err());
        assert!(
            store
                .list_for_scope(scope, MAX_ASSET_MAINTENANCE_READ_PAGE_SIZE + 1, None,)
                .await
                .is_err()
        );
        assert!(
            store
                .list_for_scope(
                    AssetMaintenanceReadScope {
                        holding_id: Uuid::nil(),
                        ..scope
                    },
                    25,
                    None,
                )
                .await
                .is_err()
        );
    }

    #[test]
    fn aggregate_type_allow_list_is_closed() {
        assert!(validate_aggregate_type("asset").is_ok());
        assert!(validate_aggregate_type("payment_instrument").is_err());
    }

    #[test]
    fn event_envelope_shape_is_canonical_and_closed() {
        let canonical = json!({
            "aggregate": {
                "aggregate_type": "asset",
                "aggregate": {}
            },
            "version": 1
        });
        assert!(validate_event_payload_shape(&canonical).is_ok());

        let legacy_redundant_type = json!({
            "aggregate_type": "asset",
            "aggregate": canonical["aggregate"].clone(),
            "version": 1
        });
        assert!(validate_event_payload_shape(&legacy_redundant_type).is_err());

        let mut extra = canonical;
        extra["unexpected"] = json!(true);
        assert!(validate_event_payload_shape(&extra).is_err());
    }

    #[test]
    fn capacity_handoff_validation_rejects_non_asset_provenance() {
        let scope = AssetScope {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
            branch_id: None,
        };
        let handoff = OperationsCapacityImpactHandoff {
            reference: "OPS-CAP-1-2".into(),
            source: erpx_domain::operations::SourceTransactionRef {
                source_domain: "finance".into(),
                source_type: "journal".into(),
                source_id: Uuid::from_u128(3),
                source_event_id: Uuid::from_u128(4),
            },
            asset_id: Uuid::from_u128(5),
            scope,
            downtime_minutes: 5,
            occurred_at: Utc::now(),
            idempotency_key: "handoff-1".into(),
        };
        assert!(validate_handoff(&handoff).is_err());
    }

    #[test]
    fn source_cursor_guard_rejects_duplicate_aggregate_versions() {
        let mut seen = HashSet::new();
        let aggregate_id = Uuid::from_u128(10);
        insert_source_event_version_key(&mut seen, "asset", aggregate_id, 1)
            .expect("first version is accepted");
        insert_source_event_version_key(&mut seen, "asset", aggregate_id, 2)
            .expect("next version is accepted");
        assert!(matches!(
            insert_source_event_version_key(&mut seen, "asset", aggregate_id, 2),
            Err(PersistenceError::InvalidData(message))
                if message.contains("duplicate aggregate version")
        ));
        insert_source_event_version_key(&mut seen, "asset", Uuid::from_u128(11), 2)
            .expect("same version on another aggregate is accepted");
    }
}
