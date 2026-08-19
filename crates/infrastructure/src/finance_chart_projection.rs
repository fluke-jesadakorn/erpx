//! Typed, rebuildable holding-read projection for the authoritative Finance
//! Chart of Accounts aggregate. Source events are read only from the holding
//! write outbox; normalized rows are written only to the holding read pool.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_domain::contracts::{ActorKind, AuthorityScope, CONTRACT_VERSION, EventEnvelope};
use erpx_domain::finance_chart::{
    AccountCategory, AccountStatus, FinanceAccount, MAX_FINANCE_ACCOUNT_CODE_BYTES,
    MAX_FINANCE_ACCOUNT_NAME_BYTES, MAX_FINANCE_CHART_ACCOUNTS, MAX_FINANCE_CHART_NAME_BYTES,
    MAX_FINANCE_IDEMPOTENCY_KEY_BYTES, NormalBalance,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_chart_persistence::{
    FINANCE_CHART_AGGREGATE, FINANCE_CHART_SAVED_EVENT, FinanceChartSavedEvent,
};
use crate::finance_persistence::begin_scoped;

pub const FINANCE_CHART_PROJECTION_NAME: &str = "finance_chart";
pub const MAX_FINANCE_CHART_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_CHART_READ_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartSourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceChartSourceEvent {
    pub envelope: EventEnvelope,
    pub payload: FinanceChartSavedEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceChartSourceEventPage {
    pub events: Vec<FinanceChartSourceEvent>,
    pub next_cursor: Option<FinanceChartSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartProjectionRebuildPreparation {
    pub data_profile_key: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
    pub receipts_cleared: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceChartReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceChartReadCursor {
    pub chart_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartSourceMetadata {
    pub contract_version: u16,
    pub event_version: u16,
    pub actor_ref: String,
    pub actor_kind: String,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceAccountReadRow {
    pub account_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub code: String,
    pub name: String,
    pub category: String,
    pub normal_balance: String,
    pub parent_account_id: Option<Uuid>,
    pub status: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: FinanceChartSourceMetadata,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartReadRow {
    pub chart_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub name: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub source: FinanceChartSourceMetadata,
    pub projected_at: DateTime<Utc>,
    pub accounts: Vec<FinanceAccountReadRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceChartReadPage {
    pub rows: Vec<FinanceChartReadRow>,
    pub next_cursor: Option<FinanceChartReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceChartProjectionStore {
    write_pool: PgPool,
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxFinanceChartProjectionStore {
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

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceChartSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceChartSourceEventPage, PersistenceError> {
        validate_page_size(page_size)?;
        if let Some(cursor) = cursor {
            validate_uuid(
                cursor.aggregate_id,
                "Finance chart source cursor aggregate ID",
            )?;
            if cursor.aggregate_version == 0 {
                return Err(invalid("Finance chart source cursor version is invalid"));
            }
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query("SELECT event_id,event_type,aggregate_type,aggregate_id,aggregate_version,occurred_at,contract_version,event_version,active_scope,actor,correlation_id,causation_id,idempotency_key,payload FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND aggregate_type=$2 AND (aggregate_id>$3 OR (aggregate_id=$3 AND aggregate_version>$4)) ORDER BY aggregate_id,aggregate_version LIMIT $5")
                .bind(&self.data_profile_key).bind(FINANCE_CHART_AGGREGATE).bind(cursor.aggregate_id).bind(as_i64(cursor.aggregate_version, "Finance chart source cursor version")?).bind(limit).fetch_all(&mut *tx).await?
        } else {
            sqlx::query("SELECT event_id,event_type,aggregate_type,aggregate_id,aggregate_version,occurred_at,contract_version,event_version,active_scope,actor,correlation_id,causation_id,idempotency_key,payload FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND aggregate_type=$2 ORDER BY aggregate_id,aggregate_version LIMIT $3")
                .bind(&self.data_profile_key).bind(FINANCE_CHART_AGGREGATE).bind(limit).fetch_all(&mut *tx).await?
        };
        tx.commit().await?;
        let length = usize::try_from(page_size)
            .map_err(|_| invalid("Finance chart page size is not representable"))?;
        let has_more = rows.len() > length;
        let events = rows
            .into_iter()
            .take(length)
            .map(decode_source_event)
            .collect::<Result<Vec<_>, _>>()?;
        for event in &events {
            validate_source_event(event, &self.data_profile_key)?;
        }
        let next_cursor =
            has_more
                .then(|| events.last())
                .flatten()
                .map(|event| FinanceChartSourceEventCursor {
                    aggregate_id: event.envelope.aggregate_id,
                    aggregate_version: event.envelope.aggregate_version,
                });
        Ok(FinanceChartSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        let count = chart_source_event_count(&mut tx, &self.data_profile_key).await?;
        tx.commit().await?;
        Ok(count)
    }

    pub async fn project_event(
        &self,
        event: &FinanceChartSourceEvent,
    ) -> Result<FinanceChartProjectionApplication, PersistenceError> {
        validate_source_event(event, &self.data_profile_key)?;
        verify_authoritative_chart_source_event(&self.write_pool, &self.data_profile_key, event)
            .await?;
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let result = self.project_event_in_transaction(&mut tx, event).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn project_event_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event: &FinanceChartSourceEvent,
    ) -> Result<FinanceChartProjectionApplication, PersistenceError> {
        if let Some(existing) = sqlx::query("SELECT aggregate_id,aggregate_version,event_type,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key FROM erpx_holding.finance_chart_projection_event_receipts WHERE data_profile_key=$1 AND projection_name=$2 AND event_id=$3 FOR UPDATE")
            .bind(&self.data_profile_key).bind(FINANCE_CHART_PROJECTION_NAME).bind(event.envelope.event_id).fetch_optional(&mut **tx).await? {
            validate_receipt(&existing, event)?;
            return Ok(FinanceChartProjectionApplication { event_id: event.envelope.event_id, aggregate_id: event.envelope.aggregate_id, aggregate_version: event.envelope.aggregate_version, applied: false });
        }
        let current = sqlx::query("SELECT source_version FROM erpx_holding.finance_chart_projection_checkpoints WHERE data_profile_key=$1 AND projection_name=$2 AND aggregate_id=$3 FOR UPDATE")
            .bind(&self.data_profile_key).bind(FINANCE_CHART_PROJECTION_NAME).bind(event.envelope.aggregate_id).fetch_optional(&mut **tx).await?;
        let current_version = current
            .map(|row| {
                as_u64(
                    row.get("source_version"),
                    "Finance chart checkpoint version",
                )
            })
            .transpose()?;
        let expected = match current_version {
            Some(version) => version
                .checked_add(1)
                .ok_or_else(|| invalid("Finance chart checkpoint version overflow"))?,
            None => 1,
        };
        if event.envelope.aggregate_version != expected {
            return Err(invalid(format!(
                "Finance chart aggregate version gap: expected {expected}, got {}",
                event.envelope.aggregate_version
            )));
        }

        sqlx::query("INSERT INTO erpx_holding.finance_chart_projection_event_receipts (data_profile_key,projection_name,event_id,aggregate_type,aggregate_id,aggregate_version,event_type,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
            .bind(&self.data_profile_key).bind(FINANCE_CHART_PROJECTION_NAME).bind(event.envelope.event_id).bind(FINANCE_CHART_AGGREGATE).bind(event.envelope.aggregate_id).bind(as_i64(event.envelope.aggregate_version, "Finance chart source version")?).bind(FINANCE_CHART_SAVED_EVENT).bind(i32::from(event.envelope.contract_version)).bind(i32::from(event.envelope.event_version)).bind(&event.envelope.actor.principal_ref).bind(actor_kind_name(&event.envelope.actor.kind)).bind(event.envelope.correlation_id).bind(event.envelope.causation_id).bind(event.envelope.idempotency_key.as_deref().unwrap_or_default()).execute(&mut **tx).await?;
        let source = &event.envelope;
        let chart = &event.payload;
        sqlx::query("INSERT INTO erpx_holding.finance_chart_read_rows (data_profile_key,chart_id,holding_id,company_id,chart_name,source_version,source_event_id,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (data_profile_key,chart_id) DO UPDATE SET holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,chart_name=EXCLUDED.chart_name,source_version=EXCLUDED.source_version,source_event_id=EXCLUDED.source_event_id,source_contract_version=EXCLUDED.source_contract_version,source_event_version=EXCLUDED.source_event_version,source_actor_ref=EXCLUDED.source_actor_ref,source_actor_kind=EXCLUDED.source_actor_kind,source_correlation_id=EXCLUDED.source_correlation_id,source_causation_id=EXCLUDED.source_causation_id,source_idempotency_key=EXCLUDED.source_idempotency_key,projected_at=now()")
            .bind(&self.data_profile_key).bind(chart.chart_id).bind(chart.holding_id).bind(chart.company_id).bind(&chart.name).bind(as_i64(source.aggregate_version, "Finance chart source version")?).bind(source.event_id).bind(i32::from(source.contract_version)).bind(i32::from(source.event_version)).bind(&source.actor.principal_ref).bind(actor_kind_name(&source.actor.kind)).bind(source.correlation_id).bind(source.causation_id).bind(source.idempotency_key.as_deref().unwrap_or_default()).execute(&mut **tx).await?;
        sqlx::query("DELETE FROM erpx_holding.finance_chart_account_read_rows WHERE data_profile_key=$1 AND chart_id=$2").bind(&self.data_profile_key).bind(chart.chart_id).execute(&mut **tx).await?;
        for account in &chart.accounts {
            sqlx::query("INSERT INTO erpx_holding.finance_chart_account_read_rows (data_profile_key,chart_id,holding_id,company_id,account_id,code,account_name,category,normal_balance,parent_account_id,status,source_version,source_event_id,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)")
                .bind(&self.data_profile_key).bind(chart.chart_id).bind(chart.holding_id).bind(chart.company_id).bind(account.id).bind(&account.code).bind(&account.name).bind(category_name(account.category)).bind(normal_balance_name(account.normal_balance)).bind(account.parent_id).bind(status_name(account.status)).bind(as_i64(source.aggregate_version, "Finance chart source version")?).bind(source.event_id).bind(i32::from(source.contract_version)).bind(i32::from(source.event_version)).bind(&source.actor.principal_ref).bind(actor_kind_name(&source.actor.kind)).bind(source.correlation_id).bind(source.causation_id).bind(source.idempotency_key.as_deref().unwrap_or_default()).execute(&mut **tx).await?;
        }
        sqlx::query("INSERT INTO erpx_holding.finance_chart_projection_checkpoints (data_profile_key,projection_name,aggregate_type,aggregate_id,holding_id,company_id,last_event_id,source_version) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (data_profile_key,projection_name,aggregate_id) DO UPDATE SET holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id,last_event_id=EXCLUDED.last_event_id,source_version=EXCLUDED.source_version,projected_at=now()")
            .bind(&self.data_profile_key).bind(FINANCE_CHART_PROJECTION_NAME).bind(FINANCE_CHART_AGGREGATE).bind(chart.chart_id).bind(chart.holding_id).bind(chart.company_id).bind(source.event_id).bind(as_i64(source.aggregate_version, "Finance chart source version")?).execute(&mut **tx).await?;
        Ok(FinanceChartProjectionApplication {
            event_id: source.event_id,
            aggregate_id: source.aggregate_id,
            aggregate_version: source.aggregate_version,
            applied: true,
        })
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceChartProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_read_rows WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_projection_checkpoints WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let receipts_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_projection_event_receipts WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(FinanceChartProjectionRebuildPreparation {
            data_profile_key: self.data_profile_key.clone(),
            rows_cleared,
            checkpoints_cleared,
            receipts_cleared,
        })
    }

    /// Atomically replace the disposable chart projection from one fenced
    /// authoritative outbox snapshot. The source share lock prevents chart
    /// writes while the source pages are read; the read transaction is only
    /// committed after every chart, account row, receipt, and checkpoint has
    /// been replayed. Any late failure preserves the prior projection.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceChartProjectionRebuildPreparation, u64), PersistenceError> {
        validate_page_size(page_size)?;
        let mut source_tx = begin_scoped(&self.write_pool, &self.data_profile_key).await?;
        sqlx::query("LOCK TABLE erpx_holding.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            chart_source_event_count(&mut source_tx, &self.data_profile_key).await?;
        if expected_source_version.is_some_and(|expected| expected != actual_source_version) {
            return Err(invalid(
                "Finance chart source fence changed before replacement rebuild",
            ));
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_read_rows WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_projection_checkpoints WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let receipts_cleared = sqlx::query(
            "DELETE FROM erpx_holding.finance_chart_projection_event_receipts WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .execute(&mut *read_tx)
        .await?
        .rows_affected();
        let preparation = FinanceChartProjectionRebuildPreparation {
            data_profile_key: self.data_profile_key.clone(),
            rows_cleared,
            checkpoints_cleared,
            receipts_cleared,
        };

        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_chart_source_event_page_in_transaction(
                &mut source_tx,
                &self.data_profile_key,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                if self
                    .project_event_in_transaction(&mut read_tx, event)
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
                return Err(invalid("Finance chart source event cursor is missing"));
            }
        }
        let final_source_version =
            chart_source_event_count(&mut source_tx, &self.data_profile_key).await?;
        if final_source_version != actual_source_version {
            return Err(invalid(
                "Finance chart source fence changed during replacement rebuild",
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub fn read_store(&self) -> SqlxFinanceChartReadStore {
        SqlxFinanceChartReadStore {
            read_pool: self.read_pool.clone(),
            data_profile_key: self.data_profile_key.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceChartReadStore {
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxFinanceChartReadStore {
    pub fn new(
        read_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            read_pool,
            data_profile_key,
        })
    }

    pub async fn list_charts(
        &self,
        scope: FinanceChartReadScope,
        page_size: u32,
        cursor: Option<FinanceChartReadCursor>,
    ) -> Result<FinanceChartReadPage, PersistenceError> {
        validate_scope(scope)?;
        validate_read_page_size(page_size)?;
        if cursor.is_some_and(|value| value.chart_id.is_nil()) {
            return Err(invalid("Finance chart read cursor is invalid"));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key).await?;
        let rows = sqlx::query("SELECT chart_id,holding_id,company_id,chart_name,source_version,source_event_id,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key,projected_at FROM erpx_holding.finance_chart_read_rows WHERE data_profile_key=$1 AND holding_id=$2 AND company_id=$3 AND ($4::UUID IS NULL OR chart_id>$4) ORDER BY chart_id LIMIT $5")
            .bind(&self.data_profile_key).bind(scope.holding_id).bind(scope.company_id).bind(cursor.map(|value| value.chart_id)).bind(limit).fetch_all(&mut *tx).await?;
        let length = page_size as usize;
        let has_more = rows.len() > length;
        let mut result = Vec::with_capacity(length.min(rows.len()));
        for row in rows.into_iter().take(length) {
            let chart_id: Uuid = row.try_get("chart_id")?;
            let accounts = sqlx::query("SELECT account_id,holding_id,company_id,code,account_name,category,normal_balance,parent_account_id,status,source_version,source_event_id,source_contract_version,source_event_version,source_actor_ref,source_actor_kind,source_correlation_id,source_causation_id,source_idempotency_key,projected_at FROM erpx_holding.finance_chart_account_read_rows WHERE data_profile_key=$1 AND chart_id=$2 AND holding_id=$3 AND company_id=$4 ORDER BY code,account_id")
                .bind(&self.data_profile_key).bind(chart_id).bind(scope.holding_id).bind(scope.company_id).fetch_all(&mut *tx).await?;
            if accounts.len() > MAX_FINANCE_CHART_ACCOUNTS {
                return Err(invalid("Finance chart read account limit exceeded"));
            }
            result.push(FinanceChartReadRow {
                chart_id,
                holding_id: row.try_get("holding_id")?,
                company_id: row.try_get("company_id")?,
                name: row.try_get("chart_name")?,
                source_version: as_u64(
                    row.try_get("source_version")?,
                    "Finance chart read source version",
                )?,
                source_event_id: row.try_get("source_event_id")?,
                source: metadata_from_row(&row)?,
                projected_at: row.try_get("projected_at")?,
                accounts: accounts
                    .into_iter()
                    .map(decode_account_row)
                    .collect::<Result<Vec<_>, _>>()?,
            });
        }
        tx.commit().await?;
        let next_cursor =
            has_more
                .then(|| result.last())
                .flatten()
                .map(|row| FinanceChartReadCursor {
                    chart_id: row.chart_id,
                });
        Ok(FinanceChartReadPage {
            rows: result,
            next_cursor,
            has_more,
        })
    }
}

async fn chart_source_event_count(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND aggregate_type=$2",
    )
    .bind(data_profile_key)
    .bind(FINANCE_CHART_AGGREGATE)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "Finance chart source event count")
}

async fn read_chart_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    cursor: Option<&FinanceChartSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceChartSourceEventPage, PersistenceError> {
    validate_page_size(page_size)?;
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_uuid(
            cursor.aggregate_id,
            "Finance chart source cursor aggregate ID",
        )?;
        if cursor.aggregate_version == 0 {
            return Err(invalid("Finance chart source cursor version is invalid"));
        }
        sqlx::query("SELECT event_id,event_type,aggregate_type,aggregate_id,aggregate_version,occurred_at,contract_version,event_version,active_scope,actor,correlation_id,causation_id,idempotency_key,payload FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND aggregate_type=$2 AND (aggregate_id>$3 OR (aggregate_id=$3 AND aggregate_version>$4)) ORDER BY aggregate_id,aggregate_version LIMIT $5")
            .bind(data_profile_key)
            .bind(FINANCE_CHART_AGGREGATE)
            .bind(cursor.aggregate_id)
            .bind(as_i64(cursor.aggregate_version, "Finance chart source cursor version")?)
            .bind(limit)
            .fetch_all(&mut **tx)
            .await?
    } else {
        sqlx::query("SELECT event_id,event_type,aggregate_type,aggregate_id,aggregate_version,occurred_at,contract_version,event_version,active_scope,actor,correlation_id,causation_id,idempotency_key,payload FROM erpx_holding.outbox_events WHERE data_profile_key=$1 AND aggregate_type=$2 ORDER BY aggregate_id,aggregate_version LIMIT $3")
            .bind(data_profile_key)
            .bind(FINANCE_CHART_AGGREGATE)
            .bind(limit)
            .fetch_all(&mut **tx)
            .await?
    };
    let length = usize::try_from(page_size)
        .map_err(|_| invalid("Finance chart source page size is not representable"))?;
    let has_more = rows.len() > length;
    let events = rows
        .into_iter()
        .take(length)
        .map(decode_source_event)
        .collect::<Result<Vec<_>, _>>()?;
    for event in &events {
        validate_source_event(event, data_profile_key)?;
    }
    let next_cursor = if has_more {
        events.last().map(|event| FinanceChartSourceEventCursor {
            aggregate_id: event.envelope.aggregate_id,
            aggregate_version: event.envelope.aggregate_version,
        })
    } else {
        None
    };
    Ok(FinanceChartSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

fn decode_source_event(
    row: sqlx::postgres::PgRow,
) -> Result<FinanceChartSourceEvent, PersistenceError> {
    let event_id: Uuid = row.try_get("event_id")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    validate_uuid(event_id, "Finance chart source event ID")?;
    validate_uuid(aggregate_id, "Finance chart source aggregate ID")?;
    let idempotency_key: String = row
        .try_get::<Option<String>, _>("idempotency_key")?
        .ok_or_else(|| invalid("Finance chart source idempotency key is missing"))?;
    let envelope = EventEnvelope {
        contract_version: u16::try_from(row.try_get::<i32, _>("contract_version")?)
            .map_err(|_| invalid("Finance chart contract version is invalid"))?,
        event_id,
        event_type: row.try_get("event_type")?,
        event_version: u16::try_from(row.try_get::<i32, _>("event_version")?)
            .map_err(|_| invalid("Finance chart event version is invalid"))?,
        occurred_at: row.try_get("occurred_at")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id,
        aggregate_version: as_u64(
            row.try_get("aggregate_version")?,
            "Finance chart source version",
        )?,
        active_scope: serde_json::from_value(row.try_get::<Value, _>("active_scope")?)?,
        actor: serde_json::from_value(row.try_get::<Value, _>("actor")?)?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        idempotency_key: Some(idempotency_key),
        payload: row.try_get("payload")?,
    };
    let payload: FinanceChartSavedEvent = serde_json::from_value(envelope.payload.clone())?;
    Ok(FinanceChartSourceEvent { envelope, payload })
}

#[derive(Debug, Clone, PartialEq)]
struct FinanceChartAuthoritativeEnvelopeMetadata {
    contract_version: Option<i32>,
    event_version: Option<i32>,
    active_scope: Option<Value>,
    actor: Option<Value>,
    correlation_id: Option<Uuid>,
    causation_id: Option<Uuid>,
    idempotency_key: Option<String>,
    occurred_at: DateTime<Utc>,
}

/// Fence every provenance field copied into the disposable read model against
/// the immutable source outbox row, including payload, aggregate identity, and
/// the envelope metadata copied into read-model provenance columns.
async fn verify_authoritative_chart_source_event(
    write_pool: &PgPool,
    data_profile_key: &str,
    event: &FinanceChartSourceEvent,
) -> Result<(), PersistenceError> {
    let mut tx = begin_scoped(write_pool, data_profile_key).await?;
    let row = sqlx::query(
        "SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                payload, contract_version, event_version, active_scope, actor,
                correlation_id, causation_id, idempotency_key, occurred_at
           FROM erpx_holding.outbox_events
          WHERE event_id=$1 AND data_profile_key=$2",
    )
    .bind(event.envelope.event_id)
    .bind(data_profile_key)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let Some(row) = row else {
        return Err(invalid(
            "Finance chart projection event is not present in the authoritative outbox",
        ));
    };
    let source_event_type: String = row.try_get("event_type")?;
    let source_aggregate_type: String = row.try_get("aggregate_type")?;
    let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let source_aggregate_version = as_u64(
        row.try_get("aggregate_version")?,
        "Finance chart source event version",
    )?;
    let source_payload: Value = row.try_get("payload")?;
    let source = FinanceChartAuthoritativeEnvelopeMetadata {
        contract_version: row.try_get("contract_version")?,
        event_version: row.try_get("event_version")?,
        active_scope: row.try_get("active_scope")?,
        actor: row.try_get("actor")?,
        correlation_id: row.try_get("correlation_id")?,
        causation_id: row.try_get("causation_id")?,
        idempotency_key: row.try_get("idempotency_key")?,
        occurred_at: row.try_get("occurred_at")?,
    };
    if source_event_type != event.envelope.event_type
        || source_aggregate_type != event.envelope.aggregate_type
        || source_aggregate_id != event.envelope.aggregate_id
        || source_aggregate_version != event.envelope.aggregate_version
        || source_payload != event.envelope.payload
    {
        return Err(invalid(
            "Finance chart projection event does not match its authoritative outbox row",
        ));
    }
    validate_authoritative_envelope_metadata(&event.envelope, &source)
}

fn validate_authoritative_envelope_metadata(
    envelope: &EventEnvelope,
    source: &FinanceChartAuthoritativeEnvelopeMetadata,
) -> Result<(), PersistenceError> {
    let active_scope = serde_json::to_value(&envelope.active_scope)?;
    let actor = serde_json::to_value(&envelope.actor)?;
    if source.contract_version != Some(i32::from(envelope.contract_version))
        || source.event_version != Some(i32::from(envelope.event_version))
        || source.active_scope.as_ref() != Some(&active_scope)
        || source.actor.as_ref() != Some(&actor)
        || source.correlation_id != Some(envelope.correlation_id)
        || source.causation_id != envelope.causation_id
        || source.idempotency_key != envelope.idempotency_key
        || source.occurred_at != envelope.occurred_at
    {
        return Err(invalid(
            "Finance chart projection envelope does not match its authoritative outbox row",
        ));
    }
    Ok(())
}

fn validate_source_event(
    event: &FinanceChartSourceEvent,
    data_profile_key: &str,
) -> Result<(), PersistenceError> {
    let envelope = &event.envelope;
    let chart = &event.payload;
    validate_uuid(envelope.event_id, "Finance chart event ID")?;
    validate_uuid(envelope.aggregate_id, "Finance chart aggregate ID")?;
    validate_uuid(envelope.correlation_id, "Finance chart correlation ID")?;
    if envelope.contract_version != CONTRACT_VERSION
        || envelope.event_version != 1
        || envelope.event_type != FINANCE_CHART_SAVED_EVENT
        || envelope.aggregate_type != FINANCE_CHART_AGGREGATE
        || envelope.aggregate_version == 0
    {
        return Err(invalid("Finance chart source envelope is invalid"));
    }
    let key = envelope
        .idempotency_key
        .as_deref()
        .ok_or_else(|| invalid("Finance chart source idempotency key is missing"))?;
    bounded(
        key,
        MAX_FINANCE_IDEMPOTENCY_KEY_BYTES,
        "Finance chart idempotency key",
    )?;
    bounded(&envelope.actor.principal_ref, 255, "Finance chart actor")?;
    validate_uuid(chart.chart_id, "Finance chart payload chart ID")?;
    validate_uuid(chart.holding_id, "Finance chart payload holding ID")?;
    validate_uuid(chart.company_id, "Finance chart payload company ID")?;
    bounded(
        &chart.name,
        MAX_FINANCE_CHART_NAME_BYTES,
        "Finance chart name",
    )?;
    if chart.chart_id != envelope.aggregate_id || chart.accounts.len() > MAX_FINANCE_CHART_ACCOUNTS
    {
        return Err(invalid(
            "Finance chart source payload identity or size is invalid",
        ));
    }
    let expected = AuthorityScope::Company {
        holding_id: chart.holding_id,
        company_id: chart.company_id,
    };
    if !envelope.active_scope.contains(&expected) || data_profile_key.trim().is_empty() {
        return Err(invalid("Finance chart source scope is invalid"));
    }
    validate_accounts(&chart.accounts)
}

fn validate_accounts(accounts: &[FinanceAccount]) -> Result<(), PersistenceError> {
    let mut ids = BTreeSet::new();
    let mut codes = BTreeSet::new();
    let mut parents = BTreeMap::new();
    for account in accounts {
        validate_uuid(account.id, "Finance chart account ID")?;
        bounded(
            &account.code,
            MAX_FINANCE_ACCOUNT_CODE_BYTES,
            "Finance chart account code",
        )?;
        bounded(
            &account.name,
            MAX_FINANCE_ACCOUNT_NAME_BYTES,
            "Finance chart account name",
        )?;
        if !ids.insert(account.id) || !codes.insert(account.code.clone()) {
            return Err(invalid(
                "Finance chart account identity or code is duplicated",
            ));
        }
        if account.parent_id == Some(account.id) {
            return Err(invalid("Finance chart account cannot parent itself"));
        }
        parents.insert(account.id, account.parent_id);
    }
    for account in accounts {
        if let Some(parent) = account.parent_id {
            if !parents.contains_key(&parent) {
                return Err(invalid("Finance chart account parent is missing"));
            }
            let mut seen = BTreeSet::new();
            let mut cursor = Some(account.id);
            while let Some(id) = cursor {
                if !seen.insert(id) {
                    return Err(invalid("Finance chart account hierarchy contains a cycle"));
                }
                cursor = parents.get(&id).copied().flatten();
            }
        }
    }
    Ok(())
}

fn validate_receipt(
    row: &sqlx::postgres::PgRow,
    event: &FinanceChartSourceEvent,
) -> Result<(), PersistenceError> {
    if row.try_get::<Uuid, _>("aggregate_id")? != event.envelope.aggregate_id
        || as_u64(
            row.try_get("aggregate_version")?,
            "Finance chart receipt version",
        )? != event.envelope.aggregate_version
        || row.try_get::<String, _>("event_type")? != FINANCE_CHART_SAVED_EVENT
        || row.try_get::<i32, _>("source_contract_version")?
            != i32::from(event.envelope.contract_version)
        || row.try_get::<i32, _>("source_event_version")? != i32::from(event.envelope.event_version)
        || row.try_get::<String, _>("source_actor_ref")? != event.envelope.actor.principal_ref
        || row.try_get::<String, _>("source_actor_kind")?
            != actor_kind_name(&event.envelope.actor.kind)
        || row.try_get::<Uuid, _>("source_correlation_id")? != event.envelope.correlation_id
        || row.try_get::<Option<Uuid>, _>("source_causation_id")? != event.envelope.causation_id
        || row.try_get::<String, _>("source_idempotency_key")?
            != event.envelope.idempotency_key.clone().unwrap_or_default()
    {
        return Err(invalid(
            "Finance chart projection receipt conflicts with source event",
        ));
    }
    Ok(())
}

fn metadata_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<FinanceChartSourceMetadata, PersistenceError> {
    Ok(FinanceChartSourceMetadata {
        contract_version: u16::try_from(row.try_get::<i32, _>("source_contract_version")?)
            .map_err(|_| invalid("Finance chart read contract version is invalid"))?,
        event_version: u16::try_from(row.try_get::<i32, _>("source_event_version")?)
            .map_err(|_| invalid("Finance chart read event version is invalid"))?,
        actor_ref: row.try_get("source_actor_ref")?,
        actor_kind: row.try_get("source_actor_kind")?,
        correlation_id: row.try_get("source_correlation_id")?,
        causation_id: row.try_get("source_causation_id")?,
        idempotency_key: row.try_get("source_idempotency_key")?,
    })
}
fn decode_account_row(
    row: sqlx::postgres::PgRow,
) -> Result<FinanceAccountReadRow, PersistenceError> {
    Ok(FinanceAccountReadRow {
        account_id: row.try_get("account_id")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        code: row.try_get("code")?,
        name: row.try_get("account_name")?,
        category: row.try_get("category")?,
        normal_balance: row.try_get("normal_balance")?,
        parent_account_id: row.try_get("parent_account_id")?,
        status: row.try_get("status")?,
        source_version: as_u64(
            row.try_get("source_version")?,
            "Finance chart account source version",
        )?,
        source_event_id: row.try_get("source_event_id")?,
        source: metadata_from_row(&row)?,
        projected_at: row.try_get("projected_at")?,
    })
}
fn category_name(value: AccountCategory) -> &'static str {
    match value {
        AccountCategory::Asset => "asset",
        AccountCategory::Liability => "liability",
        AccountCategory::Equity => "equity",
        AccountCategory::Revenue => "revenue",
        AccountCategory::Expense => "expense",
    }
}
fn normal_balance_name(value: NormalBalance) -> &'static str {
    match value {
        NormalBalance::Debit => "debit",
        NormalBalance::Credit => "credit",
    }
}
fn status_name(value: AccountStatus) -> &'static str {
    match value {
        AccountStatus::Active => "active",
        AccountStatus::Inactive => "inactive",
    }
}
fn actor_kind_name(value: &ActorKind) -> &'static str {
    match value {
        ActorKind::PlatformUser => "platform_user",
        ActorKind::HoldingUser => "holding_user",
        ActorKind::Service => "service",
        ActorKind::SystemWorker => "system_worker",
    }
}
fn validate_scope(scope: FinanceChartReadScope) -> Result<(), PersistenceError> {
    validate_uuid(scope.holding_id, "Finance chart read holding ID")?;
    validate_uuid(scope.company_id, "Finance chart read company ID")
}
fn validate_page_size(value: u32) -> Result<(), PersistenceError> {
    if value == 0 || value > MAX_FINANCE_CHART_SOURCE_EVENT_PAGE_SIZE {
        Err(invalid("Finance chart source page size is invalid"))
    } else {
        Ok(())
    }
}
fn validate_read_page_size(value: u32) -> Result<(), PersistenceError> {
    if value == 0 || value > MAX_FINANCE_CHART_READ_PAGE_SIZE {
        Err(invalid("Finance chart read page size is invalid"))
    } else {
        Ok(())
    }
}
fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ))
    } else {
        Ok(())
    }
}
fn bounded(value: &str, max: usize, field: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        Err(invalid(format!("{field} is invalid")))
    } else {
        Ok(())
    }
}
fn validate_uuid(value: Uuid, field: &str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        Err(invalid(format!("{field} cannot be nil")))
    } else {
        Ok(())
    }
}
fn as_i64(value: u64, field: &str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}
fn as_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}
fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope};

    fn envelope() -> EventEnvelope {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        EventEnvelope {
            contract_version: CONTRACT_VERSION,
            event_id: Uuid::new_v4(),
            event_type: FINANCE_CHART_SAVED_EVENT.into(),
            event_version: 1,
            occurred_at: Utc::now(),
            aggregate_type: FINANCE_CHART_AGGREGATE.into(),
            aggregate_id: Uuid::new_v4(),
            aggregate_version: 1,
            active_scope: AuthorityScope::Company {
                holding_id,
                company_id,
            },
            actor: ActorAttribution {
                principal_ref: "finance-chart-projector-test".into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::new_v4(),
            causation_id: Some(Uuid::new_v4()),
            idempotency_key: Some("finance-chart-projector-test-1".into()),
            payload: serde_json::json!({"chart_id": "fixture"}),
        }
    }

    fn source_metadata(envelope: &EventEnvelope) -> FinanceChartAuthoritativeEnvelopeMetadata {
        FinanceChartAuthoritativeEnvelopeMetadata {
            contract_version: Some(i32::from(envelope.contract_version)),
            event_version: Some(i32::from(envelope.event_version)),
            active_scope: Some(serde_json::to_value(&envelope.active_scope).expect("scope")),
            actor: Some(serde_json::to_value(&envelope.actor).expect("actor")),
            correlation_id: Some(envelope.correlation_id),
            causation_id: envelope.causation_id,
            idempotency_key: envelope.idempotency_key.clone(),
            occurred_at: envelope.occurred_at,
        }
    }

    #[test]
    fn authoritative_envelope_fence_rejects_provenance_drift() {
        let envelope = envelope();
        let source = source_metadata(&envelope);
        assert!(validate_authoritative_envelope_metadata(&envelope, &source).is_ok());

        let mut forged_actor = source.clone();
        forged_actor.actor = Some(serde_json::json!({
            "principal_ref": "forged-projector",
            "kind": "HoldingUser",
            "session_id": null,
            "represented_principal_ref": null,
            "support_capability_id": null
        }));
        assert!(validate_authoritative_envelope_metadata(&envelope, &forged_actor).is_err());

        let mut forged_key = source.clone();
        forged_key.idempotency_key = Some("forged-idempotency-key".into());
        assert!(validate_authoritative_envelope_metadata(&envelope, &forged_key).is_err());

        let mut forged_timestamp = source;
        forged_timestamp.occurred_at += chrono::Duration::seconds(1);
        assert!(validate_authoritative_envelope_metadata(&envelope, &forged_timestamp).is_err());
    }
}
