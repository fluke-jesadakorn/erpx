//! Tenant-routed SQLx persistence for Sales source aggregates.
//!
//! The repository intentionally stores one validated aggregate payload while
//! retaining typed aggregate identity and scope columns for indexing/RLS.
//! Posted state, its aggregate snapshot, outbox event, and idempotency receipt
//! commit in one write transaction. The read database is a replaceable
//! projection and is never consulted by commands.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

use erpx_domain::contracts::TenantScope;
use erpx_domain::sales::{
    CollectionState, CollectionStatus, DeliveryHandoff, HandoffStatus, InvoiceHandoff, Lead,
    LeadStatus, Opportunity, OpportunityStatus, Quote, QuoteStatus, SalesAggregateKind, SalesOrder,
    SalesOrderStatus, SalesProjectionReconciliation, deterministic_idempotency_key,
};

use crate::PersistenceError;

pub const MAX_SALES_READ_PAGE_SIZE: u32 = 500;
pub const MAX_SALES_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
/// Maximum source-event history one Sales projection invocation may drain.
pub const MAX_SALES_PROJECTION_EVENTS: u64 = 1_000_000;
const SALES_SOURCE_EVENT_TYPE_FILTER: &str = "aggregate_type IN ('lead', 'opportunity', 'quote', 'sales_order', 'delivery_handoff', 'invoice_handoff', 'collection_state')";

type SalesFields<'a> = (
    &'static str,
    Uuid,
    Uuid,
    Option<Uuid>,
    &'a str,
    &'a str,
    Value,
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SalesWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SalesSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SalesSourceEventPage {
    pub events: Vec<SalesSourceEvent>,
    pub next_cursor: Option<SalesSourceEventCursor>,
    pub has_more: bool,
}

/// Explicit company/branch scope accepted by the Sales read gateway.
/// Company scope includes its branches; branch scope is exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SalesReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReadCursor {
    pub projected_at: DateTime<Utc>,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReadListRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub reference: String,
    pub source_version: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesReadPage {
    pub rows: Vec<SalesReadListRow>,
    pub next_cursor: Option<SalesReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct SalesSourceProjectionFact {
    latest_version: u64,
    latest_event_id: Uuid,
    aggregate_payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
struct SalesProjectedFact {
    source_version: u64,
    payload: Value,
    checkpoint_source_version: Option<u64>,
    checkpoint_event_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
struct SalesReplayOutboxFact {
    event_id: Uuid,
    event_type: String,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum SalesAggregate {
    Lead(Lead),
    Opportunity(Opportunity),
    Quote(Quote),
    SalesOrder(SalesOrder),
    DeliveryHandoff(DeliveryHandoff),
    InvoiceHandoff(InvoiceHandoff),
    CollectionState(CollectionState),
}

impl SalesAggregate {
    /// Validate a generic transport save against the authenticated command.
    /// The source aggregate must carry the same immutable command identity,
    /// scope, and deterministic idempotency namespace that the server parsed;
    /// callers cannot smuggle a valid aggregate from another scope or replay
    /// it under an unrelated command.
    pub fn validate_for_command(
        &self,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<(), PersistenceError> {
        match self {
            Self::Lead(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::Opportunity(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::Quote(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::SalesOrder(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::DeliveryHandoff(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::InvoiceHandoff(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
            Self::CollectionState(value) => value
                .validate_for_save(context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string())),
        }?;
        let (_, _, _, _, _, actual_idempotency_key, _) = self.fields()?;
        let expected_idempotency_key = format!(
            "sales/{}/{}",
            self.aggregate_type(),
            context.idempotency_key
        );
        if actual_idempotency_key != expected_idempotency_key {
            return Err(PersistenceError::InvalidData(
                "Sales aggregate idempotency key is not bound to the command".into(),
            ));
        }
        let command_id = match self {
            Self::Lead(value) => value.command_id,
            Self::Opportunity(value) => value.command_id,
            Self::Quote(value) => value.command_id,
            Self::SalesOrder(value) => value.command_id,
            Self::DeliveryHandoff(value) => value.command_id,
            Self::InvoiceHandoff(value) => value.command_id,
            Self::CollectionState(value) => value.command_id,
        };
        if command_id != context.command_id {
            return Err(PersistenceError::InvalidData(
                "Sales aggregate command identity is not bound to the command".into(),
            ));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), PersistenceError> {
        let result = match self {
            Self::Lead(value) => value.validate(),
            Self::Opportunity(value) => value.validate(),
            Self::Quote(value) => value.validate(),
            Self::SalesOrder(value) => value.validate(),
            Self::DeliveryHandoff(value) => value.validate(),
            Self::InvoiceHandoff(value) => value.validate(),
            Self::CollectionState(value) => value.validate(),
        };
        result.map_err(|error| PersistenceError::InvalidData(error.to_string()))
    }

    pub fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Lead(_) => "lead",
            Self::Opportunity(_) => "opportunity",
            Self::Quote(_) => "quote",
            Self::SalesOrder(_) => "sales_order",
            Self::DeliveryHandoff(_) => "delivery_handoff",
            Self::InvoiceHandoff(_) => "invoice_handoff",
            Self::CollectionState(_) => "collection_state",
        }
    }

    pub fn aggregate_id(&self) -> Uuid {
        match self {
            Self::Lead(value) => value.id,
            Self::Opportunity(value) => value.id,
            Self::Quote(value) => value.id,
            Self::SalesOrder(value) => value.id,
            Self::DeliveryHandoff(value) => value.id,
            Self::InvoiceHandoff(value) => value.id,
            Self::CollectionState(value) => value.id,
        }
    }

    fn fields(&self) -> Result<SalesFields<'_>, PersistenceError> {
        let fields = match self {
            Self::Lead(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::Opportunity(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::Quote(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::SalesOrder(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::DeliveryHandoff(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::InvoiceHandoff(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
            Self::CollectionState(value) => (
                self.aggregate_type(),
                value.id,
                value.company_id,
                value.branch_id,
                value.reference.as_str(),
                value.idempotency_key.as_str(),
                serde_json::to_value(value)?,
            ),
        };
        Ok(fields)
    }

    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Lead(value) => &value.scope,
            Self::Opportunity(value) => &value.scope,
            Self::Quote(value) => &value.scope,
            Self::SalesOrder(value) => &value.scope,
            Self::DeliveryHandoff(value) => &value.scope,
            Self::InvoiceHandoff(value) => &value.scope,
            Self::CollectionState(value) => &value.scope,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlxSalesStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxSalesStore {
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
        aggregate: &SalesAggregate,
        expected_version: Option<u64>,
    ) -> Result<SalesWriteReceipt, PersistenceError> {
        aggregate.validate()?;
        let aggregate_type = aggregate.aggregate_type();
        let payload = serde_json::to_value(aggregate)?;
        let request_hash = hash_request(aggregate_type, &payload, expected_version)?;
        self.save_with_request_hash(aggregate, expected_version, request_hash)
            .await
    }

    async fn save_with_request_hash(
        &self,
        aggregate: &SalesAggregate,
        expected_version: Option<u64>,
        request_hash: String,
    ) -> Result<SalesWriteReceipt, PersistenceError> {
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
        let expected_replay_version = next_version(expected_version)?;
        let mut tx = self.begin_write().await?;
        if let Some(response) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, request_hash, response
             FROM erpx_tenant.sales_command_idempotency
             WHERE idempotency_key = $1 AND holding_ref = $2 FOR UPDATE",
        )
        .bind(idempotency_key)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_hash: String = response.try_get("request_hash")?;
            if stored_hash != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.into(),
                });
            }
            let stored_aggregate_type: String = response.try_get("aggregate_type")?;
            let stored_aggregate_id: Uuid = response.try_get("aggregate_id")?;
            let receipt = validate_idempotency_replay_receipt(
                stored_aggregate_type.as_str(),
                stored_aggregate_id,
                serde_json::from_value(response.try_get("response")?)?,
                aggregate_type,
                aggregate_id,
                expected_replay_version,
            )?;
            let event_payload = serde_json::json!({
                "aggregate_type": aggregate_type,
                "aggregate": serde_json::to_value(aggregate)?,
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
            .bind(as_i64(receipt.version, "Sales replay source version")?)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|row| {
                Ok::<_, PersistenceError>(SalesReplayOutboxFact {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "Sales replay source version",
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
            "SELECT version FROM erpx_tenant.sales_aggregates
             WHERE aggregate_type = $1 AND aggregate_id = $2 AND holding_ref = $3 FOR UPDATE",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| as_u64(value, "Sales aggregate version"))
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
            PersistenceError::InvalidData("Sales aggregate version overflow".into())
        })?;
        let holding_scope = aggregate.scope();
        let branch_id = branch_id.or_else(|| branch_from_scope(holding_scope));
        if current.is_some() {
            sqlx::query(
                "UPDATE erpx_tenant.sales_aggregates
                 SET company_id=$3, branch_id=$4, reference=$5, version=$6,
                     idempotency_key=$7, payload=$8, updated_at=now()
                 WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$9",
            )
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(company_id)
            .bind(branch_id)
            .bind(reference)
            .bind(as_i64(version, "Sales aggregate version")?)
            .bind(idempotency_key)
            .bind(&payload)
            .bind(&self.holding_ref)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.sales_aggregates
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
            .bind(as_i64(version, "Sales aggregate version")?)
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
        .bind(as_i64(version, "Sales aggregate version")?)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        let event_payload = serde_json::json!({
            "aggregate_type": aggregate_type,
            // The source table keeps the domain state payload, but the
            // outbox contract carries the tagged aggregate so the projector
            // can validate type and identity before writing the read model.
            "aggregate": serde_json::to_value(aggregate)?,
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
        .bind(as_i64(version, "Sales aggregate version")?)
        .bind(format!("sales.{aggregate_type}.v1"))
        .bind(event_payload)
        .execute(&mut *tx)
        .await?;
        let receipt = SalesWriteReceipt {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            version,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.sales_command_idempotency
                (idempotency_key, holding_ref, aggregate_type, aggregate_id, request_hash, response)
             VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(idempotency_key)
        .bind(&self.holding_ref)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(request_hash)
        .bind(serde_json::to_value(&receipt)?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Apply one authoritative collection payment. The operation has its own
    /// request hash (amount plus expected source version), so retrying the
    /// same command is replay-safe even after the collection source snapshot
    /// has advanced. Payment state, snapshot, outbox event, and idempotency
    /// receipt still commit through the shared atomic Sales writer.
    pub async fn record_payment(
        &self,
        aggregate_id: Uuid,
        amount_minor: i64,
        expected_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<SalesWriteReceipt, PersistenceError> {
        validate_uuid(aggregate_id, "Sales collection aggregate ID")?;
        if amount_minor <= 0 {
            return Err(PersistenceError::InvalidData(
                "Sales payment amount must be positive".into(),
            ));
        }
        if expected_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Sales payment expected version must be positive".into(),
            ));
        }
        if context.expected_aggregate_version != Some(expected_version) {
            return Err(PersistenceError::InvalidData(
                "Sales payment expected version does not match command context".into(),
            ));
        }
        let idempotency_key =
            deterministic_idempotency_key(SalesAggregateKind::CollectionState, context)
                .map_err(domain_error)?;
        let request_hash = hash_request(
            "collection_payment",
            &serde_json::json!({
                "aggregate_id": aggregate_id,
                "amount_minor": amount_minor,
            }),
            Some(expected_version),
        )?;
        if let Some(receipt) = self
            .replay_payment(
                &idempotency_key,
                &request_hash,
                aggregate_id,
                expected_version,
            )
            .await?
        {
            return Ok(receipt);
        }

        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT payload, version
               FROM erpx_tenant.sales_aggregates
              WHERE aggregate_type='collection_state'
                AND aggregate_id=$1 AND holding_ref=$2
              FOR UPDATE",
        )
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Err(PersistenceError::NotFound {
                entity: "Sales collection source aggregate",
                id: aggregate_id,
            });
        };
        let actual_version = as_u64(row.try_get("version")?, "Sales aggregate version")?;
        if actual_version != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "collection_state".into(),
                aggregate_id,
                expected: Some(expected_version),
                actual: Some(actual_version),
            });
        }
        let payload: Value = row.try_get("payload")?;
        let tagged = serde_json::json!({
            "aggregate_type": "collection_state",
            "aggregate": payload,
        });
        let mut aggregate: SalesAggregate = serde_json::from_value(tagged)?;
        let SalesAggregate::CollectionState(collection) = &mut aggregate else {
            return Err(PersistenceError::InvalidData(
                "Sales payment source aggregate is not collection state".into(),
            ));
        };
        if collection.id != aggregate_id {
            return Err(PersistenceError::InvalidData(
                "Sales payment source aggregate identity does not match the request".into(),
            ));
        }
        collection
            .record_payment(amount_minor, context, Utc::now())
            .map_err(domain_error)?;

        // A concurrent identical command may have committed between the
        // initial replay check and the source load. Let the first committed
        // command remain authoritative and return its receipt.
        if let Some(receipt) = self
            .replay_payment(
                &idempotency_key,
                &request_hash,
                aggregate_id,
                expected_version,
            )
            .await?
        {
            return Ok(receipt);
        }
        self.save_with_request_hash(&aggregate, Some(expected_version), request_hash)
            .await
    }

    async fn replay_payment(
        &self,
        idempotency_key: &str,
        request_hash: &str,
        aggregate_id: Uuid,
        expected_version: u64,
    ) -> Result<Option<SalesWriteReceipt>, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let Some(response) = sqlx::query(
            "SELECT aggregate_type, aggregate_id, request_hash, response
             FROM erpx_tenant.sales_command_idempotency
             WHERE idempotency_key=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(idempotency_key)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        else {
            tx.commit().await?;
            return Ok(None);
        };
        let stored_hash: String = response.try_get("request_hash")?;
        if stored_hash != request_hash {
            return Err(PersistenceError::IdempotencyConflict {
                key: idempotency_key.into(),
            });
        }
        let stored_aggregate_type: String = response.try_get("aggregate_type")?;
        let stored_aggregate_id: Uuid = response.try_get("aggregate_id")?;
        let receipt = validate_idempotency_replay_receipt(
            &stored_aggregate_type,
            stored_aggregate_id,
            serde_json::from_value(response.try_get("response")?)?,
            "collection_state",
            aggregate_id,
            next_version(Some(expected_version))?,
        )?;
        let rows = sqlx::query(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type='collection_state'
               AND aggregate_id=$2 AND aggregate_version=$3
             ORDER BY event_id
             FOR SHARE",
        )
        .bind(&self.holding_ref)
        .bind(aggregate_id)
        .bind(as_i64(
            receipt.version,
            "Sales payment replay source version",
        )?)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|row| {
            Ok::<_, PersistenceError>(SalesReplayOutboxFact {
                event_id: row.try_get("event_id")?,
                event_type: row.try_get("event_type")?,
                aggregate_type: row.try_get("aggregate_type")?,
                aggregate_id: row.try_get("aggregate_id")?,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Sales payment replay source version",
                )?,
                payload: row.try_get("payload")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
        let expected_payload = rows.first().map(|row| row.payload.clone()).ok_or_else(|| {
            PersistenceError::InvalidData(
                "Sales payment replay is missing its authoritative outbox event".into(),
            )
        })?;
        validate_replay_outbox_evidence(
            "collection_state",
            aggregate_id,
            receipt.version,
            &expected_payload,
            &rows,
        )?;
        let event: SalesEventPayload = serde_json::from_value(expected_payload)?;
        event.aggregate.validate()?;
        if event.aggregate_type != "collection_state"
            || event.version != receipt.version
            || event.aggregate.aggregate_type() != "collection_state"
            || event.aggregate.aggregate_id() != aggregate_id
        {
            return Err(PersistenceError::InvalidData(
                "Sales payment replay outbox payload is inconsistent".into(),
            ));
        }
        let mut receipt = receipt;
        receipt.replayed = true;
        tx.commit().await?;
        Ok(Some(receipt))
    }

    /// Apply one allow-listed domain lifecycle transition against the
    /// authoritative write aggregate. The source row is loaded from the
    /// write database, never from the replaceable read projection. The final
    /// save uses optimistic concurrency, so a concurrent writer cannot be
    /// overwritten between the load and the atomic source/outbox commit.
    pub async fn transition(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
        transition: &str,
        expected_version: u64,
        context: &erpx_domain::contracts::CommandContext,
    ) -> Result<SalesWriteReceipt, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Sales aggregate ID")?;
        if expected_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Sales transition expected version must be positive".into(),
            ));
        }
        let transition = transition.trim();
        if transition.is_empty()
            || transition.len() > 64
            || transition.chars().any(char::is_control)
        {
            return Err(PersistenceError::InvalidData(
                "Sales transition is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT payload, version
               FROM erpx_tenant.sales_aggregates
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
                entity: "Sales source aggregate",
                id: aggregate_id,
            });
        };
        let actual_version = as_u64(row.try_get("version")?, "Sales aggregate version")?;
        if actual_version != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                expected: Some(expected_version),
                actual: Some(actual_version),
            });
        }
        let payload: Value = row.try_get("payload")?;
        let tagged = serde_json::json!({
            "aggregate_type": aggregate_type,
            "aggregate": payload,
        });
        let mut aggregate: SalesAggregate = serde_json::from_value(tagged)?;
        if aggregate.aggregate_id() != aggregate_id || aggregate.aggregate_type() != aggregate_type
        {
            return Err(PersistenceError::InvalidData(
                "Sales source aggregate identity does not match the requested transition".into(),
            ));
        }
        apply_transition(&mut aggregate, transition, context, Utc::now())?;
        self.save(&aggregate, Some(expected_version)).await
    }

    /// Read the immutable Sales source stream from the write database in a
    /// stable aggregate/type/version order. This is a source-ingestion seam;
    /// callers must apply events through `project_event` and may retry pages
    /// safely because the read projection is version-idempotent.
    pub async fn read_source_event_page(
        &self,
        cursor: Option<&SalesSourceEventCursor>,
        page_size: u32,
    ) -> Result<SalesSourceEventPage, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let page =
            read_source_event_page_in_transaction(&mut tx, &self.holding_ref, cursor, page_size)
                .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Return the holding-scoped count of immutable Sales source events.
    /// This is a source fence: it is read from the authoritative outbox and
    /// never from the disposable projection. Rebuild callers must capture it
    /// before replacing read state and verify it again before committing.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count = source_event_high_watermark_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(count)
    }

    /// Compare the authoritative Sales outbox with the disposable read model.
    /// This is intentionally read-only and holding-scoped: a mismatch is a
    /// rebuild/incident signal, never permission to repair source state.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<SalesProjectionReconciliation, PersistenceError> {
        let mut source_tx = self.begin_write().await?;
        // Keep the authoritative source snapshot stable for the complete
        // reconciliation, including the read-side comparison.  A table-level
        // SHARE lock is intentional here: every Sales source write takes a
        // ROW EXCLUSIVE lock through the same transaction, so a reconciliation
        // cannot report a false green result across a moving source boundary.
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        ensure_no_duplicate_source_versions(&mut source_tx, &self.holding_ref).await?;
        ensure_source_event_type_contract(&mut source_tx, &self.holding_ref).await?;
        let source_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id,
                    MIN(aggregate_version) AS first_version,
                    MAX(aggregate_version) AS latest_version,
                    COUNT(*) AS event_count
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN
                   ('lead', 'opportunity', 'quote', 'sales_order',
                    'delivery_handoff', 'invoice_handoff', 'collection_state')
             GROUP BY aggregate_type, aggregate_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;

        let mut source_facts = HashMap::with_capacity(source_rows.len());
        let mut source_event_count = 0_u64;
        for row in source_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let first_version = as_u64(row.try_get("first_version")?, "Sales source version")?;
            let latest_version = as_u64(row.try_get("latest_version")?, "Sales source version")?;
            let event_count = as_u64(row.try_get("event_count")?, "Sales source event count")?;
            validate_aggregate_type(&aggregate_type)?;
            validate_uuid(aggregate_id, "Sales source aggregate ID")?;
            validate_source_version_span(first_version, latest_version, event_count)?;
            source_event_count = source_event_count.checked_add(event_count).ok_or_else(|| {
                PersistenceError::InvalidData("Sales event count overflow".into())
            })?;
            source_facts.insert((aggregate_type, aggregate_id), latest_version);
        }

        let source_event_rows = sqlx::query(&format!(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id"
        ))
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;
        for row in source_event_rows {
            validate_source_event_payload(
                row.try_get("event_id")?,
                row.try_get("event_type")?,
                row.try_get("aggregate_type")?,
                row.try_get("aggregate_id")?,
                as_u64(
                    row.try_get("aggregate_version")?,
                    "Sales source event version",
                )?,
                row.try_get("payload")?,
            )?;
        }

        let latest_source_rows = sqlx::query(
            "SELECT DISTINCT ON (aggregate_type, aggregate_id)
                    event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1
               AND aggregate_type IN
                   ('lead', 'opportunity', 'quote', 'sales_order',
                    'delivery_handoff', 'invoice_handoff', 'collection_state')
             ORDER BY aggregate_type, aggregate_id, aggregate_version DESC, event_id DESC",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;

        let source_snapshot_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, version, payload
             FROM erpx_tenant.sales_aggregates
             WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *source_tx)
        .await?;

        let mut source = HashMap::with_capacity(latest_source_rows.len());
        for row in latest_source_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version =
                as_u64(row.try_get("aggregate_version")?, "Sales source version")?;
            let payload: Value = row.try_get("payload")?;
            validate_uuid(event_id, "Sales source event ID")?;
            validate_uuid(aggregate_id, "Sales source aggregate ID")?;
            validate_aggregate_type(&aggregate_type)?;
            if event_type != format!("sales.{aggregate_type}.v1") {
                return Err(PersistenceError::InvalidData(
                    "Sales source event type does not match its aggregate type".into(),
                ));
            }
            let event: SalesEventPayload = serde_json::from_value(payload)?;
            event.aggregate.validate()?;
            if event.aggregate_type != aggregate_type
                || event.aggregate.aggregate_type() != aggregate_type
                || event.aggregate.aggregate_id() != aggregate_id
                || event.version != aggregate_version
            {
                return Err(PersistenceError::InvalidData(
                    "Sales source event identity does not match its payload".into(),
                ));
            }
            let key = (aggregate_type, aggregate_id);
            let Some(expected_version) = source_facts.remove(&key) else {
                return Err(PersistenceError::InvalidData(
                    "Sales latest source event is not present in its aggregate facts".into(),
                ));
            };
            if expected_version != aggregate_version {
                return Err(PersistenceError::InvalidData(
                    "Sales latest source version does not match its aggregate facts".into(),
                ));
            }
            source.insert(
                key,
                SalesSourceProjectionFact {
                    latest_version: aggregate_version,
                    latest_event_id: event_id,
                    aggregate_payload: serde_json::to_value(&event.aggregate)?,
                },
            );
        }
        if !source_facts.is_empty() {
            return Err(PersistenceError::InvalidData(
                "Sales aggregate facts are missing their latest source event".into(),
            ));
        }

        let mut source_snapshots = HashMap::with_capacity(source_snapshot_rows.len());
        for row in source_snapshot_rows {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let version = as_u64(row.try_get("version")?, "Sales source snapshot version")?;
            let payload: Value = row.try_get("payload")?;
            validate_aggregate_type(&aggregate_type)?;
            validate_uuid(aggregate_id, "Sales source snapshot aggregate ID")?;
            let aggregate = decode_source_aggregate(&aggregate_type, aggregate_id, payload)?;
            if aggregate.aggregate_type() != aggregate_type
                || aggregate.aggregate_id() != aggregate_id
            {
                return Err(PersistenceError::InvalidData(
                    "Sales source snapshot identity is invalid".into(),
                ));
            }
            source_snapshots.insert(
                (aggregate_type, aggregate_id),
                (version, serde_json::to_value(&aggregate)?),
            );
        }
        if source_snapshots.len() != source.len()
            || source.keys().any(|key| {
                source_snapshots.get(key).is_none_or(|(version, payload)| {
                    let Some(source_fact) = source.get(key) else {
                        return true;
                    };
                    *version != source_fact.latest_version
                        || *payload != source_fact.aggregate_payload
                })
            })
        {
            return Err(PersistenceError::InvalidData(
                "Sales source snapshot and outbox are inconsistent".into(),
            ));
        }
        let mut read_tx = self.begin_read().await?;
        let read_rows = sqlx::query(
            "SELECT a.aggregate_type, a.aggregate_id, a.source_version, a.payload,
                    c.source_version AS checkpoint_source_version,
                    c.last_event_id
             FROM erpx_tenant.sales_aggregates AS a
             LEFT JOIN erpx_tenant.sales_projection_checkpoints AS c
               ON c.aggregate_type=a.aggregate_type
              AND c.aggregate_id=a.aggregate_id
              AND c.holding_ref=a.holding_ref
             WHERE a.holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let projected = read_rows
            .into_iter()
            .map(|row| {
                let aggregate_type: String = row.try_get("aggregate_type")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                validate_aggregate_type(&aggregate_type)?;
                validate_uuid(aggregate_id, "Sales projected aggregate ID")?;
                let checkpoint_source_version = row
                    .try_get::<Option<i64>, _>("checkpoint_source_version")?
                    .map(|value| as_u64(value, "Sales checkpoint source version"))
                    .transpose()?;
                Ok::<_, PersistenceError>((
                    (aggregate_type, aggregate_id),
                    SalesProjectedFact {
                        source_version: as_u64(
                            row.try_get("source_version")?,
                            "Sales projected version",
                        )?,
                        payload: row.try_get("payload")?,
                        checkpoint_source_version,
                        checkpoint_event_id: row.try_get("last_event_id")?,
                    },
                ))
            })
            .collect::<Result<HashMap<(String, Uuid), SalesProjectedFact>, _>>()?;

        let workflow_linkage_error_count = validate_sales_workflow(&source)?;
        let mut reconciliation =
            reconcile_sales_projection_facts(source_event_count, &source, &projected);
        reconciliation.workflow_linkage_error_count = workflow_linkage_error_count;
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok(reconciliation)
    }

    pub async fn get(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<(SalesAggregate, u64)>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Sales aggregate ID")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT a.payload, a.source_version,
                    c.source_version AS checkpoint_source_version,
                    c.last_event_id
             FROM erpx_tenant.sales_aggregates AS a
             LEFT JOIN erpx_tenant.sales_projection_checkpoints AS c
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
            let source_version = as_u64(row.try_get("source_version")?, "Sales source version")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_source_version")?
                .map(|value| as_u64(value, "Sales checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("last_event_id")?;
            if checkpoint_version != Some(source_version)
                || checkpoint_event_id.is_none_or(|value| value.is_nil())
            {
                return Err(PersistenceError::InvalidData(
                    "Sales read projection checkpoint is missing or inconsistent".into(),
                ));
            }
            let aggregate = decode_read_aggregate(aggregate_type, aggregate_id, payload)?;
            Ok((aggregate, source_version))
        })
        .transpose()
    }

    /// Read a projected aggregate only when the caller's explicit company or
    /// branch scope contains the aggregate scope.  This guard is separate
    /// from shared role authorization: it prevents a permitted role from
    /// crossing a Sales record boundary with an otherwise valid UUID.
    pub async fn get_for_scope(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
        authorizing_scope: &TenantScope,
    ) -> Result<Option<(SalesAggregate, u64)>, PersistenceError> {
        let Some(row) = self.get(aggregate_type, aggregate_id).await? else {
            return Ok(None);
        };
        validate_read_scope(&row.0, authorizing_scope)?;
        Ok(Some(row))
    }

    /// Read a bounded typed page from the disposable Sales projection. The
    /// source database is never consulted: every row must have a complete
    /// checkpoint, a validated aggregate identity, and typed scope columns
    /// that agree with the projected aggregate before it leaves this adapter.
    pub async fn list_for_scope(
        &self,
        scope: SalesReadScope,
        page_size: u32,
        before: Option<&SalesReadCursor>,
    ) -> Result<SalesReadPage, PersistenceError> {
        validate_uuid(scope.holding_id, "Sales read holding ID")?;
        validate_uuid(scope.company_id, "Sales read company ID")?;
        if let Some(branch_id) = scope.branch_id {
            validate_uuid(branch_id, "Sales read branch ID")?;
        }
        if page_size == 0 || page_size > MAX_SALES_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Sales read page size must be 1..={MAX_SALES_READ_PAGE_SIZE}"
            )));
        }
        if let Some(cursor) = before {
            validate_aggregate_type(&cursor.aggregate_type)?;
            validate_uuid(cursor.aggregate_id, "Sales read cursor aggregate ID")?;
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
            "SELECT a.aggregate_type, a.aggregate_id, a.holding_ref,
                    a.company_id, a.branch_id, a.reference, a.source_version,
                    a.payload, a.projected_at,
                    c.source_version AS checkpoint_version,
                    c.last_event_id AS checkpoint_event_id
             FROM erpx_tenant.sales_aggregates AS a
             LEFT JOIN erpx_tenant.sales_projection_checkpoints AS c
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
        .bind(before.map(|cursor| cursor.aggregate_type.as_str()))
        .bind(before.map(|cursor| cursor.aggregate_id))
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;

        let page_len = usize::try_from(page_size)
            .map_err(|_| PersistenceError::InvalidData("Sales read page size overflow".into()))?;
        let has_more = rows.len() > page_len;
        let selected = rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut decoded = Vec::with_capacity(selected.len());
        for row in selected {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_aggregate_type(&aggregate_type)?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(aggregate_id, "Sales read aggregate ID")?;
            let holding_ref: String = row.try_get("holding_ref")?;
            if holding_ref != self.holding_ref {
                return Err(PersistenceError::InvalidData(
                    "Sales list row holding reference is inconsistent".into(),
                ));
            }
            let source_version = as_u64(row.try_get("source_version")?, "Sales source version")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|value| as_u64(value, "Sales checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("checkpoint_event_id")?;
            if checkpoint_version != Some(source_version)
                || checkpoint_event_id.is_none_or(|event_id| event_id.is_nil())
            {
                return Err(PersistenceError::InvalidData(
                    "Sales list row checkpoint is missing or inconsistent".into(),
                ));
            }
            let aggregate =
                decode_read_aggregate(&aggregate_type, aggregate_id, row.try_get("payload")?)?;
            if aggregate.scope() == &TenantScope::Platform
                || !requested_scope.contains(aggregate.scope())
            {
                return Err(PersistenceError::InvalidData(
                    "Sales list row scope is invalid".into(),
                ));
            }
            let persisted_holding_id = match aggregate.scope() {
                TenantScope::Company { holding_id, .. }
                | TenantScope::Branch { holding_id, .. } => *holding_id,
                _ => {
                    return Err(PersistenceError::InvalidData(
                        "Sales list row scope is not company or branch scoped".into(),
                    ));
                }
            };
            let persisted_company_id: Uuid = row.try_get("company_id")?;
            let persisted_branch_id: Option<Uuid> = row.try_get("branch_id")?;
            let (aggregate_company_id, aggregate_branch_id) = match aggregate.scope() {
                TenantScope::Company { company_id, .. } => (*company_id, None),
                TenantScope::Branch {
                    company_id,
                    branch_id,
                    ..
                } => (*company_id, Some(*branch_id)),
                _ => {
                    return Err(PersistenceError::InvalidData(
                        "Sales list row scope is not company or branch scoped".into(),
                    ));
                }
            };
            if persisted_holding_id != scope.holding_id
                || persisted_company_id != aggregate_company_id
                || persisted_branch_id != aggregate_branch_id
            {
                return Err(PersistenceError::InvalidData(
                    "Sales list row typed scope columns are inconsistent".into(),
                ));
            }
            let reference: String = row.try_get("reference")?;
            if reference.trim().is_empty()
                || reference.len() > 255
                || reference.chars().any(char::is_control)
            {
                return Err(PersistenceError::InvalidData(
                    "Sales list row reference is invalid".into(),
                ));
            }
            decoded.push(SalesReadListRow {
                aggregate_type,
                aggregate_id,
                holding_id: persisted_holding_id,
                company_id: persisted_company_id,
                branch_id: persisted_branch_id,
                reference,
                source_version,
                projected_at: row.try_get("projected_at")?,
            });
        }
        let next_cursor = has_more
            .then(|| {
                decoded.last().map(|row| SalesReadCursor {
                    projected_at: row.projected_at,
                    aggregate_type: row.aggregate_type.clone(),
                    aggregate_id: row.aggregate_id,
                })
            })
            .flatten();
        Ok(SalesReadPage {
            rows: decoded,
            next_cursor,
            has_more,
        })
    }

    /// Apply one validated Sales outbox payload to the disposable read pool.
    /// Version continuity is enforced so an out-of-order event cannot make a
    /// projection appear fresh while skipping an earlier aggregate state.
    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<SalesProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "Sales event ID")?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Sales aggregate ID")?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Sales aggregate version must be positive".into(),
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
        let mut tx = self.begin_read().await?;
        let result = self
            .project_event_in_transaction(
                &mut tx,
                event_id,
                &format!("sales.{aggregate_type}.v1"),
                aggregate_type,
                aggregate_id,
                aggregate_version,
                payload,
            )
            .await;
        match result {
            Ok(result) => {
                tx.commit().await?;
                Ok(result)
            }
            Err(error) => Err(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn project_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        event_type: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<SalesProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "Sales event ID")?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Sales aggregate ID")?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Sales aggregate version must be positive".into(),
            ));
        }
        if event_type != format!("sales.{aggregate_type}.v1") {
            return Err(PersistenceError::InvalidData(
                "Sales projection event type does not match its aggregate type".into(),
            ));
        }
        let event: SalesEventPayload = serde_json::from_value(payload.clone())?;
        event.aggregate.validate()?;
        validate_aggregate_type(&event.aggregate_type)?;
        if event.version != aggregate_version
            || event.aggregate_type != aggregate_type
            || event.aggregate.aggregate_type() != aggregate_type
            || event.aggregate.aggregate_id() != aggregate_id
        {
            return Err(PersistenceError::InvalidData(
                "Sales projection event identity or version does not match payload".into(),
            ));
        }
        let (_, _, company_id, branch_id, reference, _, _) = event.aggregate.fields()?;
        let aggregate_payload = serde_json::to_value(&event.aggregate)?;
        let current = sqlx::query(
            "SELECT a.source_version, a.payload,
                    c.source_version AS checkpoint_source_version,
                    c.last_event_id
             FROM erpx_tenant.sales_aggregates AS a
             LEFT JOIN erpx_tenant.sales_projection_checkpoints AS c
               ON c.aggregate_type=a.aggregate_type
              AND c.aggregate_id=a.aggregate_id
              AND c.holding_ref=a.holding_ref
             WHERE a.aggregate_type=$1 AND a.aggregate_id=$2 AND a.holding_ref=$3
             FOR UPDATE OF a",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| {
            let source_version = as_u64(
                row.try_get("source_version")?,
                "Sales projection source version",
            )?;
            let payload: Value = row.try_get("payload")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_source_version")?
                .map(|value| as_u64(value, "Sales checkpoint source version"))
                .transpose()?;
            let checkpoint_event_id = row.try_get::<Option<Uuid>, _>("last_event_id")?;
            let checkpoint_version = checkpoint_version.ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Sales projection aggregate checkpoint is missing".into(),
                )
            })?;
            let checkpoint_event_id = checkpoint_event_id.ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Sales projection aggregate checkpoint event is missing".into(),
                )
            })?;
            if checkpoint_event_id.is_nil() || checkpoint_version != source_version {
                return Err(PersistenceError::InvalidData(
                    "Sales projection aggregate and checkpoint are inconsistent".into(),
                ));
            }
            Ok::<_, PersistenceError>((source_version, payload, checkpoint_event_id))
        })
        .transpose()?;
        if let Some((current, current_payload, checkpoint_event_id)) = current {
            if current >= aggregate_version {
                if current == aggregate_version
                    && (checkpoint_event_id != event_id || current_payload != aggregate_payload)
                {
                    return Err(PersistenceError::InvalidData(
                        "Sales projection same-version event or payload conflict".into(),
                    ));
                }
                return Ok(SalesProjectionApplication {
                    event_id,
                    aggregate_type: aggregate_type.into(),
                    aggregate_id,
                    aggregate_version,
                    applied: false,
                });
            }
            if current.saturating_add(1) != aggregate_version {
                return Err(PersistenceError::InvalidData(
                    "Sales projection event is out of order".into(),
                ));
            }
        } else if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "Sales projection must start at aggregate version one".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.sales_aggregates
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
        .bind(company_id)
        .bind(branch_id)
        .bind(reference)
        .bind(as_i64(
            aggregate_version,
            "Sales projection source version",
        )?)
        .bind(aggregate_payload)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.sales_projection_checkpoints
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
            "Sales projection source version",
        )?)
        .execute(&mut **tx)
        .await?;
        Ok(SalesProjectionApplication {
            event_id,
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            aggregate_version,
            applied: true,
        })
    }

    /// A projector must only apply an event that was actually committed to
    /// this holding's authoritative write outbox. Validating the JSON shape
    /// alone is insufficient: a caller could otherwise submit a forged,
    /// same-version payload and make the disposable read model diverge while
    /// every local identity check still passed.
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
            "SELECT o.event_type, o.aggregate_type, o.aggregate_id,
                    o.aggregate_version, o.payload,
                    a.version AS source_version, a.payload AS source_payload
             FROM erpx_tenant.outbox_events AS o
             LEFT JOIN erpx_tenant.sales_aggregates AS a
               ON a.aggregate_type=o.aggregate_type
              AND a.aggregate_id=o.aggregate_id
              AND a.holding_ref=o.holding_ref
             WHERE o.event_id=$1 AND o.holding_ref=$2",
        )
        .bind(event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let Some(row) = row else {
            return Err(PersistenceError::InvalidData(
                "Sales projection event is not present in the authoritative outbox".into(),
            ));
        };
        let source_event_type: String = row.try_get("event_type")?;
        let source_aggregate_type: String = row.try_get("aggregate_type")?;
        let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let source_aggregate_version = as_u64(
            row.try_get("aggregate_version")?,
            "Sales source event version",
        )?;
        let source_payload: Value = row.try_get("payload")?;
        let source_version = row
            .try_get::<Option<i64>, _>("source_version")?
            .map(|value| as_u64(value, "Sales source snapshot version"))
            .transpose()?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Sales projection event has no authoritative source snapshot".into(),
                )
            })?;
        if source_version < aggregate_version {
            return Err(PersistenceError::InvalidData(
                "Sales projection event is ahead of the authoritative source snapshot".into(),
            ));
        }
        let expected_event_type = format!("sales.{aggregate_type}.v1");
        if source_event_type != expected_event_type
            || source_aggregate_type != aggregate_type
            || source_aggregate_id != aggregate_id
            || source_aggregate_version != aggregate_version
            || source_payload != payload.clone()
        {
            return Err(PersistenceError::InvalidData(
                "Sales projection event does not match its authoritative outbox row".into(),
            ));
        }
        let event_snapshot: SalesEventPayload = serde_json::from_value(payload.clone())?;
        event_snapshot.aggregate.validate()?;
        let (_, _, _, _, _, _, expected_source_payload) = event_snapshot.aggregate.fields()?;
        let authoritative_payload: Value = row
            .try_get::<Option<Value>, _>("source_payload")?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Sales projection source snapshot payload is missing".into(),
                )
            })?;
        if source_version == aggregate_version && authoritative_payload != expected_source_payload {
            return Err(PersistenceError::InvalidData(
                "Sales projection event does not match the current source snapshot".into(),
            ));
        }
        Ok(())
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<SalesProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.sales_aggregates WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.sales_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(SalesProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        })
    }

    /// Atomically replace the disposable Sales projection from a fenced
    /// authoritative source snapshot. The outbox share lock prevents source
    /// writes while pages are read; the read transaction is committed only
    /// after every event is validated and replayed. Any source drift, version
    /// conflict, decode failure, or read-side error drops the transaction and
    /// preserves the previously committed projection.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_event_count: Option<u64>,
        page_size: u32,
    ) -> Result<(SalesProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_SALES_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Sales source event page size must be 1..={MAX_SALES_SOURCE_EVENT_PAGE_SIZE}"
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
                "Sales source fence changed before replacement rebuild".into(),
            ));
        }
        if actual_source_event_count > MAX_SALES_PROJECTION_EVENTS {
            return Err(PersistenceError::InvalidData(
                "Sales projection source event bound exceeded".into(),
            ));
        }
        ensure_no_duplicate_source_versions(&mut source_tx, &self.holding_ref).await?;

        let mut read_tx = self.begin_read().await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.sales_aggregates WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *read_tx)
                .await?
                .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.sales_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let preparation = SalesProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        };

        let mut cursor = None;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                self.project_event_in_transaction(
                    &mut read_tx,
                    event.event_id,
                    &event.event_type,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await?;
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Sales source event cursor is missing".into(),
                ));
            }
        }

        let final_source_event_count =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_event_count != actual_source_event_count {
            return Err(PersistenceError::InvalidData(
                "Sales source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, final_source_event_count))
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
    cursor: Option<&SalesSourceEventCursor>,
    page_size: u32,
) -> Result<SalesSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_SALES_SOURCE_EVENT_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "Sales source event page size must be 1..={MAX_SALES_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_aggregate_type(&cursor.aggregate_type)?;
        validate_uuid(cursor.aggregate_id, "Sales source cursor aggregate ID")?;
        // Sales cursors are intentionally kept compatible with the existing
        // worker payload shape, which does not carry holding_ref.  Bind the
        // continuation to an authoritative source row in the routed holding
        // before using it as a keyset position; otherwise a cursor copied from
        // another holding would silently turn into a valid-looking skip.
        let cursor_is_bound = sqlx::query_scalar::<_, i32>(&format!(
            "SELECT 1
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1
                AND {SALES_SOURCE_EVENT_TYPE_FILTER}
                AND aggregate_type=$2
                AND aggregate_id=$3
                AND aggregate_version=$4
              LIMIT 1"
        ))
        .bind(holding_ref)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Sales source cursor version",
        )?)
        .fetch_optional(&mut **tx)
        .await?
        .is_some();
        if !cursor_is_bound {
            return Err(PersistenceError::InvalidData(
                "Sales source cursor is not bound to the routed holding".into(),
            ));
        }
        sqlx::query(&format!(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}
               AND (aggregate_type > $2
                    OR (aggregate_type = $2 AND aggregate_id > $3)
                    OR (aggregate_type = $2 AND aggregate_id = $3
                        AND aggregate_version > $4))
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $5"
        ))
        .bind(holding_ref)
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Sales source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(&format!(
            "SELECT event_id, event_type, aggregate_type, aggregate_id,
                    aggregate_version, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}
             ORDER BY aggregate_type, aggregate_id, aggregate_version, event_id
             LIMIT $2"
        ))
        .bind(holding_ref)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size)
        .map_err(|_| PersistenceError::InvalidData("Sales source page size overflow".into()))?;
    let has_more = rows.len() > page_len;
    let all_events = rows
        .into_iter()
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let event_type: String = row.try_get("event_type")?;
            let aggregate_version = as_u64(
                row.try_get("aggregate_version")?,
                "Sales source event version",
            )?;
            let payload: Value = row.try_get("payload")?;
            validate_uuid(event_id, "Sales source event ID")?;
            validate_uuid(aggregate_id, "Sales source aggregate ID")?;
            validate_aggregate_type(&aggregate_type)?;
            validate_source_event_payload(
                event_id,
                &event_type,
                &aggregate_type,
                aggregate_id,
                aggregate_version,
                payload.clone(),
            )?;
            Ok(SalesSourceEvent {
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
    // The extra look-ahead row makes this check boundary-safe without
    // rescanning the entire holding on every page.  A duplicate version would
    // otherwise be skipped by the version-only continuation cursor.
    if all_events.windows(2).any(|events| {
        events[0].aggregate_type == events[1].aggregate_type
            && events[0].aggregate_id == events[1].aggregate_id
            && events[0].aggregate_version == events[1].aggregate_version
    }) {
        return Err(PersistenceError::InvalidData(
            "Sales source stream contains a duplicate aggregate version".into(),
        ));
    }
    let events = all_events.into_iter().take(page_len).collect::<Vec<_>>();
    let next_cursor = has_more.then(|| {
        events.last().map(|event| SalesSourceEventCursor {
            aggregate_type: event.aggregate_type.clone(),
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    });
    Ok(SalesSourceEventPage {
        events,
        next_cursor: next_cursor.flatten(),
        has_more,
    })
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*)
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}"
    ))
    .bind(holding_ref)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "Sales source event high watermark")
}

async fn ensure_no_duplicate_source_versions(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let duplicate = sqlx::query(&format!(
        "SELECT aggregate_type, aggregate_id, aggregate_version
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}
          GROUP BY aggregate_type, aggregate_id, aggregate_version
         HAVING COUNT(*) > 1
          LIMIT 1"
    ))
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if duplicate.is_some() {
        return Err(PersistenceError::InvalidData(
            "Sales source stream contains a duplicate aggregate version".into(),
        ));
    }
    Ok(())
}

async fn ensure_source_event_type_contract(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let invalid_event = sqlx::query_scalar::<_, i32>(&format!(
        "SELECT 1
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND {SALES_SOURCE_EVENT_TYPE_FILTER}
            AND event_type <> concat('sales.', aggregate_type, '.v1')
          LIMIT 1"
    ))
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    if invalid_event.is_some() {
        return Err(PersistenceError::InvalidData(
            "Sales source event type does not match its aggregate type".into(),
        ));
    }
    Ok(())
}

fn validate_source_version_span(
    first_version: u64,
    latest_version: u64,
    event_count: u64,
) -> Result<(), PersistenceError> {
    if first_version != 1 || event_count != latest_version {
        return Err(PersistenceError::InvalidData(
            "Sales source stream has a version gap".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct SalesEventPayload {
    aggregate_type: String,
    aggregate: SalesAggregate,
    version: u64,
}

fn validate_source_event_payload(
    event_id: Uuid,
    event_type: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
) -> Result<(), PersistenceError> {
    validate_uuid(event_id, "Sales source event ID")?;
    validate_uuid(aggregate_id, "Sales source aggregate ID")?;
    validate_aggregate_type(aggregate_type)?;
    if aggregate_version == 0 || event_type != format!("sales.{aggregate_type}.v1") {
        return Err(PersistenceError::InvalidData(
            "Sales source event type or version is invalid".into(),
        ));
    }
    let event: SalesEventPayload = serde_json::from_value(payload)?;
    event.aggregate.validate()?;
    if event.aggregate_type != aggregate_type
        || event.aggregate.aggregate_type() != aggregate_type
        || event.aggregate.aggregate_id() != aggregate_id
        || event.version != aggregate_version
    {
        return Err(PersistenceError::InvalidData(
            "Sales source event identity does not match its payload".into(),
        ));
    }
    Ok(())
}

fn next_version(current: Option<u64>) -> Result<u64, PersistenceError> {
    current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("Sales aggregate version overflow".into()))
}

fn validate_idempotency_replay_receipt(
    stored_aggregate_type: &str,
    stored_aggregate_id: Uuid,
    receipt: SalesWriteReceipt,
    expected_aggregate_type: &str,
    expected_aggregate_id: Uuid,
    expected_version: u64,
) -> Result<SalesWriteReceipt, PersistenceError> {
    validate_aggregate_type(stored_aggregate_type)?;
    validate_aggregate_type(expected_aggregate_type)?;
    validate_uuid(stored_aggregate_id, "Sales stored idempotency aggregate ID")?;
    validate_uuid(expected_aggregate_id, "Sales replay aggregate ID")?;
    if stored_aggregate_type != expected_aggregate_type
        || stored_aggregate_id != expected_aggregate_id
        || receipt.aggregate_type != expected_aggregate_type
        || receipt.aggregate_id != expected_aggregate_id
        || receipt.version != expected_version
        || receipt.replayed
    {
        return Err(PersistenceError::InvalidData(
            "Sales idempotency receipt is not bound to the requested source aggregate".into(),
        ));
    }
    Ok(receipt)
}

fn validate_replay_outbox_evidence(
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    expected_payload: &Value,
    rows: &[SalesReplayOutboxFact],
) -> Result<(), PersistenceError> {
    if rows.len() != 1 {
        return Err(PersistenceError::InvalidData(
            "Sales idempotency replay requires exactly one authoritative outbox event".into(),
        ));
    }
    let row = &rows[0];
    validate_uuid(row.event_id, "Sales replay source event ID")?;
    validate_aggregate_type(&row.aggregate_type)?;
    if row.event_type != format!("sales.{}.v1", row.aggregate_type)
        || row.aggregate_type != aggregate_type
        || row.aggregate_id != aggregate_id
        || row.aggregate_version != aggregate_version
        || row.payload != *expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "Sales idempotency replay does not match the authoritative outbox".into(),
        ));
    }
    Ok(())
}

fn reconcile_sales_projection_facts(
    source_event_count: u64,
    source: &HashMap<(String, Uuid), SalesSourceProjectionFact>,
    projected: &HashMap<(String, Uuid), SalesProjectedFact>,
) -> SalesProjectionReconciliation {
    let mut reconciliation = SalesProjectionReconciliation {
        source_event_count,
        source_aggregate_count: source.len() as u64,
        projected_aggregate_count: projected.len() as u64,
        ..SalesProjectionReconciliation::default()
    };

    for (key, source_fact) in source {
        let Some(projected_fact) = projected.get(key) else {
            reconciliation.missing_aggregate_count =
                reconciliation.missing_aggregate_count.saturating_add(1);
            continue;
        };

        if projected_fact.source_version < source_fact.latest_version {
            reconciliation.stale_aggregate_count =
                reconciliation.stale_aggregate_count.saturating_add(1);
            continue;
        }

        if projected_fact.source_version > source_fact.latest_version
            || projected_fact.payload != source_fact.aggregate_payload
            || projected_fact.checkpoint_source_version != Some(projected_fact.source_version)
            || projected_fact.checkpoint_event_id != Some(source_fact.latest_event_id)
        {
            reconciliation.invalid_projection_count =
                reconciliation.invalid_projection_count.saturating_add(1);
        }
    }

    reconciliation.orphan_aggregate_count = projected
        .keys()
        .filter(|key| !source.contains_key(*key))
        .count() as u64;
    reconciliation
}

fn validate_sales_workflow(
    source: &HashMap<(String, Uuid), SalesSourceProjectionFact>,
) -> Result<u64, PersistenceError> {
    let aggregates = source
        .iter()
        .map(|((aggregate_type, aggregate_id), fact)| {
            Ok::<_, PersistenceError>((
                (aggregate_type.clone(), *aggregate_id),
                decode_read_aggregate(
                    aggregate_type,
                    *aggregate_id,
                    fact.aggregate_payload.clone(),
                )?,
            ))
        })
        .collect::<Result<HashMap<(String, Uuid), SalesAggregate>, _>>()?;

    let mut linkage_errors = 0_u64;
    for aggregate in aggregates.values() {
        let valid = match aggregate {
            SalesAggregate::Lead(_) => true,
            SalesAggregate::Opportunity(value) => value.lead_id.is_none_or(|lead_id| {
                matches!(
                    workflow_related(&aggregates, "lead", lead_id),
                    Some(SalesAggregate::Lead(lead))
                        if value.validate_against_lead(lead).is_ok()
                )
            }),
            SalesAggregate::Quote(value) => value.opportunity_id.is_none_or(|opportunity_id| {
                matches!(
                    workflow_related(&aggregates, "opportunity", opportunity_id),
                    Some(SalesAggregate::Opportunity(opportunity))
                        if value.validate_against_opportunity(opportunity).is_ok()
                )
            }),
            SalesAggregate::SalesOrder(value) => matches!(
                workflow_related(&aggregates, "quote", value.quote_id),
                Some(SalesAggregate::Quote(quote))
                    if value.validate_against_quote(quote).is_ok()
            ),
            SalesAggregate::DeliveryHandoff(value) => matches!(
                workflow_related(&aggregates, "sales_order", value.sales_order_id),
                Some(SalesAggregate::SalesOrder(order))
                    if value.validate_against_order(order).is_ok()
            ),
            SalesAggregate::InvoiceHandoff(value) => {
                let order = workflow_related(&aggregates, "sales_order", value.sales_order_id);
                let delivery = value.delivery_handoff_id.and_then(|delivery_id| {
                    workflow_related(&aggregates, "delivery_handoff", delivery_id)
                });
                matches!(order, Some(SalesAggregate::SalesOrder(order))
                    if value.validate_against_order_and_delivery(order, delivery_as_ref(delivery)).is_ok())
            }
            SalesAggregate::CollectionState(value) => matches!(
                workflow_related(&aggregates, "invoice_handoff", value.invoice_handoff_id),
                Some(SalesAggregate::InvoiceHandoff(invoice))
                    if value.validate_against_invoice(invoice).is_ok()
            ),
        };
        if !valid {
            linkage_errors = linkage_errors.saturating_add(1);
        }
    }
    Ok(linkage_errors)
}

fn workflow_related<'a>(
    aggregates: &'a HashMap<(String, Uuid), SalesAggregate>,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Option<&'a SalesAggregate> {
    aggregates.get(&(aggregate_type.to_owned(), aggregate_id))
}

fn delivery_as_ref(aggregate: Option<&SalesAggregate>) -> Option<&DeliveryHandoff> {
    match aggregate {
        Some(SalesAggregate::DeliveryHandoff(delivery)) => Some(delivery),
        _ => None,
    }
}

fn branch_from_scope(scope: &erpx_domain::contracts::TenantScope) -> Option<Uuid> {
    match scope {
        erpx_domain::contracts::TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        _ => None,
    }
}

fn decode_read_aggregate(
    aggregate_type: &str,
    aggregate_id: Uuid,
    payload: Value,
) -> Result<SalesAggregate, PersistenceError> {
    let aggregate: SalesAggregate = serde_json::from_value(payload)?;
    aggregate.validate()?;
    if aggregate.aggregate_type() != aggregate_type || aggregate.aggregate_id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Sales read projection identity does not match its key".into(),
        ));
    }
    Ok(aggregate)
}

fn decode_source_aggregate(
    aggregate_type: &str,
    aggregate_id: Uuid,
    payload: Value,
) -> Result<SalesAggregate, PersistenceError> {
    let aggregate = match aggregate_type {
        "lead" => SalesAggregate::Lead(serde_json::from_value(payload)?),
        "opportunity" => SalesAggregate::Opportunity(serde_json::from_value(payload)?),
        "quote" => SalesAggregate::Quote(serde_json::from_value(payload)?),
        "sales_order" => SalesAggregate::SalesOrder(serde_json::from_value(payload)?),
        "delivery_handoff" => SalesAggregate::DeliveryHandoff(serde_json::from_value(payload)?),
        "invoice_handoff" => SalesAggregate::InvoiceHandoff(serde_json::from_value(payload)?),
        "collection_state" => SalesAggregate::CollectionState(serde_json::from_value(payload)?),
        _ => {
            return Err(PersistenceError::InvalidData(
                "unknown Sales aggregate type".into(),
            ));
        }
    };
    aggregate.validate()?;
    if aggregate.aggregate_type() != aggregate_type || aggregate.aggregate_id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Sales source snapshot identity does not match its key".into(),
        ));
    }
    Ok(aggregate)
}

fn apply_transition(
    aggregate: &mut SalesAggregate,
    transition: &str,
    context: &erpx_domain::contracts::CommandContext,
    at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    match aggregate {
        SalesAggregate::Lead(value) => {
            let next = match transition {
                "qualified" => LeadStatus::Qualified,
                "disqualified" => LeadStatus::Disqualified,
                "converted" => LeadStatus::Converted,
                _ => return invalid_transition("lead", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::Opportunity(value) => {
            let next = match transition {
                "won" => OpportunityStatus::Won,
                "lost" => OpportunityStatus::Lost,
                "abandoned" => OpportunityStatus::Abandoned,
                _ => return invalid_transition("opportunity", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::Quote(value) => {
            let next = match transition {
                "sent" => QuoteStatus::Sent,
                "accepted" => QuoteStatus::Accepted,
                "rejected" => QuoteStatus::Rejected,
                "expired" => QuoteStatus::Expired,
                _ => return invalid_transition("quote", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::SalesOrder(value) => {
            let next = match transition {
                "confirmed" => SalesOrderStatus::Confirmed,
                "fulfillment" => SalesOrderStatus::Fulfillment,
                "completed" => SalesOrderStatus::Completed,
                "cancelled" => SalesOrderStatus::Cancelled,
                _ => return invalid_transition("sales_order", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::DeliveryHandoff(value) => {
            let next = match transition {
                "requested" => HandoffStatus::Requested,
                "handed_off" => HandoffStatus::HandedOff,
                "failed" => HandoffStatus::Failed,
                "cancelled" => HandoffStatus::Cancelled,
                _ => return invalid_transition("delivery_handoff", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::InvoiceHandoff(value) => {
            let next = match transition {
                "requested" => HandoffStatus::Requested,
                "handed_off" => HandoffStatus::HandedOff,
                "failed" => HandoffStatus::Failed,
                "cancelled" => HandoffStatus::Cancelled,
                _ => return invalid_transition("invoice_handoff", transition),
            };
            value.transition(next, context, at).map_err(domain_error)
        }
        SalesAggregate::CollectionState(value) => match transition {
            "write_off" => value.write_off(context, at),
            "paid" => value.transition(CollectionStatus::Paid, context, at),
            _ => return invalid_transition("collection_state", transition),
        }
        .map_err(domain_error),
    }
}

fn invalid_transition(aggregate_type: &str, transition: &str) -> Result<(), PersistenceError> {
    Err(PersistenceError::InvalidData(format!(
        "transition {transition:?} is not supported for Sales aggregate {aggregate_type}"
    )))
}

fn domain_error(error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::InvalidData(error.to_string())
}

fn validate_read_scope(
    aggregate: &SalesAggregate,
    authorizing_scope: &TenantScope,
) -> Result<(), PersistenceError> {
    let explicit_scope = matches!(
        authorizing_scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    );
    if explicit_scope && authorizing_scope.contains(aggregate.scope()) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "Sales read scope is not authorized".into(),
        ))
    }
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if matches!(
        value,
        "lead"
            | "opportunity"
            | "quote"
            | "sales_order"
            | "delivery_handoff"
            | "invoice_handoff"
            | "collection_state"
    ) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unknown Sales aggregate type".into(),
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
    use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
    use erpx_domain::sales::{CustomerMasterReference, Lead, Opportunity};

    #[tokio::test]
    async fn store_rejects_unsafe_holding_references_without_database_access() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxSalesStore::new(pool.clone(), pool, " ").is_err());
    }

    #[test]
    fn aggregate_type_allow_list_is_closed() {
        assert!(validate_aggregate_type("sales_order").is_ok());
        assert!(validate_aggregate_type("payment_instrument").is_err());
    }

    #[test]
    fn source_event_filter_is_closed_to_sales_aggregate_types() {
        for aggregate_type in [
            "lead",
            "opportunity",
            "quote",
            "sales_order",
            "delivery_handoff",
            "invoice_handoff",
            "collection_state",
        ] {
            assert!(SALES_SOURCE_EVENT_TYPE_FILTER.contains(&format!("'{aggregate_type}'")));
        }
        assert!(!SALES_SOURCE_EVENT_TYPE_FILTER.contains("pos_sale"));
    }

    #[test]
    fn source_version_span_rejects_gaps_and_non_one_start() {
        assert!(validate_source_version_span(1, 3, 3).is_ok());
        assert!(validate_source_version_span(1, 3, 2).is_err());
        assert!(validate_source_version_span(2, 2, 1).is_err());
    }

    fn read_test_aggregate() -> SalesAggregate {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(100),
            company_id: Uuid::from_u128(200),
        };
        let context = CommandContext::try_new(
            "sales/read-safety",
            ActorAttribution {
                principal_ref: "sales-read-test".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            "sales read safety",
        )
        .expect("valid context");
        SalesAggregate::Lead(
            Lead::new(
                Uuid::from_u128(300),
                scope,
                Uuid::from_u128(100),
                Uuid::from_u128(200),
                None,
                None,
                "Read safety lead",
                "test",
                chrono::Utc::now(),
                &context,
            )
            .expect("valid lead"),
        )
    }

    #[test]
    fn read_decoder_revalidates_payload_and_identity() {
        let aggregate = read_test_aggregate();
        let aggregate_id = aggregate.aggregate_id();
        let mut payload = serde_json::to_value(&aggregate).expect("aggregate payload");
        payload["aggregate"]["reference"] = Value::String("LEAD-TAMPERED".into());
        assert!(decode_read_aggregate("lead", aggregate_id, payload).is_err());

        let payload = serde_json::to_value(&aggregate).expect("aggregate payload");
        assert!(decode_read_aggregate("lead", Uuid::from_u128(301), payload).is_err());
    }

    #[test]
    fn source_decoder_accepts_canonical_state_payload_separately_from_tagged_outbox() {
        let aggregate = read_test_aggregate();
        let aggregate_id = aggregate.aggregate_id();
        let (_, _, _, _, _, _, state_payload) = aggregate.fields().expect("source fields");
        let decoded = decode_source_aggregate("lead", aggregate_id, state_payload)
            .expect("canonical Sales source state payload");
        assert_eq!(decoded, aggregate);
        assert!(
            decode_source_aggregate(
                "lead",
                aggregate_id,
                serde_json::to_value(&aggregate).expect("tagged outbox payload"),
            )
            .is_err()
        );
    }

    #[test]
    fn read_scope_guard_rejects_cross_company_rows() {
        let aggregate = read_test_aggregate();
        let authorized = TenantScope::Company {
            holding_id: Uuid::from_u128(100),
            company_id: Uuid::from_u128(200),
        };
        assert!(validate_read_scope(&aggregate, &authorized).is_ok());
        let denied = TenantScope::Company {
            holding_id: Uuid::from_u128(100),
            company_id: Uuid::from_u128(201),
        };
        assert!(validate_read_scope(&aggregate, &denied).is_err());
        assert!(validate_read_scope(&aggregate, &TenantScope::Platform).is_err());
        assert!(
            validate_read_scope(
                &aggregate,
                &TenantScope::Holding {
                    holding_id: Uuid::from_u128(100),
                },
            )
            .is_err()
        );
    }

    #[test]
    fn source_workflow_reconciliation_rejects_missing_linked_aggregate() {
        let holding_id = Uuid::from_u128(400);
        let company_id = Uuid::from_u128(401);
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let context = CommandContext::try_new(
            "sales/workflow-opportunity",
            ActorAttribution {
                principal_ref: "sales-workflow-test".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            "sales workflow reconciliation",
        )
        .expect("valid workflow context");
        let customer = CustomerMasterReference::new(
            Uuid::from_u128(402),
            scope.clone(),
            holding_id,
            company_id,
            None,
        )
        .expect("valid customer reference");
        let opportunity = Opportunity::new(
            Uuid::from_u128(403),
            scope,
            holding_id,
            company_id,
            None,
            Some(Uuid::from_u128(404)),
            customer,
            "Missing lead",
            "THB",
            100,
            Utc::now(),
            &context,
        )
        .expect("valid opportunity");
        let key = ("opportunity".into(), opportunity.id);
        let source = HashMap::from([(
            key,
            SalesSourceProjectionFact {
                latest_version: 1,
                latest_event_id: Uuid::from_u128(405),
                aggregate_payload: serde_json::to_value(SalesAggregate::Opportunity(opportunity))
                    .expect("opportunity payload"),
            },
        )]);
        assert_eq!(
            validate_sales_workflow(&source).expect("workflow report"),
            1
        );
    }

    #[test]
    fn source_event_validation_binds_history_to_the_typed_outbox_contract() {
        let aggregate = read_test_aggregate();
        let payload = serde_json::json!({
            "aggregate_type": aggregate.aggregate_type(),
            "aggregate": aggregate,
            "version": 1,
        });
        assert!(
            validate_source_event_payload(
                Uuid::from_u128(406),
                "sales.lead.v1",
                "lead",
                aggregate.aggregate_id(),
                1,
                payload.clone(),
            )
            .is_ok()
        );
        assert!(
            validate_source_event_payload(
                Uuid::from_u128(406),
                "sales.lead.v1",
                "lead",
                aggregate.aggregate_id(),
                2,
                payload,
            )
            .is_err()
        );
    }

    #[test]
    fn reconciliation_rejects_same_version_payload_and_checkpoint_drift() {
        let aggregate_id = Uuid::from_u128(302);
        let event_id = Uuid::from_u128(303);
        let key = ("lead".to_owned(), aggregate_id);
        let source = HashMap::from([(
            key.clone(),
            SalesSourceProjectionFact {
                latest_version: 2,
                latest_event_id: event_id,
                aggregate_payload: serde_json::json!({
                    "aggregate_type": "lead",
                    "aggregate": {"id": aggregate_id.to_string()}
                }),
            },
        )]);
        let matching = SalesProjectedFact {
            source_version: 2,
            payload: serde_json::json!({
                "aggregate_type": "lead",
                "aggregate": {"id": aggregate_id.to_string()}
            }),
            checkpoint_source_version: Some(2),
            checkpoint_event_id: Some(event_id),
        };
        let projected = HashMap::from([(key.clone(), matching.clone())]);
        assert!(reconcile_sales_projection_facts(1, &source, &projected).is_consistent());

        let payload_drift = HashMap::from([(
            key.clone(),
            SalesProjectedFact {
                payload: serde_json::json!({
                    "aggregate_type": "lead",
                    "aggregate": {"id": Uuid::from_u128(304).to_string()}
                }),
                ..matching.clone()
            },
        )]);
        let payload_report = reconcile_sales_projection_facts(1, &source, &payload_drift);
        assert_eq!(payload_report.invalid_projection_count, 1);
        assert!(!payload_report.is_consistent());

        let checkpoint_drift = HashMap::from([(
            key,
            SalesProjectedFact {
                checkpoint_event_id: Some(Uuid::from_u128(305)),
                ..matching
            },
        )]);
        let checkpoint_report = reconcile_sales_projection_facts(1, &source, &checkpoint_drift);
        assert_eq!(checkpoint_report.invalid_projection_count, 1);
        assert!(!checkpoint_report.is_consistent());
    }
}
