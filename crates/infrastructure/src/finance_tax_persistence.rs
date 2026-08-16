//! SQLx source and projection adapters for explicit Thai statutory tax facts.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_domain::finance::PostedJournalEntry;
use erpx_domain::finance_tax::{
    ThaiTaxFact, ThaiTaxKind, ThaiTaxReport, ThaiTaxStatutoryReconciliationReport,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::{PersistenceError, finance_persistence::begin_scoped};

pub const FINANCE_THAI_TAX_PROJECTION_NAME: &str = "finance_thai_tax";
pub const FINANCE_THAI_TAX_FACT_AGGREGATE: &str = "finance_thai_tax_fact";
pub const FINANCE_THAI_TAX_FACT_EVENT: &str = "finance.thai_tax_fact_recorded.v1";
pub const MAX_FINANCE_THAI_TAX_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_THAI_TAX_REPORT_FACTS: u32 = 1_000_000;
pub const MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS: u64 = 1_000_000;
pub const MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThaiTaxFactWriteReceipt {
    pub fact_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThaiTaxFactReadRow {
    pub fact_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub journal_id: Uuid,
    pub line_id: Uuid,
    pub tax_date: NaiveDate,
    pub tax_period: String,
    pub kind: ThaiTaxKind,
    pub currency: String,
    pub base_minor: i64,
    pub tax_minor: i64,
    pub tax_rate_basis_points: u32,
    pub tax_identifier: String,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThaiTaxReportRead {
    pub report: ThaiTaxReport,
    pub source_version: u64,
    pub projected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThaiTaxProjectionFactIdentity {
    fact_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    journal_id: Uuid,
    line_id: Uuid,
    tax_date: NaiveDate,
    tax_period: String,
    kind: ThaiTaxKind,
    currency: String,
    base_minor: i64,
    tax_minor: i64,
    tax_rate_basis_points: u32,
    tax_identifier: String,
    recorded_at: DateTime<Utc>,
}

impl From<&ThaiTaxFact> for ThaiTaxProjectionFactIdentity {
    fn from(fact: &ThaiTaxFact) -> Self {
        Self {
            fact_id: fact.fact_id,
            holding_id: fact.holding_id,
            company_id: fact.company_id,
            journal_id: fact.journal_id,
            line_id: fact.line_id,
            tax_date: fact.tax_date,
            tax_period: fact.tax_period.clone(),
            kind: fact.kind,
            currency: fact.currency.clone(),
            base_minor: fact.base_minor,
            tax_minor: fact.tax_minor,
            tax_rate_basis_points: fact.tax_rate_basis_points,
            tax_identifier: fact.tax_identifier.clone(),
            recorded_at: fact.recorded_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceThaiTaxProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceThaiTaxProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

/// Exact source/read-model reconciliation for one holding.
///
/// The comparison is over immutable source-event identity, aggregate version,
/// and event type. Command context and other write-only payloads deliberately
/// never cross this boundary. A report is safe to serve only when the two
/// bounded sequences are identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceThaiTaxProjectionReconciliation {
    pub source_events: u64,
    pub projected_rows: u64,
    pub mismatched_rows: u64,
}

/// Reconciliation of every bounded statutory company/period/currency report
/// in one holding. The report is an operational fence only: it never writes
/// either source facts or read rows, so a failed run is repaired by the
/// existing projection rebuild path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceThaiTaxStatutoryReconciliation {
    pub reports: Vec<ThaiTaxStatutoryReconciliationReport>,
    pub source_stable: bool,
}

impl FinanceThaiTaxStatutoryReconciliation {
    pub fn is_reconciled(&self) -> bool {
        self.source_stable && self.reports.iter().all(|report| report.is_reconciled())
    }
}

impl FinanceThaiTaxProjectionReconciliation {
    pub const fn is_reconciled(self) -> bool {
        self.source_events == self.projected_rows && self.mismatched_rows == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceThaiTaxSourceEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceThaiTaxSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceThaiTaxSourceEventPage {
    pub events: Vec<FinanceThaiTaxSourceEvent>,
    pub next_cursor: Option<FinanceThaiTaxSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceThaiTaxStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceThaiTaxStore {
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

    pub async fn record_fact(
        &self,
        fact: &ThaiTaxFact,
    ) -> Result<ThaiTaxFactWriteReceipt, PersistenceError> {
        fact.validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let response = serde_json::to_value(fact)?;
        let request_hash = sha256_hex(&serde_json::to_vec(fact)?);
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        if let Some(row) = sqlx::query(
            "SELECT request_hash, response, fact_id
               FROM erpx_tenant.finance_thai_tax_command_idempotency
              WHERE idempotency_key=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(&fact.context.idempotency_key)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.try_get::<String, _>("request_hash")? != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: fact.context.idempotency_key.clone(),
                });
            }
            let existing: ThaiTaxFact = serde_json::from_value(row.try_get("response")?)?;
            let stored_fact_id: Uuid = row.try_get("fact_id")?;
            if stored_fact_id != existing.fact_id {
                return Err(invalid(
                    "Thai tax idempotency response fact identity does not match its row",
                ));
            }
            let source_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (
                     SELECT 1
                     FROM erpx_tenant.finance_thai_tax_facts
                     WHERE holding_ref=$1 AND fact_id=$2
                 )",
            )
            .bind(&self.holding_ref)
            .bind(existing.fact_id)
            .fetch_one(&mut *tx)
            .await?;
            let outbox_event_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1
                   AND aggregate_type=$2
                   AND aggregate_id=$3
                   AND aggregate_version=1
                   AND event_type=$4",
            )
            .bind(&self.holding_ref)
            .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
            .bind(existing.fact_id)
            .bind(FINANCE_THAI_TAX_FACT_EVENT)
            .fetch_one(&mut *tx)
            .await?;
            validate_tax_fact_replay_evidence(
                source_exists,
                u64::try_from(outbox_event_count).map_err(|_| invalid("tax outbox count"))?,
            )?;
            tx.commit().await?;
            return Ok(ThaiTaxFactWriteReceipt {
                fact_id: existing.fact_id,
                version: 1,
                replayed: true,
            });
        }

        let journal_row = sqlx::query(
            "SELECT payload, currency
               FROM erpx_tenant.finance_posted_journals
              WHERE journal_id=$1 AND holding_ref=$2",
        )
        .bind(fact.journal_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "posted Finance journal",
            id: fact.journal_id,
        })?;
        let journal_payload: Value = journal_row.try_get("payload")?;
        let journal_currency: Option<String> = journal_row.try_get("currency")?;
        let journal: PostedJournalEntry = serde_json::from_value(journal_payload)?;
        if journal.holding_id() != fact.holding_id
            || journal.company_id() != fact.company_id
            || journal.entry_date() != fact.tax_date
            || !journal.lines().iter().any(|line| line.id == fact.line_id)
        {
            return Err(PersistenceError::InvalidData(
                "Thai tax fact must match an immutable posted journal line".into(),
            ));
        }
        let line = journal
            .lines()
            .iter()
            .find(|line| line.id == fact.line_id)
            .ok_or_else(|| invalid("Thai tax journal line is missing"))?;
        let line_amount_minor = line.debit_minor.max(line.credit_minor);
        if fact.base_minor > line_amount_minor {
            return Err(invalid(
                "Thai tax base cannot exceed its immutable posted journal line",
            ));
        }
        let Some(journal_currency) = journal_currency else {
            return Err(invalid(
                "Thai tax fact requires a persisted posted journal currency",
            ));
        };
        if journal_currency != fact.currency {
            return Err(invalid(
                "Thai tax fact currency does not match the immutable posted journal",
            ));
        }
        if sqlx::query_scalar::<_, Uuid>(
            "SELECT fact_id FROM erpx_tenant.finance_thai_tax_facts
              WHERE fact_id=$1 AND holding_ref=$2",
        )
        .bind(fact.fact_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .is_some()
        {
            return Err(PersistenceError::InvalidData(
                "Thai tax fact ID already exists".into(),
            ));
        }

        sqlx::query(
            "INSERT INTO erpx_tenant.finance_thai_tax_facts
                (fact_id, holding_id, company_id, journal_id, line_id, holding_ref,
                 tax_date, tax_period, kind, currency, base_minor, tax_minor,
                 tax_rate_basis_points, tax_identifier, context, recorded_at, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
        )
        .bind(fact.fact_id)
        .bind(fact.holding_id)
        .bind(fact.company_id)
        .bind(fact.journal_id)
        .bind(fact.line_id)
        .bind(&self.holding_ref)
        .bind(fact.tax_date)
        .bind(&fact.tax_period)
        .bind(fact.kind.as_str())
        .bind(&fact.currency)
        .bind(fact.base_minor)
        .bind(fact.tax_minor)
        .bind(i32::try_from(fact.tax_rate_basis_points).map_err(|_| invalid("tax rate"))?)
        .bind(&fact.tax_identifier)
        .bind(serde_json::to_value(&fact.context)?)
        .bind(fact.recorded_at)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at)
             VALUES ($1,$2,$3,$4,1,$5,$6,$7)",
        )
        .bind(Uuid::new_v4())
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .bind(fact.fact_id)
        .bind(&self.holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_EVENT)
        .bind(serde_json::json!({ "fact": response }))
        .bind(fact.recorded_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.finance_thai_tax_command_idempotency
                (idempotency_key, holding_ref, request_hash, fact_id, response)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(&fact.context.idempotency_key)
        .bind(&self.holding_ref)
        .bind(request_hash)
        .bind(fact.fact_id)
        .bind(&response)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(ThaiTaxFactWriteReceipt {
            fact_id: fact.fact_id,
            version: 1,
            replayed: false,
        })
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceThaiTaxSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceThaiTaxSourceEventPage, PersistenceError> {
        validate_page_size(page_size)?;
        let limit = i64::from(page_size) + 1;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_id, aggregate_version,
                        payload, occurred_at
                   FROM erpx_tenant.outbox_events
                  WHERE holding_ref=$1 AND aggregate_type=$2
                    AND (aggregate_id > $3 OR
                         (aggregate_id=$3 AND aggregate_version > $4))
                  ORDER BY aggregate_id, aggregate_version LIMIT $5",
            )
            .bind(&self.holding_ref)
            .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
            .bind(cursor.aggregate_id)
            .bind(i64::try_from(cursor.aggregate_version).map_err(|_| invalid("cursor"))?)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_id, aggregate_version,
                        payload, occurred_at
                   FROM erpx_tenant.outbox_events
                  WHERE holding_ref=$1 AND aggregate_type=$2
                  ORDER BY aggregate_id, aggregate_version LIMIT $3",
            )
            .bind(&self.holding_ref)
            .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let take = usize::try_from(page_size).map_err(|_| invalid("page size"))?;
        let has_more = rows.len() > take;
        let events = rows
            .into_iter()
            .take(take)
            .map(|row| {
                Ok(FinanceThaiTaxSourceEvent {
                    event_id: row.try_get("event_id")?,
                    event_type: row.try_get("event_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    aggregate_version: u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
                        .map_err(|_| invalid("source version"))?,
                    payload: row.try_get("payload")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = has_more
            .then(|| {
                events.last().map(|event| FinanceThaiTaxSourceEventCursor {
                    aggregate_id: event.aggregate_id,
                    aggregate_version: event.aggregate_version,
                })
            })
            .flatten();
        Ok(FinanceThaiTaxSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        event_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
        _occurred_at: DateTime<Utc>,
    ) -> Result<FinanceThaiTaxProjectionApplication, PersistenceError> {
        if event_type != FINANCE_THAI_TAX_FACT_EVENT || aggregate_version == 0 {
            return Err(invalid("Thai tax source event metadata"));
        }
        let fact: ThaiTaxFact = serde_json::from_value(
            payload
                .get("fact")
                .cloned()
                .ok_or_else(|| invalid("tax fact payload"))?,
        )?;
        fact.validate()
            .map_err(|error| invalid(error.to_string()))?;
        if fact.fact_id != aggregate_id {
            return Err(invalid("Thai tax aggregate identity"));
        }
        crate::finance_persistence::verify_authoritative_source_event(
            &self.write_pool,
            &self.holding_ref,
            event_id,
            FINANCE_THAI_TAX_FACT_EVENT,
            FINANCE_THAI_TAX_FACT_AGGREGATE,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let result = project_tax_event_in_transaction(
            &mut tx,
            &self.holding_ref,
            event_id,
            aggregate_id,
            aggregate_version,
            &fact,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_report(
        &self,
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: &str,
        currency: &str,
        page_size: u32,
    ) -> Result<ThaiTaxReportRead, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_THAI_TAX_REPORT_FACTS {
            return Err(invalid("Thai tax report page size"));
        }
        let reconciliation = self
            .reconcile_statutory_report(holding_id, company_id, tax_period, currency)
            .await?;
        if !reconciliation.is_reconciled() {
            return Err(invalid(
                "Thai tax report requires reconciled and stable source/read state",
            ));
        }
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT fact_id, holding_id, company_id, journal_id, line_id, tax_date,
                    tax_period, kind, currency, base_minor, tax_minor,
                    tax_rate_basis_points, tax_identifier, source_version,
                    source_event_id, recorded_at, projected_at
               FROM erpx_tenant.finance_thai_tax_fact_read_rows
              WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
                AND tax_period=$4 AND currency=$5
              ORDER BY tax_date, fact_id LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(holding_id)
        .bind(company_id)
        .bind(tax_period)
        .bind(currency)
        .bind(i64::from(page_size) + 1)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        if rows.len() > usize::try_from(page_size).map_err(|_| invalid("page size"))? {
            return Err(invalid("Thai tax report exceeds the complete-report bound"));
        }
        let mut facts = Vec::with_capacity(rows.len());
        let mut source_version = 0_u64;
        let mut projected_at = Utc::now();
        for row in rows {
            let kind = match row.try_get::<String, _>("kind")?.as_str() {
                "output_vat" => ThaiTaxKind::OutputVat,
                "input_vat" => ThaiTaxKind::InputVat,
                "withholding_payable" => ThaiTaxKind::WithholdingPayable,
                _ => return Err(invalid("Thai tax kind")),
            };
            source_version = source_version.max(
                u64::try_from(row.try_get::<i64, _>("source_version")?)
                    .map_err(|_| invalid("source version"))?,
            );
            projected_at = projected_at.min(row.try_get("projected_at")?);
            facts.push(ThaiTaxFact {
                fact_id: row.try_get("fact_id")?,
                holding_id: row.try_get("holding_id")?,
                company_id: row.try_get("company_id")?,
                journal_id: row.try_get("journal_id")?,
                line_id: row.try_get("line_id")?,
                tax_date: row.try_get("tax_date")?,
                tax_period: row.try_get::<String, _>("tax_period")?,
                kind,
                currency: row.try_get::<String, _>("currency")?,
                base_minor: row.try_get("base_minor")?,
                tax_minor: row.try_get("tax_minor")?,
                tax_rate_basis_points: u32::try_from(
                    row.try_get::<i32, _>("tax_rate_basis_points")?,
                )
                .map_err(|_| invalid("tax rate"))?,
                tax_identifier: row.try_get("tax_identifier")?,
                context: return_context(),
                recorded_at: row.try_get("recorded_at")?,
            });
        }
        // Read rows intentionally omit command context. Reconstructing the
        // report through the domain requires only the source identity; the
        // context is not exposed on the read contract.
        let report = ThaiTaxReport::from_facts(holding_id, company_id, tax_period, currency, facts)
            .map_err(|error| invalid(error.to_string()))?;
        let final_reconciliation = self
            .reconcile_statutory_report(holding_id, company_id, tax_period, currency)
            .await?;
        if !final_reconciliation.is_reconciled() {
            return Err(invalid(
                "Thai tax report source/read state changed while it was assembled",
            ));
        }
        Ok(ThaiTaxReportRead {
            report,
            source_version,
            projected_at,
        })
    }

    /// Reconcile the complete bounded tax source feed against the read model.
    ///
    /// This is intentionally a two-database comparison: the source sequence
    /// is read only from the tenant-write database and the projection sequence
    /// only from the tenant-read database. A missing, extra, reordered, or
    /// replaced source event therefore fails closed before a statutory report
    /// can be returned.
    pub async fn reconcile_projection(
        &self,
    ) -> Result<FinanceThaiTaxProjectionReconciliation, PersistenceError> {
        let source_limit = i64::try_from(MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS + 1)
            .map_err(|_| invalid("Thai tax reconciliation bound"))?;
        let mut write_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let source_rows: Vec<(Uuid, Uuid, i64, String, Value)> = sqlx::query_as(
            "SELECT aggregate_id, event_id, aggregate_version, event_type, payload
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2
              ORDER BY aggregate_id, aggregate_version, event_id
              LIMIT $3",
        )
        .bind(&self.holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .bind(source_limit)
        .fetch_all(&mut *write_tx)
        .await?;
        write_tx.commit().await?;
        if source_rows.len() as u64 > MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS {
            return Err(invalid("Thai tax source exceeds reconciliation bound"));
        }

        let mut source = BTreeMap::new();
        for (fact_id, event_id, source_version, event_type, payload) in source_rows {
            let fact: ThaiTaxFact = serde_json::from_value(
                payload
                    .get("fact")
                    .cloned()
                    .ok_or_else(|| invalid("Thai tax source fact payload"))?,
            )?;
            fact.validate()
                .map_err(|error| invalid(error.to_string()))?;
            if fact.fact_id != fact_id || source_version <= 0 {
                return Err(invalid("Thai tax source fact identity"));
            }
            if source
                .insert(
                    fact_id,
                    (
                        event_id,
                        u64::try_from(source_version)
                            .map_err(|_| invalid("Thai tax source version"))?,
                        event_type,
                        ThaiTaxProjectionFactIdentity::from(&fact),
                    ),
                )
                .is_some()
            {
                return Err(invalid("Thai tax source contains duplicate fact IDs"));
            }
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let projected_rows = sqlx::query(
            "SELECT fact_id, source_event_id, source_version,
                    holding_id, company_id, journal_id, line_id, tax_date,
                    tax_period, kind, currency, base_minor, tax_minor,
                    tax_rate_basis_points, tax_identifier, recorded_at
               FROM erpx_tenant.finance_thai_tax_fact_read_rows
              WHERE holding_ref=$1
              ORDER BY fact_id, source_version, source_event_id
              LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(source_limit)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        if projected_rows.len() as u64 > MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS {
            return Err(invalid("Thai tax projection exceeds reconciliation bound"));
        }

        let mut projected = BTreeMap::new();
        for row in projected_rows {
            let fact_id: Uuid = row.try_get("fact_id")?;
            let kind = match row.try_get::<String, _>("kind")?.as_str() {
                "output_vat" => ThaiTaxKind::OutputVat,
                "input_vat" => ThaiTaxKind::InputVat,
                "withholding_payable" => ThaiTaxKind::WithholdingPayable,
                _ => return Err(invalid("Thai tax projected kind")),
            };
            let fact = ThaiTaxFact {
                fact_id,
                holding_id: row.try_get("holding_id")?,
                company_id: row.try_get("company_id")?,
                journal_id: row.try_get("journal_id")?,
                line_id: row.try_get("line_id")?,
                tax_date: row.try_get("tax_date")?,
                tax_period: row.try_get("tax_period")?,
                kind,
                currency: row.try_get("currency")?,
                base_minor: row.try_get("base_minor")?,
                tax_minor: row.try_get("tax_minor")?,
                tax_rate_basis_points: u32::try_from(
                    row.try_get::<i32, _>("tax_rate_basis_points")?,
                )
                .map_err(|_| invalid("Thai tax projected rate"))?,
                tax_identifier: row.try_get("tax_identifier")?,
                context: return_context(),
                recorded_at: row.try_get("recorded_at")?,
            };
            fact.validate()
                .map_err(|error| invalid(error.to_string()))?;
            let value = (
                row.try_get::<Uuid, _>("source_event_id")?,
                u64::try_from(row.try_get::<i64, _>("source_version")?)
                    .map_err(|_| invalid("Thai tax projected version"))?,
                ThaiTaxProjectionFactIdentity::from(&fact),
            );
            if projected.insert(fact_id, value).is_some() {
                return Err(invalid("Thai tax projection contains duplicate fact IDs"));
            }
        }

        let mismatched_rows = source
            .iter()
            .filter(|(fact_id, (event_id, source_version, event_type, fact))| {
                projected.get(*fact_id).is_none_or(
                    |(projected_event_id, projected_version, projected_fact)| {
                        event_type != FINANCE_THAI_TAX_FACT_EVENT
                            || event_id != projected_event_id
                            || source_version != projected_version
                            || fact != projected_fact
                    },
                )
            })
            .count() as u64
            + projected
                .keys()
                .filter(|fact_id| !source.contains_key(fact_id))
                .count() as u64;

        Ok(FinanceThaiTaxProjectionReconciliation {
            source_events: source.len() as u64,
            projected_rows: projected.len() as u64,
            mismatched_rows,
        })
    }

    /// Reconcile the statutory report facts against the independently
    /// projected rows. This is stronger than event-count parity: it compares
    /// every persisted tax identity and its derived totals, includes read
    /// rows that have no source fact, and keeps the result non-green if the
    /// append-only source feed moved during the comparison.
    pub async fn reconcile_statutory_report(
        &self,
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: &str,
        currency: &str,
    ) -> Result<ThaiTaxStatutoryReconciliationReport, PersistenceError> {
        validate_report_scope(holding_id, company_id, tax_period, currency)?;
        let (source_facts, source_event_count_before, source_fact_count) = self
            .load_source_report_facts(holding_id, company_id, tax_period, currency)
            .await?;
        let projected_facts = self
            .load_projected_report_facts(holding_id, company_id, tax_period, currency)
            .await?;
        let source_event_count_after = self.source_event_count().await?;
        let source_stable = source_event_count_before == source_event_count_after
            && source_fact_count == source_event_count_before;
        ThaiTaxStatutoryReconciliationReport::from_fact_sets(
            holding_id,
            company_id,
            tax_period,
            currency,
            source_facts,
            projected_facts,
            source_stable,
        )
        .map_err(|error| invalid(error.to_string()))
    }

    /// Reconcile all statutory report scopes present on either side of the
    /// source/read boundary. Unioning the scopes is important: a projected
    /// row for a deleted or mis-scoped source fact must become an explicit
    /// unexpected-fact issue rather than disappearing from the audit.
    pub async fn reconcile_statutory_reports(
        &self,
    ) -> Result<FinanceThaiTaxStatutoryReconciliation, PersistenceError> {
        let mut scopes = BTreeSet::new();
        let source_scopes = self.load_source_report_scopes().await?;
        let projected_scopes = self.load_projected_report_scopes().await?;
        scopes.extend(source_scopes);
        scopes.extend(projected_scopes);
        if u64::try_from(scopes.len()).map_err(|_| invalid("Thai tax report scope count"))?
            > MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS
        {
            return Err(invalid(
                "Thai tax report scope count exceeds reconciliation bound",
            ));
        }

        let mut reports = Vec::with_capacity(scopes.len());
        for (holding_id, company_id, tax_period, currency) in scopes {
            reports.push(
                self.reconcile_statutory_report(holding_id, company_id, &tax_period, &currency)
                    .await?,
            );
        }
        let source_stable = reports.iter().all(|report| report.source_stable);
        Ok(FinanceThaiTaxStatutoryReconciliation {
            reports,
            source_stable,
        })
    }

    async fn load_source_report_facts(
        &self,
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: &str,
        currency: &str,
    ) -> Result<(Vec<ThaiTaxFact>, u64, u64), PersistenceError> {
        let limit = reconciliation_limit()?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let source_event_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2",
        )
        .bind(&self.holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .fetch_one(&mut *tx)
        .await?;
        let source_fact_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
               FROM erpx_tenant.finance_thai_tax_facts
              WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        let rows = sqlx::query(
            "SELECT fact_id, holding_id, company_id, journal_id, line_id,
                    tax_date, btrim(tax_period) AS tax_period, kind,
                    btrim(currency) AS currency, base_minor, tax_minor,
                    tax_rate_basis_points, tax_identifier, recorded_at
               FROM erpx_tenant.finance_thai_tax_facts
              WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
                AND btrim(tax_period)=$4 AND btrim(currency)=$5
              ORDER BY tax_date, fact_id
              LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(holding_id)
        .bind(company_id)
        .bind(tax_period)
        .bind(currency)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;

        let source_fact_ids = rows
            .iter()
            .map(|row| row.try_get("fact_id"))
            .collect::<Result<BTreeSet<Uuid>, sqlx::Error>>()?;
        let source_event_rows = sqlx::query(
            "SELECT o.event_id, o.aggregate_id, o.aggregate_version,
                    o.event_type, o.payload, f.payload AS source_fact_payload
               FROM erpx_tenant.outbox_events AS o
               JOIN erpx_tenant.finance_thai_tax_facts AS f
                 ON f.fact_id=o.aggregate_id AND f.holding_ref=o.holding_ref
              WHERE o.holding_ref=$1 AND o.aggregate_type=$2
                AND f.holding_id=$3 AND f.company_id=$4
                AND btrim(f.tax_period)=$5 AND btrim(f.currency)=$6
              ORDER BY o.aggregate_id, o.aggregate_version, o.event_id
              LIMIT $7",
        )
        .bind(&self.holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .bind(holding_id)
        .bind(company_id)
        .bind(tax_period)
        .bind(currency)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        if source_event_rows.len() != source_fact_ids.len() {
            return Err(invalid(
                "Thai tax source facts and outbox events are not one-to-one",
            ));
        }
        let mut source_event_fact_ids = BTreeSet::new();
        for row in source_event_rows {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_version: i64 = row.try_get("aggregate_version")?;
            let event_type: String = row.try_get("event_type")?;
            if event_id.is_nil() || aggregate_id.is_nil() {
                return Err(invalid("Thai tax source event identity is invalid"));
            }
            if aggregate_version != 1 {
                return Err(invalid("Thai tax source event version is invalid"));
            }
            if event_type != FINANCE_THAI_TAX_FACT_EVENT {
                return Err(invalid("Thai tax source event type is invalid"));
            }
            if !source_event_fact_ids.insert(aggregate_id) {
                return Err(invalid(
                    "Thai tax source contains duplicate outbox fact IDs",
                ));
            }
            let event_fact: ThaiTaxFact = serde_json::from_value(
                row.try_get::<Value, _>("payload")?
                    .get("fact")
                    .cloned()
                    .ok_or_else(|| invalid("Thai tax source event fact payload"))?,
            )?;
            let source_fact: ThaiTaxFact =
                serde_json::from_value(row.try_get::<Value, _>("source_fact_payload")?)?;
            event_fact
                .validate()
                .map_err(|error| invalid(error.to_string()))?;
            source_fact
                .validate()
                .map_err(|error| invalid(error.to_string()))?;
            if event_fact.fact_id != aggregate_id || event_fact != source_fact {
                return Err(invalid(
                    "Thai tax source event payload does not match its authoritative fact",
                ));
            }
        }
        if source_event_fact_ids != source_fact_ids {
            return Err(invalid(
                "Thai tax source facts and outbox event identities do not match",
            ));
        }
        tx.commit().await?;
        if u64::try_from(rows.len()).map_err(|_| invalid("Thai tax source report count"))?
            > MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS
        {
            return Err(invalid(
                "Thai tax source report exceeds reconciliation bound",
            ));
        }
        let facts = rows
            .iter()
            .map(decode_reconciliation_fact)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            facts,
            u64::try_from(source_event_count)
                .map_err(|_| invalid("Thai tax source event count"))?,
            u64::try_from(source_fact_count).map_err(|_| invalid("Thai tax source fact count"))?,
        ))
    }

    async fn load_projected_report_facts(
        &self,
        holding_id: Uuid,
        company_id: Uuid,
        tax_period: &str,
        currency: &str,
    ) -> Result<Vec<ThaiTaxFact>, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT fact_id, holding_id, company_id, journal_id, line_id,
                    tax_date, btrim(tax_period) AS tax_period, kind,
                    btrim(currency) AS currency, base_minor, tax_minor,
                    tax_rate_basis_points, tax_identifier, recorded_at
               FROM erpx_tenant.finance_thai_tax_fact_read_rows
              WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
                AND btrim(tax_period)=$4 AND btrim(currency)=$5
              ORDER BY tax_date, fact_id
              LIMIT $6",
        )
        .bind(&self.holding_ref)
        .bind(holding_id)
        .bind(company_id)
        .bind(tax_period)
        .bind(currency)
        .bind(reconciliation_limit()?)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        if u64::try_from(rows.len()).map_err(|_| invalid("Thai tax projected report count"))?
            > MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS
        {
            return Err(invalid(
                "Thai tax projected report exceeds reconciliation bound",
            ));
        }
        rows.iter()
            .map(decode_reconciliation_fact)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn load_source_report_scopes(
        &self,
    ) -> Result<Vec<(Uuid, Uuid, String, String)>, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT DISTINCT holding_id, company_id,
                    btrim(tax_period) AS tax_period, btrim(currency) AS currency
               FROM erpx_tenant.finance_thai_tax_facts
              WHERE holding_ref=$1
              ORDER BY holding_id, company_id, tax_period, currency
              LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(
            i64::try_from(MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS + 1)
                .map_err(|_| invalid("Thai tax report scope bound"))?,
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        if u64::try_from(rows.len()).map_err(|_| invalid("Thai tax source scope count"))?
            > MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS
        {
            return Err(invalid("Thai tax source report scope bound"));
        }
        rows.into_iter()
            .map(|row| {
                Ok((
                    row.try_get("holding_id")?,
                    row.try_get("company_id")?,
                    row.try_get("tax_period")?,
                    row.try_get("currency")?,
                ))
            })
            .collect()
    }

    async fn load_projected_report_scopes(
        &self,
    ) -> Result<Vec<(Uuid, Uuid, String, String)>, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows = sqlx::query(
            "SELECT DISTINCT holding_id, company_id,
                    btrim(tax_period) AS tax_period, btrim(currency) AS currency
               FROM erpx_tenant.finance_thai_tax_fact_read_rows
              WHERE holding_ref=$1
              ORDER BY holding_id, company_id, tax_period, currency
              LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(
            i64::try_from(MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS + 1)
                .map_err(|_| invalid("Thai tax report scope bound"))?,
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        if u64::try_from(rows.len()).map_err(|_| invalid("Thai tax projected scope count"))?
            > MAX_FINANCE_THAI_TAX_RECONCILIATION_REPORTS
        {
            return Err(invalid("Thai tax projected report scope bound"));
        }
        rows.into_iter()
            .map(|row| {
                Ok((
                    row.try_get("holding_id")?,
                    row.try_get("company_id")?,
                    row.try_get("tax_period")?,
                    row.try_get("currency")?,
                ))
            })
            .collect()
    }

    async fn source_event_count(&self) -> Result<u64, PersistenceError> {
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*)
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2",
        )
        .bind(&self.holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        u64::try_from(count).map_err(|_| invalid("Thai tax source event count"))
    }

    /// Atomically replace the Thai tax read projection from a fenced source
    /// snapshot. The read clear and every fact replay share one transaction;
    /// source movement, decode failure, or a late SQL error therefore leaves
    /// the previously committed report intact.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceThaiTaxProjectionRebuildPreparation, u64), PersistenceError> {
        validate_page_size(page_size)?;
        let mut source_tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            tax_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if expected_source_version.is_some_and(|expected| expected != actual_source_version) {
            return Err(invalid(
                "Thai tax source fence changed before replacement rebuild",
            ));
        }

        let mut read_tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let preparation =
            clear_tax_projection_in_transaction(&mut read_tx, &self.holding_ref).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_tax_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                let fact: ThaiTaxFact = serde_json::from_value(
                    event
                        .payload
                        .get("fact")
                        .cloned()
                        .ok_or_else(|| invalid("tax fact payload"))?,
                )?;
                fact.validate()
                    .map_err(|error| invalid(error.to_string()))?;
                if fact.fact_id != event.aggregate_id {
                    return Err(invalid("Thai tax aggregate identity"));
                }
                if project_tax_event_in_transaction(
                    &mut read_tx,
                    &self.holding_ref,
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &fact,
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
                return Err(invalid("Thai tax source event cursor is missing"));
            }
        }
        let final_source_version =
            tax_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_version != actual_source_version {
            return Err(invalid(
                "Thai tax source fence changed during replacement rebuild",
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceThaiTaxProjectionRebuildPreparation, PersistenceError> {
        let mut tx = begin_scoped(&self.read_pool, &self.holding_ref).await?;
        let rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.finance_thai_tax_fact_read_rows WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.finance_thai_tax_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(FinanceThaiTaxProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        })
    }
}

async fn project_tax_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    fact: &ThaiTaxFact,
) -> Result<FinanceThaiTaxProjectionApplication, PersistenceError> {
    let current = sqlx::query(
        "SELECT last_event_id, source_version
           FROM erpx_tenant.finance_thai_tax_projection_checkpoints
          WHERE holding_ref=$1 AND fact_id=$2 FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(fact.fact_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(current) = current {
        let version = u64::try_from(current.try_get::<i64, _>("source_version")?)
            .map_err(|_| invalid("projection version"))?;
        let current_event: Uuid = current.try_get("last_event_id")?;
        if aggregate_version < version
            || (aggregate_version == version && current_event == event_id)
        {
            return Ok(FinanceThaiTaxProjectionApplication {
                event_id,
                aggregate_id,
                aggregate_version,
                applied: false,
            });
        }
        if aggregate_version != version + 1 {
            return Err(invalid("Thai tax projection version is not contiguous"));
        }
    } else if aggregate_version != 1 {
        return Err(invalid("Thai tax projection must start at version one"));
    }
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_thai_tax_fact_read_rows
            (holding_ref, fact_id, holding_id, company_id, journal_id, line_id,
             tax_date, tax_period, kind, currency, base_minor, tax_minor,
             tax_rate_basis_points, tax_identifier, source_version,
             source_event_id, recorded_at, projected_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,now())
         ON CONFLICT (holding_ref, fact_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id, company_id=EXCLUDED.company_id,
             journal_id=EXCLUDED.journal_id, line_id=EXCLUDED.line_id,
             tax_date=EXCLUDED.tax_date, tax_period=EXCLUDED.tax_period,
             kind=EXCLUDED.kind, currency=EXCLUDED.currency,
             base_minor=EXCLUDED.base_minor, tax_minor=EXCLUDED.tax_minor,
             tax_rate_basis_points=EXCLUDED.tax_rate_basis_points,
             tax_identifier=EXCLUDED.tax_identifier, source_version=EXCLUDED.source_version,
             source_event_id=EXCLUDED.source_event_id, recorded_at=EXCLUDED.recorded_at,
             projected_at=now()",
    )
    .bind(holding_ref)
    .bind(fact.fact_id)
    .bind(fact.holding_id)
    .bind(fact.company_id)
    .bind(fact.journal_id)
    .bind(fact.line_id)
    .bind(fact.tax_date)
    .bind(&fact.tax_period)
    .bind(fact.kind.as_str())
    .bind(&fact.currency)
    .bind(fact.base_minor)
    .bind(fact.tax_minor)
    .bind(i32::try_from(fact.tax_rate_basis_points).map_err(|_| invalid("tax rate"))?)
    .bind(&fact.tax_identifier)
    .bind(i64::try_from(aggregate_version).map_err(|_| invalid("source version"))?)
    .bind(event_id)
    .bind(fact.recorded_at)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_thai_tax_projection_checkpoints
            (holding_ref, fact_id, last_event_id, source_version)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (holding_ref, fact_id) DO UPDATE SET
             last_event_id=EXCLUDED.last_event_id,
             source_version=EXCLUDED.source_version, projected_at=now()",
    )
    .bind(holding_ref)
    .bind(fact.fact_id)
    .bind(event_id)
    .bind(i64::try_from(aggregate_version).map_err(|_| invalid("source version"))?)
    .execute(&mut **tx)
    .await?;
    Ok(FinanceThaiTaxProjectionApplication {
        event_id,
        aggregate_id,
        aggregate_version,
        applied: true,
    })
}

async fn tax_source_event_count_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_type=$2",
    )
    .bind(holding_ref)
    .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
    .fetch_one(&mut **tx)
    .await?;
    u64::try_from(count).map_err(|_| invalid("Thai tax source event count"))
}

async fn read_tax_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&FinanceThaiTaxSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceThaiTaxSourceEventPage, PersistenceError> {
    validate_page_size(page_size)?;
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        if cursor.aggregate_id.is_nil() || cursor.aggregate_version == 0 {
            return Err(invalid("Thai tax source cursor"));
        }
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2
                AND (aggregate_id > $3 OR
                     (aggregate_id=$3 AND aggregate_version > $4))
              ORDER BY aggregate_id, aggregate_version LIMIT $5",
        )
        .bind(holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .bind(cursor.aggregate_id)
        .bind(i64::try_from(cursor.aggregate_version).map_err(|_| invalid("cursor"))?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id, event_type, aggregate_id, aggregate_version,
                    payload, occurred_at
               FROM erpx_tenant.outbox_events
              WHERE holding_ref=$1 AND aggregate_type=$2
              ORDER BY aggregate_id, aggregate_version LIMIT $3",
        )
        .bind(holding_ref)
        .bind(FINANCE_THAI_TAX_FACT_AGGREGATE)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let take = usize::try_from(page_size).map_err(|_| invalid("page size"))?;
    let has_more = rows.len() > take;
    let events = rows
        .into_iter()
        .take(take)
        .map(|row| {
            Ok(FinanceThaiTaxSourceEvent {
                event_id: row.try_get("event_id")?,
                event_type: row.try_get("event_type")?,
                aggregate_id: row.try_get("aggregate_id")?,
                aggregate_version: u64::try_from(row.try_get::<i64, _>("aggregate_version")?)
                    .map_err(|_| invalid("source version"))?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = if has_more {
        events.last().map(|event| FinanceThaiTaxSourceEventCursor {
            aggregate_id: event.aggregate_id,
            aggregate_version: event.aggregate_version,
        })
    } else {
        None
    };
    Ok(FinanceThaiTaxSourceEventPage {
        events,
        next_cursor,
        has_more,
    })
}

async fn clear_tax_projection_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<FinanceThaiTaxProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_tenant.finance_thai_tax_fact_read_rows WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_tenant.finance_thai_tax_projection_checkpoints WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(FinanceThaiTaxProjectionRebuildPreparation {
        holding_ref: holding_ref.to_owned(),
        rows_cleared,
        checkpoints_cleared,
    })
}

// Read-model rows deliberately contain no command attribution. This neutral
// context is only used internally while invoking the pure report aggregator;
// it can never cross the API boundary.
fn return_context() -> erpx_domain::contracts::CommandContext {
    erpx_domain::contracts::CommandContext::try_new(
        "read-model-tax-report",
        erpx_domain::contracts::ActorAttribution {
            principal_ref: "read-model".into(),
            kind: erpx_domain::contracts::ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        erpx_domain::contracts::TenantScope::Platform,
        "finance_thai_tax_read_model",
    )
    .expect("bounded read-model context")
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration("holding_ref is invalid"));
    }
    Ok(())
}

fn validate_report_scope(
    holding_id: Uuid,
    company_id: Uuid,
    tax_period: &str,
    currency: &str,
) -> Result<(), PersistenceError> {
    if holding_id.is_nil() || company_id.is_nil() {
        return Err(invalid("Thai tax report scope identifier"));
    }
    if tax_period.len() != 7
        || tax_period.as_bytes().get(4) != Some(&b'-')
        || !tax_period.chars().enumerate().all(|(index, character)| {
            (index == 4 && character == '-') || (index != 4 && character.is_ascii_digit())
        })
    {
        return Err(invalid("Thai tax report period"));
    }
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(invalid("Thai tax report currency"));
    }
    Ok(())
}

fn reconciliation_limit() -> Result<i64, PersistenceError> {
    i64::try_from(MAX_FINANCE_THAI_TAX_RECONCILIATION_FACTS + 1)
        .map_err(|_| invalid("Thai tax reconciliation bound"))
}

fn decode_reconciliation_fact(row: &PgRow) -> Result<ThaiTaxFact, PersistenceError> {
    let kind = match row.try_get::<String, _>("kind")?.as_str() {
        "output_vat" => ThaiTaxKind::OutputVat,
        "input_vat" => ThaiTaxKind::InputVat,
        "withholding_payable" => ThaiTaxKind::WithholdingPayable,
        _ => return Err(invalid("Thai tax reconciliation kind")),
    };
    let fact = ThaiTaxFact {
        fact_id: row.try_get("fact_id")?,
        holding_id: row.try_get("holding_id")?,
        company_id: row.try_get("company_id")?,
        journal_id: row.try_get("journal_id")?,
        line_id: row.try_get("line_id")?,
        tax_date: row.try_get("tax_date")?,
        tax_period: row.try_get("tax_period")?,
        kind,
        currency: row.try_get("currency")?,
        base_minor: row.try_get("base_minor")?,
        tax_minor: row.try_get("tax_minor")?,
        tax_rate_basis_points: u32::try_from(row.try_get::<i32, _>("tax_rate_basis_points")?)
            .map_err(|_| invalid("Thai tax reconciliation rate"))?,
        tax_identifier: row.try_get("tax_identifier")?,
        context: return_context(),
        recorded_at: row.try_get("recorded_at")?,
    };
    fact.validate()
        .map_err(|error| invalid(error.to_string()))?;
    Ok(fact)
}

fn validate_page_size(value: u32) -> Result<(), PersistenceError> {
    if value == 0 || value > MAX_FINANCE_THAI_TAX_SOURCE_EVENT_PAGE_SIZE {
        return Err(invalid("Thai tax source event page size"));
    }
    Ok(())
}

fn invalid(value: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(value.into())
}

fn validate_tax_fact_replay_evidence(
    source_exists: bool,
    outbox_event_count: u64,
) -> Result<(), PersistenceError> {
    if !source_exists {
        return Err(invalid(
            "Thai tax idempotent replay requires its authoritative source fact",
        ));
    }
    if outbox_event_count != 1 {
        return Err(invalid(format!(
            "Thai tax fact requires exactly one authoritative outbox event, found {outbox_event_count}"
        )));
    }
    Ok(())
}

fn sha256_hex(value: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(value);
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tax_fact_replay_requires_source_and_exact_outbox_evidence() {
        assert!(validate_tax_fact_replay_evidence(true, 1).is_ok());
        assert!(validate_tax_fact_replay_evidence(false, 1).is_err());
        assert!(validate_tax_fact_replay_evidence(true, 0).is_err());
        assert!(validate_tax_fact_replay_evidence(true, 2).is_err());
    }
}
