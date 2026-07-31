//! SQLx source-event and read-model adapter for Finance close reviews.
//!
//! Close-review rows are authoritative on the tenant write database. This
//! adapter reads their immutable outbox events from that database and writes
//! only the bounded, rebuildable projection on the physically separate tenant
//! read database. No aggregate payload is persisted in the read model.

use chrono::{DateTime, Utc};
use erpx_domain::finance_close::{CloseStatus, FinanceCloseReview};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const FINANCE_CLOSE_PROJECTION_NAME: &str = "finance_close";
pub const MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
const CLOSE_REVIEW_AGGREGATE: &str = "finance_close_review";
const CLOSE_REVIEW_EVENT: &str = "finance.close_review_saved.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseSourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceCloseSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceCloseSourceEventPage {
    pub events: Vec<FinanceCloseSourceEvent>,
    pub next_cursor: Option<FinanceCloseSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceCloseProjectionStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceCloseProjectionStore {
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

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceCloseSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceCloseSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE {
            return Err(invalid(format!(
                "Finance close source event page size must be 1..={MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        reject_duplicate_source_versions(
            has_duplicate_source_versions(&mut tx, &self.holding_ref).await?,
        )?;
        let page =
            read_source_event_page_in_transaction(&mut tx, &self.holding_ref, cursor, page_size)
                .await?;
        tx.commit().await?;
        Ok(page)
    }

    /// Return the holding-scoped count of source events consumed by the
    /// Finance close projection. This predicate must stay identical to the
    /// aggregate filter in [`read_source_event_page`]. Replacement-read
    /// rebuilds use the count before and after replay to detect source
    /// movement while the candidate is being built.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let count = source_event_high_watermark_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(count)
    }

    /// Replace the complete read projection while the authoritative source is
    /// locked against outbox movement.
    ///
    /// The source transaction holds a `SHARE` table lock for the entire read
    /// replacement. The clear, every replay, and checkpoint update share one
    /// read transaction, so any source decode, version-gap, or database error
    /// rolls back to the previously committed read model. This is the
    /// rollback boundary for a replacement rebuild; callers must not emulate
    /// it by calling `clear_projection` followed by individual `project_event`
    /// calls.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceCloseProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE {
            return Err(invalid(format!(
                "Finance close source event page size must be 1..={MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }

        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if let Some(expected) = expected_source_version
            && expected != actual_source_version
        {
            return Err(invalid(
                "Finance close source fence changed before replacement rebuild",
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
                let values = projection_values_for_event(
                    event.event_id,
                    &event.event_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                    event.occurred_at,
                )?;
                if project_event_in_transaction(
                    &mut read_tx,
                    &self.holding_ref,
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    values,
                )
                .await?
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(invalid("Finance close source event cursor is missing"));
            }
        }

        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
        occurred_at: DateTime<Utc>,
    ) -> Result<FinanceCloseProjectionApplication, PersistenceError> {
        let values = projection_values_for_event(
            event_id,
            event_type,
            aggregate_id,
            aggregate_version,
            payload,
            occurred_at,
        )?;
        crate::finance_persistence::verify_authoritative_source_event(
            &self.write_pool,
            &self.holding_ref,
            event_id,
            CLOSE_REVIEW_EVENT,
            CLOSE_REVIEW_AGGREGATE,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;

        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let applied = project_event_in_transaction(
            &mut tx,
            &self.holding_ref,
            event_id,
            aggregate_id,
            aggregate_version,
            values,
        )
        .await?;
        tx.commit().await?;
        Ok(FinanceCloseProjectionApplication {
            event_id,
            aggregate_id,
            aggregate_version,
            applied,
        })
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceCloseProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.holding_ref).await?;
        tx.commit().await?;
        Ok(preparation)
    }
}

async fn read_source_event_page_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&FinanceCloseSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceCloseSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE {
        return Err(invalid(format!(
            "Finance close source event page size must be 1..={MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    let limit = i64::from(page_size) + 1;
    reject_duplicate_source_versions(has_duplicate_source_versions(tx, holding_ref).await?)?;
    let rows = if let Some(cursor) = cursor {
        validate_uuid(cursor.aggregate_id, "Finance close cursor aggregate ID")?;
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type=$2
               AND (aggregate_id > $3
                    OR (aggregate_id = $3 AND aggregate_version > $4))
             ORDER BY aggregate_id, aggregate_version
             LIMIT $5",
        )
        .bind(holding_ref)
        .bind(CLOSE_REVIEW_AGGREGATE)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Finance close cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type=$2
             ORDER BY aggregate_id, aggregate_version
             LIMIT $3",
        )
        .bind(holding_ref)
        .bind(CLOSE_REVIEW_AGGREGATE)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };

    let page_len = usize::try_from(page_size)
        .map_err(|_| invalid("Finance close source page size is not representable"))?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(event_id, "Finance close source event ID")?;
            validate_uuid(aggregate_id, "Finance close source aggregate ID")?;
            let event_type: String = row.try_get("event_type")?;
            if event_type != CLOSE_REVIEW_EVENT {
                return Err(invalid("Finance close source event type is invalid"));
            }
            Ok(FinanceCloseSourceEvent {
                event_id,
                event_type,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Finance close source event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| FinanceCloseSourceEventCursor {
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(FinanceCloseSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn has_duplicate_source_versions(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<bool, PersistenceError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type=$2
             GROUP BY aggregate_id, aggregate_version
             HAVING COUNT(*) > 1
         )",
    )
    .bind(holding_ref)
    .bind(CLOSE_REVIEW_AGGREGATE)
    .fetch_one(&mut **tx)
    .await
    .map_err(PersistenceError::from)
}

fn reject_duplicate_source_versions(duplicate: bool) -> Result<(), PersistenceError> {
    if duplicate {
        return Err(invalid(
            "Finance close source contains duplicate events for one aggregate version",
        ));
    }
    Ok(())
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2",
    )
    .bind(holding_ref)
    .bind(CLOSE_REVIEW_AGGREGATE)
    .fetch_one(&mut **tx)
    .await?;
    u64::try_from(count).map_err(|_| {
        PersistenceError::InvalidData("Finance close source event high-watermark is invalid".into())
    })
}

async fn clear_projection_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<FinanceCloseProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_tenant.finance_close_review_read_rows WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    sqlx::query("DELETE FROM erpx_tenant.finance_close_account_read_rows WHERE holding_ref=$1")
        .bind(holding_ref)
        .execute(&mut **tx)
        .await?;
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_tenant.finance_close_projection_checkpoints
         WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(FinanceCloseProjectionRebuildPreparation {
        holding_ref: holding_ref.to_owned(),
        rows_cleared,
        checkpoints_cleared,
    })
}

fn projection_values_for_event(
    event_id: Uuid,
    event_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
    occurred_at: DateTime<Utc>,
) -> Result<ProjectionValues, PersistenceError> {
    validate_uuid(event_id, "Finance close event ID")?;
    validate_uuid(aggregate_id, "Finance close aggregate ID")?;
    if event_type != CLOSE_REVIEW_EVENT || aggregate_version == 0 {
        return Err(invalid("Finance close source event metadata is invalid"));
    }
    let payload_version = payload
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid("Finance close source payload version is invalid"))?;
    if payload_version != aggregate_version {
        return Err(invalid(
            "Finance close source version does not match payload",
        ));
    }
    let aggregate_payload = payload
        .get("aggregate")
        .cloned()
        .ok_or_else(|| invalid("Finance close source payload aggregate is missing"))?;
    let review: FinanceCloseReview = serde_json::from_value(aggregate_payload)?;
    if review.period_id() != aggregate_id
        || review.holding_id().is_nil()
        || review.company_id().is_nil()
    {
        return Err(invalid("Finance close source identity is invalid"));
    }
    projection_values(&review, occurred_at)
}

async fn project_event_in_transaction(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    holding_ref: &str,
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    values: ProjectionValues,
) -> Result<bool, PersistenceError> {
    let current = sqlx::query(
        "SELECT last_event_id, source_version
         FROM erpx_tenant.finance_close_projection_checkpoints
         WHERE holding_ref=$1 AND period_id=$2
         FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(current) = current {
        let current_event_id: Uuid = current.try_get("last_event_id")?;
        let current_version = as_u64(
            current.try_get("source_version")?,
            "Finance close projection version",
        )?;
        if aggregate_version < current_version
            || (aggregate_version == current_version && event_id == current_event_id)
        {
            return Ok(false);
        }
        if aggregate_version == current_version || aggregate_version != current_version + 1 {
            return Err(invalid(
                "Finance close projection version is not contiguous",
            ));
        }
    } else if aggregate_version != 1 {
        return Err(invalid(
            "Finance close projection must start at version one",
        ));
    }

    sqlx::query(
        "DELETE FROM erpx_tenant.finance_close_account_read_rows
         WHERE holding_ref=$1 AND period_id=$2",
    )
    .bind(holding_ref)
    .bind(aggregate_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_close_review_read_rows
            (holding_ref, holding_id, company_id, period_id, status,
             idempotency_key, opened_by, reviewed_by, closed_by,
             statement_present, statement_debit_minor, statement_credit_minor,
             statement_net_minor, statement_balanced, control_total_count,
             control_resolved_count, control_unresolved_count,
             expected_fact_count, observed_fact_count, missing_fact_count,
             account_summary_count, source_version, source_event_id,
             review_updated_at, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                 $17,$18,$19,$20,$21,$22,$23,$24,now())
         ON CONFLICT (holding_ref, period_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             status=EXCLUDED.status, idempotency_key=EXCLUDED.idempotency_key,
             opened_by=EXCLUDED.opened_by, reviewed_by=EXCLUDED.reviewed_by,
             closed_by=EXCLUDED.closed_by, statement_present=EXCLUDED.statement_present,
             statement_debit_minor=EXCLUDED.statement_debit_minor,
             statement_credit_minor=EXCLUDED.statement_credit_minor,
             statement_net_minor=EXCLUDED.statement_net_minor,
             statement_balanced=EXCLUDED.statement_balanced,
             control_total_count=EXCLUDED.control_total_count,
             control_resolved_count=EXCLUDED.control_resolved_count,
             control_unresolved_count=EXCLUDED.control_unresolved_count,
             expected_fact_count=EXCLUDED.expected_fact_count,
             observed_fact_count=EXCLUDED.observed_fact_count,
             missing_fact_count=EXCLUDED.missing_fact_count,
             account_summary_count=EXCLUDED.account_summary_count,
             source_version=EXCLUDED.source_version,
             source_event_id=EXCLUDED.source_event_id,
             review_updated_at=EXCLUDED.review_updated_at,
             projected_at=now()",
    )
    .bind(holding_ref)
    .bind(values.holding_id)
    .bind(values.company_id)
    .bind(values.period_id)
    .bind(values.status)
    .bind(values.idempotency_key)
    .bind(values.opened_by)
    .bind(values.reviewed_by)
    .bind(values.closed_by)
    .bind(values.statement_present)
    .bind(values.statement_debit_minor)
    .bind(values.statement_credit_minor)
    .bind(values.statement_net_minor)
    .bind(values.statement_balanced)
    .bind(values.control_total_count)
    .bind(values.control_resolved_count)
    .bind(values.control_unresolved_count)
    .bind(values.expected_fact_count)
    .bind(values.expected_fact_count)
    .bind(0_i32)
    .bind(values.account_summary_count)
    .bind(as_i64(aggregate_version, "Finance close source version")?)
    .bind(event_id)
    .bind(values.review_updated_at)
    .execute(&mut **tx)
    .await?;
    for account in values.accounts {
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_close_account_read_rows
                (holding_ref, holding_id, company_id, period_id, account_id,
                 debit_minor, credit_minor, net_minor, source_version,
                 source_event_id, projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now())",
        )
        .bind(holding_ref)
        .bind(values.holding_id)
        .bind(values.company_id)
        .bind(values.period_id)
        .bind(account.account_id)
        .bind(account.debit_minor)
        .bind(account.credit_minor)
        .bind(account.net_minor)
        .bind(as_i64(aggregate_version, "Finance close source version")?)
        .bind(event_id)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_close_projection_checkpoints
            (holding_ref, holding_id, company_id, period_id, last_event_id,
             source_version, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,now())
         ON CONFLICT (holding_ref, period_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             last_event_id=EXCLUDED.last_event_id,
             source_version=EXCLUDED.source_version, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(values.holding_id)
    .bind(values.company_id)
    .bind(values.period_id)
    .bind(event_id)
    .bind(as_i64(aggregate_version, "Finance close source version")?)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

struct ProjectionValues {
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    status: &'static str,
    idempotency_key: String,
    opened_by: String,
    reviewed_by: Option<String>,
    closed_by: Option<String>,
    statement_present: bool,
    statement_debit_minor: Option<i64>,
    statement_credit_minor: Option<i64>,
    statement_net_minor: Option<i64>,
    statement_balanced: Option<bool>,
    control_total_count: i32,
    control_resolved_count: i32,
    control_unresolved_count: i32,
    expected_fact_count: i32,
    account_summary_count: i32,
    review_updated_at: DateTime<Utc>,
    accounts: Vec<AccountValue>,
}

struct AccountValue {
    account_id: Uuid,
    debit_minor: i64,
    credit_minor: i64,
    net_minor: i64,
}

fn projection_values(
    review: &FinanceCloseReview,
    review_updated_at: DateTime<Utc>,
) -> Result<ProjectionValues, PersistenceError> {
    let idempotency_key = match review.status() {
        CloseStatus::Open => review.opened_context().idempotency_key.clone(),
        CloseStatus::Review => review
            .reviewed_context()
            .ok_or_else(|| invalid("Finance close review context is missing"))?
            .idempotency_key
            .clone(),
        CloseStatus::Closed => review
            .closed_context()
            .ok_or_else(|| invalid("Finance close review context is missing"))?
            .idempotency_key
            .clone(),
    };
    let opened_by = bounded_text(review.opened_by(), "Finance close opener")?;
    let reviewed_by = review
        .reviewed_by()
        .map(|value| bounded_text(value, "Finance close reviewer"))
        .transpose()?;
    let closed_by = review
        .closed_by()
        .map(|value| bounded_text(value, "Finance close closer"))
        .transpose()?;
    let mut values = ProjectionValues {
        holding_id: review.holding_id(),
        company_id: review.company_id(),
        period_id: review.period_id(),
        status: close_status(review.status()),
        idempotency_key: bounded_text(&idempotency_key, "Finance close idempotency key")?,
        opened_by,
        reviewed_by,
        closed_by,
        statement_present: false,
        statement_debit_minor: None,
        statement_credit_minor: None,
        statement_net_minor: None,
        statement_balanced: None,
        control_total_count: 0,
        control_resolved_count: 0,
        control_unresolved_count: 0,
        expected_fact_count: 0,
        account_summary_count: 0,
        review_updated_at,
        accounts: Vec::new(),
    };
    let Some(input) = review.input() else {
        if review.status() != CloseStatus::Open {
            return Err(invalid("non-open Finance close review has no input"));
        }
        return Ok(values);
    };
    if input.facts().len() > 4_096 || input.controls().len() > 4_096 {
        return Err(invalid("Finance close projection input exceeds bounds"));
    }
    let totals = input
        .statement_totals()
        .map_err(|error| invalid(error.to_string()))?;
    let accounts = input
        .drill_down()
        .map_err(|error| invalid(error.to_string()))?;
    if accounts.len() > 8_192 {
        return Err(invalid("Finance close account projection exceeds bounds"));
    }
    let total = i32::try_from(input.controls().len())
        .map_err(|_| invalid("Finance close control count exceeds bounds"))?;
    let resolved = i32::try_from(
        input
            .controls()
            .iter()
            .filter(|control| control.resolved())
            .count(),
    )
    .map_err(|_| invalid("Finance close resolved control count exceeds bounds"))?;
    values.statement_present = true;
    values.statement_debit_minor = Some(totals.debit_minor);
    values.statement_credit_minor = Some(totals.credit_minor);
    values.statement_net_minor = Some(totals.net_minor);
    values.statement_balanced = Some(totals.is_balanced());
    values.control_total_count = total;
    values.control_resolved_count = resolved;
    values.control_unresolved_count = total
        .checked_sub(resolved)
        .ok_or_else(|| invalid("Finance close control counts are invalid"))?;
    values.expected_fact_count = i32::try_from(input.facts().len())
        .map_err(|_| invalid("Finance close fact count exceeds bounds"))?;
    values.accounts = accounts
        .into_iter()
        .map(|account| AccountValue {
            account_id: account.account_id,
            debit_minor: account.debit_minor,
            credit_minor: account.credit_minor,
            net_minor: account.net_minor,
        })
        .collect();
    values.account_summary_count = i32::try_from(values.accounts.len())
        .map_err(|_| invalid("Finance close account count exceeds bounds"))?;
    Ok(values)
}

fn close_status(status: CloseStatus) -> &'static str {
    match status {
        CloseStatus::Open => "open",
        CloseStatus::Review => "review",
        CloseStatus::Closed => "closed",
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

fn validate_uuid(value: Uuid, field: &str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn bounded_text(value: &str, field: &str) -> Result<String, PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} is invalid")));
    }
    Ok(value.to_owned())
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

    #[test]
    fn status_and_bounds_are_explicit() {
        assert_eq!(close_status(CloseStatus::Open), "open");
        assert_eq!(close_status(CloseStatus::Review), "review");
        assert_eq!(close_status(CloseStatus::Closed), "closed");
        assert!(validate_holding_ref("").is_err());
        assert!(validate_uuid(Uuid::nil(), "id").is_err());
        assert!(bounded_text("actor\n", "actor").is_err());
    }

    #[test]
    fn source_page_size_is_bounded() {
        assert_eq!(MAX_FINANCE_CLOSE_SOURCE_EVENT_PAGE_SIZE, 500);
    }

    #[test]
    fn duplicate_source_versions_fail_closed_before_paging() {
        assert!(reject_duplicate_source_versions(false).is_ok());
        assert!(matches!(
            reject_duplicate_source_versions(true),
            Err(PersistenceError::InvalidData(message))
                if message == "Finance close source contains duplicate events for one aggregate version"
        ));
    }
}
