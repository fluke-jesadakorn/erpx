//! Tenant-routed SQLx persistence for Procurement and Inventory aggregates.
//!
//! The write database is authoritative.  The read database is a disposable
//! projection rebuilt from the source outbox.  Every write validates the
//! domain aggregate before serialization and commits source state, the shared
//! snapshot, the shared outbox event, and the request-bound receipt together.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use erpx_domain::contracts::TenantScope;
use erpx_domain::procurement_inventory::{
    GoodsReceipt, GoodsReceiptStatus, Product, PurchaseOrder, PurchaseOrderStatus, PurchaseRequest,
    PurchaseRequestStatus, ScmAggregateKind, ScmProjectionReconciliation,
    ScmReconciliationEvidence, StockBalance, StockMovement, StockMovementKind, StockMovementStatus,
    StockReservation, StockTransfer, Warehouse, deterministic_idempotency_key,
};

use crate::PersistenceError;

pub const MAX_SCM_READ_PAGE_SIZE: u32 = 500;
pub const MAX_SCM_SOURCE_EVENT_PAGE_SIZE: u32 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScmWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScmSourceEventCursor {
    /// The routed holding this continuation belongs to. A cursor copied from
    /// another holding must never be allowed to advance this source stream.
    pub holding_ref: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    /// The event ID makes the source ordering total even if a legacy/import
    /// path contains two rows for one aggregate version.
    pub event_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScmSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScmSourceEventPage {
    pub events: Vec<ScmSourceEvent>,
    pub next_cursor: Option<ScmSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
struct ScmSourceAggregateFact {
    event_id: Uuid,
    event_type: String,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
    event_count: u64,
    first_version: u64,
    distinct_version_count: u64,
    conflicting_version_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct ScmReplayOutboxFact {
    event_id: Uuid,
    event_type: String,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub aggregate: ScmAggregate,
}

/// Explicit tenant scope accepted by the SCM read gateway. Company scope
/// includes its branches; branch scope is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScmReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScmReadCursor {
    pub projected_at: DateTime<Utc>,
    pub aggregate_type: &'static str,
    pub aggregate_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmReadListRow {
    pub row: ScmReadRow,
    pub reference: String,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmReadPage {
    pub rows: Vec<ScmReadListRow>,
    pub next_cursor: Option<ScmReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum ScmAggregate {
    Product(Product),
    Warehouse(Warehouse),
    PurchaseRequest(PurchaseRequest),
    PurchaseOrder(PurchaseOrder),
    GoodsReceipt(GoodsReceipt),
    StockMovement(StockMovement),
    StockTransfer(StockTransfer),
    StockReservation(StockReservation),
}

impl ScmAggregate {
    /// Validate a generic transport save against the authenticated command.
    /// Scope, immutable command identity, and deterministic idempotency are
    /// all checked before the SQLx store can persist the aggregate.
    pub fn validate_for_command(
        &self,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<(), PersistenceError> {
        match self {
            Self::Product(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::Warehouse(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::PurchaseRequest(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::PurchaseOrder(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::GoodsReceipt(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::StockMovement(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::StockTransfer(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::StockReservation(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
        }?;
        let (_, _, _, _, _, actual_idempotency_key, _) = self.fields()?;
        let expected_idempotency_key =
            format!("scm/{}/{}", self.aggregate_type(), context.idempotency_key);
        if actual_idempotency_key != expected_idempotency_key {
            return Err(PersistenceError::InvalidData(
                "SCM aggregate idempotency key is not bound to the command".into(),
            ));
        }
        let command_id = match self {
            Self::Product(value) => value.command_id,
            Self::Warehouse(value) => value.command_id,
            Self::PurchaseRequest(value) => value.command_id,
            Self::PurchaseOrder(value) => value.command_id,
            Self::GoodsReceipt(value) => value.command_id,
            Self::StockMovement(value) => value.command_id,
            Self::StockTransfer(value) => value.command_id,
            Self::StockReservation(value) => value.command_id,
        };
        if command_id != context.command_id {
            return Err(PersistenceError::InvalidData(
                "SCM aggregate command identity is not bound to the command".into(),
            ));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PersistenceError> {
        let result = match self {
            Self::Product(value) => value.validate(),
            Self::Warehouse(value) => value.validate(),
            Self::PurchaseRequest(value) => value.validate(),
            Self::PurchaseOrder(value) => value.validate(),
            Self::GoodsReceipt(value) => value.validate(),
            Self::StockMovement(value) => value.validate(),
            Self::StockTransfer(value) => value.validate(),
            Self::StockReservation(value) => value.validate(),
        };
        result.map_err(|error| PersistenceError::InvalidData(error.to_string()))
    }

    pub fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Product(_) => ScmAggregateKind::Product.as_str(),
            Self::Warehouse(_) => ScmAggregateKind::Warehouse.as_str(),
            Self::PurchaseRequest(_) => ScmAggregateKind::PurchaseRequest.as_str(),
            Self::PurchaseOrder(_) => ScmAggregateKind::PurchaseOrder.as_str(),
            Self::GoodsReceipt(_) => ScmAggregateKind::GoodsReceipt.as_str(),
            Self::StockMovement(_) => ScmAggregateKind::StockMovement.as_str(),
            Self::StockTransfer(_) => ScmAggregateKind::StockTransfer.as_str(),
            Self::StockReservation(_) => ScmAggregateKind::StockReservation.as_str(),
        }
    }

    pub fn aggregate_id(&self) -> Uuid {
        match self {
            Self::Product(value) => value.id,
            Self::Warehouse(value) => value.id,
            Self::PurchaseRequest(value) => value.id,
            Self::PurchaseOrder(value) => value.id,
            Self::GoodsReceipt(value) => value.id,
            Self::StockMovement(value) => value.id,
            Self::StockTransfer(value) => value.id,
            Self::StockReservation(value) => value.id,
        }
    }

    /// Returns the immutable tenant scope carried by the source aggregate.
    /// Read adapters use this value only after loading from the routed
    /// disposable projection to enforce company/branch containment.
    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Product(value) => &value.scope,
            Self::Warehouse(value) => &value.scope,
            Self::PurchaseRequest(value) => &value.scope,
            Self::PurchaseOrder(value) => &value.scope,
            Self::GoodsReceipt(value) => &value.scope,
            Self::StockMovement(value) => &value.scope,
            Self::StockTransfer(value) => &value.scope,
            Self::StockReservation(value) => &value.scope,
        }
    }

    fn fields(&self) -> Result<ScmFields<'_>, PersistenceError> {
        let fields = match self {
            Self::Product(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::Warehouse(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                Some(value.branch_id),
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::PurchaseRequest(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::PurchaseOrder(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::GoodsReceipt(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::StockMovement(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::StockTransfer(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
            Self::StockReservation(value) => fields_for(
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                &value.reference,
                &value.idempotency_key,
                value,
            )?,
        };
        Ok(fields)
    }
}

type ScmFields<'a> = (
    &'static str,
    Uuid,
    Uuid,
    Option<Uuid>,
    &'a str,
    &'a str,
    Value,
);

fn fields_for<'a, T: Serialize>(
    aggregate_type: &'static str,
    aggregate_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    reference: &'a str,
    idempotency_key: &'a str,
    aggregate: &T,
) -> Result<ScmFields<'a>, PersistenceError> {
    Ok((
        aggregate_type,
        aggregate_id,
        company_id,
        branch_id,
        reference,
        idempotency_key,
        serde_json::to_value(aggregate)?,
    ))
}

#[derive(Debug, Clone)]
pub struct SqlxScmStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxScmStore {
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

    /// Persist one validated source aggregate and all its durable delivery
    /// records in one tenant-scoped transaction.
    pub async fn save(
        &self,
        aggregate: &ScmAggregate,
        expected_version: Option<u64>,
    ) -> Result<ScmWriteReceipt, PersistenceError> {
        aggregate.validate()?;
        let (
            aggregate_type,
            aggregate_id,
            company_id,
            branch_id,
            reference,
            idempotency_key,
            payload,
        ) = aggregate.fields()?;
        let request_hash = hash_request(aggregate_type, &payload, expected_version)?;
        let expected_replay_version = next_version(expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, request_hash, response
             FROM erpx_tenant.scm_command_idempotency
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
            let stored_aggregate_type: String = row.try_get("aggregate_type")?;
            let stored_aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let receipt = validate_idempotency_replay_receipt(
                stored_aggregate_type.as_str(),
                stored_aggregate_id,
                serde_json::from_value(row.try_get("response")?)?,
                aggregate_type,
                aggregate_id,
                expected_replay_version,
            )?;
            let event_payload = serde_json::json!({
                "aggregate_type": aggregate_type,
                "aggregate": payload,
                "version": receipt.version,
            });
            let rows = sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
                   AND aggregate_version=$4
                 ORDER BY event_id
                 FOR SHARE",
            )
            .bind(&self.holding_ref)
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(as_i64(receipt.version, "SCM replay source version")?)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(ScmReplayOutboxFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "SCM replay source version",
                    )?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            validate_replay_outbox_evidence(
                aggregate_type,
                aggregate_id,
                receipt.version,
                &event_payload,
                &rows,
            )?;
            let mut receipt = receipt;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }

        let current = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM erpx_tenant.scm_aggregates
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| as_u64(value, "SCM aggregate version"))
        .transpose()?;
        if current != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                expected: expected_version,
                actual: current,
            });
        }
        let version = current.unwrap_or(0).checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("SCM aggregate version overflow".into())
        })?;
        let branch_id = branch_id.or_else(|| branch_from_scope(aggregate.scope()));
        if current.is_some() {
            sqlx::query(
                "UPDATE erpx_tenant.scm_aggregates
                 SET company_id=$3, branch_id=$4, reference=$5, version=$6,
                     idempotency_key=$7, payload=$8, updated_at=now()
                 WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$9",
            )
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(reference)
            .bind(as_i64(version, "SCM aggregate version")?)
            .bind(idempotency_key)
            .bind(&payload)
            .bind(&self.holding_ref)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.scm_aggregates
                    (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
                     reference, version, idempotency_key, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(aggregate_id)
            .bind(aggregate_type)
            .bind(&self.holding_ref)
            .bind(company_id)
            .bind(branch_id)
            .bind(reference)
            .bind(as_i64(version, "SCM aggregate version")?)
            .bind(idempotency_key)
            .bind(&payload)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "INSERT INTO erpx_tenant.aggregate_snapshots
                (aggregate_type, aggregate_id, holding_ref, version, state)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (aggregate_type, aggregate_id)
             DO UPDATE SET holding_ref=$3, version=$4, state=$5, updated_at=now()",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(as_i64(version, "SCM aggregate version")?)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        let event_payload = serde_json::json!({
            "aggregate_type": aggregate_type,
            "aggregate": payload,
            "version": version,
        });
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
        )
        .bind(Uuid::new_v4())
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(as_i64(version, "SCM aggregate version")?)
        .bind(format!("scm.{aggregate_type}.v1"))
        .bind(event_payload)
        .execute(&mut *tx)
        .await?;
        let receipt = ScmWriteReceipt {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            version,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_command_idempotency
                (holding_ref, idempotency_key, aggregate_type, aggregate_id, request_hash, response)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(request_hash)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Apply an allow-listed Procurement lifecycle transition to authoritative
    /// source state, then reuse the atomic SCM writer for source, snapshot,
    /// shared outbox, and idempotency persistence. The source version is
    /// optimistic, so a concurrent writer cannot be overwritten.
    pub async fn transition(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
        transition: &str,
        expected_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<ScmWriteReceipt, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "SCM aggregate ID")?;
        if expected_version == 0 {
            return Err(PersistenceError::InvalidData(
                "SCM transition expected version must be positive".into(),
            ));
        }
        let transition = transition.trim();
        if transition.is_empty()
            || transition.len() > 64
            || transition.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "SCM transition is outside its bound".into(),
            ));
        }
        let kind = match aggregate_type {
            "product" => ScmAggregateKind::Product,
            "warehouse" => ScmAggregateKind::Warehouse,
            "purchase_request" => ScmAggregateKind::PurchaseRequest,
            "purchase_order" => ScmAggregateKind::PurchaseOrder,
            "goods_receipt" => ScmAggregateKind::GoodsReceipt,
            "stock_movement" => ScmAggregateKind::StockMovement,
            "stock_transfer" => ScmAggregateKind::StockTransfer,
            "stock_reservation" => ScmAggregateKind::StockReservation,
            _ => unreachable!("validated SCM aggregate type"),
        };
        let idempotency_key = deterministic_idempotency_key(kind, context).map_err(domain_error)?;
        if aggregate_type == "goods_receipt" && transition == "posted" {
            return self
                .post_goods_receipt(aggregate_id, expected_version, context)
                .await;
        }
        if aggregate_type == "stock_movement" && transition == "posted" {
            return self
                .post_stock_movement(aggregate_id, expected_version, context)
                .await;
        }
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, request_hash, response
             FROM erpx_tenant.scm_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_type: String = row.try_get("aggregate_type")?;
            let stored_id: Uuid = row.try_get("aggregate_id")?;
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_type != aggregate_type || stored_id != aggregate_id {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key,
                });
            }
            let mut receipt: ScmWriteReceipt = serde_json::from_value(row.try_get("response")?)?;
            let replay_version = expected_version.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("SCM transition replay version overflow".into())
            })?;
            if receipt.version != replay_version
                || receipt.aggregate_type != aggregate_type
                || receipt.aggregate_id != aggregate_id
                || receipt.replayed
            {
                return Err(PersistenceError::InvalidData(
                    "SCM transition idempotency receipt is not bound to the requested version"
                        .into(),
                ));
            }
            let rows = sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
                   AND aggregate_version=$4
                 ORDER BY event_id",
            )
            .bind(&self.holding_ref)
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(as_i64(replay_version, "SCM transition replay version")?)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(ScmReplayOutboxFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "SCM transition replay version",
                    )?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            validate_transition_replay_outbox_evidence(
                aggregate_type,
                aggregate_id,
                expected_version,
                transition,
                &stored_hash,
                &rows,
            )?;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }
        let row = sqlx::query(
            "SELECT payload, version
               FROM erpx_tenant.scm_aggregates
              WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3
              FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Err(PersistenceError::NotFound {
                entity: "SCM source aggregate",
                id: aggregate_id,
            });
        };
        let actual_version = as_u64(row.try_get("version")?, "SCM aggregate version")?;
        if actual_version != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                expected: Some(expected_version),
                actual: Some(actual_version),
            });
        }
        let payload: Value = row.try_get("payload")?;
        let mut aggregate = decode_aggregate(aggregate_type, payload)?;
        if aggregate.aggregate_id() != aggregate_id {
            return Err(PersistenceError::InvalidData(
                "SCM source aggregate identity does not match the requested transition".into(),
            ));
        }
        apply_transition(&mut aggregate, transition, context, Utc::now())?;
        self.save(&aggregate, Some(expected_version)).await
    }

    /// Post a Goods Receipt together with the authoritative Purchase Order it
    /// references. The domain operation validates the full procure-to-pay
    /// relationship and advances both aggregates in one write transaction;
    /// each resulting source state receives its own append-only outbox event,
    /// while the command idempotency receipt is stored once for replay.
    pub async fn post_goods_receipt(
        &self,
        receipt_id: Uuid,
        expected_receipt_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<ScmWriteReceipt, PersistenceError> {
        validate_uuid(receipt_id, "SCM goods receipt ID")?;
        if expected_receipt_version == 0 {
            return Err(PersistenceError::InvalidData(
                "SCM goods receipt expected version must be positive".into(),
            ));
        }
        let idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::GoodsReceipt, context)
                .map_err(domain_error)?;
        let request_payload = serde_json::json!({
            "operation": "post_goods_receipt",
            "receipt_id": receipt_id,
            "expected_receipt_version": expected_receipt_version,
        });
        let request_hash = hash_request(
            "goods_receipt.post",
            &request_payload,
            Some(expected_receipt_version),
        )?;
        let mut tx = self.begin_write().await?;

        if let Some(row) = sqlx::query(
            "SELECT request_hash, response
             FROM erpx_tenant.scm_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.clone(),
                });
            }
            let stored_response: Value = row.try_get("response")?;
            let mut receipt: ScmWriteReceipt = serde_json::from_value(stored_response.clone())?;
            let replay_version = expected_receipt_version.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("SCM goods receipt replay version overflow".into())
            })?;
            if receipt.aggregate_type != "goods_receipt"
                || receipt.aggregate_id != receipt_id
                || receipt.version != replay_version
                || receipt.replayed
            {
                return Err(PersistenceError::InvalidData(
                    "SCM goods receipt replay receipt is not bound to the requested version".into(),
                ));
            }
            let receipt_rows = sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type='goods_receipt'
                   AND aggregate_id=$2 AND aggregate_version=$3
                 ORDER BY event_id",
            )
            .bind(&self.holding_ref)
            .bind(receipt_id)
            .bind(as_i64(replay_version, "SCM goods receipt replay version")?)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(ScmReplayOutboxFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "SCM goods receipt replay version",
                    )?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            let replayed_receipt = validate_replay_source_event_rows(
                "goods_receipt",
                receipt_id,
                replay_version,
                &receipt_rows,
            )?;
            if !matches!(
                replayed_receipt,
                ScmAggregate::GoodsReceipt(GoodsReceipt {
                    status: GoodsReceiptStatus::Posted,
                    ..
                })
            ) {
                return Err(PersistenceError::InvalidData(
                    "SCM goods receipt replay event is not posted".into(),
                ));
            }

            // New receipts record the paired order version in the durable
            // idempotency response. Validate that second event as well; old
            // receipts without this metadata retain receipt-event validation.
            let pair = parse_goods_receipt_pair_metadata(&stored_response)?;
            if let Some((order_id, order_version)) = pair {
                let order_rows = sqlx::query(
                    "SELECT event_id, event_type, aggregate_type, aggregate_id,
                            aggregate_version, payload
                     FROM erpx_tenant.outbox_events
                     WHERE holding_ref=$1 AND aggregate_type='purchase_order'
                       AND aggregate_id=$2 AND aggregate_version=$3
                     ORDER BY event_id",
                )
                .bind(&self.holding_ref)
                .bind(order_id)
                .bind(as_i64(order_version, "SCM purchase order replay version")?)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|row| {
                    Ok::<_, PersistenceError>(ScmReplayOutboxFact {
                        event_id: row.try_get("event_id")?,
                        event_type: row.try_get("event_type")?,
                        aggregate_type: row.try_get("aggregate_type")?,
                        aggregate_id: row.try_get("aggregate_id")?,
                        aggregate_version: as_u64(
                            row.try_get("aggregate_version")?,
                            "SCM purchase order replay version",
                        )?,
                        payload: row.try_get("payload")?,
                    })
                })
                .collect::<Result<Vec<_>, PersistenceError>>()?;
                let replayed_order = validate_replay_source_event_rows(
                    "purchase_order",
                    order_id,
                    order_version,
                    &order_rows,
                )?;
                if replayed_order.aggregate_id() != order_id {
                    return Err(PersistenceError::InvalidData(
                        "SCM purchase order replay identity is inconsistent".into(),
                    ));
                }
            }
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }

        let receipt_row = sqlx::query(
            "SELECT payload, version
             FROM erpx_tenant.scm_aggregates
             WHERE aggregate_type='goods_receipt'
               AND aggregate_id=$1 AND holding_ref=$2
             FOR UPDATE",
        )
        .bind(receipt_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "SCM goods receipt",
            id: receipt_id,
        })?;
        let receipt_version = as_u64(receipt_row.try_get("version")?, "SCM goods receipt version")?;
        if receipt_version != expected_receipt_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "goods_receipt".into(),
                aggregate_id: receipt_id,
                expected: Some(expected_receipt_version),
                actual: Some(receipt_version),
            });
        }
        let receipt_payload: Value = receipt_row.try_get("payload")?;
        let mut receipt = decode_aggregate("goods_receipt", receipt_payload)?;
        let purchase_order_id = match &receipt {
            ScmAggregate::GoodsReceipt(value) => value.purchase_order_id,
            _ => unreachable!("goods receipt decoder returned another aggregate"),
        };

        let order_row = sqlx::query(
            "SELECT payload, version
             FROM erpx_tenant.scm_aggregates
             WHERE aggregate_type='purchase_order'
               AND aggregate_id=$1 AND holding_ref=$2
             FOR UPDATE",
        )
        .bind(purchase_order_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "SCM purchase order for goods receipt",
            id: purchase_order_id,
        })?;
        let order_current_version =
            as_u64(order_row.try_get("version")?, "SCM purchase order version")?;
        let order_payload: Value = order_row.try_get("payload")?;
        let mut order = decode_aggregate("purchase_order", order_payload)?;
        match (&mut order, &mut receipt) {
            (ScmAggregate::PurchaseOrder(order), ScmAggregate::GoodsReceipt(receipt)) => {
                order
                    .post_receipt(receipt, context, Utc::now())
                    .map_err(domain_error)?;
            }
            _ => unreachable!("SCM pair decoder returned another aggregate"),
        }
        order.validate()?;
        receipt.validate()?;

        let (order_version, _) = persist_aggregate_update_in_transaction(
            &mut tx,
            &order,
            order_current_version,
            &self.holding_ref,
        )
        .await?;
        let (receipt_version, _) = persist_aggregate_update_in_transaction(
            &mut tx,
            &receipt,
            receipt_version,
            &self.holding_ref,
        )
        .await?;

        let response = ScmWriteReceipt {
            aggregate_type: "goods_receipt".into(),
            aggregate_id: receipt_id,
            version: receipt_version,
            replayed: false,
        };
        let response_payload = serde_json::json!({
            "aggregate_type": response.aggregate_type.clone(),
            "aggregate_id": response.aggregate_id,
            "version": response.version,
            "replayed": response.replayed,
            "paired_aggregate_type": "purchase_order",
            "paired_aggregate_id": order.aggregate_id(),
            "paired_version": order_version,
        });
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_command_idempotency
                (holding_ref, idempotency_key, aggregate_type, aggregate_id,
                 request_hash, response)
             VALUES ($1,$2,'goods_receipt',$3,$4,$5)",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .bind(receipt_id)
        .bind(request_hash)
        .bind(response_payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(response)
    }

    /// Post a draft stock movement against the authoritative movement ledger.
    ///
    /// SCM does not maintain a second authoritative balance table. Instead,
    /// the current product/warehouse balance is replayed from the latest
    /// authoritative stock-movement snapshots while every movement snapshot
    /// is locked in this transaction. The candidate is then validated by the
    /// domain operation and committed together with its source snapshot,
    /// outbox event, and idempotency receipt. A concurrent post for the same
    /// product/warehouse therefore cannot both pass the available-stock or
    /// weighted-average-cost checks.
    pub async fn post_stock_movement(
        &self,
        movement_id: Uuid,
        expected_movement_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<ScmWriteReceipt, PersistenceError> {
        validate_uuid(movement_id, "SCM stock movement ID")?;
        if expected_movement_version == 0 {
            return Err(PersistenceError::InvalidData(
                "SCM stock movement expected version must be positive".into(),
            ));
        }
        let idempotency_key =
            deterministic_idempotency_key(ScmAggregateKind::StockMovement, context)
                .map_err(domain_error)?;
        let request_payload = serde_json::json!({
            "operation": "post_stock_movement",
            "movement_id": movement_id,
            "expected_movement_version": expected_movement_version,
        });
        let request_hash = hash_request(
            "stock_movement.post",
            &request_payload,
            Some(expected_movement_version),
        )?;
        let mut tx = self.begin_write().await?;

        if let Some(row) = sqlx::query(
            "SELECT request_hash, aggregate_type, aggregate_id, response
             FROM erpx_tenant.scm_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = row.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key,
                });
            }
            let stored_type: String = row.try_get("aggregate_type")?;
            let stored_id: Uuid = row.try_get("aggregate_id")?;
            let expected_version = expected_movement_version.checked_add(1).ok_or_else(|| {
                PersistenceError::InvalidData("SCM stock movement version overflow".into())
            })?;
            let receipt = validate_idempotency_replay_receipt(
                &stored_type,
                stored_id,
                serde_json::from_value(row.try_get("response")?)?,
                "stock_movement",
                movement_id,
                expected_version,
            )?;
            let rows = sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type='stock_movement'
                   AND aggregate_id=$2 AND aggregate_version=$3
                 ORDER BY event_id",
            )
            .bind(&self.holding_ref)
            .bind(movement_id)
            .bind(as_i64(
                expected_version,
                "SCM stock movement replay version",
            )?)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(ScmReplayOutboxFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "SCM stock movement replay version",
                    )?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            if rows.len() != 1 {
                return Err(PersistenceError::InvalidData(
                    "SCM stock movement replay requires exactly one authoritative outbox event"
                        .into(),
                ));
            }
            let row = &rows[0];
            validate_uuid(row.event_id, "SCM stock movement replay event ID")?;
            validate_source_event_type(&row.aggregate_type, &row.event_type)?;
            let aggregate = validate_source_event_payload(
                &row.aggregate_type,
                row.aggregate_id,
                row.aggregate_version,
                &row.payload,
            )?;
            if !matches!(
                aggregate,
                ScmAggregate::StockMovement(StockMovement {
                    status: StockMovementStatus::Posted,
                    ..
                })
            ) {
                return Err(PersistenceError::InvalidData(
                    "SCM stock movement replay event is not posted".into(),
                ));
            }
            let mut receipt = receipt;
            receipt.replayed = true;
            tx.commit().await?;
            return Ok(receipt);
        }

        // Lock every source movement before reconstructing the balance. All
        // concurrent movement posts use this same ordering, making the
        // product/warehouse check serializable without a new schema object.
        let snapshot_rows = sqlx::query(
            "SELECT aggregate_id, version, payload
             FROM erpx_tenant.scm_aggregates
             WHERE holding_ref=$1 AND aggregate_type='stock_movement'
             ORDER BY aggregate_id
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *tx)
        .await?;
        let mut snapshots = HashMap::with_capacity(snapshot_rows.len());
        for row in snapshot_rows {
            let id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(id, "SCM stock movement source ID")?;
            let version = as_u64(row.try_get("version")?, "SCM stock movement source version")?;
            snapshots.insert(id, (version, row.try_get::<Value, _>("payload")?));
        }
        let source_versions = snapshots
            .iter()
            .map(|(id, (version, _))| (*id, *version))
            .collect::<HashMap<_, _>>();

        let source_rows = sqlx::query(
            "WITH latest AS (
                 SELECT DISTINCT ON (aggregate_id)
                        event_id, event_type, aggregate_id, aggregate_version,
                        payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type='stock_movement'
                 ORDER BY aggregate_id, aggregate_version DESC, event_id DESC
             )
             SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM latest
             ORDER BY occurred_at, event_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *tx)
        .await?;
        let mut movements = Vec::with_capacity(source_rows.len());
        for row in source_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "SCM stock movement event version",
            )?;
            let event_type: String = row.try_get("event_type")?;
            let event_payload: Value = row.try_get("payload")?;
            validate_uuid(event_id, "SCM stock movement event ID")?;
            validate_uuid(aggregate_id, "SCM stock movement event aggregate ID")?;
            validate_source_event_type("stock_movement", &event_type)?;
            let event = ScmEventPayload::deserialize(event_payload.clone())?;
            let aggregate = validate_source_event_payload(
                "stock_movement",
                aggregate_id,
                aggregate_version,
                &event_payload,
            )?;
            let (source_version, source_payload) =
                snapshots.remove(&aggregate_id).ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "SCM stock movement outbox event has no source snapshot".into(),
                    )
                })?;
            if source_version != aggregate_version || source_payload != event.aggregate {
                return Err(PersistenceError::InvalidData(
                    "SCM stock movement source snapshot and outbox disagree".into(),
                ));
            }
            let ScmAggregate::StockMovement(movement) = aggregate else {
                unreachable!("stock movement source decoder returned another aggregate")
            };
            movements.push((
                row.try_get::<DateTime<Utc>, _>("occurred_at")?,
                event_id,
                movement,
            ));
        }
        if !snapshots.is_empty() {
            return Err(PersistenceError::InvalidData(
                "SCM stock movement source snapshot has no outbox event".into(),
            ));
        }

        let (candidate, candidate_version) = movements
            .iter()
            .find(|(_, _, movement)| movement.id == movement_id)
            .map(|(_, _, movement)| {
                (
                    movement.clone(),
                    source_versions
                        .get(&movement.id)
                        .copied()
                        .unwrap_or_default(),
                )
            })
            .ok_or(PersistenceError::NotFound {
                entity: "SCM stock movement",
                id: movement_id,
            })?;
        if candidate_version != expected_movement_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "stock_movement".into(),
                aggregate_id: movement_id,
                expected: Some(expected_movement_version),
                actual: Some(candidate_version),
            });
        }

        let mut balance = StockBalance::new(candidate.product.clone(), candidate.warehouse.clone())
            .map_err(domain_error)?;
        for (_, _, movement) in &movements {
            if movement.status != StockMovementStatus::Posted
                || movement.product != candidate.product
                || movement.warehouse != candidate.warehouse
            {
                continue;
            }
            apply_posted_stock_movement(&mut balance, movement)?;
        }
        let mut candidate = candidate;
        candidate
            .post(&mut balance, context, Utc::now())
            .map_err(domain_error)?;
        let (version, _) = persist_aggregate_update_in_transaction(
            &mut tx,
            &ScmAggregate::StockMovement(candidate),
            expected_movement_version,
            &self.holding_ref,
        )
        .await?;
        let response = ScmWriteReceipt {
            aggregate_type: "stock_movement".into(),
            aggregate_id: movement_id,
            version,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_command_idempotency
                (holding_ref, idempotency_key, aggregate_type, aggregate_id,
                 request_hash, response)
             VALUES ($1,$2,'stock_movement',$3,$4,$5)",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .bind(movement_id)
        .bind(request_hash)
        .bind(serde_json::to_value(&response)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(response)
    }

    /// Read the immutable SCM source stream in a bounded, stable order for
    /// live projection delivery or replacement-read-database replay.
    pub async fn read_source_event_page(
        &self,
        cursor: Option<&ScmSourceEventCursor>,
        page_size: u32,
    ) -> Result<ScmSourceEventPage, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let page = read_scm_source_event_page_in_transaction(
            &mut tx,
            &self.holding_ref,
            cursor,
            page_size,
        )
        .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Reconcile one bounded page of authoritative source aggregate facts.
    /// The source page is ordered by aggregate identity and contains the
    /// highest version/event ID for each identity, so a page boundary cannot
    /// make an older duplicate version look authoritative. Read rows are
    /// fetched only for the page's identities.
    pub async fn reconcile_projection_page(
        &self,
        cursor: Option<&ScmSourceEventCursor>,
        page_size: u32,
    ) -> Result<
        (
            ScmProjectionReconciliation,
            ScmReconciliationEvidence,
            Option<ScmSourceEventCursor>,
            bool,
        ),
        PersistenceError,
    > {
        if page_size == 0 || page_size > MAX_SCM_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "SCM reconciliation page size must be 1..={MAX_SCM_READ_PAGE_SIZE}"
            )));
        }
        validate_reconciliation_cursor(&self.holding_ref, cursor)?;
        let (mut facts, has_more) = self.read_source_aggregate_page(cursor, page_size).await?;
        if has_more {
            facts.truncate(page_size as usize);
        }

        let mut source_event_count = 0_u64;
        for fact in &facts {
            validate_source_aggregate_fact(fact)?;
            source_event_count = source_event_count
                .checked_add(fact.event_count)
                .ok_or_else(|| PersistenceError::InvalidData("SCM event count overflow".into()))?;
        }

        let projected = self.read_projected_page(&facts).await?;
        let mut reconciliation = ScmProjectionReconciliation {
            source_event_count,
            source_aggregate_count: facts.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            ..ScmProjectionReconciliation::default()
        };
        for fact in &facts {
            reconciliation.source_gap_count = reconciliation
                .source_gap_count
                .checked_add(source_version_gap_count(
                    fact.first_version,
                    fact.aggregate_version,
                    fact.event_count,
                    fact.distinct_version_count,
                )?)
                .ok_or_else(|| {
                    PersistenceError::InvalidData("SCM source gap count overflow".into())
                })?;
            let key = (fact.aggregate_type.clone(), fact.aggregate_id);
            let Some((projected_version, projected_payload, checkpoint)) = projected.get(&key)
            else {
                reconciliation.missing_aggregate_count =
                    reconciliation.missing_aggregate_count.saturating_add(1);
                reconciliation.missing_checkpoint_count =
                    reconciliation.missing_checkpoint_count.saturating_add(1);
                continue;
            };
            let projection_is_valid = validate_projected_aggregate(
                &fact.aggregate_type,
                fact.aggregate_id,
                *projected_version,
                projected_payload,
            )
            .is_ok();
            if !projection_is_valid {
                reconciliation.invalid_projection_count =
                    reconciliation.invalid_projection_count.saturating_add(1);
            }
            if *projected_version < fact.aggregate_version {
                reconciliation.stale_aggregate_count =
                    reconciliation.stale_aggregate_count.saturating_add(1);
            } else if *projected_version > fact.aggregate_version {
                reconciliation.ahead_aggregate_count =
                    reconciliation.ahead_aggregate_count.saturating_add(1);
            } else if projection_is_valid {
                let event: ScmEventPayload = serde_json::from_value(fact.payload.clone())?;
                if *projected_payload != event.aggregate {
                    reconciliation.invalid_projection_count =
                        reconciliation.invalid_projection_count.saturating_add(1);
                }
            }
            match checkpoint {
                None => {
                    reconciliation.missing_checkpoint_count =
                        reconciliation.missing_checkpoint_count.saturating_add(1);
                }
                Some((checkpoint_version, checkpoint_event_id)) => {
                    if *checkpoint_version != *projected_version
                        || (*projected_version == fact.aggregate_version
                            && *checkpoint_event_id != fact.event_id)
                    {
                        reconciliation.stale_checkpoint_count =
                            reconciliation.stale_checkpoint_count.saturating_add(1);
                    }
                    if checkpoint_event_id.is_nil() {
                        reconciliation.invalid_projection_count =
                            reconciliation.invalid_projection_count.saturating_add(1);
                    }
                }
            }
        }

        // The final source page is also the only safe point to inspect
        // read-side rows that have no source identity. Include those residual
        // differences in the evidence emitted for this page so a durable
        // evidence fact cannot claim a healthy run while an orphan projection
        // or checkpoint is present.
        if !has_more {
            let (_, orphan_count, orphan_checkpoint_count, invalid_orphan_count) =
                self.read_projection_residuals(page_size).await?;
            reconciliation.orphan_aggregate_count = orphan_count;
            reconciliation.orphan_checkpoint_count = orphan_checkpoint_count;
            reconciliation.invalid_projection_count = reconciliation
                .invalid_projection_count
                .checked_add(invalid_orphan_count)
                .ok_or_else(|| {
                    PersistenceError::InvalidData("SCM invalid projection count overflow".into())
                })?;
        }

        let next_cursor = has_more
            .then(|| {
                facts.last().map(|fact| ScmSourceEventCursor {
                    holding_ref: self.holding_ref.clone(),
                    aggregate_type: fact.aggregate_type.clone(),
                    aggregate_id: fact.aggregate_id,
                    aggregate_version: fact.aggregate_version,
                    event_id: fact.event_id,
                })
            })
            .flatten();
        let source_digest = digest_source_aggregate_page(&facts)?;
        let after_event_id = cursor.map(|value| value.event_id);
        let last_event_id = facts.last().map(|fact| fact.event_id);
        let evidence_id = reconciliation_evidence_id(
            &self.holding_ref,
            after_event_id,
            last_event_id,
            &source_digest,
            reconciliation,
        );
        let evidence = ScmReconciliationEvidence {
            evidence_id,
            holding_ref: self.holding_ref.clone(),
            after_event_id,
            first_event_id: facts.first().map(|fact| fact.event_id),
            last_event_id,
            has_more,
            source_event_count,
            source_aggregate_count: facts.len() as u64,
            difference_count: reconciliation.difference_count(),
            healthy: reconciliation.is_consistent(),
            rollback_required: !reconciliation.is_consistent(),
            reconciliation,
            source_digest,
        };
        evidence
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        Ok((reconciliation, evidence, next_cursor, has_more))
    }

    /// Compare the entire source/read boundary by repeatedly consuming
    /// bounded, ordered reconciliation pages. The returned report preserves
    /// the historical aggregate-level counters while keeping application
    /// memory bounded by one page.
    pub async fn reconcile_projection_bounded(
        &self,
        page_size: u32,
    ) -> Result<ScmProjectionReconciliation, PersistenceError> {
        if page_size == 0 || page_size > MAX_SCM_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "SCM reconciliation page size must be 1..={MAX_SCM_READ_PAGE_SIZE}"
            )));
        }
        let mut total = ScmProjectionReconciliation::default();
        let mut cursor = None;
        loop {
            let (page, _, next_cursor, has_more) = self
                .reconcile_projection_page(cursor.as_ref(), page_size)
                .await?;
            add_reconciliation_counts(&mut total, page)?;
            if !has_more {
                break;
            }
            cursor = next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "SCM reconciliation page omitted its continuation cursor".into(),
                ));
            }
        }
        let (projected_count, _, _, _) = self.read_projection_residuals(page_size).await?;
        total.projected_aggregate_count = projected_count;
        Ok(total)
    }

    /// Compare the authoritative SCM outbox with the disposable read model.
    /// A mismatch is reported for rebuild/review; this method never mutates
    /// source state or silently repairs a projection.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<ScmProjectionReconciliation, PersistenceError> {
        self.reconcile_projection_bounded(MAX_SCM_READ_PAGE_SIZE)
            .await
    }

    /// Run a bounded source/read reconciliation and durably record every page
    /// that was inspected.  The source outbox and read projection are not
    /// modified: a mismatch remains a fail-closed rollback/rebuild signal.
    ///
    /// Page evidence is persisted before advancing its continuation cursor so
    /// an interrupted run can be retried without losing the exact source
    /// boundary that was checked.  Replaying an unchanged run is idempotent
    /// because [`Self::persist_reconciliation_evidence`] keys each evidence
    /// fact by its deterministic page identity.
    pub async fn reconcile_projection_and_persist_evidence(
        &self,
        page_size: u32,
    ) -> Result<ScmProjectionReconciliation, PersistenceError> {
        if page_size == 0 || page_size > MAX_SCM_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "SCM reconciliation page size must be 1..={MAX_SCM_READ_PAGE_SIZE}"
            )));
        }

        let mut total = ScmProjectionReconciliation::default();
        let mut cursor = None;
        loop {
            let (page, evidence, next_cursor, has_more) = self
                .reconcile_projection_page(cursor.as_ref(), page_size)
                .await?;
            self.persist_reconciliation_evidence(&evidence).await?;
            add_reconciliation_counts(&mut total, page)?;
            if !has_more {
                break;
            }
            cursor = next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "SCM reconciliation page omitted its continuation cursor".into(),
                ));
            }
        }

        // Source pages only inspect source-backed identities. Residuals are
        // read-side rows/checkpoints with no authoritative source identity,
        // so replace the page-local projected count and add their drift after
        // the source pages have been durably evidenced.
        let (projected_count, _, _, _) = self.read_projection_residuals(page_size).await?;
        total.projected_aggregate_count = projected_count;
        Ok(total)
    }

    /// Persist one page's evidence and its matching immutable outbox fact in
    /// one tenant-scoped transaction. Repeating the same page is a replay;
    /// changing the evidence for an existing page identity is rejected.
    pub async fn persist_reconciliation_evidence(
        &self,
        evidence: &ScmReconciliationEvidence,
    ) -> Result<Uuid, PersistenceError> {
        evidence
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if evidence.holding_ref != self.holding_ref {
            return Err(PersistenceError::InvalidData(
                "SCM reconciliation evidence holding does not match the routed store".into(),
            ));
        }
        let idempotency_key = reconciliation_evidence_key(evidence);
        let expected_id = reconciliation_evidence_id(
            &self.holding_ref,
            evidence.after_event_id,
            evidence.last_event_id,
            &evidence.source_digest,
            evidence.reconciliation,
        );
        if evidence.evidence_id != expected_id {
            return Err(PersistenceError::InvalidData(
                "SCM reconciliation evidence ID is not deterministic".into(),
            ));
        }
        let request_hash = hash_reconciliation_evidence(evidence)?;
        let response = serde_json::to_value(evidence)?;
        let outbox_payload = serde_json::json!({
            "aggregate_type": "scm_reconciliation",
            "aggregate": response,
            "version": 1_u64,
        });
        let mut tx = self.begin_write().await?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.scm_command_idempotency
                (holding_ref, idempotency_key, aggregate_type, aggregate_id,
                 request_hash, response)
             VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (holding_ref, idempotency_key) DO NOTHING",
        )
        .bind(&self.holding_ref)
        .bind(&idempotency_key)
        .bind("scm_reconciliation")
        .bind(evidence.evidence_id)
        .bind(&request_hash)
        .bind(&response)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        let stored = sqlx::query(
            "SELECT request_hash, aggregate_type, aggregate_id, response
             FROM erpx_tenant.scm_command_idempotency
             WHERE holding_ref=$1 AND idempotency_key=$2 FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(&idempotency_key)
        .fetch_one(&mut *tx)
        .await?;
        let stored_hash: String = stored.try_get("request_hash")?;
        let stored_type: String = stored.try_get("aggregate_type")?;
        let stored_id: Uuid = stored.try_get("aggregate_id")?;
        let stored_evidence: ScmReconciliationEvidence =
            serde_json::from_value(stored.try_get("response")?)?;
        if stored_hash != request_hash
            || stored_type != "scm_reconciliation"
            || stored_id != evidence.evidence_id
            || stored_evidence != *evidence
        {
            return Err(PersistenceError::IdempotencyConflict {
                key: idempotency_key,
            });
        }
        if inserted {
            sqlx::query(
                "INSERT INTO erpx_tenant.outbox_events
                    (event_id, aggregate_type, aggregate_id, holding_ref,
                     aggregate_version, event_type, payload, occurred_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,now())
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(evidence.evidence_id)
            .bind("scm_reconciliation")
            .bind(evidence.evidence_id)
            .bind(&self.holding_ref)
            .bind(1_i64)
            .bind("scm.reconciliation.v1")
            .bind(&outbox_payload)
            .execute(&mut *tx)
            .await?;
        }
        let outbox = sqlx::query(
            "SELECT aggregate_type, aggregate_id, holding_ref,
                    aggregate_version, event_type, payload
             FROM erpx_tenant.outbox_events
             WHERE event_id=$1 AND holding_ref=$2",
        )
        .bind(evidence.evidence_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(outbox) = outbox else {
            return Err(PersistenceError::InvalidData(
                "SCM reconciliation evidence is missing its outbox fact".into(),
            ));
        };
        if outbox.try_get::<String, _>("aggregate_type")? != "scm_reconciliation"
            || outbox.try_get::<Uuid, _>("aggregate_id")? != evidence.evidence_id
            || outbox.try_get::<String, _>("holding_ref")? != self.holding_ref
            || as_u64(outbox.try_get("aggregate_version")?, "SCM evidence version")? != 1
            || outbox.try_get::<String, _>("event_type")? != "scm.reconciliation.v1"
            || outbox.try_get::<Value, _>("payload")? != outbox_payload
        {
            return Err(PersistenceError::IdempotencyConflict {
                key: idempotency_key,
            });
        }
        tx.commit().await?;
        Ok(evidence.evidence_id)
    }

    async fn read_source_aggregate_page(
        &self,
        cursor: Option<&ScmSourceEventCursor>,
        page_size: u32,
    ) -> Result<(Vec<ScmSourceAggregateFact>, bool), PersistenceError> {
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let base = "WITH latest AS (
                SELECT DISTINCT ON (aggregate_type, aggregate_id)
                       event_id, event_type, aggregate_type, aggregate_id,
                       aggregate_version, payload
                FROM erpx_tenant.outbox_events
                WHERE holding_ref=$1
                  AND aggregate_type IN
                      ('product', 'warehouse', 'purchase_request', 'purchase_order',
                       'goods_receipt', 'stock_movement', 'stock_transfer',
                       'stock_reservation')";
        let query_text = if cursor.is_some() {
            format!(
                "{base}
                  AND (aggregate_type, aggregate_id) > ($2, $3)
                 ORDER BY aggregate_type, aggregate_id,
                          aggregate_version DESC, event_id DESC
             ), counts AS (
                 SELECT aggregate_type, aggregate_id,
                        MIN(aggregate_version) AS first_version,
                        COUNT(DISTINCT aggregate_version) AS distinct_version_count,
                        COUNT(*) AS event_count
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN
                       ('product', 'warehouse', 'purchase_request', 'purchase_order',
                        'goods_receipt', 'stock_movement', 'stock_transfer',
                        'stock_reservation')
                 GROUP BY aggregate_type, aggregate_id
             ), conflicting_versions AS (
                 SELECT aggregate_type, aggregate_id, aggregate_version
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN
                       ('product', 'warehouse', 'purchase_request', 'purchase_order',
                        'goods_receipt', 'stock_movement', 'stock_transfer',
                        'stock_reservation')
                 GROUP BY aggregate_type, aggregate_id, aggregate_version
                 HAVING COUNT(DISTINCT payload) > 1
                     OR COUNT(DISTINCT event_type) > 1
             ), conflict_counts AS (
                 SELECT aggregate_type, aggregate_id,
                        COUNT(*) AS conflicting_version_count
                 FROM conflicting_versions
                 GROUP BY aggregate_type, aggregate_id
             )
             SELECT latest.event_id, latest.event_type, latest.aggregate_type,
                    latest.aggregate_id,
                    latest.aggregate_version, latest.payload,
                    counts.first_version, counts.distinct_version_count,
                    counts.event_count,
                    COALESCE(conflict_counts.conflicting_version_count, 0)
                        AS conflicting_version_count
             FROM latest
             JOIN counts ON counts.aggregate_type=latest.aggregate_type
                         AND counts.aggregate_id=latest.aggregate_id
             LEFT JOIN conflict_counts
                    ON conflict_counts.aggregate_type=latest.aggregate_type
                   AND conflict_counts.aggregate_id=latest.aggregate_id
             ORDER BY latest.aggregate_type, latest.aggregate_id
             LIMIT $4"
            )
        } else {
            format!(
                "{base}
                 ORDER BY aggregate_type, aggregate_id,
                          aggregate_version DESC, event_id DESC
             ), counts AS (
                 SELECT aggregate_type, aggregate_id,
                        MIN(aggregate_version) AS first_version,
                        COUNT(DISTINCT aggregate_version) AS distinct_version_count,
                        COUNT(*) AS event_count
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN
                       ('product', 'warehouse', 'purchase_request', 'purchase_order',
                        'goods_receipt', 'stock_movement', 'stock_transfer',
                        'stock_reservation')
                 GROUP BY aggregate_type, aggregate_id
             ), conflicting_versions AS (
                 SELECT aggregate_type, aggregate_id, aggregate_version
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type IN
                       ('product', 'warehouse', 'purchase_request', 'purchase_order',
                        'goods_receipt', 'stock_movement', 'stock_transfer',
                        'stock_reservation')
                 GROUP BY aggregate_type, aggregate_id, aggregate_version
                 HAVING COUNT(DISTINCT payload) > 1
                     OR COUNT(DISTINCT event_type) > 1
             ), conflict_counts AS (
                 SELECT aggregate_type, aggregate_id,
                        COUNT(*) AS conflicting_version_count
                 FROM conflicting_versions
                 GROUP BY aggregate_type, aggregate_id
             )
             SELECT latest.event_id, latest.event_type, latest.aggregate_type,
                    latest.aggregate_id,
                    latest.aggregate_version, latest.payload,
                    counts.first_version, counts.distinct_version_count,
                    counts.event_count,
                    COALESCE(conflict_counts.conflicting_version_count, 0)
                        AS conflicting_version_count
             FROM latest
             JOIN counts ON counts.aggregate_type=latest.aggregate_type
                         AND counts.aggregate_id=latest.aggregate_id
             LEFT JOIN conflict_counts
                    ON conflict_counts.aggregate_type=latest.aggregate_type
                   AND conflict_counts.aggregate_id=latest.aggregate_id
             ORDER BY latest.aggregate_type, latest.aggregate_id
             LIMIT $2"
            )
        };
        let mut query = sqlx::query(&query_text).bind(&self.holding_ref);
        if let Some(cursor) = cursor {
            query = query
                .bind(&cursor.aggregate_type)
                .bind(cursor.aggregate_id)
                .bind(limit);
        } else {
            query = query.bind(limit);
        }
        let rows = query.fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let has_more = rows.len() > page_size as usize;
        let facts = rows
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(ScmSourceAggregateFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "SCM source aggregate version",
                    )?,
                    payload: row.try_get("payload")?,
                    event_count: as_u64(row.try_get("event_count")?, "SCM source event count")?,
                    first_version: as_u64(
                        row.try_get("first_version")?,
                        "SCM first source aggregate version",
                    )?,
                    distinct_version_count: as_u64(
                        row.try_get("distinct_version_count")?,
                        "SCM distinct source aggregate version count",
                    )?,
                    conflicting_version_count: as_u64(
                        row.try_get("conflicting_version_count")?,
                        "SCM conflicting source aggregate version count",
                    )?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        Ok((facts, has_more))
    }

    async fn read_projected_page(
        &self,
        facts: &[ScmSourceAggregateFact],
    ) -> Result<HashMap<(String, Uuid), (u64, Value, Option<(u64, Uuid)>)>, PersistenceError> {
        let mut projected = HashMap::with_capacity(facts.len());
        if facts.is_empty() {
            return Ok(projected);
        }
        let predicates = facts
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let type_param = 2 + index * 2;
                let id_param = type_param + 1;
                format!("(a.aggregate_type=${type_param} AND a.aggregate_id=${id_param})")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        let query_text = format!(
            "SELECT a.aggregate_type, a.aggregate_id, a.source_version, a.payload,
                    c.source_version AS checkpoint_version,
                    c.last_event_id AS checkpoint_event_id
             FROM erpx_tenant.scm_aggregates AS a
             LEFT JOIN erpx_tenant.scm_projection_checkpoints AS c
               ON c.aggregate_type=a.aggregate_type
              AND c.aggregate_id=a.aggregate_id
              AND c.holding_ref=a.holding_ref
             WHERE a.holding_ref=$1 AND ({predicates})"
        );
        let mut tx = self.begin_read().await?;
        let mut query = sqlx::query(&query_text).bind(&self.holding_ref);
        for fact in facts {
            query = query.bind(&fact.aggregate_type).bind(fact.aggregate_id);
        }
        let rows = query.fetch_all(&mut *tx).await?;
        tx.commit().await?;
        for row in rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let source_version = as_u64(row.try_get("source_version")?, "SCM projected version")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|value| as_u64(value, "SCM checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("checkpoint_event_id")?;
            let checkpoint = match (checkpoint_version, checkpoint_event_id) {
                (None, None) => None,
                (Some(version), Some(event_id)) => Some((version, event_id)),
                _ => {
                    return Err(PersistenceError::InvalidData(
                        "SCM projection checkpoint columns are incomplete".into(),
                    ));
                }
            };
            projected.insert(
                (aggregate_type, aggregate_id),
                (source_version, row.try_get("payload")?, checkpoint),
            );
        }
        Ok(projected)
    }

    async fn read_projection_residuals(
        &self,
        page_size: u32,
    ) -> Result<(u64, u64, u64, u64), PersistenceError> {
        let mut tx = self.begin_read().await?;
        let projected_count = as_u64(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM erpx_tenant.scm_aggregates
                 WHERE holding_ref=$1",
            )
            .bind(&self.holding_ref)
            .fetch_one(&mut *tx)
            .await?,
            "SCM projected aggregate count",
        )?;
        let orphan_checkpoint_count = as_u64(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM erpx_tenant.scm_projection_checkpoints AS c
                 WHERE c.holding_ref=$1
                   AND NOT EXISTS (
                       SELECT 1 FROM erpx_tenant.scm_aggregates AS p
                       WHERE p.holding_ref=c.holding_ref
                         AND p.aggregate_type=c.aggregate_type
                         AND p.aggregate_id=c.aggregate_id
                   )",
            )
            .bind(&self.holding_ref)
            .fetch_one(&mut *tx)
            .await?,
            "SCM orphan checkpoint count",
        )?;
        tx.commit().await?;

        let mut orphan_count = 0_u64;
        let mut invalid_orphan_count = 0_u64;
        let mut cursor: Option<(String, Uuid)> = None;
        loop {
            let limit = i64::from(page_size) + 1;
            let mut read_tx = self.begin_read().await?;
            let query_text = if cursor.is_some() {
                "SELECT p.aggregate_type, p.aggregate_id, p.source_version, p.payload
                 FROM erpx_tenant.scm_aggregates AS p
                 WHERE p.holding_ref=$1
                   AND (p.aggregate_type, p.aggregate_id) > ($2, $3)
                 ORDER BY p.aggregate_type, p.aggregate_id
                 LIMIT $4"
            } else {
                "SELECT p.aggregate_type, p.aggregate_id, p.source_version, p.payload
                 FROM erpx_tenant.scm_aggregates AS p
                 WHERE p.holding_ref=$1
                 ORDER BY p.aggregate_type, p.aggregate_id
                 LIMIT $2"
            };
            let mut query = sqlx::query(query_text).bind(&self.holding_ref);
            if let Some((aggregate_type, aggregate_id)) = &cursor {
                query = query.bind(aggregate_type).bind(*aggregate_id).bind(limit);
            } else {
                query = query.bind(limit);
            }
            let rows = query.fetch_all(&mut *read_tx).await?;
            read_tx.commit().await?;
            let has_more = rows.len() > page_size as usize;
            let page_rows = &rows[..rows.len().min(page_size as usize)];
            let keys = page_rows
                .iter()
                .map(|row| {
                    Ok::<_, PersistenceError>((
                        row.try_get("aggregate_type")?,
                        row.try_get("aggregate_id")?,
                    ))
                })
                .collect::<Result<Vec<_>, PersistenceError>>()?;
            let source_keys = self.read_source_aggregate_keys(&keys).await?;
            for (row, key) in page_rows.iter().zip(keys) {
                let source_version =
                    as_u64(row.try_get("source_version")?, "SCM projected version")?;
                let payload: Value = row.try_get("payload")?;
                if !source_keys.contains(&key) {
                    orphan_count = orphan_count.saturating_add(1);
                    if validate_projected_aggregate(&key.0, key.1, source_version, &payload)
                        .is_err()
                    {
                        invalid_orphan_count = invalid_orphan_count.saturating_add(1);
                    }
                }
            }
            if !has_more {
                break;
            }
            let last = rows
                .get(page_size as usize - 1)
                .ok_or_else(|| PersistenceError::InvalidData("SCM orphan page is empty".into()))?;
            cursor = Some((
                last.try_get("aggregate_type")?,
                last.try_get("aggregate_id")?,
            ));
        }
        Ok((
            projected_count,
            orphan_count,
            orphan_checkpoint_count,
            invalid_orphan_count,
        ))
    }

    async fn read_source_aggregate_keys(
        &self,
        keys: &[(String, Uuid)],
    ) -> Result<HashSet<(String, Uuid)>, PersistenceError> {
        let mut source_keys = HashSet::with_capacity(keys.len());
        if keys.is_empty() {
            return Ok(source_keys);
        }
        let predicates = keys
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let type_param = 2 + index * 2;
                let id_param = type_param + 1;
                format!("(aggregate_type=${type_param} AND aggregate_id=${id_param})")
            })
            .collect::<Vec<_>>()
            .join(" OR ");
        let query_text = format!(
            "SELECT aggregate_type, aggregate_id
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN
                   ('product', 'warehouse', 'purchase_request', 'purchase_order',
                    'goods_receipt', 'stock_movement', 'stock_transfer',
                    'stock_reservation')
               AND ({predicates})"
        );
        let mut tx = self.begin_write().await?;
        let mut query = sqlx::query(&query_text).bind(&self.holding_ref);
        for (aggregate_type, aggregate_id) in keys {
            query = query.bind(aggregate_type).bind(*aggregate_id);
        }
        let rows = query.fetch_all(&mut *tx).await?;
        tx.commit().await?;
        for row in rows {
            source_keys.insert((row.try_get("aggregate_type")?, row.try_get("aggregate_id")?));
        }
        Ok(source_keys)
    }

    /// Read only from the disposable projection.  Commands never call this
    /// method to decide whether a write is allowed.
    pub async fn get(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<ScmReadRow>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "SCM aggregate ID")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT a.payload, a.source_version,
                    c.source_version AS checkpoint_version,
                    c.last_event_id AS checkpoint_event_id
             FROM erpx_tenant.scm_aggregates AS a
             LEFT JOIN erpx_tenant.scm_projection_checkpoints AS c
               ON c.aggregate_type=a.aggregate_type
              AND c.aggregate_id=a.aggregate_id
              AND c.holding_ref=a.holding_ref
             WHERE a.aggregate_type=$1 AND a.aggregate_id=$2 AND a.holding_ref=$3",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(|row| {
            let payload: Value = row.try_get("payload")?;
            let source_version = as_u64(row.try_get("source_version")?, "SCM source version")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|value| as_u64(value, "SCM checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("checkpoint_event_id")?;
            if checkpoint_version != Some(source_version)
                || checkpoint_event_id.is_none_or(|value| value.is_nil())
            {
                return Err(PersistenceError::InvalidData(
                    "SCM read projection checkpoint is missing or inconsistent".into(),
                ));
            }
            let aggregate = decode_aggregate(aggregate_type, payload)?;
            aggregate.validate()?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
            {
                return Err(PersistenceError::InvalidData(
                    "SCM read projection identity does not match its key".into(),
                ));
            }
            Ok(ScmReadRow {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                aggregate_version: source_version,
                aggregate,
            })
        })
        .transpose()
    }

    /// Read a bounded, typed page from the disposable SCM projection. The
    /// source database is deliberately not consulted; every row must carry a
    /// complete checkpoint and a validated aggregate scope before it leaves
    /// this adapter.
    pub async fn list_for_scope(
        &self,
        scope: ScmReadScope,
        page_size: u32,
        before: Option<&ScmReadCursor>,
    ) -> Result<ScmReadPage, PersistenceError> {
        validate_read_scope(scope)?;
        if page_size == 0 || page_size > MAX_SCM_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "SCM read page size must be 1..={MAX_SCM_READ_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = before {
            validate_aggregate_type(cursor.aggregate_type)?;
            validate_uuid(cursor.aggregate_id, "SCM read cursor aggregate ID")?;
        }
        let requested_scope = match scope.branch_id {
            Some(branch_id) => TenantScope::Branch {
                holding_id: scope.holding_id,
                company_id: scope.company_id,
                branch_id,
            },
            None => TenantScope::Company {
                holding_id: scope.holding_id,
                company_id: scope.company_id,
            },
        };
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT a.aggregate_type, a.aggregate_id, a.company_id, a.branch_id,
                    a.reference, a.source_version, a.payload, a.projected_at,
                    c.source_version AS checkpoint_version,
                    c.last_event_id AS checkpoint_event_id
             FROM erpx_tenant.scm_aggregates AS a
             LEFT JOIN erpx_tenant.scm_projection_checkpoints AS c
               ON c.aggregate_type=a.aggregate_type
              AND c.aggregate_id=a.aggregate_id
              AND c.holding_ref=a.holding_ref
             WHERE a.holding_ref=$1 AND a.company_id=$2
               AND ($3::UUID IS NULL OR a.branch_id=$3)
               AND ($4::TIMESTAMPTZ IS NULL OR
                    (a.projected_at, a.aggregate_type, a.aggregate_id) <
                    ($4, $5, $6))
             ORDER BY a.projected_at DESC, a.aggregate_type DESC, a.aggregate_id DESC
             LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.projected_at))
        .bind(before.map(|cursor| cursor.aggregate_type))
        .bind(before.map(|cursor| cursor.aggregate_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let page_len = usize::try_from(page_size)
            .map_err(|_| PersistenceError::InvalidData("SCM read page size overflow".into()))?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut decoded = Vec::with_capacity(selected.len());
        for row in selected {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_aggregate_type(&aggregate_type)?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(aggregate_id, "SCM read aggregate ID")?;
            let source_version = as_u64(row.try_get("source_version")?, "SCM source version")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|value| as_u64(value, "SCM checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("checkpoint_event_id")?;
            if checkpoint_version != Some(source_version)
                || checkpoint_event_id.is_none_or(|event_id| event_id.is_nil())
            {
                return Err(PersistenceError::InvalidData(
                    "SCM list row checkpoint is missing or inconsistent".into(),
                ));
            }
            let aggregate = decode_aggregate(&aggregate_type, row.try_get("payload")?)?;
            aggregate.validate()?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
                || aggregate.scope() == &TenantScope::Platform
                || !requested_scope.contains(aggregate.scope())
            {
                return Err(PersistenceError::InvalidData(
                    "SCM list row identity or scope is invalid".into(),
                ));
            }
            let persisted_company_id: Uuid = row.try_get("company_id")?;
            let persisted_branch_id: Option<Uuid> = row.try_get("branch_id")?;
            let (aggregate_company_id, aggregate_branch_id) = match aggregate.scope() {
                TenantScope::Company { company_id, .. } => (*company_id, None),
                TenantScope::Branch {
                    company_id,
                    branch_id,
                    ..
                } => (*company_id, Some(*branch_id)),
                _ => (persisted_company_id, persisted_branch_id),
            };
            if persisted_company_id != aggregate_company_id
                || persisted_branch_id != aggregate_branch_id
            {
                return Err(PersistenceError::InvalidData(
                    "SCM list row typed scope columns are inconsistent".into(),
                ));
            }
            let reference: String = row.try_get("reference")?;
            if reference.trim().is_empty()
                || reference.len() > 255
                || reference.chars().any(char::is_control)
            {
                return Err(PersistenceError::InvalidData(
                    "SCM list row reference is invalid".into(),
                ));
            }
            decoded.push(ScmReadListRow {
                row: ScmReadRow {
                    aggregate_type: aggregate_type.clone(),
                    aggregate_id,
                    aggregate_version: source_version,
                    aggregate,
                },
                reference,
                projected_at: row.try_get("projected_at")?,
            });
        }
        let next_cursor = has_more
            .then(|| {
                decoded.last().map(|row| ScmReadCursor {
                    projected_at: row.projected_at,
                    aggregate_type: match row.row.aggregate_type.as_str() {
                        "product" => "product",
                        "warehouse" => "warehouse",
                        "purchase_request" => "purchase_request",
                        "purchase_order" => "purchase_order",
                        "goods_receipt" => "goods_receipt",
                        "stock_movement" => "stock_movement",
                        "stock_transfer" => "stock_transfer",
                        "stock_reservation" => "stock_reservation",
                        _ => unreachable!("validated SCM aggregate type"),
                    },
                    aggregate_id: row.row.aggregate_id,
                })
            })
            .flatten();
        Ok(ScmReadPage {
            rows: decoded,
            next_cursor,
            has_more,
        })
    }

    /// Read a projection only after applying the product's explicit
    /// company/branch containment contract. Shared authorization remains the
    /// authority for roles and permissions; this guard prevents a permitted
    /// actor from crossing a product scope boundary with a read-model ID.
    /// Unauthorized rows fail closed instead of being returned as an empty
    /// result, allowing API adapters to emit a redacted audited denial.
    pub async fn get_for_scope(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
        authorizing_scope: &TenantScope,
    ) -> Result<Option<ScmReadRow>, PersistenceError> {
        validate_authorizing_scope(authorizing_scope)?;
        let Some(row) = self.get(aggregate_type, aggregate_id).await? else {
            return Ok(None);
        };
        if !authorizing_scope.contains(row.aggregate.scope()) {
            return Err(PersistenceError::InvalidData(
                "SCM read scope is not authorized".into(),
            ));
        }
        Ok(Some(row))
    }

    /// Apply one outbox payload to the replaceable read database.  A replay of
    /// an already applied version is a no-op; a gap is rejected so checkpoints
    /// remain contiguous for every aggregate identity.
    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<ScmProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "SCM event ID")?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "SCM aggregate ID")?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "SCM aggregate version must be positive".into(),
            ));
        }
        self.verify_authoritative_source_event(
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let aggregate = validate_source_event_payload(
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )?;
        let (_, _, company_id, branch_id, reference, _, aggregate_payload) = aggregate.fields()?;
        let mut tx = self.begin_read().await?;
        lock_scm_projection_transaction(&mut tx, &self.holding_ref).await?;
        let current = sqlx::query(
            "SELECT source_version, payload FROM erpx_tenant.scm_aggregates
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(
                    row.try_get("source_version")?,
                    "SCM projection source version",
                )?,
                row.try_get::<Value, _>("payload")?,
            ))
        })
        .transpose()?;
        let checkpoint = sqlx::query(
            "SELECT source_version, last_event_id
             FROM erpx_tenant.scm_projection_checkpoints
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(
                    row.try_get("source_version")?,
                    "SCM checkpoint source version",
                )?,
                row.try_get::<Uuid, _>("last_event_id")?,
            ))
        })
        .transpose()?;
        if current.is_some() != checkpoint.is_some() {
            return Err(PersistenceError::InvalidData(
                "SCM projection aggregate and checkpoint are not paired".into(),
            ));
        }
        if let Some((current_version, current_payload)) = current.as_ref() {
            let (checkpoint_version, checkpoint_event_id) =
                checkpoint.as_ref().ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "SCM projection aggregate checkpoint is missing".into(),
                    )
                })?;
            if current_version != checkpoint_version {
                return Err(PersistenceError::InvalidData(
                    "SCM projection aggregate and checkpoint versions differ".into(),
                ));
            }
            if checkpoint_event_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "SCM projection checkpoint event ID is invalid".into(),
                ));
            }
            if *current_version >= aggregate_version {
                if *current_version == aggregate_version && current_payload != &aggregate_payload {
                    return Err(PersistenceError::InvalidData(
                        "SCM projection same-version payload conflict".into(),
                    ));
                }
                // Multiple identical same-version source rows can exist
                // after a legacy/import replay. The payload is the
                // aggregate identity, while event IDs provide ordering;
                // an equivalent duplicate is a no-op and must not make a
                // rebuild unrecoverable.
                tx.commit().await?;
                return Ok(ScmProjectionApplication {
                    event_id,
                    aggregate_type: aggregate_type.into(),
                    aggregate_id,
                    aggregate_version,
                    applied: false,
                });
            }
            if current_version.saturating_add(1) != aggregate_version {
                return Err(PersistenceError::InvalidData(
                    "SCM projection event is out of order".into(),
                ));
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "SCM projection must start at aggregate version one".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_aggregates
                (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
                 reference, source_version, payload, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                 holding_ref=$3, company_id=$4, branch_id=$5, reference=$6,
                 source_version=$7, payload=$8, projected_at=now()",
        )
        .bind(aggregate_id)
        .bind(aggregate_type)
        .bind(&self.holding_ref)
        .bind(company_id)
        .bind(branch_id)
        .bind(reference)
        .bind(as_i64(aggregate_version, "SCM projection source version")?)
        .bind(aggregate_payload)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_projection_checkpoints
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
        .bind(as_i64(aggregate_version, "SCM projection source version")?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ScmProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    async fn project_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        aggregate: &ScmAggregate,
    ) -> Result<ScmProjectionApplication, PersistenceError> {
        let (_, _, company_id, branch_id, reference, _, aggregate_payload) = aggregate.fields()?;
        let current = sqlx::query(
            "SELECT source_version, payload FROM erpx_tenant.scm_aggregates
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(
                    row.try_get("source_version")?,
                    "SCM projection source version",
                )?,
                row.try_get::<Value, _>("payload")?,
            ))
        })
        .transpose()?;
        let checkpoint = sqlx::query(
            "SELECT source_version, last_event_id
             FROM erpx_tenant.scm_projection_checkpoints
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(
                    row.try_get("source_version")?,
                    "SCM checkpoint source version",
                )?,
                row.try_get::<Uuid, _>("last_event_id")?,
            ))
        })
        .transpose()?;
        if current.is_some() != checkpoint.is_some() {
            return Err(PersistenceError::InvalidData(
                "SCM projection aggregate and checkpoint are not paired".into(),
            ));
        }
        if let Some((current_version, current_payload)) = current.as_ref() {
            let (checkpoint_version, checkpoint_event_id) =
                checkpoint.as_ref().ok_or_else(|| {
                    PersistenceError::InvalidData("SCM projection checkpoint is missing".into())
                })?;
            if current_version != checkpoint_version || checkpoint_event_id.is_nil() {
                return Err(PersistenceError::InvalidData(
                    "SCM projection aggregate and checkpoint are inconsistent".into(),
                ));
            }
            if *current_version >= aggregate_version {
                if *current_version == aggregate_version && current_payload != &aggregate_payload {
                    return Err(PersistenceError::InvalidData(
                        "SCM projection same-version payload conflict".into(),
                    ));
                }
                return Ok(ScmProjectionApplication {
                    event_id,
                    aggregate_type: aggregate_type.into(),
                    aggregate_id,
                    aggregate_version,
                    applied: false,
                });
            }
            if current_version.saturating_add(1) != aggregate_version {
                return Err(PersistenceError::InvalidData(
                    "SCM projection event is out of order".into(),
                ));
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "SCM projection must start at aggregate version one".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_aggregates
                (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
                 reference, source_version, payload, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now())
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                 holding_ref=$3, company_id=$4, branch_id=$5, reference=$6,
                 source_version=$7, payload=$8, projected_at=now()",
        )
        .bind(aggregate_id)
        .bind(aggregate_type)
        .bind(&self.holding_ref)
        .bind(company_id)
        .bind(branch_id)
        .bind(reference)
        .bind(as_i64(aggregate_version, "SCM projection source version")?)
        .bind(aggregate_payload)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.scm_projection_checkpoints
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
        .bind(as_i64(aggregate_version, "SCM projection source version")?)
        .execute(&mut **tx)
        .await?;
        Ok(ScmProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    /// Named replay entry point for workers rebuilding a replacement read
    /// database.  It deliberately shares the same contiguous-version guard
    /// as live projection delivery.
    pub async fn replay_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<ScmProjectionApplication, PersistenceError> {
        self.project_event(
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await
    }

    /// Atomically replace the disposable SCM projection from a fenced source
    /// outbox snapshot. The previous read model remains committed until every
    /// source event has validated and every replacement row/checkpoint is
    /// ready; any error rolls the replacement transaction back.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_event_count: Option<u64>,
        page_size: u32,
    ) -> Result<(ScmProjectionRebuildPreparation, u64, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_SCM_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "SCM source event page size must be 1..={MAX_SCM_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_count =
            scm_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if expected_source_event_count.is_some_and(|expected| expected != source_count) {
            return Err(PersistenceError::InvalidData(
                "SCM source fence changed before replacement rebuild".into(),
            ));
        }
        if source_count > SCM_MAX_REBUILD_EVENTS {
            return Err(PersistenceError::InvalidData(
                "SCM projection source event bound exceeded".into(),
            ));
        }

        let mut read_tx = self.begin_read().await?;
        lock_scm_projection_transaction(&mut read_tx, &self.holding_ref).await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.scm_aggregates WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *read_tx)
                .await?
                .rows_affected();
        let checkpoints_cleared =
            sqlx::query("DELETE FROM erpx_tenant.scm_projection_checkpoints WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *read_tx)
                .await?
                .rows_affected();
        let preparation = ScmProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        };

        let mut cursor = None;
        let mut events_replayed = 0_u64;
        let mut pages_read = 0_u64;
        loop {
            pages_read = pages_read.saturating_add(1);
            let page = read_scm_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                let aggregate = validate_source_event_payload(
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )?;
                validate_scm_source_snapshot(&mut source_tx, &self.holding_ref, event, &aggregate)
                    .await?;
                if self
                    .project_event_in_transaction(
                        &mut read_tx,
                        event.event_id,
                        &event.aggregate_type,
                        event.aggregate_id,
                        event.aggregate_version,
                        &aggregate,
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
                    "SCM source event cursor is missing".into(),
                ));
            }
        }
        let final_source_count =
            scm_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_count != source_count {
            return Err(PersistenceError::InvalidData(
                "SCM source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed, pages_read))
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<ScmProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.scm_aggregates WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        let checkpoints_cleared =
            sqlx::query("DELETE FROM erpx_tenant.scm_projection_checkpoints WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        tx.commit().await?;
        Ok(ScmProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        })
    }

    /// A read projector may only consume the exact event committed by the
    /// holding's authoritative write database. Local JSON validation alone is
    /// insufficient because a caller could otherwise manufacture a valid
    /// aggregate payload and write it to the disposable read model.
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
            "SELECT e.event_type, e.aggregate_type, e.aggregate_id,
                    e.aggregate_version, e.payload,
                    a.version AS source_version, a.payload AS source_payload
             FROM erpx_tenant.outbox_events AS e
             JOIN erpx_tenant.scm_aggregates AS a
               ON a.aggregate_type=e.aggregate_type
              AND a.aggregate_id=e.aggregate_id
              AND a.holding_ref=e.holding_ref
             WHERE e.event_id=$1 AND e.holding_ref=$2",
        )
        .bind(event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let Some(row) = row else {
            return Err(PersistenceError::InvalidData(
                "SCM projection event is not authoritative".into(),
            ));
        };
        let source_event_type: String = row.try_get("event_type")?;
        let source_aggregate_type: String = row.try_get("aggregate_type")?;
        let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let source_version = as_u64(
            row.try_get("aggregate_version")?,
            "SCM source event version",
        )?;
        let source_payload: Value = row.try_get("payload")?;
        let aggregate_snapshot_version = as_u64(
            row.try_get("source_version")?,
            "SCM source aggregate version",
        )?;
        let aggregate_snapshot: Value = row.try_get("source_payload")?;
        if source_event_type != format!("scm.{aggregate_type}.v1")
            || source_aggregate_type != aggregate_type
            || source_aggregate_id != aggregate_id
            || source_version != aggregate_version
            || source_payload != *payload
            || aggregate_snapshot_version < aggregate_version
        {
            return Err(PersistenceError::InvalidData(
                "SCM projection event does not match the authoritative outbox".into(),
            ));
        }
        if aggregate_snapshot_version == aggregate_version {
            let event: ScmEventPayload = serde_json::from_value(source_payload)?;
            if aggregate_snapshot != event.aggregate {
                return Err(PersistenceError::InvalidData(
                    "SCM source snapshot does not match its authoritative outbox event".into(),
                ));
            }
        }
        Ok(())
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref).await
    }
}

const SCM_MAX_REBUILD_EVENTS: u64 = 1_000_000;

async fn scm_source_event_count_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1
           AND aggregate_type IN
               ('product', 'warehouse', 'purchase_request', 'purchase_order',
                'goods_receipt', 'stock_movement', 'stock_transfer',
                'stock_reservation')",
    )
    .bind(holding_ref)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "SCM source event count")
}

async fn validate_scm_source_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event: &ScmSourceEvent,
    aggregate: &ScmAggregate,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT version, payload
         FROM erpx_tenant.scm_aggregates
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
         FOR SHARE",
    )
    .bind(holding_ref)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| {
        PersistenceError::InvalidData("SCM source event has no aggregate snapshot".into())
    })?;
    let source_version = as_u64(row.try_get("version")?, "SCM source snapshot version")?;
    let source_payload: Value = row.try_get("payload")?;
    let (_, _, _, _, _, _, event_aggregate_payload) = aggregate.fields()?;
    if source_version < event.aggregate_version
        || (source_version == event.aggregate_version && source_payload != event_aggregate_payload)
    {
        return Err(PersistenceError::InvalidData(
            "SCM source snapshot does not match its authoritative outbox event".into(),
        ));
    }
    Ok(())
}

async fn lock_scm_projection_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("scm:projection:{holding_ref}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn read_scm_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&ScmSourceEventCursor>,
    page_size: u32,
) -> Result<ScmSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_SCM_SOURCE_EVENT_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "SCM source event page size must be 1..={MAX_SCM_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    validate_holding_ref(holding_ref)?;
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_holding_ref(&cursor.holding_ref)?;
        if cursor.holding_ref != holding_ref {
            return Err(PersistenceError::InvalidData(
                "SCM source cursor holding does not match the routed holding".into(),
            ));
        }
        validate_aggregate_type(&cursor.aggregate_type)?;
        validate_uuid(cursor.aggregate_id, "SCM source cursor aggregate ID")?;
        if cursor.aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "SCM source cursor version must be positive".into(),
            ));
        }
        validate_uuid(cursor.event_id, "SCM source cursor event ID")?;
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN
                   ('product', 'warehouse', 'purchase_request', 'purchase_order',
                    'goods_receipt', 'stock_movement', 'stock_transfer',
                    'stock_reservation')
               AND (aggregate_type > $2
                    OR (aggregate_type = $2 AND aggregate_id > $3)
                    OR (aggregate_type = $2 AND aggregate_id = $3
                        AND aggregate_version > $4)
                    OR (aggregate_type = $2 AND aggregate_id = $3
                        AND aggregate_version = $4 AND event_id > $5))
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $6",
        )
        .bind(holding_ref)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "SCM source cursor version",
        )?)
        .bind(cursor.event_id)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN
                   ('product', 'warehouse', 'purchase_request', 'purchase_order',
                    'goods_receipt', 'stock_movement', 'stock_transfer',
                    'stock_reservation')
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $2",
        )
        .bind(holding_ref)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let requested = usize::try_from(page_size)
        .map_err(|_| PersistenceError::InvalidData("SCM source page size overflow".into()))?;
    let has_more = rows.len() > requested;
    let events = rows
        .into_iter()
        .take(requested)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "SCM source event version",
            )?;
            let payload: Value = row.try_get("payload")?;
            validate_uuid(event_id, "SCM source event ID")?;
            validate_uuid(aggregate_id, "SCM source aggregate ID")?;
            validate_aggregate_type(&aggregate_type)?;
            validate_source_event_type(&aggregate_type, &event_type)?;
            validate_source_event_payload(
                &aggregate_type,
                aggregate_id,
                aggregate_version,
                &payload,
            )?;
            Ok(ScmSourceEvent {
                event_id,
                event_type,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                payload,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = has_more
        .then(|| {
            events.last().map(|event| ScmSourceEventCursor {
                holding_ref: holding_ref.to_owned(),
                aggregate_type: event.aggregate_type.clone(),
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
                event_id: event.event_id,
            })
        })
        .flatten();
    Ok(ScmSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn persist_aggregate_update_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    aggregate: &ScmAggregate,
    current_version: u64,
    holding_ref: &str,
) -> Result<(u64, Uuid), PersistenceError> {
    let next_version = current_version
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("SCM aggregate version overflow".into()))?;
    aggregate.validate()?;
    let (aggregate_type, aggregate_id, company_id, branch_id, reference, idempotency_key, payload) =
        aggregate.fields()?;
    let branch_id = branch_id.or_else(|| branch_from_scope(aggregate.scope()));
    sqlx::query(
        "UPDATE erpx_tenant.scm_aggregates
         SET company_id=$3, branch_id=$4, reference=$5, version=$6,
             idempotency_key=$7, payload=$8, updated_at=now()
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$9",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(company_id)
    .bind(branch_id)
    .bind(reference)
    .bind(as_i64(next_version, "SCM aggregate version")?)
    .bind(idempotency_key)
    .bind(&payload)
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.aggregate_snapshots
            (aggregate_type, aggregate_id, holding_ref, version, state)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (aggregate_type, aggregate_id)
         DO UPDATE SET holding_ref=$3, version=$4, state=$5, updated_at=now()",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(next_version, "SCM aggregate version")?)
    .bind(&payload)
    .execute(&mut **tx)
    .await?;
    let event_id = Uuid::new_v4();
    let event_payload = serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
        "version": next_version,
    });
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(event_id)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(next_version, "SCM aggregate version")?)
    .bind(format!("scm.{aggregate_type}.v1"))
    .bind(event_payload)
    .execute(&mut **tx)
    .await?;
    Ok((next_version, event_id))
}

#[derive(Debug, Clone, Deserialize)]
struct ScmEventPayload {
    aggregate_type: String,
    aggregate: Value,
    version: u64,
}

fn decode_aggregate(
    aggregate_type: &str,
    payload: Value,
) -> Result<ScmAggregate, PersistenceError> {
    let tagged = serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
    });
    let aggregate: ScmAggregate = serde_json::from_value(tagged)?;
    if aggregate.aggregate_type() != aggregate_type {
        return Err(PersistenceError::InvalidData(
            "SCM aggregate type does not match its storage key".into(),
        ));
    }
    Ok(aggregate)
}

fn validate_source_event_payload(
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<ScmAggregate, PersistenceError> {
    validate_aggregate_type(aggregate_type)?;
    validate_uuid(aggregate_id, "SCM source aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "SCM source event version must be positive".into(),
        ));
    }
    let event: ScmEventPayload = serde_json::from_value(payload.clone())?;
    if event.aggregate_type != aggregate_type || event.version != aggregate_version {
        return Err(PersistenceError::InvalidData(
            "SCM source event identity or version does not match its key".into(),
        ));
    }
    let aggregate = decode_aggregate(aggregate_type, event.aggregate)?;
    aggregate.validate()?;
    if aggregate.aggregate_id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "SCM source event aggregate ID does not match its key".into(),
        ));
    }
    Ok(aggregate)
}

fn validate_source_aggregate_fact(
    fact: &ScmSourceAggregateFact,
) -> Result<ScmAggregate, PersistenceError> {
    validate_aggregate_type(&fact.aggregate_type)?;
    validate_uuid(fact.event_id, "SCM source event ID")?;
    validate_uuid(fact.aggregate_id, "SCM source aggregate ID")?;
    validate_source_event_type(&fact.aggregate_type, &fact.event_type)?;
    if fact.conflicting_version_count != 0 {
        return Err(PersistenceError::InvalidData(
            "SCM source aggregate has conflicting duplicate-version events".into(),
        ));
    }
    validate_source_event_payload(
        &fact.aggregate_type,
        fact.aggregate_id,
        fact.aggregate_version,
        &fact.payload,
    )
}

fn next_version(current: Option<u64>) -> Result<u64, PersistenceError> {
    current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("SCM aggregate version overflow".into()))
}

fn validate_idempotency_replay_receipt(
    stored_aggregate_type: &str,
    stored_aggregate_id: Uuid,
    receipt: ScmWriteReceipt,
    expected_aggregate_type: &str,
    expected_aggregate_id: Uuid,
    expected_version: u64,
) -> Result<ScmWriteReceipt, PersistenceError> {
    validate_aggregate_type(stored_aggregate_type)?;
    validate_aggregate_type(expected_aggregate_type)?;
    validate_uuid(stored_aggregate_id, "SCM stored idempotency aggregate ID")?;
    validate_uuid(expected_aggregate_id, "SCM replay aggregate ID")?;
    if stored_aggregate_type != expected_aggregate_type
        || stored_aggregate_id != expected_aggregate_id
        || receipt.aggregate_type != expected_aggregate_type
        || receipt.aggregate_id != expected_aggregate_id
        || receipt.version != expected_version
        || receipt.replayed
    {
        return Err(PersistenceError::InvalidData(
            "SCM idempotency receipt is not bound to the requested source aggregate".into(),
        ));
    }
    Ok(receipt)
}

fn validate_replay_outbox_evidence(
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    expected_payload: &Value,
    rows: &[ScmReplayOutboxFact],
) -> Result<(), PersistenceError> {
    if rows.len() != 1 {
        return Err(PersistenceError::InvalidData(
            "SCM idempotency replay requires exactly one authoritative outbox event".into(),
        ));
    }
    let row = &rows[0];
    validate_uuid(row.event_id, "SCM replay source event ID")?;
    validate_aggregate_type(&row.aggregate_type)?;
    validate_source_event_type(&row.aggregate_type, &row.event_type)?;
    if row.aggregate_type != aggregate_type
        || row.aggregate_id != aggregate_id
        || row.aggregate_version != aggregate_version
        || row.payload != *expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "SCM idempotency replay does not match the authoritative outbox".into(),
        ));
    }
    validate_source_event_payload(
        aggregate_type,
        aggregate_id,
        aggregate_version,
        &row.payload,
    )?;
    Ok(())
}

fn validate_replay_source_event_rows(
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    rows: &[ScmReplayOutboxFact],
) -> Result<ScmAggregate, PersistenceError> {
    if rows.len() != 1 {
        return Err(PersistenceError::InvalidData(
            "SCM replay requires exactly one authoritative outbox event".into(),
        ));
    }
    let row = &rows[0];
    validate_uuid(row.event_id, "SCM replay source event ID")?;
    validate_aggregate_type(&row.aggregate_type)?;
    validate_source_event_type(&row.aggregate_type, &row.event_type)?;
    if row.aggregate_type != aggregate_type
        || row.aggregate_id != aggregate_id
        || row.aggregate_version != aggregate_version
    {
        return Err(PersistenceError::InvalidData(
            "SCM replay source event does not match its requested identity".into(),
        ));
    }
    validate_source_event_payload(
        aggregate_type,
        aggregate_id,
        aggregate_version,
        &row.payload,
    )
}

fn parse_goods_receipt_pair_metadata(
    response: &Value,
) -> Result<Option<(Uuid, u64)>, PersistenceError> {
    let Some(order_type) = response.get("paired_aggregate_type") else {
        return Ok(None);
    };
    let order_type = order_type.as_str().ok_or_else(|| {
        PersistenceError::InvalidData("SCM goods receipt paired aggregate type is invalid".into())
    })?;
    let order_id = response
        .get("paired_aggregate_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            PersistenceError::InvalidData("SCM goods receipt paired aggregate ID is missing".into())
        })?
        .parse::<Uuid>()
        .map_err(|_| {
            PersistenceError::InvalidData("SCM goods receipt paired aggregate ID is invalid".into())
        })?;
    let order_version = response
        .get("paired_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            PersistenceError::InvalidData("SCM goods receipt paired version is missing".into())
        })?;
    if order_type != "purchase_order" || order_id.is_nil() || order_version == 0 {
        return Err(PersistenceError::InvalidData(
            "SCM goods receipt paired aggregate metadata is invalid".into(),
        ));
    }
    Ok(Some((order_id, order_version)))
}

fn validate_transition_replay_outbox_evidence(
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected_version: u64,
    transition: &str,
    stored_request_hash: &str,
    rows: &[ScmReplayOutboxFact],
) -> Result<(), PersistenceError> {
    let replay_version = expected_version.checked_add(1).ok_or_else(|| {
        PersistenceError::InvalidData("SCM transition replay version overflow".into())
    })?;
    if rows.len() != 1 {
        return Err(PersistenceError::InvalidData(
            "SCM transition replay requires exactly one authoritative outbox event".into(),
        ));
    }
    let row = &rows[0];
    validate_uuid(row.event_id, "SCM transition replay event ID")?;
    validate_aggregate_type(&row.aggregate_type)?;
    validate_source_event_type(&row.aggregate_type, &row.event_type)?;
    if row.aggregate_type != aggregate_type
        || row.aggregate_id != aggregate_id
        || row.aggregate_version != replay_version
    {
        return Err(PersistenceError::InvalidData(
            "SCM transition replay does not match the authoritative outbox".into(),
        ));
    }
    let event: ScmEventPayload = serde_json::from_value(row.payload.clone())?;
    if event.aggregate_type != aggregate_type || event.version != replay_version {
        return Err(PersistenceError::InvalidData(
            "SCM transition replay envelope is inconsistent".into(),
        ));
    }
    let aggregate =
        validate_source_event_payload(aggregate_type, aggregate_id, replay_version, &row.payload)?;
    validate_transition_replay_intent(aggregate_type, transition, &aggregate)?;
    let expected_hash = hash_request(aggregate_type, &event.aggregate, Some(expected_version))?;
    if stored_request_hash != expected_hash {
        return Err(PersistenceError::InvalidData(
            "SCM transition replay request hash does not match the authoritative outbox".into(),
        ));
    }
    Ok(())
}

fn validate_transition_replay_intent(
    aggregate_type: &str,
    transition: &str,
    aggregate: &ScmAggregate,
) -> Result<(), PersistenceError> {
    let matches = match aggregate {
        ScmAggregate::PurchaseRequest(value) => matches!(
            (transition, value.status),
            ("submitted", PurchaseRequestStatus::Submitted)
                | ("approved", PurchaseRequestStatus::Approved)
                | ("rejected", PurchaseRequestStatus::Rejected)
                | ("cancelled", PurchaseRequestStatus::Cancelled)
                | ("closed", PurchaseRequestStatus::Closed)
        ),
        ScmAggregate::PurchaseOrder(value) => matches!(
            (transition, value.status),
            ("submitted", PurchaseOrderStatus::Submitted)
                | ("approved", PurchaseOrderStatus::Approved)
                | ("rejected", PurchaseOrderStatus::Rejected)
                | ("issued", PurchaseOrderStatus::Issued)
                | ("cancelled", PurchaseOrderStatus::Cancelled)
        ),
        ScmAggregate::GoodsReceipt(value) => matches!(
            (transition, value.status),
            ("cancelled", GoodsReceiptStatus::Cancelled)
        ),
        ScmAggregate::StockMovement(value) => matches!(
            (transition, value.status),
            ("cancelled", StockMovementStatus::Cancelled)
        ),
        ScmAggregate::StockTransfer(value) => matches!(
            (transition, value.status),
            (
                "cancelled",
                erpx_domain::procurement_inventory::StockTransferStatus::Cancelled
            )
        ),
        ScmAggregate::StockReservation(value) => matches!(
            (transition, value.status),
            (
                "cancelled",
                erpx_domain::procurement_inventory::StockReservationStatus::Cancelled
            )
        ),
        ScmAggregate::Product(_) | ScmAggregate::Warehouse(_) => false,
    };
    if matches {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(format!(
            "SCM transition replay intent {transition:?} does not match {aggregate_type} state"
        )))
    }
}

fn apply_transition(
    aggregate: &mut ScmAggregate,
    transition: &str,
    context: &erpx_domain::contracts::CommandContext,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    match aggregate {
        ScmAggregate::PurchaseRequest(value) => {
            let next = match transition {
                "submitted" => PurchaseRequestStatus::Submitted,
                "approved" => PurchaseRequestStatus::Approved,
                "rejected" => PurchaseRequestStatus::Rejected,
                "cancelled" => PurchaseRequestStatus::Cancelled,
                "closed" => PurchaseRequestStatus::Closed,
                _ => return invalid_transition("purchase_request", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        ScmAggregate::PurchaseOrder(value) => {
            let next = match transition {
                "submitted" => PurchaseOrderStatus::Submitted,
                "approved" => PurchaseOrderStatus::Approved,
                "rejected" => PurchaseOrderStatus::Rejected,
                "issued" => PurchaseOrderStatus::Issued,
                "cancelled" => PurchaseOrderStatus::Cancelled,
                _ => return invalid_transition("purchase_order", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        ScmAggregate::GoodsReceipt(value) => {
            let next = match transition {
                "cancelled" => GoodsReceiptStatus::Cancelled,
                _ => return invalid_transition("goods_receipt", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        ScmAggregate::StockMovement(value) => match transition {
            "cancelled" => value.cancel(context, at).map_err(domain_error),
            _ => invalid_transition("stock_movement", transition),
        },
        ScmAggregate::StockTransfer(value) => match transition {
            "cancelled" => value.cancel(context, at).map_err(domain_error),
            _ => invalid_transition("stock_transfer", transition),
        },
        ScmAggregate::StockReservation(value) => match transition {
            "cancelled" => value.cancel(context, at).map_err(domain_error),
            _ => invalid_transition("stock_reservation", transition),
        },
        _ => invalid_transition(aggregate.aggregate_type(), transition),
    }
}

fn invalid_transition(aggregate_type: &str, transition: &str) -> Result<(), PersistenceError> {
    Err(PersistenceError::InvalidData(format!(
        "transition {transition:?} is not supported for SCM aggregate {aggregate_type}"
    )))
}

fn domain_error(error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn apply_posted_stock_movement(
    balance: &mut StockBalance,
    movement: &StockMovement,
) -> Result<(), PersistenceError> {
    movement.validate().map_err(domain_error)?;
    if movement.status != StockMovementStatus::Posted {
        return Err(PersistenceError::InvalidData(
            "SCM balance replay contains a non-posted movement".into(),
        ));
    }
    match movement.kind {
        StockMovementKind::Receipt | StockMovementKind::AdjustmentIncrease => balance
            .receive(movement.quantity, movement.unit_cost_minor)
            .map_err(domain_error),
        StockMovementKind::Issue | StockMovementKind::AdjustmentDecrease => {
            let average_cost = balance
                .average_unit_cost_minor
                .ok_or_else(|| domain_error("SCM issue movement has no weighted-average cost"))?;
            if movement.unit_cost_minor != average_cost {
                return Err(domain_error(
                    "SCM issue movement cost does not match weighted-average cost",
                ));
            }
            balance.issue(movement.quantity).map_err(domain_error)
        }
    }
}

fn validate_projected_aggregate(
    aggregate_type: &str,
    aggregate_id: Uuid,
    source_version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    if source_version == 0 {
        return Err(PersistenceError::InvalidData(
            "SCM projected version must be positive".into(),
        ));
    }
    let aggregate = decode_aggregate(aggregate_type, payload.clone())?;
    aggregate.validate()?;
    if aggregate.aggregate_type() != aggregate_type || aggregate.aggregate_id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "SCM projected aggregate identity does not match its key".into(),
        ));
    }
    Ok(())
}

fn branch_from_scope(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        _ => None,
    }
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if matches!(
        value,
        "product"
            | "warehouse"
            | "purchase_request"
            | "purchase_order"
            | "goods_receipt"
            | "stock_movement"
            | "stock_transfer"
            | "stock_reservation"
    ) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unknown SCM aggregate type".into(),
        ))
    }
}

fn validate_read_scope(scope: ScmReadScope) -> Result<(), PersistenceError> {
    if scope.holding_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "SCM read holding ID is invalid".into(),
        ));
    }
    if scope.company_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "SCM read company ID is invalid".into(),
        ));
    }
    if scope.branch_id.is_some_and(|branch_id| branch_id.is_nil()) {
        return Err(PersistenceError::InvalidData(
            "SCM read branch ID is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_authorizing_scope(scope: &TenantScope) -> Result<(), PersistenceError> {
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
            "SCM read authorization requires an explicit company or branch scope".into(),
        )),
    }
}

fn validate_source_event_type(
    aggregate_type: &str,
    event_type: &str,
) -> Result<(), PersistenceError> {
    if event_type == format!("scm.{aggregate_type}.v1") {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "SCM source event type does not match its aggregate type".into(),
        ))
    }
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
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn validate_reconciliation_cursor(
    holding_ref: &str,
    cursor: Option<&ScmSourceEventCursor>,
) -> Result<(), PersistenceError> {
    let Some(cursor) = cursor else {
        return Ok(());
    };
    if cursor.holding_ref != holding_ref {
        return Err(PersistenceError::InvalidData(
            "SCM reconciliation cursor holding does not match the routed store".into(),
        ));
    }
    validate_aggregate_type(&cursor.aggregate_type)?;
    validate_uuid(
        cursor.aggregate_id,
        "SCM reconciliation cursor aggregate ID",
    )?;
    validate_uuid(cursor.event_id, "SCM reconciliation cursor event ID")?;
    if cursor.aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "SCM reconciliation cursor version must be positive".into(),
        ));
    }
    Ok(())
}

fn digest_source_aggregate_page(
    facts: &[ScmSourceAggregateFact],
) -> Result<String, PersistenceError> {
    let mut digest = Sha256::new();
    for fact in facts {
        let bytes = serde_json::to_vec(&(
            fact.event_id,
            &fact.event_type,
            &fact.aggregate_type,
            fact.aggregate_id,
            fact.aggregate_version,
            &fact.payload,
            fact.event_count,
            fact.first_version,
            fact.distinct_version_count,
            fact.conflicting_version_count,
        ))?;
        digest.update((bytes.len() as u64).to_be_bytes());
        digest.update(bytes);
    }
    Ok(hex::encode(digest.finalize()))
}

fn source_version_gap_count(
    first_version: u64,
    latest_version: u64,
    event_count: u64,
    distinct_version_count: u64,
) -> Result<u64, PersistenceError> {
    if first_version == 0
        || latest_version == 0
        || distinct_version_count == 0
        || first_version > latest_version
        || distinct_version_count > event_count
        || distinct_version_count > latest_version
    {
        return Err(PersistenceError::InvalidData(
            "SCM source aggregate version facts are invalid".into(),
        ));
    }
    // Positive integer versions are expected to start at one. Counting
    // distinct versions tolerates duplicate same-version legacy/import rows,
    // while every absent version up to the latest one becomes a source gap.
    latest_version
        .checked_sub(distinct_version_count)
        .ok_or_else(|| PersistenceError::InvalidData("SCM source gap count underflow".into()))
}

fn optional_event_id(value: Option<Uuid>) -> String {
    value.map_or_else(|| "root".into(), |id| id.to_string())
}

fn reconciliation_evidence_key(evidence: &ScmReconciliationEvidence) -> String {
    format!("scm/reconciliation/evidence/{}", evidence.evidence_id)
}

fn reconciliation_evidence_id(
    holding_ref: &str,
    after_event_id: Option<Uuid>,
    last_event_id: Option<Uuid>,
    source_digest: &str,
    reconciliation: ScmProjectionReconciliation,
) -> Uuid {
    let key = serde_json::to_vec(&(
        "scm-reconciliation-v1",
        holding_ref,
        optional_event_id(after_event_id),
        optional_event_id(last_event_id),
        source_digest,
        reconciliation,
    ))
    .expect("SCM reconciliation facts are serializable");
    let digest = Sha256::digest(key);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn hash_reconciliation_evidence(
    evidence: &ScmReconciliationEvidence,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(evidence)?)))
}

fn add_reconciliation_counts(
    total: &mut ScmProjectionReconciliation,
    page: ScmProjectionReconciliation,
) -> Result<(), PersistenceError> {
    let add = |target: &mut u64, value: u64| {
        *target = target.checked_add(value).ok_or_else(|| {
            PersistenceError::InvalidData("SCM reconciliation count overflow".into())
        })?;
        Ok::<_, PersistenceError>(())
    };
    add(&mut total.source_event_count, page.source_event_count)?;
    add(
        &mut total.source_aggregate_count,
        page.source_aggregate_count,
    )?;
    add(&mut total.source_gap_count, page.source_gap_count)?;
    add(
        &mut total.projected_aggregate_count,
        page.projected_aggregate_count,
    )?;
    add(
        &mut total.missing_aggregate_count,
        page.missing_aggregate_count,
    )?;
    add(&mut total.stale_aggregate_count, page.stale_aggregate_count)?;
    add(&mut total.ahead_aggregate_count, page.ahead_aggregate_count)?;
    add(
        &mut total.orphan_aggregate_count,
        page.orphan_aggregate_count,
    )?;
    add(
        &mut total.missing_checkpoint_count,
        page.missing_checkpoint_count,
    )?;
    add(
        &mut total.stale_checkpoint_count,
        page.stale_checkpoint_count,
    )?;
    add(
        &mut total.orphan_checkpoint_count,
        page.orphan_checkpoint_count,
    )?;
    add(
        &mut total.invalid_projection_count,
        page.invalid_projection_count,
    )?;
    Ok(())
}

fn hash_request(
    aggregate_type: &str,
    payload: &Value,
    expected_version: Option<u64>,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        aggregate_type,
        payload,
        expected_version,
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
    use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext};
    use erpx_domain::procurement_inventory::Product;

    #[tokio::test]
    async fn store_rejects_unsafe_holding_references_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxScmStore::new(pool.clone(), pool, " ").is_err());
    }

    #[test]
    fn aggregate_type_allow_list_is_closed() {
        assert!(validate_aggregate_type("purchase_order").is_ok());
        assert!(validate_aggregate_type("payment_instrument").is_err());
    }

    #[test]
    fn aggregate_payload_round_trip_preserves_identity() {
        let payload = serde_json::json!({
            "id": Uuid::new_v4(),
            "reference": "not-used"
        });
        let tagged = serde_json::json!({
            "aggregate_type": "product",
            "aggregate": payload
        });
        assert!(serde_json::from_value::<ScmAggregate>(tagged).is_err());
    }

    #[test]
    fn projected_payload_validation_is_fail_closed() {
        let aggregate_id = Uuid::new_v4();
        let malformed = serde_json::json!({
            "id": aggregate_id,
            "reference": "not-used"
        });
        assert!(validate_projected_aggregate("product", aggregate_id, 1, &malformed).is_err());
        assert!(validate_projected_aggregate("product", aggregate_id, 0, &malformed).is_err());
        assert!(validate_projected_aggregate("unknown", aggregate_id, 1, &malformed).is_err());
    }

    #[test]
    fn source_version_gap_detection_tolerates_duplicates_but_flags_missing_versions() {
        assert_eq!(
            source_version_gap_count(1, 3, 3, 3).expect("contiguous source"),
            0
        );
        assert_eq!(
            source_version_gap_count(1, 3, 4, 3).expect("duplicate version is allowed"),
            0
        );
        assert_eq!(source_version_gap_count(1, 3, 2, 2).expect("source gap"), 1);
        assert_eq!(
            source_version_gap_count(2, 3, 2, 2).expect("initial gap"),
            1
        );
        assert!(source_version_gap_count(1, 3, 2, 4).is_err());
    }

    #[test]
    fn source_cursor_fence_binds_holding_and_positive_source_identity() {
        let mut cursor = ScmSourceEventCursor {
            holding_ref: "holding-scm-a".into(),
            aggregate_type: "product".into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            event_id: Uuid::new_v4(),
        };
        assert!(validate_reconciliation_cursor("holding-scm-a", Some(&cursor)).is_ok());

        cursor.holding_ref = "holding-scm-b".into();
        assert!(validate_reconciliation_cursor("holding-scm-a", Some(&cursor)).is_err());

        cursor.holding_ref = "holding-scm-a".into();
        cursor.aggregate_version = 0;
        assert!(validate_reconciliation_cursor("holding-scm-a", Some(&cursor)).is_err());
    }

    #[test]
    fn aggregate_scope_is_the_read_authorization_boundary() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let sibling_company_id = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let context = CommandContext::try_new(
            "scm/product-read-boundary",
            ActorAttribution {
                principal_ref: "scm-test-actor".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            "scm.read.boundary",
        )
        .expect("valid command context");
        let aggregate = ScmAggregate::Product(
            Product::new(
                Uuid::new_v4(),
                scope.clone(),
                holding_id,
                company_id,
                None,
                "SKU-READ-BOUNDARY",
                "Read boundary product",
                "each",
                Utc::now(),
                &context,
            )
            .expect("valid product"),
        );

        assert!(scope.contains(aggregate.scope()));
        assert!(
            !TenantScope::Company {
                holding_id,
                company_id: sibling_company_id,
            }
            .contains(aggregate.scope())
        );
    }

    #[test]
    fn read_authorization_requires_explicit_company_or_branch_scope() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        assert!(
            validate_authorizing_scope(&TenantScope::Company {
                holding_id,
                company_id,
            })
            .is_ok()
        );
        assert!(
            validate_authorizing_scope(&TenantScope::Branch {
                holding_id,
                company_id,
                branch_id,
            })
            .is_ok()
        );
        assert!(validate_authorizing_scope(&TenantScope::Platform).is_err());
        assert!(validate_authorizing_scope(&TenantScope::Holding { holding_id }).is_err());
        assert!(
            validate_authorizing_scope(&TenantScope::Subtree {
                holding_id,
                company_ids: vec![company_id],
                branch_ids: vec![branch_id],
            })
            .is_err()
        );
    }
}
