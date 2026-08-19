//! Bounded read composition for Finance close reviews.
//!
//! Close reviews are authoritative on the holding write database and are
//! projected into the disposable holding read database by the Finance close
//! worker. This adapter only performs bounded read-side composition from that
//! projection before returning a safe DTO.

use chrono::{DateTime, Duration, Utc};
use erpx_domain::finance_close::CloseStatus;
#[cfg(test)]
use erpx_domain::finance_close::FinanceCloseReview;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use crate::PersistenceError;
#[cfg(test)]
use crate::finance_close_persistence::CloseReviewSnapshot;
use crate::finance_persistence::begin_scoped;

pub const FINANCE_CLOSE_READ_STALE_AFTER_SECONDS: i64 = 300;
pub const MAX_FINANCE_CLOSE_READ_PAGE_SIZE: u32 = 100;
pub const MAX_FINANCE_CLOSE_REPORT_FACTS: usize = 4_096;
pub const MAX_FINANCE_CLOSE_REPORT_CONTROLS: usize = 4_096;
pub const MAX_FINANCE_CLOSE_REPORT_ACCOUNTS: usize = 8_192;
const MAX_FINANCE_CLOSE_ACTOR_BYTES: usize = 255;

/// Errors deliberately contain no SQL, JSON, credentials, or holding payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FinanceCloseReadError {
    #[error("Finance close read request is invalid")]
    InvalidRequest,
    #[error("Finance close read projection is invalid")]
    InvalidProjection,
    #[error("Finance close read persistence is unavailable")]
    PersistenceUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadCursor {
    pub projected_at: DateTime<Utc>,
    pub period_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadListRow {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub status: FinanceCloseReviewStatus,
    pub source_version: u64,
    pub projected_at: DateTime<Utc>,
    pub freshness: FinanceCloseReadFreshness,
    pub control_total: u32,
    pub unresolved_controls: u32,
    pub net_minor: Option<i64>,
    pub statement_present: bool,
    pub balanced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadPage {
    pub rows: Vec<FinanceCloseReadListRow>,
    pub next_cursor: Option<FinanceCloseReadCursor>,
    pub has_more: bool,
}

impl FinanceCloseReadScope {
    pub fn new(holding_id: Uuid, company_id: Uuid) -> Result<Self, FinanceCloseReadError> {
        if holding_id.is_nil() || company_id.is_nil() {
            return Err(FinanceCloseReadError::InvalidRequest);
        }
        Ok(Self {
            holding_id,
            company_id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceCloseReviewStatus {
    Open,
    Review,
    Closed,
}

impl From<CloseStatus> for FinanceCloseReviewStatus {
    fn from(status: CloseStatus) -> Self {
        match status {
            CloseStatus::Open => Self::Open,
            CloseStatus::Review => Self::Review,
            CloseStatus::Closed => Self::Closed,
        }
    }
}

/// Safe close metadata.  The persisted aggregate JSON and command contexts
/// are intentionally absent; only bounded lifecycle and identity metadata is
/// returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReviewProjection {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub status: FinanceCloseReviewStatus,
    pub version: u64,
    pub idempotency_key: String,
    pub opened_by: String,
    pub reviewed_by: Option<String>,
    pub closed_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseStatementSummary {
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
    pub balanced: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseAccountSummary {
    pub account_id: Uuid,
    pub debit_minor: i64,
    pub credit_minor: i64,
    pub net_minor: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseControlSummary {
    pub total: u32,
    pub resolved: u32,
    pub unresolved: u32,
}

/// Bounded report data derived through the domain reporting APIs.  It does
/// not expose journal facts, control payloads, or command contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReport {
    pub statement: Option<FinanceCloseStatementSummary>,
    pub controls: FinanceCloseControlSummary,
    pub accounts: Vec<FinanceCloseAccountSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceCloseReadAvailability {
    Present,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinanceCloseReadFreshness {
    Fresh,
    Stale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadMetadata {
    pub period_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub availability: FinanceCloseReadAvailability,
    pub freshness: FinanceCloseReadFreshness,
    pub read_only: bool,
    pub review_updated_at: Option<DateTime<Utc>>,
    pub projection_updated_at: Option<DateTime<Utc>>,
    pub source_version: Option<u64>,
    pub expected_fact_count: u32,
    pub observed_fact_count: u32,
    pub missing_fact_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseReadView {
    pub review: FinanceCloseReviewProjection,
    pub report: FinanceCloseReport,
    pub metadata: FinanceCloseReadMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceCloseReadResult {
    Present(Box<FinanceCloseReadView>),
    Missing(FinanceCloseReadMetadata),
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceCloseReadAdapter {
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxFinanceCloseReadAdapter {
    pub fn new(
        read_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, FinanceCloseReadError> {
        let data_profile_key = data_profile_key.into();
        if data_profile_key.trim().is_empty()
            || data_profile_key.len() > 255
            || data_profile_key.chars().any(char::is_control)
        {
            return Err(FinanceCloseReadError::InvalidRequest);
        }
        Ok(Self {
            read_pool,
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    pub async fn read_review(
        &self,
        period_id: Uuid,
        scope: FinanceCloseReadScope,
        now: DateTime<Utc>,
    ) -> Result<FinanceCloseReadResult, FinanceCloseReadError> {
        if period_id.is_nil() || scope.holding_id.is_nil() || scope.company_id.is_nil() {
            return Err(FinanceCloseReadError::InvalidRequest);
        }
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key)
            .await
            .map_err(map_persistence_error)?;
        let row = sqlx::query(
            "SELECT holding_id, company_id, period_id, status,
                    idempotency_key, opened_by, reviewed_by, closed_by,
                    statement_present, statement_debit_minor,
                    statement_credit_minor, statement_net_minor,
                    statement_balanced, control_total_count,
                    control_resolved_count, control_unresolved_count,
                    expected_fact_count, observed_fact_count, missing_fact_count,
                    account_summary_count, source_version, source_event_id,
                    review_updated_at, projected_at
             FROM erpx_holding.finance_close_review_read_rows
             WHERE data_profile_key=$1 AND period_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(period_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
        let Some(row) = row else {
            tx.commit()
                .await
                .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
            return Ok(FinanceCloseReadResult::Missing(missing_metadata(
                period_id, scope,
            )));
        };
        let holding_id: Uuid = row
            .try_get("holding_id")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let company_id: Uuid = row
            .try_get("company_id")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let stored_period_id: Uuid = row
            .try_get("period_id")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        if holding_id != scope.holding_id
            || company_id != scope.company_id
            || stored_period_id != period_id
        {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        let status: String = row
            .try_get("status")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let status = match status.as_str() {
            "open" => FinanceCloseReviewStatus::Open,
            "review" => FinanceCloseReviewStatus::Review,
            "closed" => FinanceCloseReviewStatus::Closed,
            _ => return Err(FinanceCloseReadError::InvalidProjection),
        };
        let idempotency_key = bounded_text(
            &row.try_get::<String, _>("idempotency_key")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
        )?;
        let opened_by = bounded_text(
            &row.try_get::<String, _>("opened_by")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
        )?;
        let reviewed_by = row
            .try_get::<Option<String>, _>("reviewed_by")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?
            .as_deref()
            .map(bounded_text)
            .transpose()?;
        let closed_by = row
            .try_get::<Option<String>, _>("closed_by")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?
            .as_deref()
            .map(bounded_text)
            .transpose()?;
        let source_version = u64::try_from(
            row.try_get::<i64, _>("source_version")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
        )
        .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let source_event_id: Uuid = row
            .try_get("source_event_id")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        if source_version == 0 || source_event_id.is_nil() {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        let checkpoint = sqlx::query(
            "SELECT last_event_id, source_version
             FROM erpx_holding.finance_close_projection_checkpoints
             WHERE data_profile_key=$1 AND period_id=$2",
        )
        .bind(&self.data_profile_key)
        .bind(period_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
        let Some(checkpoint) = checkpoint else {
            return Err(FinanceCloseReadError::InvalidProjection);
        };
        let checkpoint_event_id: Uuid = checkpoint
            .try_get("last_event_id")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let checkpoint_source_version: i64 = checkpoint
            .try_get("source_version")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        validate_projection_checkpoint(
            source_version,
            source_event_id,
            checkpoint_source_version,
            checkpoint_event_id,
        )?;
        let version = source_version;
        let account_summary_count = bounded_i64_count(
            i64::from(
                row.try_get::<i32, _>("account_summary_count")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            ),
            "Finance close account summary count",
        )?;
        let account_summary_count = usize::try_from(account_summary_count)
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        if account_summary_count > MAX_FINANCE_CLOSE_REPORT_ACCOUNTS {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        let account_rows = sqlx::query(
            "SELECT holding_id, company_id, period_id, account_id,
                    debit_minor, credit_minor, net_minor,
                    source_version, source_event_id
             FROM erpx_holding.finance_close_account_read_rows
             WHERE data_profile_key=$1 AND period_id=$2
             ORDER BY account_id
             LIMIT $3",
        )
        .bind(&self.data_profile_key)
        .bind(period_id)
        .bind(
            i64::try_from(MAX_FINANCE_CLOSE_REPORT_ACCOUNTS.saturating_add(1))
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
        if account_rows.len() > MAX_FINANCE_CLOSE_REPORT_ACCOUNTS
            || account_rows.len() != account_summary_count
        {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        let accounts = account_rows
            .into_iter()
            .map(|account| {
                let account_holding_id: Uuid = account
                    .try_get("holding_id")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let account_company_id: Uuid = account
                    .try_get("company_id")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let account_period_id: Uuid = account
                    .try_get("period_id")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let account_id: Uuid = account
                    .try_get("account_id")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let account_source_version = u64::try_from(
                    account
                        .try_get::<i64, _>("source_version")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                )
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let account_source_event_id: Uuid = account
                    .try_get("source_event_id")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let debit_minor: i64 = account
                    .try_get("debit_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let credit_minor: i64 = account
                    .try_get("credit_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let net_minor: i64 = account
                    .try_get("net_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
                let expected_net = debit_minor
                    .checked_sub(credit_minor)
                    .ok_or(FinanceCloseReadError::InvalidProjection)?;
                if account_holding_id != holding_id
                    || account_company_id != company_id
                    || account_period_id != period_id
                    || account_id.is_nil()
                    || account_source_version != source_version
                    || account_source_event_id != source_event_id
                    || debit_minor < 0
                    || credit_minor < 0
                    || net_minor != expected_net
                {
                    return Err(FinanceCloseReadError::InvalidProjection);
                }
                Ok(FinanceCloseAccountSummary {
                    account_id,
                    debit_minor,
                    credit_minor,
                    net_minor,
                })
            })
            .collect::<Result<Vec<_>, FinanceCloseReadError>>()?;
        let statement_present: bool = row
            .try_get("statement_present")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let statement = if statement_present {
            Some(FinanceCloseStatementSummary {
                debit_minor: row
                    .try_get("statement_debit_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                credit_minor: row
                    .try_get("statement_credit_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                net_minor: row
                    .try_get("statement_net_minor")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                balanced: row
                    .try_get("statement_balanced")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            })
        } else {
            None
        };
        let controls = FinanceCloseControlSummary {
            total: bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("control_total_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close control total",
            )?,
            resolved: bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("control_resolved_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close control resolved",
            )?,
            unresolved: bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("control_unresolved_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close control unresolved",
            )?,
        };
        validate_control_summary_for_status(status, controls)?;
        let expected_fact_count = bounded_i64_count(
            i64::from(
                row.try_get::<i32, _>("expected_fact_count")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            ),
            "Finance close expected fact count",
        )?;
        let observed_fact_count = bounded_i64_count(
            i64::from(
                row.try_get::<i32, _>("observed_fact_count")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            ),
            "Finance close observed fact count",
        )?;
        let missing_fact_count = bounded_i64_count(
            i64::from(
                row.try_get::<i32, _>("missing_fact_count")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            ),
            "Finance close missing fact count",
        )?;
        if expected_fact_count != observed_fact_count.saturating_add(missing_fact_count) {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        let review_updated_at = row
            .try_get("review_updated_at")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let projection_updated_at = row
            .try_get("projected_at")
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
        let freshness = classify_freshness(missing_fact_count, Some(projection_updated_at), now);
        let review = FinanceCloseReviewProjection {
            holding_id,
            company_id,
            period_id,
            status,
            version,
            idempotency_key,
            opened_by,
            reviewed_by,
            closed_by,
        };
        let report = FinanceCloseReport {
            statement,
            controls,
            accounts,
        };
        let metadata = FinanceCloseReadMetadata {
            period_id,
            holding_id,
            company_id,
            availability: FinanceCloseReadAvailability::Present,
            freshness,
            read_only: true,
            review_updated_at: Some(review_updated_at),
            projection_updated_at: Some(projection_updated_at),
            source_version: Some(source_version),
            expected_fact_count,
            observed_fact_count,
            missing_fact_count,
        };
        Ok(FinanceCloseReadResult::Present(Box::new(
            FinanceCloseReadView {
                review,
                report,
                metadata,
            },
        )))
    }

    /// Return only bounded lifecycle/report scalars for every projected close
    /// review in one explicit company scope. The read pool and its checkpoint
    /// are authoritative for this query; aggregate payloads and journal facts
    /// remain outside the transport boundary.
    pub async fn list_reviews(
        &self,
        scope: FinanceCloseReadScope,
        page_size: u32,
        before: Option<FinanceCloseReadCursor>,
        now: DateTime<Utc>,
    ) -> Result<FinanceCloseReadPage, FinanceCloseReadError> {
        if page_size == 0 || page_size > MAX_FINANCE_CLOSE_READ_PAGE_SIZE {
            return Err(FinanceCloseReadError::InvalidRequest);
        }
        let mut tx = begin_scoped(&self.read_pool, &self.data_profile_key)
            .await
            .map_err(map_persistence_error)?;
        let limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT r.holding_id, r.company_id, r.period_id, r.status,
                    r.source_version, r.source_event_id, r.projected_at,
                    r.statement_present, r.statement_net_minor,
                    r.statement_balanced, r.control_total_count,
                    r.control_unresolved_count, r.missing_fact_count,
                    c.last_event_id,
                    c.source_version AS checkpoint_source_version
             FROM erpx_holding.finance_close_review_read_rows r
             JOIN erpx_holding.finance_close_projection_checkpoints c
               ON c.data_profile_key = r.data_profile_key AND c.period_id = r.period_id
             WHERE r.data_profile_key=$1 AND r.holding_id=$2 AND r.company_id=$3
               AND ($4::timestamptz IS NULL
                    OR (r.projected_at, r.period_id) < ($4::timestamptz, $5::uuid))
             ORDER BY r.projected_at DESC, r.period_id DESC
             LIMIT $6",
        )
        .bind(&self.data_profile_key)
        .bind(scope.holding_id)
        .bind(scope.company_id)
        .bind(before.as_ref().map(|cursor| cursor.projected_at))
        .bind(before.as_ref().map(|cursor| cursor.period_id))
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| FinanceCloseReadError::PersistenceUnavailable)?;

        let has_more = rows.len() > page_size as usize;
        let rows = rows
            .into_iter()
            .take(page_size as usize)
            .collect::<Vec<_>>();
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let holding_id: Uuid = row
                .try_get("holding_id")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let company_id: Uuid = row
                .try_get("company_id")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let period_id: Uuid = row
                .try_get("period_id")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let source_version = u64::try_from(
                row.try_get::<i64, _>("source_version")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            )
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let checkpoint_source_version = u64::try_from(
                row.try_get::<i64, _>("checkpoint_source_version")
                    .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
            )
            .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let source_event_id: Uuid = row
                .try_get("source_event_id")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let checkpoint_event_id: Uuid = row
                .try_get("last_event_id")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let projected_at: DateTime<Utc> = row
                .try_get("projected_at")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            if holding_id != scope.holding_id
                || company_id != scope.company_id
                || period_id.is_nil()
                || source_version == 0
                || source_version != checkpoint_source_version
                || source_event_id.is_nil()
                || source_event_id != checkpoint_event_id
            {
                return Err(FinanceCloseReadError::InvalidProjection);
            }
            let status = match row
                .try_get::<String, _>("status")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?
                .as_str()
            {
                "open" => FinanceCloseReviewStatus::Open,
                "review" => FinanceCloseReviewStatus::Review,
                "closed" => FinanceCloseReviewStatus::Closed,
                _ => return Err(FinanceCloseReadError::InvalidProjection),
            };
            let control_total = bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("control_total_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close list control total",
            )?;
            let unresolved_controls = bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("control_unresolved_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close list unresolved controls",
            )?;
            validate_control_summary_for_status(
                status,
                FinanceCloseControlSummary {
                    total: control_total,
                    resolved: control_total.saturating_sub(unresolved_controls),
                    unresolved: unresolved_controls,
                },
            )?;
            let missing_fact_count = bounded_i64_count(
                i64::from(
                    row.try_get::<i32, _>("missing_fact_count")
                        .map_err(|_| FinanceCloseReadError::InvalidProjection)?,
                ),
                "Finance close list missing facts",
            )?;
            let statement_present: bool = row
                .try_get("statement_present")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let net_minor: Option<i64> = row
                .try_get("statement_net_minor")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            let balanced: Option<bool> = row
                .try_get("statement_balanced")
                .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
            if statement_present != net_minor.is_some() || statement_present != balanced.is_some() {
                return Err(FinanceCloseReadError::InvalidProjection);
            }
            result.push(FinanceCloseReadListRow {
                holding_id,
                company_id,
                period_id,
                status,
                source_version,
                projected_at,
                freshness: classify_freshness(missing_fact_count, Some(projected_at), now),
                control_total,
                unresolved_controls,
                net_minor,
                statement_present,
                balanced: balanced.unwrap_or(false),
            });
        }
        let next_cursor = has_more
            .then(|| {
                result.last().map(|row| FinanceCloseReadCursor {
                    projected_at: row.projected_at,
                    period_id: row.period_id,
                })
            })
            .flatten();
        Ok(FinanceCloseReadPage {
            rows: result,
            next_cursor,
            has_more,
        })
    }
}

#[cfg(test)]
fn project_review(
    snapshot: &CloseReviewSnapshot,
) -> Result<FinanceCloseReviewProjection, FinanceCloseReadError> {
    if snapshot.version == 0
        || snapshot.review.period_id().is_nil()
        || snapshot.review.holding_id().is_nil()
        || snapshot.review.company_id().is_nil()
    {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    let idempotency_key = bounded_text(&snapshot.idempotency_key)?;
    let opened_by = bounded_text(snapshot.review.opened_by())?;
    let reviewed_by = snapshot
        .review
        .reviewed_by()
        .map(bounded_text)
        .transpose()?;
    let closed_by = snapshot.review.closed_by().map(bounded_text).transpose()?;
    Ok(FinanceCloseReviewProjection {
        holding_id: snapshot.review.holding_id(),
        company_id: snapshot.review.company_id(),
        period_id: snapshot.review.period_id(),
        status: snapshot.review.status().into(),
        version: snapshot.version,
        idempotency_key,
        opened_by,
        reviewed_by,
        closed_by,
    })
}

#[cfg(test)]
fn build_report(review: &FinanceCloseReview) -> Result<FinanceCloseReport, FinanceCloseReadError> {
    let Some(input) = review.input() else {
        if review.status() != CloseStatus::Open {
            return Err(FinanceCloseReadError::InvalidProjection);
        }
        return Ok(FinanceCloseReport {
            statement: None,
            controls: FinanceCloseControlSummary {
                total: 0,
                resolved: 0,
                unresolved: 0,
            },
            accounts: Vec::new(),
        });
    };
    if input.facts().len() > MAX_FINANCE_CLOSE_REPORT_FACTS
        || input.controls().len() > MAX_FINANCE_CLOSE_REPORT_CONTROLS
    {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    let totals = input
        .statement_totals()
        .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
    let accounts = input
        .drill_down()
        .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
    if accounts.len() > MAX_FINANCE_CLOSE_REPORT_ACCOUNTS {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    let total = bounded_count(input.controls().len(), "Finance close control count")?;
    let resolved = bounded_count(
        input
            .controls()
            .iter()
            .filter(|control| control.resolved())
            .count(),
        "Finance close resolved control count",
    )?;
    let unresolved = total
        .checked_sub(resolved)
        .ok_or(FinanceCloseReadError::InvalidProjection)?;
    Ok(FinanceCloseReport {
        statement: Some(FinanceCloseStatementSummary {
            debit_minor: totals.debit_minor,
            credit_minor: totals.credit_minor,
            net_minor: totals.net_minor,
            balanced: totals.is_balanced(),
        }),
        controls: FinanceCloseControlSummary {
            total,
            resolved,
            unresolved,
        },
        accounts: accounts
            .into_iter()
            .map(|row| FinanceCloseAccountSummary {
                account_id: row.account_id,
                debit_minor: row.debit_minor,
                credit_minor: row.credit_minor,
                net_minor: row.net_minor,
            })
            .collect(),
    })
}

fn missing_metadata(period_id: Uuid, scope: FinanceCloseReadScope) -> FinanceCloseReadMetadata {
    FinanceCloseReadMetadata {
        period_id,
        holding_id: scope.holding_id,
        company_id: scope.company_id,
        availability: FinanceCloseReadAvailability::Missing,
        freshness: FinanceCloseReadFreshness::Unknown,
        read_only: true,
        review_updated_at: None,
        projection_updated_at: None,
        source_version: None,
        expected_fact_count: 0,
        observed_fact_count: 0,
        missing_fact_count: 0,
    }
}

fn classify_freshness(
    missing_fact_count: u32,
    projected_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> FinanceCloseReadFreshness {
    if missing_fact_count > 0 {
        return FinanceCloseReadFreshness::Stale;
    }
    let Some(projected_at) = projected_at else {
        return FinanceCloseReadFreshness::Stale;
    };
    if projected_at > now {
        return FinanceCloseReadFreshness::Unknown;
    }
    if now.signed_duration_since(projected_at)
        > Duration::seconds(FINANCE_CLOSE_READ_STALE_AFTER_SECONDS)
    {
        FinanceCloseReadFreshness::Stale
    } else {
        FinanceCloseReadFreshness::Fresh
    }
}

fn validate_projection_checkpoint(
    source_version: u64,
    source_event_id: Uuid,
    checkpoint_source_version: i64,
    checkpoint_event_id: Uuid,
) -> Result<(), FinanceCloseReadError> {
    let checkpoint_source_version = u64::try_from(checkpoint_source_version)
        .map_err(|_| FinanceCloseReadError::InvalidProjection)?;
    if checkpoint_source_version == 0
        || checkpoint_event_id.is_nil()
        || source_version != checkpoint_source_version
        || source_event_id != checkpoint_event_id
    {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    Ok(())
}

fn validate_control_summary_for_status(
    status: FinanceCloseReviewStatus,
    controls: FinanceCloseControlSummary,
) -> Result<(), FinanceCloseReadError> {
    if controls.resolved > controls.total
        || controls.unresolved > controls.total
        || controls.resolved.checked_add(controls.unresolved) != Some(controls.total)
        || (status == FinanceCloseReviewStatus::Closed && controls.unresolved != 0)
    {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    Ok(())
}

fn bounded_text(value: &str) -> Result<String, FinanceCloseReadError> {
    if value.trim().is_empty()
        || value.len() > MAX_FINANCE_CLOSE_ACTOR_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    Ok(value.to_owned())
}

fn bounded_count(value: usize, _field: &'static str) -> Result<u32, FinanceCloseReadError> {
    u32::try_from(value).map_err(|_| FinanceCloseReadError::InvalidProjection)
}

fn bounded_i64_count(value: i64, field: &'static str) -> Result<u32, FinanceCloseReadError> {
    if value < 0 {
        return Err(FinanceCloseReadError::InvalidProjection);
    }
    bounded_count(
        usize::try_from(value).map_err(|_| FinanceCloseReadError::InvalidProjection)?,
        field,
    )
}

fn map_persistence_error(error: PersistenceError) -> FinanceCloseReadError {
    match error {
        PersistenceError::Database(_) | PersistenceError::Migration(_) => {
            FinanceCloseReadError::PersistenceUnavailable
        }
        PersistenceError::InvalidData(_)
        | PersistenceError::Serialization(_)
        | PersistenceError::OptimisticConcurrency { .. }
        | PersistenceError::IdempotencyConflict { .. }
        | PersistenceError::NotFound { .. }
        | PersistenceError::ClaimLost { .. }
        | PersistenceError::Configuration(_) => FinanceCloseReadError::InvalidProjection,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
    use erpx_domain::finance::AccountingPeriod;
    use erpx_domain::finance_close::{
        CloseControlFlag, CloseInput, CloseJournalFact, CloseJournalLine,
    };

    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
    }

    fn context(holding_id: Uuid, company_id: Uuid, actor: &str) -> CommandContext {
        CommandContext::try_new(
            format!("close-read-{actor}"),
            ActorAttribution {
                principal_ref: actor.into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "finance.period_close",
        )
        .unwrap()
    }

    fn review() -> CloseReviewSnapshot {
        let (holding_id, company_id, period_id) = ids();
        let period = AccountingPeriod::new(
            period_id,
            holding_id,
            company_id,
            "2026-01",
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
        )
        .unwrap();
        let aggregate =
            FinanceCloseReview::open(&period, &context(holding_id, company_id, "opener")).unwrap();
        CloseReviewSnapshot {
            review: aggregate,
            version: 1,
            idempotency_key: "close-read-1".into(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn status_mapping_is_explicit_and_stable() {
        assert_eq!(
            FinanceCloseReviewStatus::from(CloseStatus::Open),
            FinanceCloseReviewStatus::Open
        );
        assert_eq!(
            FinanceCloseReviewStatus::from(CloseStatus::Review),
            FinanceCloseReviewStatus::Review
        );
        assert_eq!(
            FinanceCloseReviewStatus::from(CloseStatus::Closed),
            FinanceCloseReviewStatus::Closed
        );
    }

    #[test]
    fn scope_identity_and_safe_projection_are_enforced() {
        let snapshot = review();
        let projected = project_review(&snapshot).unwrap();
        assert_eq!(projected.holding_id, snapshot.review.holding_id());
        assert_eq!(projected.company_id, snapshot.review.company_id());
        assert_eq!(projected.period_id, snapshot.review.period_id());
        let serialized = serde_json::to_string(&projected).unwrap();
        assert!(!serialized.contains("payload"));
        assert!(!serialized.contains("opened_context"));
        assert!(!serialized.contains("input"));
        assert!(bounded_text("actor\nname").is_err());
    }

    #[test]
    fn missing_and_stale_metadata_are_explicit() {
        let (holding_id, company_id, period_id) = ids();
        let scope = FinanceCloseReadScope::new(holding_id, company_id).unwrap();
        let missing = missing_metadata(period_id, scope);
        assert_eq!(missing.availability, FinanceCloseReadAvailability::Missing);
        assert_eq!(missing.freshness, FinanceCloseReadFreshness::Unknown);
        let stale = classify_freshness(
            0,
            Some(Utc::now() - Duration::seconds(FINANCE_CLOSE_READ_STALE_AFTER_SECONDS + 1)),
            Utc::now(),
        );
        assert_eq!(stale, FinanceCloseReadFreshness::Stale);
        assert_eq!(
            classify_freshness(1, Some(Utc::now()), Utc::now()),
            FinanceCloseReadFreshness::Stale
        );
    }

    #[test]
    fn report_is_bounded_and_uses_domain_totals() {
        let snapshot = review();
        let mut aggregate = snapshot.review;
        let holding_id = aggregate.holding_id();
        let company_id = aggregate.company_id();
        let period_id = aggregate.period_id();
        let fact = CloseJournalFact::new(
            Uuid::from_u128(10),
            holding_id,
            company_id,
            period_id,
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            vec![
                CloseJournalLine::new(Uuid::from_u128(20), 100, 0).unwrap(),
                CloseJournalLine::new(Uuid::from_u128(21), 0, 100).unwrap(),
            ],
        )
        .unwrap();
        aggregate
            .submit_for_review(
                &AccountingPeriod::new(
                    period_id,
                    holding_id,
                    company_id,
                    "2026-01",
                    NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
                )
                .unwrap(),
                &context(holding_id, company_id, "reviewer"),
                CloseInput::new(
                    vec![fact],
                    vec![CloseControlFlag::new("bank", true).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        let report = build_report(&aggregate).unwrap();
        assert_eq!(report.statement.unwrap().debit_minor, 100);
        assert_eq!(
            report.controls,
            FinanceCloseControlSummary {
                total: 1,
                resolved: 1,
                unresolved: 0
            }
        );
        assert_eq!(report.accounts.len(), 2);
    }

    #[test]
    fn invalid_scope_and_counts_fail_closed() {
        assert_eq!(
            FinanceCloseReadScope::new(Uuid::nil(), Uuid::from_u128(2)),
            Err(FinanceCloseReadError::InvalidRequest)
        );
        assert_eq!(
            bounded_count(usize::MAX, "test"),
            Err(FinanceCloseReadError::InvalidProjection)
        );
        assert_eq!(
            bounded_i64_count(-1, "test"),
            Err(FinanceCloseReadError::InvalidProjection)
        );
        assert!(
            validate_control_summary_for_status(
                FinanceCloseReviewStatus::Closed,
                FinanceCloseControlSummary {
                    total: 1,
                    resolved: 0,
                    unresolved: 1,
                },
            )
            .is_err()
        );
        assert!(
            validate_control_summary_for_status(
                FinanceCloseReviewStatus::Review,
                FinanceCloseControlSummary {
                    total: 1,
                    resolved: 0,
                    unresolved: 1,
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn singular_read_requires_matching_projection_checkpoint() {
        let event_id = Uuid::from_u128(4);
        assert!(validate_projection_checkpoint(1, event_id, 0, event_id).is_err());
        assert!(validate_projection_checkpoint(1, event_id, 2, event_id).is_err());
        assert!(validate_projection_checkpoint(1, event_id, 1, Uuid::from_u128(5)).is_err());
        assert!(validate_projection_checkpoint(1, event_id, 1, Uuid::nil()).is_err());
        validate_projection_checkpoint(1, event_id, 1, event_id)
            .expect("projected row and checkpoint must share source identity");
    }
}
