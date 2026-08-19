//! Holding-routed SQLx persistence for the Finance write boundary.
//!
//! Posted journals are immutable authoritative rows. Trial balance is
//! calculated from those rows; no balance is accepted from a client or read
//! projection.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_domain::finance::{
    Account, AccountKind, AccountingPeriod, ChartOfAccounts, ConsolidatedFinancialReport,
    ConsolidatedTrialBalance, ConsolidationEliminationEntry, ConsolidationFxRateSet,
    ConsolidationOwnership, JournalEntry, PeriodStatus, PostReceipt, PostedJournalEntry,
    calculate_consolidated_financial_report, calculate_consolidated_trial_balance,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_chart_persistence::mirror_legacy_chart_into_normalized;

/// A complete report is still bounded to protect the read route, but it is
/// large enough for the supported holding book size. The previous 500-row
/// limit silently omitted older posted journals from trial balance totals.
pub const MAX_FINANCE_TRIAL_BALANCE_JOURNALS: u32 = 1_000_000;
pub const MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_READ_JOURNAL_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_CONSOLIDATED_CHARTS: usize = 1_000;
pub const MAX_FINANCE_PROJECTION_RECONCILIATION_JOURNALS: u32 = MAX_FINANCE_TRIAL_BALANCE_JOURNALS;

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceReadJournal {
    pub journal_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub source_version: u64,
    pub posted_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionRebuildPreparation {
    pub data_profile_key: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionReconciliation {
    pub source_events: u64,
    pub projected_rows: u64,
    pub mismatched_rows: u64,
    pub source_event_high_watermark: u64,
    pub source_stable: bool,
    pub complete: bool,
}

impl FinanceProjectionReconciliation {
    pub const fn is_reconciled(&self) -> bool {
        self.complete && self.source_stable && self.mismatched_rows == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceSourceEventPage {
    pub events: Vec<FinanceSourceEvent>,
    pub next_cursor: Option<FinanceSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceStore {
    write_pool: PgPool,
    read_pool: PgPool,
    data_profile_key: String,
}

impl SqlxFinanceStore {
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            read_pool,
            data_profile_key,
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), data_profile_key)
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    pub async fn save_chart(
        &self,
        chart: &ChartOfAccounts,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        chart
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(chart)?;
        let mut tx = self.begin_write().await?;
        let next = expected_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                PersistenceError::InvalidData("finance chart version overflow".into())
            })?;
        let affected = if expected_version.is_some() {
            sqlx::query(
                "UPDATE erpx_holding.finance_charts
                 SET version=$2, payload=$3, updated_at=now()
                 WHERE chart_id=$1 AND data_profile_key=$4 AND version=$5",
            )
            .bind(chart.id)
            .bind(as_i64(next, "chart version")?)
            .bind(&payload)
            .bind(&self.data_profile_key)
            .bind(as_i64(expected_version.unwrap_or(0), "chart version")?)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.finance_charts
                    (chart_id, holding_id, company_id, data_profile_key, version, payload)
                 VALUES ($1,$2,$3,$4,1,$5) ON CONFLICT (chart_id) DO NOTHING",
            )
            .bind(chart.id)
            .bind(chart.holding_id)
            .bind(chart.company_id)
            .bind(&self.data_profile_key)
            .bind(&payload)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        };
        if affected != 1 {
            let actual = sqlx::query_scalar::<_, i64>(
                "SELECT version FROM erpx_holding.finance_charts
                 WHERE chart_id=$1 AND data_profile_key=$2",
            )
            .bind(chart.id)
            .bind(&self.data_profile_key)
            .fetch_optional(&mut *tx)
            .await?
            .map(|value| u64::try_from(value).unwrap_or(u64::MAX));
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "finance_chart".into(),
                aggregate_id: chart.id,
                expected: expected_version,
                actual,
            });
        }
        mirror_legacy_chart_into_normalized(
            &mut tx,
            &self.data_profile_key,
            chart,
            expected_version,
            next,
        )
        .await?;
        write_finance_source_event(
            &mut tx,
            &self.data_profile_key,
            "finance_chart",
            chart.id,
            next,
            &payload,
            "finance.chart_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(next)
    }

    pub async fn save_period(
        &self,
        period: &AccountingPeriod,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        period
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(period)?;
        let status = period_status(period.status);
        let mut tx = self.begin_write().await?;
        let next = expected_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                PersistenceError::InvalidData("finance period version overflow".into())
            })?;
        let affected = if let Some(expected) = expected_version {
            let current = sqlx::query(
                "SELECT holding_id, company_id
                   FROM erpx_holding.finance_periods
                  WHERE period_id=$1 AND data_profile_key=$2
                  FOR UPDATE",
            )
            .bind(period.id)
            .bind(&self.data_profile_key)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(current) = current
                && (current.try_get::<Uuid, _>("holding_id")? != period.holding_id
                    || current.try_get::<Uuid, _>("company_id")? != period.company_id)
            {
                return Err(PersistenceError::InvalidData(
                    "Finance accounting period identity is immutable".into(),
                ));
            }
            sqlx::query(
                "UPDATE erpx_holding.finance_periods
                 SET status=$2, version=$3, payload=$4, updated_at=now()
                 WHERE period_id=$1 AND data_profile_key=$5 AND version=$6",
            )
            .bind(period.id)
            .bind(status)
            .bind(as_i64(next, "period version")?)
            .bind(&payload)
            .bind(&self.data_profile_key)
            .bind(as_i64(expected, "period version")?)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "INSERT INTO erpx_holding.finance_periods
                    (period_id, holding_id, company_id, data_profile_key, status, version, payload)
                 VALUES ($1,$2,$3,$4,$5,1,$6) ON CONFLICT (period_id) DO NOTHING",
            )
            .bind(period.id)
            .bind(period.holding_id)
            .bind(period.company_id)
            .bind(&self.data_profile_key)
            .bind(status)
            .bind(&payload)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        };
        if affected != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "finance_period".into(),
                aggregate_id: period.id,
                expected: expected_version,
                actual: None,
            });
        }
        write_finance_source_event(
            &mut tx,
            &self.data_profile_key,
            "finance_period",
            period.id,
            next,
            &payload,
            "finance.period_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(next)
    }

    /// Validate and atomically insert one immutable posted journal. The
    /// period row is locked before the command is accepted, and idempotency
    /// replay returns the original posted DTO.
    pub async fn post_journal(
        &self,
        entry: JournalEntry,
        chart: &ChartOfAccounts,
        period: &AccountingPeriod,
        posted_at: DateTime<Utc>,
    ) -> Result<PostReceipt, PersistenceError> {
        entry
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        period
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let request_hash = sha256_hex(&serde_json::to_vec(&(&entry, posted_at))?);
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(
            "SELECT request_hash, journal_id, response
             FROM erpx_holding.finance_command_idempotency
             WHERE idempotency_key=$1 AND data_profile_key=$2 FOR UPDATE",
        )
        .bind(entry.idempotency_key())
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_hash")? != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: entry.idempotency_key().into(),
                });
            }
            let stored_journal_id: Uuid = row.try_get("journal_id")?;
            let existing_response: Value = row.try_get("response")?;
            let existing: PostedJournalEntry = serde_json::from_value(existing_response.clone())?;
            validate_posted_journal_replay_evidence(
                &mut tx,
                &self.data_profile_key,
                stored_journal_id,
                &existing,
            )
            .await?;
            tx.commit().await?;
            return Ok(PostReceipt {
                journal: existing,
                replayed: true,
            });
        }

        let authoritative =
            load_authoritative_posting_chart(&mut tx, &self.data_profile_key, &entry, chart)
                .await?;
        let authoritative_period =
            load_authoritative_period(&mut tx, &self.data_profile_key, period.id).await?;
        if authoritative_period != *period {
            return Err(PersistenceError::InvalidData(
                "Finance accounting period does not match authoritative holding write state".into(),
            ));
        }
        let posted = entry
            .clone()
            .post(&authoritative.chart, &authoritative_period, posted_at)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if let Some(existing) = sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM erpx_holding.finance_posted_journals
             WHERE journal_id=$1 AND data_profile_key=$2",
        )
        .bind(posted.id())
        .bind(&self.data_profile_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let existing: PostedJournalEntry = serde_json::from_value(existing)?;
            if existing.idempotency_key() == posted.idempotency_key() {
                validate_posted_journal_replay_evidence(
                    &mut tx,
                    &self.data_profile_key,
                    posted.id(),
                    &existing,
                )
                .await?;
                tx.commit().await?;
                return Ok(PostReceipt {
                    journal: existing,
                    replayed: true,
                });
            }
            return Err(PersistenceError::InvalidData(
                "finance journal ID already exists".into(),
            ));
        }
        let response = serde_json::to_value(&posted)?;
        let total_minor = posted
            .lines()
            .iter()
            .try_fold(0_i64, |total, line| total.checked_add(line.debit_minor))
            .ok_or_else(|| {
                PersistenceError::InvalidData("finance journal total overflowed".into())
            })?;
        sqlx::query(
            "INSERT INTO erpx_holding.finance_posted_journals
                (journal_id, holding_id, company_id, period_id, data_profile_key, version,
                 idempotency_key, posted_at, currency, total_minor, payload)
             VALUES ($1,$2,$3,$4,$5,1,$6,$7,$8,$9,$10)",
        )
        .bind(posted.id())
        .bind(posted.holding_id())
        .bind(posted.company_id())
        .bind(posted.period_id())
        .bind(&self.data_profile_key)
        .bind(posted.idempotency_key())
        .bind(posted.posted_at())
        .bind(&authoritative.chart.currency)
        .bind(total_minor)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        record_authoritative_account_usage(
            &mut tx,
            &self.data_profile_key,
            &posted,
            authoritative.chart.id,
            authoritative.chart_version,
            &authoritative.line_counts,
        )
        .await?;
        let event_payload = serde_json::json!({
            "posted_journal": response,
            "context": posted.context(),
        });
        sqlx::query(
            "INSERT INTO erpx_holding.outbox_events
                (event_id, aggregate_type, aggregate_id, data_profile_key,
                 aggregate_version, event_type, payload, occurred_at)
             VALUES ($1,$2,$3,$4,1,$5,$6,$7)",
        )
        .bind(Uuid::new_v4())
        .bind("finance_journal")
        .bind(posted.id())
        .bind(&self.data_profile_key)
        .bind("finance.journal_posted.v1")
        .bind(event_payload)
        .bind(posted.posted_at())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_holding.finance_command_idempotency
                (idempotency_key, data_profile_key, request_hash, journal_id, response)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(posted.idempotency_key())
        .bind(&self.data_profile_key)
        .bind(request_hash)
        .bind(posted.id())
        .bind(response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(PostReceipt {
            journal: posted,
            replayed: false,
        })
    }

    pub async fn trial_balance(
        &self,
        chart: &ChartOfAccounts,
    ) -> Result<erpx_domain::finance::TrialBalance, PersistenceError> {
        chart
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        self.validate_authoritative_chart_for_read(chart).await?;
        let reconciliation = self.reconcile_projection().await?;
        if !reconciliation.is_reconciled() {
            return Err(PersistenceError::InvalidData(
                "Finance trial balance requires a reconciled and stable journal read model".into(),
            ));
        }
        let source_fence = reconciliation.source_event_high_watermark;
        let mut tx = self.begin_read().await?;
        let rows = sqlx::query(
            "SELECT payload FROM erpx_holding.finance_posted_journals
             WHERE data_profile_key=$1 AND company_id=$2
             ORDER BY posted_at, journal_id LIMIT $3",
        )
        .bind(&self.data_profile_key)
        .bind(chart.company_id)
        .bind(i64::from(MAX_FINANCE_TRIAL_BALANCE_JOURNALS) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        if rows.len() > MAX_FINANCE_TRIAL_BALANCE_JOURNALS as usize {
            return Err(PersistenceError::InvalidData(
                "Finance trial balance exceeds the complete-report bound".into(),
            ));
        }
        let journals = rows
            .into_iter()
            .map(|row| -> Result<PostedJournalEntry, PersistenceError> {
                let payload: Value = row.try_get("payload")?;
                Ok(serde_json::from_value(payload)?)
            })
            .collect::<Result<Vec<PostedJournalEntry>, PersistenceError>>()?;
        if self.source_event_high_watermark().await? != source_fence {
            return Err(PersistenceError::InvalidData(
                "Finance trial balance source changed while it was assembled".into(),
            ));
        }
        chart
            .trial_balance(&journals)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))
    }

    /// Aggregate all projected immutable journals for the explicitly supplied
    /// company charts. The existing Finance source events and read projector
    /// remain authoritative; this method adds no inferred or client-owned
    /// balances and keeps the complete report bounded.
    pub async fn consolidated_trial_balance(
        &self,
        charts: &[ChartOfAccounts],
    ) -> Result<(ConsolidatedTrialBalance, u64), PersistenceError> {
        if charts.is_empty() || charts.len() > MAX_FINANCE_CONSOLIDATED_CHARTS {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation chart count is outside its bound".into(),
            ));
        }
        for chart in charts {
            chart
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            self.validate_authoritative_chart_for_read(chart).await?;
        }
        let reconciliation = self.reconcile_projection().await?;
        if !reconciliation.is_reconciled() {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation requires a reconciled and stable journal read model".into(),
            ));
        }
        let source_fence = reconciliation.source_event_high_watermark;
        let (journals, source_version) = self.read_all_projected_journals().await?;
        if self.source_event_high_watermark().await? != source_fence {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation source changed while it was assembled".into(),
            ));
        }
        let report = calculate_consolidated_trial_balance(charts, &journals)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        Ok((report, source_version))
    }

    async fn validate_authoritative_chart_for_read(
        &self,
        supplied_chart: &ChartOfAccounts,
    ) -> Result<(), PersistenceError> {
        let mut tx = self.begin_write().await?;
        let chart_row = sqlx::query(
            "SELECT holding_id, company_id, chart_name, version
               FROM erpx_holding.finance_chart_of_accounts
              WHERE data_profile_key=$1 AND chart_id=$2
              FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(supplied_chart.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "authoritative Finance chart",
            id: supplied_chart.id,
        })?;
        let holding_id: Uuid = chart_row.try_get("holding_id")?;
        let company_id: Uuid = chart_row.try_get("company_id")?;
        let normalized_version = as_u64(chart_row.try_get("version")?, "Finance chart version")?;
        if holding_id != supplied_chart.holding_id || company_id != supplied_chart.company_id {
            return Err(PersistenceError::InvalidData(
                "Finance read chart scope does not match normalized authority".into(),
            ));
        }
        let chart_name: String = chart_row.try_get("chart_name")?;
        let account_rows = sqlx::query(
            "SELECT account_id, account_code, account_name, category,
                    parent_account_id, status
               FROM erpx_holding.finance_chart_accounts
              WHERE data_profile_key=$1 AND chart_id=$2
              ORDER BY account_id
              FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(supplied_chart.id)
        .fetch_all(&mut *tx)
        .await?;
        let accounts = account_rows
            .into_iter()
            .map(|row| {
                Ok(Account {
                    id: row.try_get("account_id")?,
                    code: row.try_get("account_code")?,
                    name: row.try_get("account_name")?,
                    kind: parse_authoritative_account_kind(&row.try_get::<String, _>("category")?)?,
                    parent_id: row.try_get("parent_account_id")?,
                    active: parse_authoritative_account_status(
                        &row.try_get::<String, _>("status")?,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let normalized = ChartOfAccounts::new(
            supplied_chart.id,
            holding_id,
            company_id,
            chart_name,
            supplied_chart.currency.clone(),
            accounts,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let legacy = sqlx::query(
            "SELECT holding_id, company_id, version, payload
               FROM erpx_holding.finance_charts
              WHERE data_profile_key=$1 AND chart_id=$2
              FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(supplied_chart.id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        if normalized != *supplied_chart {
            return Err(PersistenceError::InvalidData(
                "Finance read chart differs from normalized account authority".into(),
            ));
        }
        if let Some(legacy) = legacy {
            let legacy_holding_id: Uuid = legacy.try_get("holding_id")?;
            let legacy_company_id: Uuid = legacy.try_get("company_id")?;
            let legacy_version = as_u64(legacy.try_get("version")?, "Finance chart version")?;
            let legacy_chart: ChartOfAccounts = serde_json::from_value(legacy.try_get("payload")?)?;
            legacy_chart
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            if legacy_holding_id != holding_id
                || legacy_company_id != company_id
                || legacy_version != normalized_version
                || legacy_chart != *supplied_chart
            {
                return Err(PersistenceError::InvalidData(
                    "Finance read chart differs from mirrored source authority".into(),
                ));
            }
        }
        Ok(())
    }

    /// Calculate an FX/ownership/elimination-aware report from the complete
    /// disposable journal read model, but only after source/read projection
    /// reconciliation proves that the read model is complete and stable.
    #[allow(clippy::too_many_arguments)]
    pub async fn consolidated_financial_report(
        &self,
        report_id: Uuid,
        charts: &[ChartOfAccounts],
        reporting_currency: impl Into<String>,
        as_of: chrono::NaiveDate,
        ownership: ConsolidationOwnership,
        fx_rates: ConsolidationFxRateSet,
        eliminations: Vec<ConsolidationEliminationEntry>,
    ) -> Result<ConsolidatedFinancialReport, PersistenceError> {
        if charts.is_empty() || charts.len() > MAX_FINANCE_CONSOLIDATED_CHARTS {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation chart count is outside its bound".into(),
            ));
        }
        let reconciliation = self.reconcile_projection().await?;
        if !reconciliation.is_reconciled() {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation requires a reconciled and stable journal read model".into(),
            ));
        }
        let source_fence = self.source_event_high_watermark().await?;
        let (journals, _) = self.read_all_projected_journals().await?;
        let report = calculate_consolidated_financial_report(
            report_id,
            charts,
            &journals,
            reporting_currency,
            as_of,
            ownership,
            fx_rates,
            eliminations,
            source_fence,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if self.source_event_high_watermark().await? != source_fence {
            return Err(PersistenceError::InvalidData(
                "Finance consolidation source changed while the report was assembled".into(),
            ));
        }
        Ok(report)
    }

    async fn read_all_projected_journals(
        &self,
    ) -> Result<(Vec<PostedJournalEntry>, u64), PersistenceError> {
        let mut journals = Vec::new();
        let mut source_version = 0_u64;
        let mut before_journal_id = None;
        loop {
            let (page, has_more) = self
                .read_journal_page(MAX_FINANCE_READ_JOURNAL_PAGE_SIZE, before_journal_id)
                .await?;
            if page.is_empty() && has_more {
                return Err(PersistenceError::InvalidData(
                    "Finance consolidation pagination made no progress".into(),
                ));
            }
            source_version = page.iter().fold(source_version, |current, journal| {
                current.max(journal.source_version)
            });
            before_journal_id = page.last().map(|journal| journal.journal_id);
            journals.extend(
                page.into_iter()
                    .map(|journal| serde_json::from_value(journal.payload))
                    .collect::<Result<Vec<PostedJournalEntry>, _>>()?,
            );
            if !has_more {
                break;
            }
            if before_journal_id.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Finance consolidation cursor is missing".into(),
                ));
            }
        }
        Ok((journals, source_version))
    }

    pub async fn read_journal_page(
        &self,
        page_size: u32,
        before_journal_id: Option<Uuid>,
    ) -> Result<(Vec<FinanceReadJournal>, bool), PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_READ_JOURNAL_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance journal page size must be 1..={MAX_FINANCE_READ_JOURNAL_PAGE_SIZE}"
            )));
        }
        if let Some(before_journal_id) = before_journal_id {
            validate_uuid(before_journal_id, "Finance journal cursor")?;
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_read().await?;
        let rows = if let Some(before_journal_id) = before_journal_id {
            sqlx::query(
                "SELECT journal_id, company_id, period_id, source_version,
                        posted_at, payload
                 FROM erpx_holding.finance_posted_journals
                 WHERE data_profile_key=$1 AND journal_id < $2
                 ORDER BY journal_id DESC LIMIT $3",
            )
            .bind(&self.data_profile_key)
            .bind(before_journal_id)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT journal_id, company_id, period_id, source_version,
                        posted_at, payload
                 FROM erpx_holding.finance_posted_journals
                 WHERE data_profile_key=$1
                 ORDER BY journal_id DESC LIMIT $2",
            )
            .bind(&self.data_profile_key)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Finance journal page size is not representable".into())
        })?;
        let has_more = rows.len() > page_len;
        let rows = rows
            .into_iter()
            .take(page_len)
            .map(|row| {
                let journal_id: Uuid = row.try_get("journal_id")?;
                validate_uuid(journal_id, "Finance journal ID")?;
                let company_id: Uuid = row.try_get("company_id")?;
                let period_id: Uuid = row.try_get("period_id")?;
                validate_uuid(company_id, "Finance journal company ID")?;
                validate_uuid(period_id, "Finance journal period ID")?;
                let source_version = as_u64(
                    row.try_get("source_version")?,
                    "Finance journal source version",
                )?;
                if source_version == 0 {
                    return Err(PersistenceError::InvalidData(
                        "Finance journal source version must be positive".into(),
                    ));
                }
                let payload: Value = row.try_get("payload")?;
                let posted: PostedJournalEntry = serde_json::from_value(payload.clone())?;
                if !posted.is_immutable()
                    || posted.id() != journal_id
                    || posted.company_id() != company_id
                    || posted.period_id() != period_id
                {
                    return Err(PersistenceError::InvalidData(
                        "Finance journal read columns diverge from the authoritative payload"
                            .into(),
                    ));
                }
                Ok(FinanceReadJournal {
                    journal_id,
                    company_id,
                    period_id,
                    source_version,
                    posted_at: row.try_get("posted_at")?,
                    payload,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        Ok((rows, has_more))
    }

    /// Read immutable posted-journal source events from the holding write
    /// database in stable aggregate/version order. Chart and period state do
    /// not yet emit source events, so this stream is restricted to journals.
    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance source event page size must be 1..={MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = if let Some(cursor) = cursor {
            validate_uuid(cursor.aggregate_id, "Finance source cursor aggregate ID")?;
            sqlx::query(
                "SELECT event_id, event_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type='finance_journal'
                   AND (aggregate_id > $2
                        OR (aggregate_id = $2 AND aggregate_version > $3))
                 ORDER BY aggregate_id, aggregate_version
                 LIMIT $4",
            )
            .bind(&self.data_profile_key)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "Finance source cursor version",
            )?)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_holding.outbox_events
                 WHERE data_profile_key=$1 AND aggregate_type='finance_journal'
                 ORDER BY aggregate_id, aggregate_version
                 LIMIT $2",
            )
            .bind(&self.data_profile_key)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let page_len = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Finance source page size is not representable".into())
        })?;
        let has_more = rows.len() > page_len;
        let events = rows
            .into_iter()
            .take(page_len)
            .map(|row| {
                let event_id: Uuid = row.try_get("event_id")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                validate_uuid(event_id, "Finance source event ID")?;
                validate_uuid(aggregate_id, "Finance source aggregate ID")?;
                Ok(FinanceSourceEvent {
                    event_id,
                    event_type: row.try_get("event_type")?,
                    aggregate_id,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "Finance source event version",
                    )?,
                    payload: row.try_get("payload")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = if has_more {
            events.last().map(|event| FinanceSourceEventCursor {
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
            })
        } else {
            None
        };
        Ok(FinanceSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    /// Return the holding-scoped number of immutable journal source events.
    /// The count is a fence, not a business balance and is never read from
    /// the disposable projection.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_journal'",
        )
        .bind(&self.data_profile_key)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        as_u64(count, "Finance source event high watermark")
    }

    /// Compare immutable journal source events with the complete disposable
    /// journal projection.  A source event is matched by aggregate identity,
    /// event identity, version, and the posted-journal payload; a checkpoint
    /// without a projected row is also a mismatch.  The returned report is
    /// safe to use as a read gate only when [`Self::is_reconciled`] is true.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<FinanceProjectionReconciliation, PersistenceError> {
        let max_rows = usize::try_from(MAX_FINANCE_PROJECTION_RECONCILIATION_JOURNALS)
            .map_err(|_| PersistenceError::InvalidData("Finance reconciliation bound".into()))?;
        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_holding.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let source_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_journal'",
        )
        .bind(&self.data_profile_key)
        .fetch_one(&mut *source_tx)
        .await?;
        let source_rows =
            sqlx::query(
                "SELECT event_id, aggregate_id, aggregate_version,
                    event_type, payload
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_journal'
              ORDER BY aggregate_id, aggregate_version, event_id
              LIMIT $2",
            )
            .bind(&self.data_profile_key)
            .bind(i64::try_from(max_rows + 1).map_err(|_| {
                PersistenceError::InvalidData("Finance reconciliation bound".into())
            })?)
            .fetch_all(&mut *source_tx)
            .await?;
        source_tx.commit().await?;

        let source_event_high_watermark = as_u64(source_count, "Finance source event count")?;
        let source_truncated = source_event_high_watermark > max_rows as u64;
        let mut source = BTreeMap::new();
        for row in source_rows.into_iter().take(max_rows) {
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let value = (
                row.try_get("event_id")?,
                as_u64(row.try_get("aggregate_version")?, "Finance source version")?,
                row.try_get::<String, _>("event_type")?,
                row.try_get::<Value, _>("payload")?,
            );
            if source.insert(aggregate_id, value).is_some() {
                return Err(PersistenceError::InvalidData(
                    "Finance source contains duplicate journal aggregates".into(),
                ));
            }
        }

        let mut read_tx = self.begin_read().await?;
        let read_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM erpx_holding.finance_posted_journals
              WHERE data_profile_key=$1",
        )
        .bind(&self.data_profile_key)
        .fetch_one(&mut *read_tx)
        .await?;
        let read_rows =
            sqlx::query(
                "SELECT journal.journal_id, journal.source_version,
                    journal.company_id, journal.period_id, journal.payload,
                    checkpoint.last_event_id,
                    checkpoint.source_version AS checkpoint_version
               FROM erpx_holding.finance_posted_journals AS journal
          LEFT JOIN erpx_holding.finance_projection_checkpoints AS checkpoint
                 ON checkpoint.data_profile_key=journal.data_profile_key
                AND checkpoint.journal_id=journal.journal_id
              WHERE journal.data_profile_key=$1
              ORDER BY journal.journal_id
              LIMIT $2",
            )
            .bind(&self.data_profile_key)
            .bind(i64::try_from(max_rows + 1).map_err(|_| {
                PersistenceError::InvalidData("Finance reconciliation bound".into())
            })?)
            .fetch_all(&mut *read_tx)
            .await?;
        read_tx.commit().await?;

        let projected_rows_count = as_u64(read_count, "Finance projected journal count")?;
        let read_truncated = projected_rows_count > max_rows as u64;
        let mut projected = BTreeMap::new();
        for row in read_rows.into_iter().take(max_rows) {
            let journal_id: Uuid = row.try_get("journal_id")?;
            let checkpoint_version = row
                .try_get::<Option<i64>, _>("checkpoint_version")?
                .map(|version| as_u64(version, "Finance checkpoint version"))
                .transpose()?;
            let company_id: Uuid = row.try_get("company_id")?;
            let period_id: Uuid = row.try_get("period_id")?;
            validate_uuid(company_id, "Finance projected journal company ID")?;
            validate_uuid(period_id, "Finance projected journal period ID")?;
            let value = (
                row.try_get::<i64, _>("source_version")?,
                company_id,
                period_id,
                row.try_get::<Value, _>("payload")?,
                row.try_get::<Option<Uuid>, _>("last_event_id")?,
                checkpoint_version,
            );
            if projected.insert(journal_id, value).is_some() {
                return Err(PersistenceError::InvalidData(
                    "Finance projection contains duplicate journal rows".into(),
                ));
            }
        }

        let mut mismatched_rows = 0_u64;
        for (aggregate_id, (event_id, version, event_type, payload)) in &source {
            let matches = projected.get(aggregate_id).is_some_and(
                |(
                    projected_version,
                    projected_company_id,
                    projected_period_id,
                    projected_payload,
                    projected_event_id,
                    checkpoint_version,
                )| {
                    let posted_payload = payload.get("posted_journal");
                    let authoritative = posted_payload
                        .cloned()
                        .and_then(|value| serde_json::from_value::<PostedJournalEntry>(value).ok());
                    event_type == "finance.journal_posted.v1"
                        && *version > 0
                        && *projected_version == i64::try_from(*version).unwrap_or(i64::MIN)
                        && *checkpoint_version == Some(*version)
                        && *projected_event_id == Some(*event_id)
                        && posted_payload == Some(projected_payload)
                        && authoritative.as_ref().is_some_and(|posted| {
                            posted.is_immutable()
                                && posted.id() == *aggregate_id
                                && posted.company_id() == *projected_company_id
                                && posted.period_id() == *projected_period_id
                        })
                },
            );
            if !matches {
                mismatched_rows = mismatched_rows.saturating_add(1);
            }
        }
        mismatched_rows = mismatched_rows.saturating_add(
            projected
                .keys()
                .filter(|journal_id| !source.contains_key(journal_id))
                .count() as u64,
        );
        let source_after = self.source_event_high_watermark().await?;
        Ok(FinanceProjectionReconciliation {
            source_events: source.len() as u64,
            projected_rows: projected.len() as u64,
            mismatched_rows,
            source_event_high_watermark,
            source_stable: source_after == source_event_high_watermark,
            complete: !source_truncated && !read_truncated,
        })
    }

    /// Atomically replace the disposable journal projection from a fenced
    /// source snapshot.  Any malformed event, version gap, or read-database
    /// failure rolls back the clear and all replay writes, preserving the
    /// previously committed read model.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance source event page size must be 1..={MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        if expected_source_version == Some(0) {
            return Err(PersistenceError::InvalidData(
                "Finance source fence must be positive".into(),
            ));
        }

        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_holding.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.data_profile_key)
                .await?;
        if expected_source_version.is_some_and(|expected| expected != actual_source_version) {
            return Err(PersistenceError::InvalidData(
                "Finance source fence changed before replacement rebuild".into(),
            ));
        }

        let mut read_tx = self.begin_read().await?;
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
                if event.event_type != "finance.journal_posted.v1" {
                    return Err(PersistenceError::InvalidData(
                        "Finance source event type is invalid".into(),
                    ));
                }
                let application = project_event_in_transaction(
                    &mut read_tx,
                    &self.data_profile_key,
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await?;
                if application.applied {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Finance source event cursor is missing".into(),
                ));
            }
        }
        let final_source_version =
            source_event_high_watermark_in_transaction(&mut source_tx, &self.data_profile_key)
                .await?;
        if final_source_version != actual_source_version {
            return Err(PersistenceError::InvalidData(
                "Finance source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    /// Apply one posted-journal event to the disposable Finance read model.
    /// Duplicate delivery is a no-op and aggregate-version gaps fail closed.
    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<FinanceProjectionApplication, PersistenceError> {
        verify_authoritative_source_event(
            &self.write_pool,
            &self.data_profile_key,
            event_id,
            "finance.journal_posted.v1",
            "finance_journal",
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let mut tx = self.begin_read().await?;
        let result = project_event_in_transaction(
            &mut tx,
            &self.data_profile_key,
            event_id,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let preparation = clear_projection_in_transaction(&mut tx, &self.data_profile_key).await?;
        tx.commit().await?;
        Ok(preparation)
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.data_profile_key).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.data_profile_key).await
    }
}

/// A read projector must only consume an event that was committed to the
/// holding's authoritative write outbox. Shape validation alone is
/// insufficient: a forged same-version payload could otherwise be accepted
/// by the disposable read model while all local identity checks passed.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn verify_authoritative_source_event(
    write_pool: &PgPool,
    data_profile_key: &str,
    event_id: Uuid,
    expected_event_type: &str,
    expected_aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    validate_uuid(event_id, "Finance source event ID")?;
    validate_uuid(aggregate_id, "Finance source aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "Finance source event version must be positive".into(),
        ));
    }
    if expected_event_type.trim().is_empty() || expected_aggregate_type.trim().is_empty() {
        return Err(PersistenceError::Configuration(
            "Finance source event metadata is invalid",
        ));
    }
    let mut tx = begin_scoped(write_pool, data_profile_key).await?;
    let row = sqlx::query(
        "SELECT event_type, aggregate_type, aggregate_id, aggregate_version, payload
           FROM erpx_holding.outbox_events
          WHERE event_id=$1 AND data_profile_key=$2",
    )
    .bind(event_id)
    .bind(data_profile_key)
    .fetch_optional(&mut *tx)
    .await?;
    tx.commit().await?;

    let Some(row) = row else {
        return Err(PersistenceError::InvalidData(
            "Finance projection event is not present in the authoritative outbox".into(),
        ));
    };
    let source_event_type: String = row.try_get("event_type")?;
    let source_aggregate_type: String = row.try_get("aggregate_type")?;
    let source_aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let source_aggregate_version = as_u64(
        row.try_get("aggregate_version")?,
        "Finance source event version",
    )?;
    let source_payload: Value = row.try_get("payload")?;
    if source_event_type != expected_event_type
        || source_aggregate_type != expected_aggregate_type
        || source_aggregate_id != aggregate_id
        || source_aggregate_version != aggregate_version
        || source_payload != *payload
    {
        return Err(PersistenceError::InvalidData(
            "Finance projection event does not match its authoritative outbox row".into(),
        ));
    }
    Ok(())
}

async fn source_event_high_watermark_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
           FROM erpx_holding.outbox_events
          WHERE data_profile_key=$1 AND aggregate_type='finance_journal'",
    )
    .bind(data_profile_key)
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "Finance source event count")
}

async fn read_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    cursor: Option<&FinanceSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(format!(
            "Finance source event page size must be 1..={MAX_FINANCE_SOURCE_EVENT_PAGE_SIZE}"
        )));
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_uuid(cursor.aggregate_id, "Finance source cursor aggregate ID")?;
        if cursor.aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Finance source cursor version must be positive".into(),
            ));
        }
        sqlx::query(
            "SELECT event_id, aggregate_id, aggregate_version,
                    event_type, payload, occurred_at
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_journal'
                AND (aggregate_id > $2
                     OR (aggregate_id=$2 AND aggregate_version > $3))
              ORDER BY aggregate_id, aggregate_version, event_id
              LIMIT $4",
        )
        .bind(data_profile_key)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Finance source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, aggregate_id, aggregate_version,
                    event_type, payload, occurred_at
               FROM erpx_holding.outbox_events
              WHERE data_profile_key=$1 AND aggregate_type='finance_journal'
              ORDER BY aggregate_id, aggregate_version, event_id
              LIMIT $2",
        )
        .bind(data_profile_key)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let page_len = usize::try_from(page_size).map_err(|_| {
        PersistenceError::InvalidData("Finance source page size is not representable".into())
    })?;
    let has_more = rows.len() > page_len;
    let events = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            validate_uuid(event_id, "Finance source event ID")?;
            validate_uuid(aggregate_id, "Finance source aggregate ID")?;
            Ok(FinanceSourceEvent {
                event_id,
                event_type: row.try_get("event_type")?,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Finance source event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| FinanceSourceEventCursor {
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(FinanceSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn clear_projection_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
) -> Result<FinanceProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_holding.finance_posted_journals WHERE data_profile_key=$1")
            .bind(data_profile_key)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_holding.finance_projection_checkpoints WHERE data_profile_key=$1",
    )
    .bind(data_profile_key)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(FinanceProjectionRebuildPreparation {
        data_profile_key: data_profile_key.into(),
        rows_cleared,
        checkpoints_cleared,
    })
}

async fn project_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: &Value,
) -> Result<FinanceProjectionApplication, PersistenceError> {
    validate_uuid(event_id, "Finance event ID")?;
    validate_uuid(aggregate_id, "Finance aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "Finance aggregate version must be positive".into(),
        ));
    }
    let posted_payload = payload
        .get("posted_journal")
        .cloned()
        .ok_or_else(|| PersistenceError::InvalidData("Finance event payload is invalid".into()))?;
    let posted: PostedJournalEntry = serde_json::from_value(posted_payload.clone())?;
    if posted.id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance event identity does not match payload".into(),
        ));
    }
    let checkpoint = sqlx::query(
        "SELECT last_event_id, source_version
           FROM erpx_holding.finance_projection_checkpoints
          WHERE data_profile_key=$1 AND journal_id=$2 FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    let projected = sqlx::query(
        "SELECT source_version, payload
           FROM erpx_holding.finance_posted_journals
          WHERE data_profile_key=$1 AND journal_id=$2 FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(checkpoint) = checkpoint {
        let current_event_id: Uuid = checkpoint.try_get("last_event_id")?;
        let current = as_u64(
            checkpoint.try_get("source_version")?,
            "Finance projection version",
        )?;
        let projected = projected.ok_or_else(|| {
            PersistenceError::InvalidData("Finance projection checkpoint has no journal row".into())
        })?;
        let projected_version = as_u64(
            projected.try_get("source_version")?,
            "Finance projected journal version",
        )?;
        let projected_payload: Value = projected.try_get("payload")?;
        if projected_version != current {
            return Err(PersistenceError::InvalidData(
                "Finance projection checkpoint and journal version diverge".into(),
            ));
        }
        if aggregate_version == current
            && current_event_id == event_id
            && projected_payload == posted_payload
        {
            return Ok(FinanceProjectionApplication {
                event_id,
                aggregate_id,
                aggregate_version,
                applied: false,
            });
        }
        if aggregate_version <= current {
            return Err(PersistenceError::InvalidData(
                "Finance projection replay conflicts with its immutable checkpoint".into(),
            ));
        }
        if aggregate_version != current.saturating_add(1) {
            return Err(PersistenceError::InvalidData(
                "Finance projection version is not contiguous".into(),
            ));
        }
    } else {
        if projected.is_some() {
            return Err(PersistenceError::InvalidData(
                "Finance projection journal exists without a checkpoint".into(),
            ));
        }
        if aggregate_version != 1 {
            return Err(PersistenceError::InvalidData(
                "Finance projection must start at version one".into(),
            ));
        }
    }
    sqlx::query(
        "INSERT INTO erpx_holding.finance_posted_journals
            (journal_id, holding_id, company_id, period_id, data_profile_key,
             source_version, posted_at, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (journal_id) DO UPDATE SET
            source_version=$6, posted_at=$7, payload=$8, projected_at=now()",
    )
    .bind(posted.id())
    .bind(posted.holding_id())
    .bind(posted.company_id())
    .bind(posted.period_id())
    .bind(data_profile_key)
    .bind(as_i64(aggregate_version, "Finance source version")?)
    .bind(posted.posted_at())
    .bind(&posted_payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_holding.finance_projection_checkpoints
            (journal_id, data_profile_key, last_event_id, source_version)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (journal_id) DO UPDATE SET
            data_profile_key=$2, last_event_id=$3, source_version=$4, projected_at=now()",
    )
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(event_id)
    .bind(as_i64(aggregate_version, "Finance source version")?)
    .execute(&mut **tx)
    .await?;
    Ok(FinanceProjectionApplication {
        event_id,
        aggregate_id,
        aggregate_version,
        applied: true,
    })
}

#[derive(Debug)]
struct AuthoritativePostingChart {
    chart: ChartOfAccounts,
    chart_version: u64,
    line_counts: BTreeMap<Uuid, u64>,
}

async fn load_authoritative_period(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    period_id: Uuid,
) -> Result<AccountingPeriod, PersistenceError> {
    validate_uuid(period_id, "Finance accounting period ID")?;
    let row: Option<(Uuid, Uuid, String, Value)> = sqlx::query_as(
        "SELECT holding_id, company_id, status, payload
           FROM erpx_holding.finance_periods
          WHERE data_profile_key=$1 AND period_id=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(period_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((row_holding_id, row_company_id, row_status, payload)) = row else {
        return Err(PersistenceError::NotFound {
            entity: "finance period",
            id: period_id,
        });
    };
    let period: AccountingPeriod = serde_json::from_value(payload)?;
    period
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if period.id != period_id
        || row_holding_id != period.holding_id
        || row_company_id != period.company_id
        || row_status != period_status(period.status)
    {
        return Err(PersistenceError::InvalidData(
            "persisted Finance accounting period columns do not match its payload".into(),
        ));
    }
    Ok(period)
}

async fn load_authoritative_posting_chart(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    entry: &JournalEntry,
    supplied_chart: &ChartOfAccounts,
) -> Result<AuthoritativePostingChart, PersistenceError> {
    let chart_row = sqlx::query(
        "SELECT holding_id, company_id, chart_name, version
           FROM erpx_holding.finance_chart_of_accounts
          WHERE data_profile_key=$1 AND chart_id=$2
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(supplied_chart.id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(PersistenceError::NotFound {
        entity: "authoritative Finance chart",
        id: supplied_chart.id,
    })?;

    let source_holding_id: Uuid = chart_row.try_get("holding_id")?;
    let source_company_id: Uuid = chart_row.try_get("company_id")?;
    if supplied_chart.holding_id != source_holding_id
        || supplied_chart.company_id != source_company_id
        || entry.holding_id() != source_holding_id
        || entry.company_id() != source_company_id
    {
        return Err(PersistenceError::InvalidData(
            "authoritative Finance chart scope does not match the journal".into(),
        ));
    }
    let chart_version = as_u64(
        chart_row.try_get("version")?,
        "authoritative Finance chart version",
    )?;
    // The normalized chart tables are authoritative for account identity and
    // status, while the legacy chart payload remains the durable source of
    // the chart currency until that field is normalized. When the mirrored
    // row exists, reject any caller-supplied currency or identity drift.
    if let Some(legacy_row) = sqlx::query(
        "SELECT holding_id, company_id, version, payload
           FROM erpx_holding.finance_charts
          WHERE data_profile_key=$1 AND chart_id=$2
          FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(supplied_chart.id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let legacy_holding_id: Uuid = legacy_row.try_get("holding_id")?;
        let legacy_company_id: Uuid = legacy_row.try_get("company_id")?;
        let legacy_version = as_u64(legacy_row.try_get("version")?, "Finance chart version")?;
        let legacy: ChartOfAccounts = serde_json::from_value(legacy_row.try_get("payload")?)?;
        legacy
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if legacy_holding_id != source_holding_id
            || legacy_company_id != source_company_id
            || legacy.id != supplied_chart.id
            || legacy.holding_id != source_holding_id
            || legacy.company_id != source_company_id
            || legacy_version != chart_version
            || legacy.currency != supplied_chart.currency
        {
            return Err(PersistenceError::InvalidData(
                "Finance chart currency or mirrored identity diverges from normalized authority"
                    .into(),
            ));
        }
    }

    let account_rows = sqlx::query(
        "SELECT holding_id, company_id, account_id, account_code,
                account_name, category, parent_account_id, status
           FROM erpx_holding.finance_chart_accounts
          WHERE data_profile_key=$1 AND chart_id=$2
          ORDER BY account_id
          FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(supplied_chart.id)
    .fetch_all(&mut **tx)
    .await?;

    let mut accounts = Vec::with_capacity(account_rows.len());
    let mut account_ids = BTreeMap::new();
    for row in account_rows {
        let row_holding_id: Uuid = row.try_get("holding_id")?;
        let row_company_id: Uuid = row.try_get("company_id")?;
        if row_holding_id != source_holding_id || row_company_id != source_company_id {
            return Err(PersistenceError::InvalidData(
                "authoritative Finance account scope is inconsistent".into(),
            ));
        }
        let account_id: Uuid = row.try_get("account_id")?;
        let account = Account {
            id: account_id,
            code: row.try_get("account_code")?,
            name: row.try_get("account_name")?,
            kind: parse_authoritative_account_kind(&row.try_get::<String, _>("category")?)?,
            parent_id: row.try_get("parent_account_id")?,
            active: parse_authoritative_account_status(&row.try_get::<String, _>("status")?)?,
        };
        account_ids.insert(account_id, account.clone());
        accounts.push(account);
    }

    let mut line_counts = BTreeMap::new();
    for line in entry.lines() {
        let count = line_counts.entry(line.account_id).or_insert(0_u64);
        *count = count.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("Finance journal line-count overflow".into())
        })?;
        let account = account_ids
            .get(&line.account_id)
            .ok_or(PersistenceError::NotFound {
                entity: "authoritative Finance account",
                id: line.account_id,
            })?;
        if !account.active {
            return Err(PersistenceError::InvalidData(
                "authoritative Finance account is inactive".into(),
            ));
        }
    }

    let chart = ChartOfAccounts::new(
        supplied_chart.id,
        source_holding_id,
        source_company_id,
        chart_row.try_get::<String, _>("chart_name")?,
        supplied_chart.currency.clone(),
        accounts,
    )
    .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    Ok(AuthoritativePostingChart {
        chart,
        chart_version,
        line_counts,
    })
}

fn parse_authoritative_account_kind(value: &str) -> Result<AccountKind, PersistenceError> {
    match value {
        "asset" => Ok(AccountKind::Asset),
        "liability" => Ok(AccountKind::Liability),
        "equity" => Ok(AccountKind::Equity),
        "revenue" => Ok(AccountKind::Revenue),
        "expense" => Ok(AccountKind::Expense),
        _ => Err(PersistenceError::InvalidData(
            "invalid authoritative Finance account category".into(),
        )),
    }
}

fn parse_authoritative_account_status(value: &str) -> Result<bool, PersistenceError> {
    match value {
        "active" => Ok(true),
        "inactive" => Ok(false),
        _ => Err(PersistenceError::InvalidData(
            "invalid authoritative Finance account status".into(),
        )),
    }
}

async fn record_authoritative_account_usage(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    posted: &PostedJournalEntry,
    chart_id: Uuid,
    chart_version: u64,
    line_counts: &BTreeMap<Uuid, u64>,
) -> Result<(), PersistenceError> {
    for (account_id, line_count) in line_counts {
        sqlx::query(
            "INSERT INTO erpx_holding.finance_posted_journal_account_usage
                (data_profile_key, journal_id, chart_id, holding_id, company_id,
                 account_id, chart_version, line_count)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(data_profile_key)
        .bind(posted.id())
        .bind(chart_id)
        .bind(posted.holding_id())
        .bind(posted.company_id())
        .bind(account_id)
        .bind(as_i64(
            chart_version,
            "authoritative Finance chart version",
        )?)
        .bind(as_i64(*line_count, "Finance journal account line count")?)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(crate) async fn write_finance_source_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    version: u64,
    payload: &Value,
    event_type: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(Uuid::new_v4())
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(version, "Finance aggregate version")?)
    .bind(event_type)
    .bind(serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": payload,
        "version": version,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn begin_scoped<'a>(
    pool: &'a PgPool,
    data_profile_key: &str,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(data_profile_key)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| character.is_control())
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn period_status(status: PeriodStatus) -> &'static str {
    match status {
        PeriodStatus::Open => "open",
        PeriodStatus::Locked => "locked",
        PeriodStatus::Closed => "closed",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn validate_journal_replay_evidence(
    source_matches: bool,
    expected_outbox_payload: &Value,
    outbox_event_payloads: &[Value],
) -> Result<(), PersistenceError> {
    if !source_matches {
        return Err(PersistenceError::InvalidData(
            "Finance journal idempotent replay requires its authoritative source row".into(),
        ));
    }
    if outbox_event_payloads.len() != 1 {
        return Err(PersistenceError::InvalidData(format!(
            "Finance journal idempotent replay requires exactly one authoritative outbox event, found {}",
            outbox_event_payloads.len()
        )));
    }
    if outbox_event_payloads[0] != *expected_outbox_payload {
        return Err(PersistenceError::InvalidData(
            "Finance journal idempotent replay outbox payload does not match its source row".into(),
        ));
    }
    Ok(())
}

async fn validate_posted_journal_replay_evidence(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    journal_id: Uuid,
    existing: &PostedJournalEntry,
) -> Result<(), PersistenceError> {
    let existing_response = serde_json::to_value(existing)?;
    let source: Option<(Uuid, Uuid, Uuid, Uuid, i64, Value)> = sqlx::query_as(
        "SELECT journal_id, holding_id, company_id, period_id, version, payload
         FROM erpx_holding.finance_posted_journals
         WHERE data_profile_key=$1 AND journal_id=$2",
    )
    .bind(data_profile_key)
    .bind(journal_id)
    .fetch_optional(&mut **tx)
    .await?;
    let source_matches = source.as_ref().is_some_and(
        |(source_id, holding_id, company_id, period_id, version, payload)| {
            *source_id == journal_id
                && *source_id == existing.id()
                && *holding_id == existing.holding_id()
                && *company_id == existing.company_id()
                && *period_id == existing.period_id()
                && *version == 1
                && payload == &existing_response
        },
    );
    let outbox_event_payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1
           AND aggregate_type='finance_journal'
           AND aggregate_id=$2
           AND aggregate_version=1
           AND event_type='finance.journal_posted.v1'
         ORDER BY event_id
         LIMIT 2",
    )
    .bind(data_profile_key)
    .bind(journal_id)
    .fetch_all(&mut **tx)
    .await?;
    let expected_outbox_payload = serde_json::json!({
        "posted_journal": existing_response,
        "context": existing.context(),
    });
    validate_journal_replay_evidence(
        source_matches,
        &expected_outbox_payload,
        &outbox_event_payloads,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
    use erpx_domain::finance::{JournalEntry, JournalLine};

    #[tokio::test]
    async fn data_profile_reference_is_bounded_and_redacted() {
        assert!(
            SqlxFinanceStore::new(
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                " ",
            )
            .is_err()
        );
        assert!(
            SqlxFinanceStore::new(
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
                "holding-safe",
            )
            .expect("valid store")
            .data_profile_key()
            .contains("holding-safe")
        );
    }

    #[test]
    fn status_mapping_is_closed() {
        assert_eq!(period_status(PeriodStatus::Open), "open");
        assert_eq!(period_status(PeriodStatus::Locked), "locked");
        assert_eq!(period_status(PeriodStatus::Closed), "closed");
    }

    #[test]
    fn journal_replay_requires_source_and_exact_matching_outbox_evidence() {
        let payload = serde_json::json!({
            "posted_journal": {"journal_id": Uuid::from_u128(10)},
            "context": {"command": "finance.post"},
        });
        assert!(validate_journal_replay_evidence(false, &payload, &[]).is_err());
        assert!(
            validate_journal_replay_evidence(
                true,
                &payload,
                &[payload.clone(), serde_json::json!({"posted_journal": {}})]
            )
            .is_err()
        );
        assert!(
            validate_journal_replay_evidence(
                true,
                &payload,
                &[serde_json::json!({"posted_journal": {}})]
            )
            .is_err()
        );
        validate_journal_replay_evidence(true, &payload, std::slice::from_ref(&payload))
            .expect("journal replay must have one matching authoritative outbox event");
    }

    #[tokio::test]
    #[ignore = "requires one disposable holding PostgreSQL write database"]
    async fn legacy_chart_save_is_promoted_to_normalized_posting_authority() {
        let url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
        let pool = PgPool::connect(&url).await.expect("write pool");
        erpx_migration::migrate_holding_write(&pool)
            .await
            .expect("holding-write migrations");

        let data_profile_key = format!("holding-finance-legacy-{}", Uuid::new_v4());
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let debit_id = Uuid::new_v4();
        let credit_id = Uuid::new_v4();
        let chart_id = Uuid::new_v4();
        let period = AccountingPeriod::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            "2026-07",
            NaiveDate::from_ymd_opt(2026, 7, 1).expect("period start"),
            NaiveDate::from_ymd_opt(2026, 7, 31).expect("period end"),
        )
        .expect("period");
        let finance_store = SqlxFinanceStore::new(pool.clone(), pool.clone(), &data_profile_key)
            .expect("finance store");
        finance_store
            .save_period(&period, None)
            .await
            .expect("period source");

        let legacy_chart = ChartOfAccounts::new(
            chart_id,
            holding_id,
            company_id,
            "Legacy compatibility chart",
            "THB",
            vec![
                Account {
                    id: debit_id,
                    code: "1000".into(),
                    name: "Cash".into(),
                    kind: AccountKind::Asset,
                    parent_id: None,
                    active: true,
                },
                Account {
                    id: credit_id,
                    code: "4000".into(),
                    name: "Revenue".into(),
                    kind: AccountKind::Revenue,
                    parent_id: None,
                    active: true,
                },
            ],
        )
        .expect("legacy chart");
        assert_eq!(
            finance_store
                .save_chart(&legacy_chart, None)
                .await
                .expect("legacy chart source"),
            1
        );

        let journal_context = CommandContext::try_new(
            "legacy-chart-posting",
            ActorAttribution {
                principal_ref: "finance-test".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "finance.legacy.chart.compatibility.test",
        )
        .expect("journal context");
        let entry = JournalEntry::new(
            Uuid::new_v4(),
            holding_id,
            company_id,
            period.id,
            NaiveDate::from_ymd_opt(2026, 7, 10).expect("journal date"),
            "Legacy chart posting",
            vec![
                JournalLine::new(Uuid::new_v4(), debit_id, 1_000, 0, None).expect("debit line"),
                JournalLine::new(Uuid::new_v4(), credit_id, 0, 1_000, None).expect("credit line"),
            ],
            journal_context,
        )
        .expect("journal");
        let posted = finance_store
            .post_journal(entry, &legacy_chart, &period, Utc::now())
            .await
            .expect("legacy chart posting");
        assert!(!posted.replayed);

        let mut tx = pool.begin().await.expect("inspection transaction");
        sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
            .bind(&data_profile_key)
            .execute(&mut *tx)
            .await
            .expect("holding scope");
        let normalized: (i64, i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT count(*) FROM erpx_holding.finance_chart_of_accounts WHERE chart_id=$1),
                (SELECT count(*) FROM erpx_holding.finance_chart_accounts WHERE chart_id=$1),
                (SELECT count(*) FROM erpx_holding.finance_posted_journal_account_usage
                  WHERE chart_id=$1)",
        )
        .bind(chart_id)
        .fetch_one(&mut *tx)
        .await
        .expect("normalized compatibility rows");
        assert_eq!(normalized, (1, 2, 2));
        tx.commit().await.expect("inspection commit");
    }
}
