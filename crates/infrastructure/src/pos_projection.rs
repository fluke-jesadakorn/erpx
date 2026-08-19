//! SQLx source-event and typed read-model adapter for POS sales.
//!
//! POS sale rows are authoritative on the holding write database.  This
//! adapter consumes the normalized aggregate outbox and writes only bounded
//! sale, line, and payment facts to the physically separate read database.
//! Aggregate JSON is decoded at the source boundary and is never persisted in
//! the read model.

use chrono::{DateTime, Utc};
use erpx_domain::contracts::AuthorityScope;
use erpx_domain::pos::{
    PaymentMethod, PosEventType, PosProjectionReconciliation, PosScopedReference, Sale, SaleLine,
    SaleStatus,
};
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const POS_PROJECTION_NAME: &str = "pos";
pub const MAX_POS_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_POS_READ_PAGE_SIZE: u32 = 100;
pub const MAX_POS_READ_CHILD_ROWS_PER_SALE: u32 = 100;
const POS_AGGREGATE: &str = "pos_sale";
const MAX_POS_REBUILD_EVENTS: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceProjectionIdentity {
    version: u64,
    event_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SaleScopeIdentity {
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PaymentProjectionSummary {
    count: u64,
    amount_minor: i64,
    source_version_min: u64,
    source_version_max: u64,
    source_event_count: u64,
    source_event_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionRebuildPreparation {
    pub data_profile_key: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub receipts_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PosSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PosSourceEventPage {
    pub events: Vec<PosSourceEvent>,
    pub next_cursor: Option<PosSourceEventCursor>,
    pub has_more: bool,
}

/// Explicit holding scope accepted by the POS read gateway.  A company scope
/// includes its branches; a branch scope is exact.  Platform and subtree
/// scopes are rejected by the transport before this type is constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosSaleReadCursor {
    pub updated_at: DateTime<Utc>,
    pub sale_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleReadLine {
    pub line_id: Uuid,
    pub line_no: i32,
    pub item_reference: String,
    pub description: String,
    pub quantity: i64,
    pub unit_price_minor: i64,
    pub tax_minor: i64,
    pub subtotal_minor: i64,
    pub total_minor: i64,
    pub product_id: Option<Uuid>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleReadPayment {
    pub payment_id: Uuid,
    pub amount_minor: i64,
    pub method: String,
    pub received_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleReadRow {
    pub sale_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub reference: String,
    pub customer_id: Option<Uuid>,
    pub register_id: Option<Uuid>,
    pub currency: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub paid_minor: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
    pub lines: Vec<PosSaleReadLine>,
    pub payments: Vec<PosSaleReadPayment>,
    pub lines_truncated: bool,
    pub payments_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleReadPage {
    pub rows: Vec<PosSaleReadRow>,
    pub next_cursor: Option<PosSaleReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxPosProjectionStore {
    write_pool: PgPool,
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxPosProjectionStore {
    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool: write_pool.clone(),
            read_pool: read_pool.clone(),
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Read typed POS facts from the physically separate holding read pool.
    /// No source aggregate JSON, idempotency key, command history, or payment
    /// instrument is exposed.  Child rows are independently bounded so one
    /// sale cannot create an unbounded response.
    pub async fn list_sales(
        &self,
        scope: PosReadScope,
        page_size: u32,
        before: Option<&PosSaleReadCursor>,
    ) -> Result<PosSaleReadPage, PersistenceError> {
        validate_read_scope(scope)?;
        validate_read_page_size(page_size)?;
        if let Some(cursor) = before {
            validate_uuid(cursor.sale_id, "POS read cursor sale ID")?;
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let sale_rows = sqlx::query(
            "SELECT sale_id, holding_id, company_id, branch_id, reference,
                    customer_id, register_id, currency, subtotal_minor, tax_minor,
                    total_minor, paid_minor, status, created_at, updated_at,
                    source_version, source_event_id, projected_at
             FROM erpx_holding.pos_sale_read_rows
             WHERE data_profile_key=$1 AND holding_id=$2 AND company_id=$3
               AND ($4::UUID IS NULL OR branch_id=$4)
               AND ($5::TIMESTAMPTZ IS NULL OR (updated_at, sale_id) < ($5, $6))
             ORDER BY updated_at DESC, sale_id DESC
             LIMIT $7",
        )
        .bind(&self.data_profile_key)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(scope.branch_id)
        .bind(before.map(|cursor| cursor.updated_at))
        .bind(before.map(|cursor| cursor.sale_id))
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let page_len = usize::try_from(page_size)
            .map_err(|_| invalid("POS read page size is not representable"))?;
        let has_more = sale_rows.len() > page_len;
        let selected = sale_rows.into_iter().take(page_len).collect::<Vec<_>>();
        let mut rows = Vec::with_capacity(selected.len());
        for sale_row in selected {
            let sale_id: Uuid = sale_row.try_get("sale_id")?;
            let lines = sqlx::query(
                "SELECT line_id, line_no, item_reference, description, quantity,
                        unit_price_minor, tax_minor, subtotal_minor, total_minor,
                        product_id, source_version, source_event_id, projected_at
                 FROM erpx_holding.pos_sale_line_read_rows
                 WHERE data_profile_key=$1 AND sale_id=$2
                 ORDER BY line_no
                 LIMIT $3",
            )
            .bind(&self.data_profile_key)
            .bind(sale_id)
            .bind(i64::from(MAX_POS_READ_CHILD_ROWS_PER_SALE) + 1)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(decode_sale_line)
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            let payments = sqlx::query(
                "SELECT payment_id, amount_minor, method, received_at,
                        source_version, source_event_id, projected_at
                 FROM erpx_holding.pos_sale_payment_read_rows
                 WHERE data_profile_key=$1 AND sale_id=$2
                 ORDER BY received_at, payment_id
                 LIMIT $3",
            )
            .bind(&self.data_profile_key)
            .bind(sale_id)
            .bind(i64::from(MAX_POS_READ_CHILD_ROWS_PER_SALE) + 1)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(decode_sale_payment)
            .collect::<Result<Vec<_>, PersistenceError>>()?;
            let lines_truncated = lines.len() > MAX_POS_READ_CHILD_ROWS_PER_SALE as usize;
            let payments_truncated = payments.len() > MAX_POS_READ_CHILD_ROWS_PER_SALE as usize;
            let mut lines = lines;
            let mut payments = payments;
            lines.truncate(MAX_POS_READ_CHILD_ROWS_PER_SALE as usize);
            payments.truncate(MAX_POS_READ_CHILD_ROWS_PER_SALE as usize);
            rows.push(PosSaleReadRow {
                sale_id,
                holding_id: sale_row.try_get("holding_id")?,
                company_id: sale_row.try_get("company_id")?,
                branch_id: sale_row.try_get("branch_id")?,
                reference: sale_row.try_get("reference")?,
                customer_id: sale_row.try_get("customer_id")?,
                register_id: sale_row.try_get("register_id")?,
                currency: sale_row.try_get("currency")?,
                subtotal_minor: sale_row.try_get("subtotal_minor")?,
                tax_minor: sale_row.try_get("tax_minor")?,
                total_minor: sale_row.try_get("total_minor")?,
                paid_minor: sale_row.try_get("paid_minor")?,
                status: sale_row.try_get("status")?,
                created_at: sale_row.try_get("created_at")?,
                updated_at: sale_row.try_get("updated_at")?,
                source_version: as_u64(
                    sale_row.try_get("source_version")?,
                    "POS read source version",
                )?,
                source_event_id: sale_row.try_get("source_event_id")?,
                projected_at: sale_row.try_get("projected_at")?,
                lines,
                payments,
                lines_truncated,
                payments_truncated,
            });
        }
        tx.commit().await?;
        let next_cursor = if has_more {
            rows.last().map(|row| PosSaleReadCursor {
                updated_at: row.updated_at,
                sale_id: row.sale_id,
            })
        } else {
            None
        };
        Ok(PosSaleReadPage {
            rows,
            next_cursor,
            has_more,
        })
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&PosSourceEventCursor>,
        page_size: u32,
    ) -> Result<PosSourceEventPage, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let page = read_source_event_page_in_transaction(
            &mut tx,
            &self.data_profile_key,
            cursor,
            page_size,
        )
        .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Return the holding-scoped source-event count used to prove that a
    /// bounded incremental drain did not race an append-only source stream.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        self.source_event_count_after_cursor(None).await
    }

    /// Count the validated source events after an optional cursor without
    /// touching the disposable read database. Workers use this as a
    /// preflight so an oversized drain cannot partially advance the read
    /// model before its event budget is rejected.
    pub async fn source_event_count_after_cursor(
        &self,
        cursor: Option<&PosSourceEventCursor>,
    ) -> Result<u64, PersistenceError> {
        if let Some(cursor) = cursor {
            validate_source_cursor(cursor)?;
        }
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        validate_source_event_versions(&mut tx, &self.data_profile_key).await?;
        let count: i64 = if let Some(cursor) = cursor {
            sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type=$2
                   AND (aggregate_id > $3
                        OR (aggregate_id = $3 AND aggregate_version > $4))",
            )
            .bind(&self.data_profile_key)
            .bind(POS_AGGREGATE)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "POS source cursor version",
            )?)
            .fetch_one(&mut *tx)
            .await?
        } else {
            sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type=$2",
            )
            .bind(&self.data_profile_key)
            .bind(POS_AGGREGATE)
            .fetch_one(&mut *tx)
            .await?
        };
        tx.commit().await?;
        as_u64(count, "POS source event high-watermark")
    }

    /// Rebuild the disposable POS read model inside one read transaction while
    /// the authoritative source outbox is locked against movement. Any decode,
    /// version, or SQL failure drops the read transaction and preserves the
    /// previously committed read model.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(PosProjectionRebuildPreparation, u64), PersistenceError> {
        validate_page_size(page_size)?;
        let mut source_tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        sqlx::query("LOCK TABLE erpx_holding.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_version: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type=$2",
        )
        .bind(&self.data_profile_key)
        .bind(POS_AGGREGATE)
        .fetch_one(&mut *source_tx)
        .await?;
        let source_version = as_u64(source_version, "POS source event high-watermark")?;
        if source_version > MAX_POS_REBUILD_EVENTS {
            return Err(invalid("POS rebuild event bound exceeded"));
        }
        if expected_source_version.is_some_and(|expected| expected != source_version) {
            return Err(invalid("POS source fence changed before rebuild"));
        }
        validate_source_event_versions(&mut source_tx, &self.data_profile_key).await?;
        verify_authoritative_source_snapshots_in_transaction(
            &mut source_tx,
            &self.data_profile_key,
        )
        .await?;

        let mut read_tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let preparation =
            clear_projection_in_transaction(&mut read_tx, &self.data_profile_key).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_source_event_page_in_transaction(
                &mut source_tx,
                &self.data_profile_key,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                let sale = decode_sale(event)?;
                let values = projection_values(&sale)?;
                if self
                    .project_event_in_transaction(&mut read_tx, event, &sale, &values)
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
                return Err(invalid("POS source page omitted its continuation cursor"));
            }
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    /// Compare the authoritative POS outbox with the disposable projection
    /// checkpoints, including the immutable latest-event identity. The result
    /// is a read-only, holding-scoped signal for the worker/operator
    /// reconciliation path.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<PosProjectionReconciliation, PersistenceError> {
        let mut source_tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        validate_source_event_versions(&mut source_tx, &self.data_profile_key).await?;
        verify_authoritative_source_snapshots_in_transaction(
            &mut source_tx,
            &self.data_profile_key,
        )
        .await?;
        let source_rows = sqlx::query(
            "WITH ranked_source AS (
                 SELECT aggregate_id, aggregate_version, event_id,
                        COUNT(*) OVER (PARTITION BY aggregate_id) AS event_count,
                        ROW_NUMBER() OVER (
                            PARTITION BY aggregate_id
                            ORDER BY aggregate_version DESC, event_id DESC
                        ) AS latest_rank
                 FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type=$2
             )
             SELECT aggregate_id, aggregate_version AS latest_version,
                    event_id AS latest_event_id, event_count
             FROM ranked_source
             WHERE latest_rank=1",
        )
        .bind(&self.data_profile_key)
        .bind(POS_AGGREGATE)
        .fetch_all(&mut *source_tx)
        .await?;
        let source_scope_rows = sqlx::query(
            "SELECT sale_id, holding_id, company_id, branch_id
             FROM erpx_holding.pos_sales
             WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source = HashMap::with_capacity(source_rows.len());
        let mut source_event_count = 0_u64;
        for row in source_rows {
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let latest_version =
                u64::try_from(row.try_get::<i64, _>("latest_version")?).map_err(|_| {
                    PersistenceError::InvalidData("POS source version is negative".into())
                })?;
            let latest_event_id: Uuid = row.try_get("latest_event_id")?;
            validate_uuid(latest_event_id, "POS source event ID")?;
            let event_count =
                u64::try_from(row.try_get::<i64, _>("event_count")?).map_err(|_| {
                    PersistenceError::InvalidData("POS source event count is negative".into())
                })?;
            source_event_count = source_event_count
                .checked_add(event_count)
                .ok_or_else(|| PersistenceError::InvalidData("POS event count overflow".into()))?;
            source.insert(
                aggregate_id,
                SourceProjectionIdentity {
                    version: latest_version,
                    event_id: latest_event_id,
                },
            );
        }
        let source_scopes = source_scope_rows
            .into_iter()
            .map(|row| {
                let sale_id: Uuid = row.try_get("sale_id")?;
                let holding_id: Uuid = row.try_get("holding_id")?;
                let company_id: Uuid = row.try_get("company_id")?;
                validate_uuid(sale_id, "POS source sale ID")?;
                validate_uuid(holding_id, "POS source sale holding ID")?;
                validate_uuid(company_id, "POS source sale company ID")?;
                Ok::<_, PersistenceError>((
                    sale_id,
                    SaleScopeIdentity {
                        holding_id,
                        company_id,
                        branch_id: row.try_get("branch_id")?,
                    },
                ))
            })
            .collect::<Result<HashMap<Uuid, SaleScopeIdentity>, _>>()?;
        if source.len() != source_scopes.len()
            || source
                .keys()
                .any(|sale_id| !source_scopes.contains_key(sale_id))
        {
            return Err(invalid(
                "POS source normalized sales and events are inconsistent",
            ));
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_id, source_version, last_event_id
             FROM erpx_holding.pos_projection_checkpoints
             WHERE data_profile_key=$1 AND projection_name=$2",
        )
        .bind(&self.data_profile_key)
        .bind(POS_PROJECTION_NAME)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let projected = read_rows
            .into_iter()
            .map(|row| {
                let version =
                    u64::try_from(row.try_get::<i64, _>("source_version")?).map_err(|_| {
                        PersistenceError::InvalidData("POS projected version is negative".into())
                    })?;
                let event_id: Uuid = row.try_get("last_event_id")?;
                validate_uuid(event_id, "POS projected event ID")?;
                Ok::<_, PersistenceError>((
                    row.try_get("aggregate_id")?,
                    SourceProjectionIdentity { version, event_id },
                ))
            })
            .collect::<Result<HashMap<Uuid, SourceProjectionIdentity>, _>>()?;

        // A checkpoint is only a cursor.  It does not prove that the
        // materialized sale row survived or still carries the source identity
        // that the checkpoint claims to have applied.  Verify the row itself
        // so a deleted or forged read fact cannot make reconciliation appear
        // healthy.
        let mut read_tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let read_sale_rows = sqlx::query(
            "SELECT sale_id, holding_id, company_id, branch_id,
                    source_version, source_event_id
             FROM erpx_holding.pos_sale_read_rows
             WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let mut materialized = HashMap::with_capacity(read_sale_rows.len());
        let mut materialized_scopes = HashMap::with_capacity(read_sale_rows.len());
        for row in read_sale_rows {
            let sale_id: Uuid = row.try_get("sale_id")?;
            let event_id: Uuid = row.try_get("source_event_id")?;
            let holding_id: Uuid = row.try_get("holding_id")?;
            let company_id: Uuid = row.try_get("company_id")?;
            validate_uuid(sale_id, "POS materialized sale ID")?;
            validate_uuid(event_id, "POS materialized event ID")?;
            validate_uuid(holding_id, "POS materialized sale holding ID")?;
            validate_uuid(company_id, "POS materialized sale company ID")?;
            materialized.insert(
                sale_id,
                SourceProjectionIdentity {
                    version: as_u64(
                        row.try_get("source_version")?,
                        "POS materialized source version",
                    )?,
                    event_id,
                },
            );
            materialized_scopes.insert(
                sale_id,
                SaleScopeIdentity {
                    holding_id,
                    company_id,
                    branch_id: row.try_get("branch_id")?,
                },
            );
        }
        if materialized_scopes
            .keys()
            .any(|sale_id| !source.contains_key(sale_id))
        {
            return Err(invalid("POS materialized sale scope contains an orphan"));
        }
        for (sale_id, source_scope) in &source_scopes {
            if let Some(materialized_scope) = materialized_scopes.get(sale_id)
                && materialized_scope != source_scope
            {
                return Err(invalid(
                    "POS materialized sale scope is stale or inconsistent",
                ));
            }
        }

        // A sale checkpoint and header can still look current while a payment
        // child row is missing, duplicated, or carries a stale source
        // identity.  Payment totals are authoritative on the write database,
        // so compare the normalized payment facts on both sides before
        // reporting the projection as healthy.
        self.reconcile_payment_facts(&source).await?;

        let missing_aggregate_count = source
            .keys()
            .filter(|key| !projected.contains_key(*key))
            .count() as u64;
        let stale_aggregate_count = source
            .iter()
            .filter(|(key, source_identity)| {
                let checkpoint_is_stale = projected.get(*key).is_some_and(|projected_identity| {
                    projected_identity.version != source_identity.version
                        || projected_identity.event_id != source_identity.event_id
                });
                let read_row_is_stale = projected.get(*key).is_some_and(|projected_identity| {
                    projected_identity == *source_identity
                        && !materialized.get(*key).is_some_and(|materialized_identity| {
                            materialized_identity == *source_identity
                        })
                });
                checkpoint_is_stale || read_row_is_stale
            })
            .count() as u64;
        let orphan_aggregate_count = projected
            .keys()
            .filter(|key| !source.contains_key(*key))
            .count() as u64;
        Ok(PosProjectionReconciliation {
            source_event_count,
            source_aggregate_count: source.len() as u64,
            projected_aggregate_count: projected.len() as u64,
            missing_aggregate_count,
            stale_aggregate_count,
            orphan_aggregate_count,
        })
    }

    async fn reconcile_payment_facts(
        &self,
        source_events: &HashMap<Uuid, SourceProjectionIdentity>,
    ) -> Result<(), PersistenceError> {
        let mut source_tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let source_sales = sqlx::query(
            "SELECT sale_id, paid_minor
             FROM erpx_holding.pos_sales
             WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *source_tx)
        .await?;
        let source_payments = sqlx::query(
            "SELECT sale_id, COUNT(*)::BIGINT AS payment_count,
                    COALESCE(SUM(amount_minor), 0)::BIGINT AS amount_minor
             FROM erpx_holding.pos_sale_payments
             WHERE data_profile_key=$1
             GROUP BY sale_id",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *source_tx)
        .await?;
        source_tx.commit().await?;

        let mut source_paid = HashMap::with_capacity(source_sales.len());
        for row in source_sales {
            let sale_id: Uuid = row.try_get("sale_id")?;
            validate_uuid(sale_id, "POS source payment sale ID")?;
            let paid_minor: i64 = row.try_get("paid_minor")?;
            if paid_minor < 0 {
                return Err(invalid("POS source paid amount is negative"));
            }
            source_paid.insert(sale_id, paid_minor);
        }
        let mut source_payment_facts = HashMap::with_capacity(source_payments.len());
        for row in source_payments {
            let sale_id: Uuid = row.try_get("sale_id")?;
            validate_uuid(sale_id, "POS source payment sale ID")?;
            source_payment_facts.insert(
                sale_id,
                PaymentProjectionSummary {
                    count: as_u64(row.try_get("payment_count")?, "POS source payment count")?,
                    amount_minor: row.try_get("amount_minor")?,
                    ..PaymentProjectionSummary::default()
                },
            );
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let read_sales = sqlx::query(
            "SELECT sale_id, paid_minor
             FROM erpx_holding.pos_sale_read_rows
             WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *read_tx)
        .await?;
        let read_payments = sqlx::query(
            "SELECT sale_id, COUNT(*)::BIGINT AS payment_count,
                    COALESCE(SUM(amount_minor), 0)::BIGINT AS amount_minor,
                    MIN(source_version)::BIGINT AS source_version_min,
                    MAX(source_version)::BIGINT AS source_version_max,
                    COUNT(DISTINCT source_event_id)::BIGINT AS source_event_count,
                    MIN(source_event_id::TEXT) AS source_event_id
             FROM erpx_holding.pos_sale_payment_read_rows
             WHERE data_profile_key=$1
             GROUP BY sale_id",
        )
        .bind(&self.data_profile_key)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;

        let mut read_paid = HashMap::with_capacity(read_sales.len());
        for row in read_sales {
            let sale_id: Uuid = row.try_get("sale_id")?;
            validate_uuid(sale_id, "POS projected payment sale ID")?;
            let paid_minor: i64 = row.try_get("paid_minor")?;
            if paid_minor < 0 {
                return Err(invalid("POS projected paid amount is negative"));
            }
            read_paid.insert(sale_id, paid_minor);
        }
        let mut read_payment_facts = HashMap::with_capacity(read_payments.len());
        for row in read_payments {
            let sale_id: Uuid = row.try_get("sale_id")?;
            validate_uuid(sale_id, "POS projected payment sale ID")?;
            let source_event_id = row
                .try_get::<Option<String>, _>("source_event_id")?
                .map(|value| {
                    Uuid::parse_str(&value)
                        .map_err(|_| invalid("POS projected payment source event ID is invalid"))
                })
                .transpose()?;
            read_payment_facts.insert(
                sale_id,
                PaymentProjectionSummary {
                    count: as_u64(row.try_get("payment_count")?, "POS projected payment count")?,
                    amount_minor: row.try_get("amount_minor")?,
                    source_version_min: as_u64(
                        row.try_get("source_version_min")?,
                        "POS projected payment source version",
                    )?,
                    source_version_max: as_u64(
                        row.try_get("source_version_max")?,
                        "POS projected payment source version",
                    )?,
                    source_event_count: as_u64(
                        row.try_get("source_event_count")?,
                        "POS projected payment source event count",
                    )?,
                    source_event_id,
                },
            );
        }

        for sale_id in source_paid.keys() {
            let source_identity = source_events
                .get(sale_id)
                .ok_or_else(|| invalid("POS source sale has no authoritative source event"))?;
            let source_paid_minor = source_paid[sale_id];
            let source_payment = source_payment_facts
                .get(sale_id)
                .copied()
                .unwrap_or_default();
            let read_paid_minor = read_paid.get(sale_id).copied().ok_or_else(|| {
                invalid("POS projected sale with payments is missing its sale row")
            })?;
            let read_payment = read_payment_facts.get(sale_id).copied().unwrap_or_default();

            if source_paid_minor != source_payment.amount_minor {
                return Err(invalid(
                    "POS source paid total disagrees with source payments",
                ));
            }
            if read_paid_minor != read_payment.amount_minor {
                return Err(invalid(
                    "POS projected paid total disagrees with projected payments",
                ));
            }
            if source_payment.count != read_payment.count
                || source_payment.amount_minor != read_payment.amount_minor
            {
                return Err(invalid("POS projected payment facts disagree with source"));
            }
            if read_payment.count > 0
                && (read_payment.source_version_min != source_identity.version
                    || read_payment.source_version_max != source_identity.version
                    || read_payment.source_event_count != 1
                    || read_payment.source_event_id != Some(source_identity.event_id))
            {
                return Err(invalid(
                    "POS projected payment source identity is stale or inconsistent",
                ));
            }
        }

        if read_paid
            .keys()
            .any(|sale_id| !source_paid.contains_key(sale_id))
            || read_payment_facts
                .keys()
                .any(|sale_id| !source_paid.contains_key(sale_id))
        {
            return Err(invalid(
                "POS projected payment facts contain an orphan sale",
            ));
        }
        Ok(())
    }

    pub async fn project_event(
        &self,
        event: &PosSourceEvent,
    ) -> Result<PosProjectionApplication, PersistenceError> {
        validate_uuid(event.event_id, "POS event ID")?;
        validate_uuid(event.aggregate_id, "POS aggregate ID")?;
        validate_event_type(&event.event_type)?;
        if event.aggregate_version == 0 {
            return Err(invalid("POS source event version is invalid"));
        }
        self.verify_authoritative_source_event(event).await?;
        let sale = decode_sale(event)?;
        self.verify_authoritative_source_sale(&sale).await?;
        let values = projection_values(&sale)?;

        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let result = self
            .project_event_in_transaction(&mut tx, event, &sale, &values)
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn project_event_in_transaction(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: &PosSourceEvent,
        sale: &Sale,
        values: &ProjectionValues,
    ) -> Result<PosProjectionApplication, PersistenceError> {
        if let Some(receipt) = sqlx::query(
            "SELECT aggregate_id, aggregate_version
             FROM erpx_holding.pos_projection_event_receipts
             WHERE data_profile_key=$1 AND projection_name=$2 AND event_id=$3
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(POS_PROJECTION_NAME)
        .bind(event.event_id)
        .fetch_optional(&mut **tx)
        .await?
        {
            let receipt_aggregate_id: Uuid = receipt.try_get("aggregate_id")?;
            let receipt_version =
                as_u64(receipt.try_get("aggregate_version")?, "POS receipt version")?;
            if receipt_aggregate_id != event.aggregate_id
                || receipt_version != event.aggregate_version
            {
                return Err(invalid("POS projection event receipt identity conflicts"));
            }
            return Ok(PosProjectionApplication {
                event_id: event.event_id,
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
                applied: false,
            });
        }

        let checkpoint = sqlx::query(
            "SELECT last_event_id, source_version
             FROM erpx_holding.pos_projection_checkpoints
             WHERE data_profile_key=$1 AND projection_name=$2 AND aggregate_id=$3
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(POS_PROJECTION_NAME)
        .bind(event.aggregate_id)
        .fetch_optional(&mut **tx)
        .await?;
        let checkpoint_decision = if let Some(checkpoint) = checkpoint {
            let current_event_id: Uuid = checkpoint.try_get("last_event_id")?;
            let current_version = as_u64(
                checkpoint.try_get("source_version")?,
                "POS checkpoint version",
            )?;
            checkpoint_decision(
                Some((current_version, current_event_id)),
                event.aggregate_version,
                event.event_id,
            )?
        } else if event.aggregate_version != 1 {
            return Err(invalid("POS projection must start at version one"));
        } else {
            CheckpointDecision::Apply
        };
        if checkpoint_decision == CheckpointDecision::Ignore {
            insert_receipt(tx, &self.data_profile_key, event).await?;
            return Ok(PosProjectionApplication {
                event_id: event.event_id,
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
                applied: false,
            });
        }

        sqlx::query(
            "DELETE FROM erpx_holding.pos_sale_line_read_rows
             WHERE data_profile_key=$1 AND sale_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(event.aggregate_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "DELETE FROM erpx_holding.pos_sale_payment_read_rows
             WHERE data_profile_key=$1 AND sale_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(event.aggregate_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_holding.pos_sale_read_rows
                (data_profile_key, sale_id, holding_id, company_id, branch_id, reference,
                 customer_id, customer_scope_kind, customer_scope_holding_id,
                 customer_scope_company_id, customer_scope_branch_id,
                 register_id, register_scope_kind, register_scope_holding_id,
                 register_scope_company_id, register_scope_branch_id, currency,
                 subtotal_minor, tax_minor, total_minor, paid_minor, status,
                 command_id, idempotency_key, created_at, updated_at,
                 source_version, source_event_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                     $17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28)
             ON CONFLICT (data_profile_key, sale_id) DO UPDATE SET
                 holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
                 branch_id=EXCLUDED.branch_id, reference=EXCLUDED.reference,
                 customer_id=EXCLUDED.customer_id,
                 customer_scope_kind=EXCLUDED.customer_scope_kind,
                 customer_scope_holding_id=EXCLUDED.customer_scope_holding_id,
                 customer_scope_company_id=EXCLUDED.customer_scope_company_id,
                 customer_scope_branch_id=EXCLUDED.customer_scope_branch_id,
                 register_id=EXCLUDED.register_id,
                 register_scope_kind=EXCLUDED.register_scope_kind,
                 register_scope_holding_id=EXCLUDED.register_scope_holding_id,
                 register_scope_company_id=EXCLUDED.register_scope_company_id,
                 register_scope_branch_id=EXCLUDED.register_scope_branch_id,
                 currency=EXCLUDED.currency, subtotal_minor=EXCLUDED.subtotal_minor,
                 tax_minor=EXCLUDED.tax_minor, total_minor=EXCLUDED.total_minor,
                 paid_minor=EXCLUDED.paid_minor, status=EXCLUDED.status,
                 command_id=EXCLUDED.command_id, idempotency_key=EXCLUDED.idempotency_key,
                 created_at=EXCLUDED.created_at, updated_at=EXCLUDED.updated_at,
                 source_version=EXCLUDED.source_version,
                 source_event_id=EXCLUDED.source_event_id, projected_at=now()",
        )
        .bind(&self.data_profile_key)
        .bind(sale_id(sale))
        .bind(values.holding_id)
        .bind(values.company_id)
        .bind(values.branch_id)
        .bind(&values.reference)
        .bind(values.customer.id)
        .bind(values.customer.kind)
        .bind(values.customer.holding_id)
        .bind(values.customer.company_id)
        .bind(values.customer.branch_id)
        .bind(values.register.id)
        .bind(values.register.kind)
        .bind(values.register.holding_id)
        .bind(values.register.company_id)
        .bind(values.register.branch_id)
        .bind(&values.currency)
        .bind(values.subtotal_minor)
        .bind(values.tax_minor)
        .bind(values.total_minor)
        .bind(values.paid_minor)
        .bind(values.status)
        .bind(values.command_id)
        .bind(&values.idempotency_key)
        .bind(values.created_at)
        .bind(values.updated_at)
        .bind(as_i64(event.aggregate_version, "POS source version")?)
        .bind(event.event_id)
        .execute(&mut **tx)
        .await?;

        for (line_no, line) in sale.lines.iter().enumerate() {
            let line_values = line_values(line, line_no)?;
            sqlx::query(
                "INSERT INTO erpx_holding.pos_sale_line_read_rows
                    (data_profile_key, sale_id, line_id, line_no, item_reference,
                     description, quantity, unit_price_minor, tax_minor,
                     subtotal_minor, total_minor, product_id, product_scope_kind,
                     product_scope_holding_id, product_scope_company_id,
                     product_scope_branch_id, source_version, source_event_id)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)",
            )
            .bind(&self.data_profile_key)
            .bind(sale_id(sale))
            .bind(line_values.line_id)
            .bind(line_values.line_no)
            .bind(line_values.item_reference)
            .bind(line_values.description)
            .bind(line_values.quantity)
            .bind(line_values.unit_price_minor)
            .bind(line_values.tax_minor)
            .bind(line_values.subtotal_minor)
            .bind(line_values.total_minor)
            .bind(line_values.product.id)
            .bind(line_values.product.kind)
            .bind(line_values.product.holding_id)
            .bind(line_values.product.company_id)
            .bind(line_values.product.branch_id)
            .bind(as_i64(event.aggregate_version, "POS source version")?)
            .bind(event.event_id)
            .execute(&mut **tx)
            .await?;
        }
        for payment in &sale.payments {
            sqlx::query(
                "INSERT INTO erpx_holding.pos_sale_payment_read_rows
                    (data_profile_key, sale_id, payment_id, amount_minor, method,
                     received_at, source_version, source_event_id)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
            )
            .bind(&self.data_profile_key)
            .bind(sale_id(sale))
            .bind(payment.payment_id)
            .bind(payment.amount_minor)
            .bind(payment_method(payment.method))
            .bind(payment.received_at)
            .bind(as_i64(event.aggregate_version, "POS source version")?)
            .bind(event.event_id)
            .execute(&mut **tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO erpx_holding.pos_projection_checkpoints
                (data_profile_key, projection_name, aggregate_id, holding_id,
                 company_id, branch_id, last_event_id, source_version)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
             ON CONFLICT (data_profile_key, projection_name, aggregate_id) DO UPDATE SET
                 holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
                 branch_id=EXCLUDED.branch_id, last_event_id=EXCLUDED.last_event_id,
                 source_version=EXCLUDED.source_version, projected_at=now()",
        )
        .bind(&self.data_profile_key)
        .bind(POS_PROJECTION_NAME)
        .bind(sale_id(sale))
        .bind(values.holding_id)
        .bind(values.company_id)
        .bind(values.branch_id)
        .bind(event.event_id)
        .bind(as_i64(event.aggregate_version, "POS source version")?)
        .execute(&mut **tx)
        .await?;
        insert_receipt(tx, &self.data_profile_key, event).await?;
        Ok(PosProjectionApplication {
            event_id: event.event_id,
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
            applied: true,
        })
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<PosProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.data_profile_key).await?;
        tx.commit().await?;
        Ok(preparation)
    }

    async fn verify_authoritative_source_sale(&self, sale: &Sale) -> Result<(), PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        verify_authoritative_source_sale_in_transaction(
            &mut tx,
            &self.data_profile_key,
            sale,
            None,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The event object is supplied by a worker boundary, so it must be
    /// compared with the immutable write-side outbox before any read-side
    /// receipt or normalized row is written.
    async fn verify_authoritative_source_event(
        &self,
        event: &PosSourceEvent,
    ) -> Result<(), PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let row = sqlx::query(
            "SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM erpx_holding.outbox_events
             WHERE event_id=$1 AND data_profile_key=$2",
        )
        .bind(event.event_id)
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let Some(row) = row else {
            return Err(invalid("POS projection event is not authoritative"));
        };
        let source_event_type: String = row.try_get("event_type")?;
        let source_aggregate_type: String = row.try_get("aggregate_type")?;
        let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
        let source_version = as_u64(row.try_get("aggregate_version")?, "POS source version")?;
        let source_payload: Value = row.try_get("payload")?;
        let source_occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
        if source_event_type != event.event_type
            || source_aggregate_type != POS_AGGREGATE
            || source_aggregate_id != event.aggregate_id
            || source_version != event.aggregate_version
            || source_payload != event.payload
            || source_occurred_at != event.occurred_at
        {
            return Err(invalid(
                "POS projection event does not match the authoritative outbox",
            ));
        }
        Ok(())
    }
}

async fn verify_authoritative_source_snapshots_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
) -> Result<(), PersistenceError> {
    let rows = sqlx::query(
        "SELECT DISTINCT ON (aggregate_id)
                event_id, event_type, aggregate_id, aggregate_version,
                payload, occurred_at
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2
         ORDER BY aggregate_id, aggregate_version DESC, event_id DESC",
    )
    .bind(data_profile_key)
    .bind(POS_AGGREGATE)
    .fetch_all(&mut **tx)
    .await?;
    for row in rows {
        let event = PosSourceEvent {
            event_id: row.try_get("event_id")?,
            event_type: row.try_get("event_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            aggregate_version: as_u64(
                row.try_get("aggregate_version")?,
                "POS source snapshot version",
            )?,
            payload: row.try_get("payload")?,
            occurred_at: row.try_get("occurred_at")?,
        };
        validate_uuid(event.event_id, "POS source snapshot event ID")?;
        validate_uuid(event.aggregate_id, "POS source snapshot aggregate ID")?;
        validate_event_type(&event.event_type)?;
        let sale = decode_sale(&event)?;
        verify_authoritative_source_sale_in_transaction(
            tx,
            data_profile_key,
            &sale,
            Some(event.aggregate_version),
        )
        .await?;
    }
    Ok(())
}

async fn verify_authoritative_source_sale_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
    sale: &Sale,
    expected_current_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let values = projection_values(sale)?;
    let row = sqlx::query(
        "SELECT version, holding_id, company_id, branch_id, reference,
                customer_id, customer_scope_kind, customer_scope_holding_id,
                customer_scope_company_id, customer_scope_branch_id,
                register_id, register_scope_kind, register_scope_holding_id,
                register_scope_company_id, register_scope_branch_id, currency,
                subtotal_minor, tax_minor, total_minor, paid_minor, status,
                command_id, idempotency_key, created_at, updated_at
         FROM erpx_holding.pos_sales
         WHERE data_profile_key=$1 AND sale_id=$2
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(sale.id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("POS source sale snapshot is missing"))?;
    let source_version = as_u64(row.try_get("version")?, "POS source snapshot version")?;
    if source_version < sale.version {
        return Err(invalid("POS source sale snapshot is behind its event"));
    }
    if expected_current_version.is_some_and(|version| version != source_version) {
        return Err(invalid("POS source sale snapshot version is not current"));
    }
    let source_scope = (
        row.try_get::<Uuid, _>("holding_id")?,
        row.try_get::<Uuid, _>("company_id")?,
        row.try_get::<Option<Uuid>, _>("branch_id")?,
    );
    if source_scope != (values.holding_id, values.company_id, values.branch_id) {
        return Err(invalid("POS source sale scope disagrees with its event"));
    }
    if source_version != sale.version {
        return Ok(());
    }
    if row.try_get::<String, _>("reference")? != values.reference
        || row.try_get::<String, _>("currency")? != values.currency
        || row.try_get::<i64, _>("subtotal_minor")? != values.subtotal_minor
        || row.try_get::<i64, _>("tax_minor")? != values.tax_minor
        || row.try_get::<i64, _>("total_minor")? != values.total_minor
        || row.try_get::<i64, _>("paid_minor")? != values.paid_minor
        || row.try_get::<String, _>("status")? != values.status
        || row.try_get::<Uuid, _>("command_id")? != values.command_id
        || row.try_get::<String, _>("idempotency_key")? != values.idempotency_key
        || row.try_get::<DateTime<Utc>, _>("created_at")? != values.created_at
        || row.try_get::<DateTime<Utc>, _>("updated_at")? != values.updated_at
        || !reference_columns_match(&row, "customer", values.customer)?
        || !reference_columns_match(&row, "register", values.register)?
    {
        return Err(invalid("POS source sale snapshot disagrees with its event"));
    }

    let line_rows = sqlx::query(
        "SELECT line_id, line_no, item_reference, description, quantity,
                unit_price_minor, tax_minor, subtotal_minor, total_minor,
                product_id, product_scope_kind, product_scope_holding_id,
                product_scope_company_id, product_scope_branch_id
         FROM erpx_holding.pos_sale_lines
         WHERE data_profile_key=$1 AND sale_id=$2
         ORDER BY line_no",
    )
    .bind(data_profile_key)
    .bind(sale.id)
    .fetch_all(&mut **tx)
    .await?;
    if line_rows.len() != sale.lines.len() {
        return Err(invalid("POS source sale line snapshot is inconsistent"));
    }
    for (index, row) in line_rows.iter().enumerate() {
        let expected = line_values(&sale.lines[index], index)?;
        if row.try_get::<Uuid, _>("line_id")? != expected.line_id
            || row.try_get::<i32, _>("line_no")? != expected.line_no
            || row.try_get::<String, _>("item_reference")? != expected.item_reference
            || row.try_get::<String, _>("description")? != expected.description
            || row.try_get::<i64, _>("quantity")? != expected.quantity
            || row.try_get::<i64, _>("unit_price_minor")? != expected.unit_price_minor
            || row.try_get::<i64, _>("tax_minor")? != expected.tax_minor
            || row.try_get::<i64, _>("subtotal_minor")? != expected.subtotal_minor
            || row.try_get::<i64, _>("total_minor")? != expected.total_minor
            || !reference_columns_match(row, "product", expected.product)?
        {
            return Err(invalid(
                "POS source sale line snapshot disagrees with its event",
            ));
        }
    }

    let payment_rows = sqlx::query(
        "SELECT payment_id, amount_minor, method, received_at
         FROM erpx_holding.pos_sale_payments
         WHERE data_profile_key=$1 AND sale_id=$2
         ORDER BY payment_id",
    )
    .bind(data_profile_key)
    .bind(sale.id)
    .fetch_all(&mut **tx)
    .await?;
    if payment_rows.len() != sale.payments.len()
        || payment_rows.iter().any(|row| {
            let payment_id = row.try_get::<Uuid, _>("payment_id");
            let amount = row.try_get::<i64, _>("amount_minor");
            let method = row.try_get::<String, _>("method");
            let received_at = row.try_get::<DateTime<Utc>, _>("received_at");
            match (payment_id, amount, method, received_at) {
                (Ok(payment_id), Ok(amount), Ok(method), Ok(received_at)) => {
                    !sale.payments.iter().any(|payment| {
                        payment.payment_id == payment_id
                            && payment.amount_minor == amount
                            && payment_method(payment.method) == method
                            && payment.received_at == received_at
                    })
                }
                _ => true,
            }
        })
    {
        return Err(invalid(
            "POS source sale payment snapshot disagrees with its event",
        ));
    }
    Ok(())
}

fn reference_columns_match(
    row: &sqlx::postgres::PgRow,
    prefix: &str,
    expected: ReferenceColumns,
) -> Result<bool, PersistenceError> {
    let id = row.try_get::<Option<Uuid>, _>(format!("{prefix}_id").as_str())?;
    let kind = row.try_get::<Option<String>, _>(format!("{prefix}_scope_kind").as_str())?;
    let holding_id =
        row.try_get::<Option<Uuid>, _>(format!("{prefix}_scope_holding_id").as_str())?;
    let company_id =
        row.try_get::<Option<Uuid>, _>(format!("{prefix}_scope_company_id").as_str())?;
    let branch_id = row.try_get::<Option<Uuid>, _>(format!("{prefix}_scope_branch_id").as_str())?;
    Ok(id == expected.id
        && kind.as_deref() == expected.kind
        && holding_id == expected.holding_id
        && company_id == expected.company_id
        && branch_id == expected.branch_id)
}

async fn read_source_event_page_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
    cursor: Option<&PosSourceEventCursor>,
    page_size: u32,
) -> Result<PosSourceEventPage, PersistenceError> {
    validate_page_size(page_size)?;
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_source_cursor(cursor)?;
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type=$2
               AND (aggregate_id > $3
                    OR (aggregate_id = $3 AND aggregate_version > $4))
             ORDER BY aggregate_id, aggregate_version
             LIMIT $5",
        )
        .bind(data_profile_key)
        .bind(POS_AGGREGATE)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "POS source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM erpx_holding.outbox_events
             WHERE data_profile_key=$1 AND aggregate_type=$2
             ORDER BY aggregate_id, aggregate_version
             LIMIT $3",
        )
        .bind(data_profile_key)
        .bind(POS_AGGREGATE)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size)
        .map_err(|_| invalid("POS source page size is not representable"))?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(event_id, "POS source event ID")?;
            validate_uuid(aggregate_id, "POS source aggregate ID")?;
            let event_type: String = row.try_get("event_type")?;
            validate_event_type(&event_type)?;
            Ok(PosSourceEvent {
                event_id,
                event_type,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "POS source event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| PosSourceEventCursor {
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(PosSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn clear_projection_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
) -> Result<PosProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_holding.pos_sale_read_rows WHERE data_profile_key=$1")
            .bind(data_profile_key)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    sqlx::query("DELETE FROM erpx_holding.pos_sale_line_read_rows WHERE data_profile_key=$1")
        .bind(data_profile_key)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM erpx_holding.pos_sale_payment_read_rows WHERE data_profile_key=$1")
        .bind(data_profile_key)
        .execute(&mut **tx)
        .await?;
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_holding.pos_projection_checkpoints WHERE data_profile_key=$1",
    )
    .bind(data_profile_key)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    let receipts_cleared = sqlx::query(
        "DELETE FROM erpx_holding.pos_projection_event_receipts WHERE data_profile_key=$1",
    )
    .bind(data_profile_key)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(PosProjectionRebuildPreparation {
        data_profile_key: data_profile_key.to_owned(),
        rows_cleared,
        checkpoints_cleared,
        receipts_cleared,
    })
}

async fn insert_receipt(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
    event: &PosSourceEvent,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.pos_projection_event_receipts
            (data_profile_key, projection_name, event_id, aggregate_id,
             aggregate_version, event_type)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (data_profile_key, projection_name, event_id) DO NOTHING",
    )
    .bind(data_profile_key)
    .bind(POS_PROJECTION_NAME)
    .bind(event.event_id)
    .bind(event.aggregate_id)
    .bind(as_i64(event.aggregate_version, "POS source version")?)
    .bind(&event.event_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

struct ProjectionValues {
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    reference: String,
    customer: ReferenceColumns,
    register: ReferenceColumns,
    currency: String,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    paid_minor: i64,
    status: &'static str,
    command_id: Uuid,
    idempotency_key: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

struct LineValues {
    line_id: Uuid,
    line_no: i32,
    item_reference: String,
    description: String,
    quantity: i64,
    unit_price_minor: i64,
    tax_minor: i64,
    subtotal_minor: i64,
    total_minor: i64,
    product: ReferenceColumns,
}

#[derive(Clone, Copy)]
struct ReferenceColumns {
    id: Option<Uuid>,
    kind: Option<&'static str>,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
}

fn projection_values(sale: &Sale) -> Result<ProjectionValues, PersistenceError> {
    let (holding_id, company_id, branch_id) = sale_scope(&sale.scope)?;
    Ok(ProjectionValues {
        holding_id,
        company_id,
        branch_id,
        reference: bounded_text(&sale.reference, "POS sale reference")?,
        customer: reference_columns(sale.customer.as_ref(), "customer")?,
        register: reference_columns(sale.register.as_ref(), "register")?,
        currency: bounded_text(&sale.currency, "POS currency")?,
        subtotal_minor: sale.subtotal_minor,
        tax_minor: sale.tax_minor,
        total_minor: sale.total_minor,
        paid_minor: sale.paid_minor,
        status: sale_status(sale.status),
        command_id: sale.command_id,
        idempotency_key: bounded_text(&sale.idempotency_key, "POS idempotency key")?,
        created_at: sale.created_at,
        updated_at: sale.updated_at,
    })
}

fn line_values(line: &SaleLine, line_no: usize) -> Result<LineValues, PersistenceError> {
    let line_no = i32::try_from(line_no).map_err(|_| invalid("POS line number exceeds INTEGER"))?;
    Ok(LineValues {
        line_id: line.line_id,
        line_no,
        item_reference: bounded_text(&line.item_reference, "POS line item reference")?,
        description: bounded_text(&line.description, "POS line description")?,
        quantity: i64::try_from(line.quantity)
            .map_err(|_| invalid("POS line quantity exceeds BIGINT"))?,
        unit_price_minor: line.unit_price_minor,
        tax_minor: line.tax_minor,
        subtotal_minor: line
            .subtotal_minor()
            .map_err(|error| invalid(error.to_string()))?,
        total_minor: line
            .total_minor()
            .map_err(|error| invalid(error.to_string()))?,
        product: reference_columns(line.product.as_ref(), "product")?,
    })
}

fn decode_sale(event: &PosSourceEvent) -> Result<Sale, PersistenceError> {
    let aggregate_type = event
        .payload
        .get("aggregate_type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("POS source aggregate type is missing"))?;
    if aggregate_type != POS_AGGREGATE {
        return Err(invalid("POS source aggregate type is invalid"));
    }
    let payload_version = event
        .payload
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("POS source payload version is invalid"))?;
    if payload_version != event.aggregate_version {
        return Err(invalid("POS source version does not match payload"));
    }
    let aggregate = event
        .payload
        .get("aggregate")
        .cloned()
        .ok_or_else(|| invalid("POS source aggregate is missing"))?;
    let sale: Sale = serde_json::from_value(aggregate)?;
    sale.validate()
        .map_err(|error| invalid(format!("invalid POS source aggregate: {error}")))?;
    if sale.id != event.aggregate_id || sale.version != event.aggregate_version {
        return Err(invalid("POS source aggregate identity is invalid"));
    }
    let metadata = sale
        .command_history
        .last()
        .ok_or_else(|| invalid("POS source event metadata is missing"))?
        .event
        .clone();
    if metadata.event_id != event.event_id
        || metadata.aggregate_id != event.aggregate_id
        || metadata.aggregate_version != event.aggregate_version
        || metadata.active_scope != sale.scope
        || event_type(metadata.event_type) != event.event_type
    {
        return Err(invalid("POS source event identity is invalid"));
    }
    Ok(sale)
}

fn sale_id(sale: &Sale) -> Uuid {
    sale.id
}

fn sale_scope(scope: &AuthorityScope) -> Result<(Uuid, Uuid, Option<Uuid>), PersistenceError> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "POS holding ID")?;
            validate_uuid(*company_id, "POS company ID")?;
            Ok((*holding_id, *company_id, None))
        }
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "POS holding ID")?;
            validate_uuid(*company_id, "POS company ID")?;
            validate_uuid(*branch_id, "POS branch ID")?;
            Ok((*holding_id, *company_id, Some(*branch_id)))
        }
        _ => Err(invalid("POS sale scope must be company or branch")),
    }
}

fn reference_columns(
    reference: Option<&PosScopedReference>,
    label: &'static str,
) -> Result<ReferenceColumns, PersistenceError> {
    let Some(reference) = reference else {
        return Ok(ReferenceColumns {
            id: None,
            kind: None,
            holding_id: None,
            company_id: None,
            branch_id: None,
        });
    };
    reference
        .validate()
        .map_err(|error| invalid(format!("invalid POS {label} reference: {error}")))?;
    let (kind, holding_id, company_id, branch_id) = match &reference.scope {
        AuthorityScope::Holding { holding_id } => {
            validate_uuid(*holding_id, "POS reference holding ID")?;
            ("holding", *holding_id, None, None)
        }
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "POS reference holding ID")?;
            validate_uuid(*company_id, "POS reference company ID")?;
            ("company", *holding_id, Some(*company_id), None)
        }
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "POS reference holding ID")?;
            validate_uuid(*company_id, "POS reference company ID")?;
            validate_uuid(*branch_id, "POS reference branch ID")?;
            ("branch", *holding_id, Some(*company_id), Some(*branch_id))
        }
        _ => return Err(invalid("POS reference scope is not persistable")),
    };
    Ok(ReferenceColumns {
        id: Some(reference.id),
        kind: Some(kind),
        holding_id: Some(holding_id),
        company_id,
        branch_id,
    })
}

fn event_type(event: PosEventType) -> &'static str {
    match event {
        PosEventType::SaleCreated => "pos.sale_created.v1",
        PosEventType::SaleOpened => "pos.sale_opened.v1",
        PosEventType::SaleHeld => "pos.sale_held.v1",
        PosEventType::SaleResumed => "pos.sale_resumed.v1",
        PosEventType::PaymentRecorded => "pos.payment_recorded.v1",
        PosEventType::SaleCompleted => "pos.sale_completed.v1",
        PosEventType::SaleVoided => "pos.sale_voided.v1",
        PosEventType::SaleRefunded => "pos.sale_refunded.v1",
    }
}

fn validate_event_type(value: &str) -> Result<(), PersistenceError> {
    if [
        "pos.sale_created.v1",
        "pos.sale_opened.v1",
        "pos.sale_held.v1",
        "pos.sale_resumed.v1",
        "pos.payment_recorded.v1",
        "pos.sale_completed.v1",
        "pos.sale_voided.v1",
        "pos.sale_refunded.v1",
    ]
    .contains(&value)
    {
        Ok(())
    } else {
        Err(invalid("POS source event type is invalid"))
    }
}

async fn validate_source_event_versions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    data_profile_key: &str,
) -> Result<(), PersistenceError> {
    let rows = sqlx::query(
        "SELECT aggregate_id,
                COUNT(*)::BIGINT AS event_count,
                COUNT(DISTINCT aggregate_version)::BIGINT AS version_count,
                MIN(aggregate_version)::BIGINT AS first_version,
                MAX(aggregate_version)::BIGINT AS last_version
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type=$2
         GROUP BY aggregate_id",
    )
    .bind(data_profile_key)
    .bind(POS_AGGREGATE)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        let aggregate_id: Uuid = row.try_get("aggregate_id")?;
        validate_uuid(aggregate_id, "POS source aggregate ID")?;
        let event_count = as_u64(row.try_get("event_count")?, "POS source event count")?;
        let version_count = as_u64(row.try_get("version_count")?, "POS source version count")?;
        let first_version = as_u64(row.try_get("first_version")?, "POS source first version")?;
        let last_version = as_u64(row.try_get("last_version")?, "POS source last version")?;
        validate_source_version_stats(event_count, version_count, first_version, last_version)?;
    }
    Ok(())
}

fn validate_source_version_stats(
    event_count: u64,
    version_count: u64,
    first_version: u64,
    last_version: u64,
) -> Result<(), PersistenceError> {
    if event_count == 0
        || version_count != event_count
        || first_version != 1
        || last_version != event_count
    {
        return Err(invalid(
            "POS source aggregate versions are not unique and contiguous",
        ));
    }
    Ok(())
}

fn payment_method(method: PaymentMethod) -> &'static str {
    match method {
        PaymentMethod::Cash => "cash",
        PaymentMethod::Card => "card",
        PaymentMethod::BankTransfer => "bank_transfer",
        PaymentMethod::MobileWallet => "mobile_wallet",
        PaymentMethod::Voucher => "voucher",
        PaymentMethod::Other => "other",
    }
}

fn sale_status(status: SaleStatus) -> &'static str {
    match status {
        SaleStatus::Draft => "draft",
        SaleStatus::Open => "open",
        SaleStatus::Held => "held",
        SaleStatus::Completed => "completed",
        SaleStatus::Voided => "voided",
        SaleStatus::Refunded => "refunded",
    }
}

fn decode_sale_line(row: sqlx::postgres::PgRow) -> Result<PosSaleReadLine, PersistenceError> {
    Ok(PosSaleReadLine {
        line_id: row.try_get("line_id")?,
        line_no: row.try_get("line_no")?,
        item_reference: row.try_get("item_reference")?,
        description: row.try_get("description")?,
        quantity: row.try_get("quantity")?,
        unit_price_minor: row.try_get("unit_price_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        subtotal_minor: row.try_get("subtotal_minor")?,
        total_minor: row.try_get("total_minor")?,
        product_id: row.try_get("product_id")?,
        source_version: as_u64(row.try_get("source_version")?, "POS line source version")?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn decode_sale_payment(row: sqlx::postgres::PgRow) -> Result<PosSaleReadPayment, PersistenceError> {
    Ok(PosSaleReadPayment {
        payment_id: row.try_get("payment_id")?,
        amount_minor: row.try_get("amount_minor")?,
        method: row.try_get("method")?,
        received_at: row.try_get("received_at")?,
        source_version: as_u64(row.try_get("source_version")?, "POS payment source version")?,
        source_event_id: row.try_get("source_event_id")?,
        projected_at: row.try_get("projected_at")?,
    })
}

fn validate_read_scope(scope: PosReadScope) -> Result<(), PersistenceError> {
    validate_uuid(scope.holding_id, "POS read holding ID")?;
    validate_uuid(scope.company_id, "POS read company ID")?;
    if let Some(branch_id) = scope.branch_id {
        validate_uuid(branch_id, "POS read branch ID")?;
    }
    Ok(())
}

fn validate_read_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_POS_READ_PAGE_SIZE {
        return Err(invalid(format!(
            "POS read page size must be 1..={MAX_POS_READ_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn validate_page_size(page_size: u32) -> Result<(), PersistenceError> {
    if page_size == 0 || page_size > MAX_POS_SOURCE_EVENT_PAGE_SIZE {
        return Err(invalid(format!(
            "POS source event page size must be 1..={MAX_POS_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn validate_source_cursor(cursor: &PosSourceEventCursor) -> Result<(), PersistenceError> {
    validate_uuid(cursor.aggregate_id, "POS source cursor aggregate ID")?;
    if cursor.aggregate_version == 0 {
        return Err(invalid("POS source cursor version is invalid"));
    }
    Ok(())
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn bounded_text(value: &str, field: &'static str) -> Result<String, PersistenceError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(value.to_owned())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckpointDecision {
    Apply,
    Ignore,
}

fn checkpoint_decision(
    current: Option<(u64, Uuid)>,
    incoming_version: u64,
    incoming_event_id: Uuid,
) -> Result<CheckpointDecision, PersistenceError> {
    validate_uuid(incoming_event_id, "POS incoming event ID")?;
    let Some((current_version, current_event_id)) = current else {
        return if incoming_version == 1 {
            Ok(CheckpointDecision::Apply)
        } else {
            Err(invalid("POS projection must start at version one"))
        };
    };
    if incoming_version < current_version {
        return Ok(CheckpointDecision::Ignore);
    }
    if incoming_version == current_version {
        return if current_event_id == incoming_event_id {
            Ok(CheckpointDecision::Ignore)
        } else {
            Err(invalid("POS projection checkpoint identity conflicts"))
        };
    }
    if incoming_version == current_version + 1 {
        Ok(CheckpointDecision::Apply)
    } else {
        Err(invalid("POS projection version is not contiguous"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn constructor_and_page_bounds_fail_closed() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxPosProjectionStore::from_pools(&pool, &pool, " ").is_err());
        assert!(SqlxPosProjectionStore::from_pools(&pool, &pool, "holding\nref").is_err());
        assert!(
            validate_read_scope(PosReadScope {
                holding_id: Uuid::nil(),
                company_id: Uuid::new_v4(),
                branch_id: None,
            })
            .is_err()
        );
        assert!(validate_read_page_size(0).is_err());
        assert!(validate_read_page_size(MAX_POS_READ_PAGE_SIZE + 1).is_err());
        assert!(validate_page_size(0).is_err());
        assert!(validate_page_size(MAX_POS_SOURCE_EVENT_PAGE_SIZE + 1).is_err());
    }

    #[test]
    fn source_event_allow_list_and_cursor_bounds_are_explicit() {
        assert!(validate_event_type("pos.sale_created.v1").is_ok());
        assert!(validate_event_type("pos.sale_refunded.v1").is_ok());
        assert!(validate_event_type("pos.unknown.v1").is_err());
        assert!(validate_event_type("pos.refund_posted.v1").is_err());
        assert!(as_i64(1, "version").is_ok());
        assert!(as_u64(-1, "version").is_err());
    }

    #[test]
    fn source_event_identity_errors_do_not_echo_payloads() {
        let error = validate_event_type("payment-secret").expect_err("closed event type");
        assert!(!error.to_string().contains("payment-secret"));
    }

    #[test]
    fn checkpoint_decision_is_idempotent_and_contiguous() {
        let event_id = Uuid::new_v4();
        assert_eq!(
            checkpoint_decision(None, 1, event_id).expect("first event"),
            CheckpointDecision::Apply
        );
        assert_eq!(
            checkpoint_decision(Some((2, event_id)), 2, event_id).expect("duplicate event"),
            CheckpointDecision::Ignore
        );
        assert_eq!(
            checkpoint_decision(Some((3, event_id)), 2, event_id).expect("stale event"),
            CheckpointDecision::Ignore
        );
        assert_eq!(
            checkpoint_decision(Some((2, event_id)), 3, Uuid::new_v4()).expect("next event"),
            CheckpointDecision::Apply
        );
        assert!(checkpoint_decision(Some((2, event_id)), 4, Uuid::new_v4()).is_err());
        assert!(checkpoint_decision(Some((2, event_id)), 2, Uuid::new_v4()).is_err());
    }

    #[test]
    fn source_version_stats_reject_duplicate_and_gapped_streams() {
        assert!(validate_source_version_stats(3, 3, 1, 3).is_ok());
        assert!(validate_source_version_stats(3, 2, 1, 3).is_err());
        assert!(validate_source_version_stats(3, 3, 1, 4).is_err());
        assert!(validate_source_version_stats(2, 2, 2, 3).is_err());
    }
}
