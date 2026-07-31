//! SQLx persistence for company-scoped Finance close reviews.
//!
//! The close aggregate intentionally has no independent identifier or version
//! field.  A period can have one close review, so `period_id` is the aggregate
//! identity and this adapter owns the monotonically increasing persistence
//! version.  Every accepted domain transition updates the tenant write row,
//! aggregate snapshot, and outbox event in one holding-scoped transaction.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_domain::contracts::CommandContext;
use erpx_domain::finance::{AccountingPeriod, PeriodStatus, PostedJournalEntry};
use erpx_domain::finance_close::{
    CloseInput, CloseJournalFact, CloseStatus, FinanceCloseReview, MAX_CONTROL_CODE_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_close_control_evidence::{
    FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE, FINANCE_CLOSE_CONTROL_EVENT_TYPE,
    deterministic_control_aggregate_id_for_scope,
};
use crate::finance_persistence::{begin_scoped, write_finance_snapshot_and_outbox};

type CloseReviewRow = (
    i64,
    Value,
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);
type CloseReviewSnapshotRow = (
    i64,
    Value,
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    DateTime<Utc>,
);

const CLOSE_REVIEW_AGGREGATE: &str = "finance_close_review";
const CLOSE_REVIEW_EVENT: &str = "finance.close_review_saved.v1";
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 255;
const MAX_AUTHORITATIVE_CLOSE_FACTS: usize = 4_096;

type AuthoritativeJournalRow = (Uuid, Uuid, Uuid, Uuid, i64, Value);
type CloseControlEvidenceRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    String,
    Uuid,
    i64,
    String,
    bool,
    DateTime<Utc>,
    Value,
);
type CloseControlEvidenceEventRow = (Uuid, i64, String, Value);
type CloseReviewOutboxRow = (String, Value);
type CloseReviewSnapshotEvidenceRow = (i64, Value, String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FinanceCloseSourceReconciliation {
    source_version: u64,
    authoritative_fact_count: usize,
    submitted_fact_count: usize,
    missing_authoritative_fact_count: usize,
    unsubmitted_authoritative_fact_count: usize,
}

impl FinanceCloseSourceReconciliation {
    fn is_exact(&self) -> bool {
        self.missing_authoritative_fact_count == 0 && self.unsubmitted_authoritative_fact_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FinanceCloseControlReconciliation {
    authoritative_control_count: usize,
    submitted_control_count: usize,
    unresolved_authoritative_control_count: usize,
    source_version: u64,
}

impl FinanceCloseControlReconciliation {
    fn is_exact(&self) -> bool {
        self.authoritative_control_count == self.submitted_control_count
    }

    fn is_closeable(&self) -> bool {
        self.is_exact() && self.unresolved_authoritative_control_count == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseReviewSaveReceipt {
    pub review_id: Uuid,
    pub version: u64,
    pub status: CloseStatus,
    pub idempotency_key: String,
    pub replayed: bool,
}

/// A validated, read-only view of the close row metadata that is not part of
/// the pure domain aggregate.  The payload is decoded and validated before
/// this type is returned; callers never receive the raw JSONB value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloseReviewSnapshot {
    pub(crate) review: FinanceCloseReview,
    pub(crate) version: u64,
    pub(crate) idempotency_key: String,
    pub(crate) updated_at: DateTime<Utc>,
}

/// A control result produced by a Finance source context.  Close review
/// persistence only accepts flags that match a row in this evidence ledger;
/// callers cannot mark a control resolved by changing the close payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceCloseControlEvidence {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub control_code: String,
    pub source_type: String,
    pub source_aggregate_id: Uuid,
    pub evidence_ref: String,
    pub resolved: bool,
    pub observed_at: DateTime<Utc>,
    pub payload: Value,
}

impl FinanceCloseControlEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        control_code: impl Into<String>,
        source_type: impl Into<String>,
        source_aggregate_id: Uuid,
        evidence_ref: impl Into<String>,
        resolved: bool,
        observed_at: DateTime<Utc>,
        payload: Value,
    ) -> Result<Self, PersistenceError> {
        let evidence = Self {
            holding_id,
            company_id,
            period_id,
            control_code: control_code.into(),
            source_type: source_type.into(),
            source_aggregate_id,
            evidence_ref: evidence_ref.into(),
            resolved,
            observed_at,
            payload,
        };
        validate_control_evidence(&evidence)?;
        Ok(evidence)
    }
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceCloseStore {
    write_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceCloseStore {
    pub fn new(
        write_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            holding_ref,
        })
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Persist an open, review, or closed candidate after replaying the
    /// corresponding domain transition against the stored aggregate.
    ///
    /// `expected_version == None` is reserved for the initial open.  Accepted
    /// transitions advance the adapter-owned version by one and emit one
    /// snapshot/outbox pair atomically.  The current command context is the
    /// idempotency binding; it is deliberately not accepted as an independent
    /// string so callers cannot persist a key belonging to another actor or
    /// transition.
    pub async fn save_review(
        &self,
        review: &FinanceCloseReview,
        period: &AccountingPeriod,
        expected_version: Option<u64>,
    ) -> Result<CloseReviewSaveReceipt, PersistenceError> {
        validate_period(period)?;
        validate_review_identity(review, period)?;
        let context = mutation_context(review)?;
        validate_idempotency_key(&context.idempotency_key)?;
        let payload = serde_json::to_value(review)?;
        let review_id = review.period_id();

        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let authoritative_period =
            load_authoritative_period(&mut tx, &self.holding_ref, period.id).await?;
        if authoritative_period != *period {
            return Err(invalid(
                "Finance close period does not match authoritative tenant write state",
            ));
        }
        lock_review(&mut tx, &self.holding_ref, review_id).await?;
        let stored: Option<CloseReviewRow> = sqlx::query_as(
            "SELECT version, payload, holding_id, company_id, period_id, status,
                    idempotency_key, opened_by, reviewed_by, closed_by
             FROM erpx_tenant.finance_close_reviews
             WHERE holding_ref=$1 AND period_id=$2
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(review_id)
        .fetch_optional(&mut *tx)
        .await?;

        let receipt = match stored {
            None => {
                if expected_version.is_some() {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: CLOSE_REVIEW_AGGREGATE.into(),
                        aggregate_id: review_id,
                        expected: expected_version,
                        actual: None,
                    });
                }
                if review.status() != CloseStatus::Open {
                    return Err(invalid("a new Finance close review must start open"));
                }
                let expected = FinanceCloseReview::open(&authoritative_period, context)
                    .map_err(domain_error)?;
                if &expected != review {
                    return Err(invalid(
                        "open close review does not match its domain construction",
                    ));
                }
                let conflicting_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT period_id
                     FROM erpx_tenant.finance_close_reviews
                     WHERE holding_ref=$1 AND idempotency_key=$2",
                )
                .bind(&self.holding_ref)
                .bind(&context.idempotency_key)
                .fetch_optional(&mut *tx)
                .await?;
                if conflicting_id.is_some() {
                    return Err(PersistenceError::IdempotencyConflict {
                        key: context.idempotency_key.clone(),
                    });
                }
                ensure_close_review_next_version_is_unclaimed(
                    &mut tx,
                    &self.holding_ref,
                    review_id,
                    None,
                    1,
                )
                .await?;
                insert_review(&mut tx, &self.holding_ref, review, 1, &payload).await?;
                write_finance_snapshot_and_outbox(
                    &mut tx,
                    &self.holding_ref,
                    CLOSE_REVIEW_AGGREGATE,
                    review_id,
                    1,
                    &payload,
                    CLOSE_REVIEW_EVENT,
                )
                .await?;
                CloseReviewSaveReceipt {
                    review_id,
                    version: 1,
                    status: review.status(),
                    idempotency_key: context.idempotency_key.clone(),
                    replayed: false,
                }
            }
            Some(row) => {
                let stored_version = as_u64(row.0, "Finance close review version")?;
                validate_persisted_row(&row, review_id)?;
                let current: FinanceCloseReview = serde_json::from_value(row.1.clone())?;
                validate_review_identity(&current, period)?;
                validate_persisted_identity(&row, &current)?;
                if row.6 == context.idempotency_key {
                    if row.1 == payload {
                        validate_close_review_persistence_evidence(
                            &mut tx,
                            &self.holding_ref,
                            review_id,
                            stored_version,
                            &payload,
                        )
                        .await?;
                        tx.commit().await?;
                        return Ok(CloseReviewSaveReceipt {
                            review_id,
                            version: stored_version,
                            status: review.status(),
                            idempotency_key: context.idempotency_key.clone(),
                            replayed: true,
                        });
                    }
                    return Err(PersistenceError::IdempotencyConflict {
                        key: context.idempotency_key.clone(),
                    });
                }
                // Every accepted transition must preserve the complete
                // snapshot/outbox history. The generic snapshot helper uses
                // an upsert and the outbox has no aggregate/version unique
                // constraint, so accepting a transition over a damaged
                // predecessor would silently hide an authoritative gap.
                validate_close_review_persistence_evidence(
                    &mut tx,
                    &self.holding_ref,
                    review_id,
                    stored_version,
                    &row.1,
                )
                .await?;
                if let Some(input) = review.input() {
                    let control_reconciliation = validate_authoritative_input(
                        &mut tx,
                        &self.holding_ref,
                        &authoritative_period,
                        input,
                    )
                    .await?;
                    if review.status() == CloseStatus::Closed
                        && !control_reconciliation.is_closeable()
                    {
                        return Err(invalid(format!(
                            "Finance close controls are not closeable after authoritative reconciliation (source_version={}, authoritative={}, submitted={}, unresolved={})",
                            control_reconciliation.source_version,
                            control_reconciliation.authoritative_control_count,
                            control_reconciliation.submitted_control_count,
                            control_reconciliation.unresolved_authoritative_control_count,
                        )));
                    }
                }
                let expected =
                    expected_version.ok_or_else(|| PersistenceError::OptimisticConcurrency {
                        aggregate_type: CLOSE_REVIEW_AGGREGATE.into(),
                        aggregate_id: review_id,
                        expected: None,
                        actual: Some(stored_version),
                    })?;
                if expected != stored_version {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: CLOSE_REVIEW_AGGREGATE.into(),
                        aggregate_id: review_id,
                        expected: Some(expected),
                        actual: Some(stored_version),
                    });
                }
                validate_transition(&current, review, &authoritative_period)?;
                let next_version = expected
                    .checked_add(1)
                    .ok_or_else(|| invalid("Finance close review version overflow"))?;
                ensure_close_review_next_version_is_unclaimed(
                    &mut tx,
                    &self.holding_ref,
                    review_id,
                    Some(stored_version),
                    next_version,
                )
                .await?;
                let affected = sqlx::query(
                    "UPDATE erpx_tenant.finance_close_reviews
                     SET holding_id=$3, company_id=$4, status=$5, version=$6,
                         idempotency_key=$7, opened_by=$8, reviewed_by=$9,
                         closed_by=$10, payload=$11, updated_at=now()
                     WHERE holding_ref=$1 AND period_id=$2 AND version=$12",
                )
                .bind(&self.holding_ref)
                .bind(review_id)
                .bind(review.holding_id())
                .bind(review.company_id())
                .bind(close_status(review.status()))
                .bind(as_i64(next_version, "Finance close review version")?)
                .bind(&context.idempotency_key)
                .bind(review.opened_by())
                .bind(review.reviewed_by())
                .bind(review.closed_by())
                .bind(&payload)
                .bind(as_i64(expected, "Finance close review version")?)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: CLOSE_REVIEW_AGGREGATE.into(),
                        aggregate_id: review_id,
                        expected: Some(expected),
                        actual: Some(stored_version),
                    });
                }
                write_finance_snapshot_and_outbox(
                    &mut tx,
                    &self.holding_ref,
                    CLOSE_REVIEW_AGGREGATE,
                    review_id,
                    next_version,
                    &payload,
                    CLOSE_REVIEW_EVENT,
                )
                .await?;
                CloseReviewSaveReceipt {
                    review_id,
                    version: next_version,
                    status: review.status(),
                    idempotency_key: context.idempotency_key.clone(),
                    replayed: false,
                }
            }
        };
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn get_review(
        &self,
        period_id: Uuid,
    ) -> Result<Option<FinanceCloseReview>, PersistenceError> {
        Ok(self
            .get_review_snapshot(period_id)
            .await?
            .map(|snapshot| snapshot.review))
    }

    /// Read a validated close snapshot without mutating the write database.
    /// The close-review table remains authoritative until a dedicated close
    /// read projection is introduced by a later migration packet.
    pub(crate) async fn get_review_snapshot(
        &self,
        period_id: Uuid,
    ) -> Result<Option<CloseReviewSnapshot>, PersistenceError> {
        if period_id.is_nil() {
            return Err(invalid("period_id cannot be nil"));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row: Option<CloseReviewSnapshotRow> = sqlx::query_as(
            "SELECT version, payload, holding_id, company_id, period_id, status,
                    idempotency_key, opened_by, reviewed_by, closed_by, updated_at
             FROM erpx_tenant.finance_close_reviews
             WHERE holding_ref=$1 AND period_id=$2
             FOR SHARE",
        )
        .bind(&self.holding_ref)
        .bind(period_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let base_row: CloseReviewRow = (
            row.0,
            row.1.clone(),
            row.2,
            row.3,
            row.4,
            row.5.clone(),
            row.6.clone(),
            row.7.clone(),
            row.8.clone(),
            row.9.clone(),
        );
        let version = as_u64(base_row.0, "Finance close review version")?;
        validate_persisted_row(&base_row, period_id)?;
        validate_idempotency_key(&base_row.6)?;
        let review: FinanceCloseReview = serde_json::from_value(row.1.clone())?;
        validate_persisted_identity(&base_row, &review)?;
        validate_close_review_persistence_evidence(
            &mut tx,
            &self.holding_ref,
            period_id,
            version,
            &row.1,
        )
        .await?;
        tx.commit().await?;
        Ok(Some(CloseReviewSnapshot {
            review,
            version,
            idempotency_key: base_row.6,
            updated_at: row.10,
        }))
    }
}

async fn load_authoritative_period(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    period_id: Uuid,
) -> Result<AccountingPeriod, PersistenceError> {
    let row: Option<(Uuid, Uuid, String, Value)> = sqlx::query_as(
        "SELECT holding_id, company_id, status, payload
         FROM erpx_tenant.finance_periods
         WHERE holding_ref=$1 AND period_id=$2
         FOR SHARE",
    )
    .bind(holding_ref)
    .bind(period_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((row_holding_id, row_company_id, row_status, payload)) = row else {
        return Err(PersistenceError::NotFound {
            entity: "Finance accounting period",
            id: period_id,
        });
    };
    let period: AccountingPeriod = serde_json::from_value(payload)?;
    period.validate().map_err(domain_error)?;
    if row_holding_id != period.holding_id
        || row_company_id != period.company_id
        || row_status != accounting_period_status(period.status)
    {
        return Err(invalid(
            "persisted Finance accounting period columns do not match its payload",
        ));
    }
    Ok(period)
}

fn accounting_period_status(status: PeriodStatus) -> &'static str {
    match status {
        PeriodStatus::Open => "open",
        PeriodStatus::Locked => "locked",
        PeriodStatus::Closed => "closed",
    }
}

async fn lock_review(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    review_id: Uuid,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{holding_ref}:finance-close-review:{review_id}"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_review(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    review: &FinanceCloseReview,
    version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    let context = mutation_context(review)?;
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_close_reviews
            (holding_ref, holding_id, company_id, period_id, status, version,
             idempotency_key, opened_by, reviewed_by, closed_by, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(holding_ref)
    .bind(review.holding_id())
    .bind(review.company_id())
    .bind(review.period_id())
    .bind(close_status(review.status()))
    .bind(as_i64(version, "Finance close review version")?)
    .bind(&context.idempotency_key)
    .bind(review.opened_by())
    .bind(review.reviewed_by())
    .bind(review.closed_by())
    .bind(payload)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(())
}

fn validate_transition(
    current: &FinanceCloseReview,
    candidate: &FinanceCloseReview,
    period: &AccountingPeriod,
) -> Result<(), PersistenceError> {
    let expected = match (current.status(), candidate.status()) {
        (CloseStatus::Open, CloseStatus::Review) => {
            let input = candidate
                .input()
                .cloned()
                .ok_or_else(|| invalid("review close candidate has no input"))?;
            let context = candidate
                .reviewed_context()
                .ok_or_else(|| invalid("review close candidate has no reviewer context"))?;
            let mut expected = current.clone();
            expected
                .submit_for_review(period, context, input)
                .map_err(domain_error)?;
            expected
        }
        (CloseStatus::Review, CloseStatus::Review) => {
            let input = candidate
                .input()
                .cloned()
                .ok_or_else(|| invalid("revised close candidate has no input"))?;
            let context = candidate
                .reviewed_context()
                .ok_or_else(|| invalid("revised close candidate has no reviewer context"))?;
            let mut expected = current.clone();
            expected
                .revise_review_input(period, context, input)
                .map_err(domain_error)?;
            expected
        }
        (CloseStatus::Review, CloseStatus::Closed) => {
            let context = candidate
                .closed_context()
                .ok_or_else(|| invalid("closed close candidate has no closer context"))?;
            let mut expected = current.clone();
            expected.close(period, context).map_err(domain_error)?;
            expected
        }
        (from, to) => {
            return Err(invalid(format!(
                "Finance close review transition from {from:?} to {to:?} is not supported"
            )));
        }
    };
    if &expected != candidate {
        return Err(invalid(
            "Finance close review candidate does not match the domain transition",
        ));
    }
    Ok(())
}

async fn validate_authoritative_input(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    period: &AccountingPeriod,
    input: &CloseInput,
) -> Result<FinanceCloseControlReconciliation, PersistenceError> {
    let reconciliation = reconcile_authoritative_input(tx, holding_ref, period, input).await?;
    if !reconciliation.is_exact() {
        return Err(invalid(format!(
            "Finance close facts do not exactly reconcile with the immutable journal source (source_version={}, authoritative={}, submitted={}, missing={}, unsubmitted={})",
            reconciliation.source_version,
            reconciliation.authoritative_fact_count,
            reconciliation.submitted_fact_count,
            reconciliation.missing_authoritative_fact_count,
            reconciliation.unsubmitted_authoritative_fact_count,
        )));
    }
    validate_authoritative_controls(tx, holding_ref, period, input).await
}

/// Reconcile a close candidate against one immutable source snapshot.
///
/// Finance posting owns `finance_posted_journals`; close review may only
/// reference those facts.  The SHARE table lock prevents a concurrent post
/// from changing the source set while this exact-set check runs.  A close
/// candidate is rejected when it omits a posted journal or introduces a fact
/// that is not present in the authoritative source.  This is deliberately
/// bounded to the same 4,096-fact reporting limit as the close projection.
async fn reconcile_authoritative_input(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    period: &AccountingPeriod,
    input: &CloseInput,
) -> Result<FinanceCloseSourceReconciliation, PersistenceError> {
    if input.facts().len() > MAX_AUTHORITATIVE_CLOSE_FACTS {
        return Err(invalid(
            "Finance close input exceeds authoritative fact bound",
        ));
    }

    sqlx::query("LOCK TABLE erpx_tenant.finance_posted_journals IN SHARE MODE")
        .execute(&mut **tx)
        .await?;
    let rows: Vec<AuthoritativeJournalRow> = sqlx::query_as(
        "SELECT journal_id, holding_id, company_id, period_id, version, payload
         FROM erpx_tenant.finance_posted_journals
         WHERE holding_ref=$1 AND period_id=$2
         ORDER BY journal_id
         LIMIT $3",
    )
    .bind(holding_ref)
    .bind(period.id)
    .bind(
        i64::try_from(MAX_AUTHORITATIVE_CLOSE_FACTS + 1)
            .map_err(|_| invalid("Finance close authoritative fact bound is not representable"))?,
    )
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() > MAX_AUTHORITATIVE_CLOSE_FACTS {
        return Err(invalid(
            "Finance close authoritative journal set exceeds fact bound",
        ));
    }

    let mut source = BTreeMap::new();
    let mut source_version = 0_u64;
    for (journal_id, holding_id, company_id, period_id, version, payload) in rows {
        let version = as_u64(version, "authoritative Finance journal source version")?;
        source_version = source_version.max(version);
        let posted: PostedJournalEntry = serde_json::from_value(payload).map_err(|error| {
            invalid(format!(
                "authoritative Finance journal {journal_id} is invalid: {error}"
            ))
        })?;
        validate_posted_journal_record(
            &posted, journal_id, holding_id, company_id, period_id, period,
        )?;
        if source
            .insert(journal_id, (holding_id, company_id, period_id, posted))
            .is_some()
        {
            return Err(invalid(
                "Finance close authoritative journal source is duplicated",
            ));
        }
    }

    let submitted_ids: BTreeSet<Uuid> = input
        .facts()
        .iter()
        .map(CloseJournalFact::fact_id)
        .collect();
    let missing_authoritative_fact_count = input
        .facts()
        .iter()
        .filter(|fact| !source.contains_key(&fact.fact_id()))
        .count();
    for fact in input.facts() {
        let Some((holding_id, company_id, period_id, posted)) = source.get(&fact.fact_id()) else {
            continue;
        };
        validate_posted_journal_source(posted, *holding_id, *company_id, *period_id, fact, period)?;
    }
    let unsubmitted_authoritative_fact_count = source
        .keys()
        .filter(|journal_id| !submitted_ids.contains(journal_id))
        .count();
    Ok(FinanceCloseSourceReconciliation {
        source_version,
        authoritative_fact_count: source.len(),
        submitted_fact_count: input.facts().len(),
        missing_authoritative_fact_count,
        unsubmitted_authoritative_fact_count,
    })
}

fn validate_posted_journal_record(
    posted: &PostedJournalEntry,
    row_journal_id: Uuid,
    row_holding_id: Uuid,
    row_company_id: Uuid,
    row_period_id: Uuid,
    period: &AccountingPeriod,
) -> Result<(), PersistenceError> {
    if posted.id() != row_journal_id
        || posted.holding_id() != row_holding_id
        || posted.company_id() != row_company_id
        || posted.period_id() != row_period_id
        || posted.holding_id() != period.holding_id
        || posted.company_id() != period.company_id
        || posted.period_id() != period.id
    {
        return Err(invalid(format!(
            "authoritative Finance journal {row_journal_id} has invalid close identity"
        )));
    }
    if posted.entry_date() < period.starts_on || posted.entry_date() > period.ends_on {
        return Err(invalid(format!(
            "authoritative Finance journal {row_journal_id} is outside the accounting period"
        )));
    }
    if posted.lines().len() < 2 {
        return Err(invalid(format!(
            "authoritative Finance journal {row_journal_id} has too few lines"
        )));
    }
    Ok(())
}

fn validate_posted_journal_source(
    posted: &PostedJournalEntry,
    row_holding_id: Uuid,
    row_company_id: Uuid,
    row_period_id: Uuid,
    fact: &CloseJournalFact,
    period: &AccountingPeriod,
) -> Result<(), PersistenceError> {
    validate_posted_journal_record(
        posted,
        fact.fact_id(),
        row_holding_id,
        row_company_id,
        row_period_id,
        period,
    )?;
    if posted.entry_date() != fact.entry_date() {
        return Err(invalid(format!(
            "Finance close fact {} does not match its immutable posted journal",
            fact.fact_id()
        )));
    }

    let mut source_totals: BTreeMap<Uuid, (i64, i64)> = BTreeMap::new();
    let mut debit_total = 0_i64;
    let mut credit_total = 0_i64;
    for line in posted.lines() {
        line.validate().map_err(domain_error)?;
        debit_total = debit_total
            .checked_add(line.debit_minor)
            .ok_or_else(|| invalid("authoritative Finance journal debit overflowed"))?;
        credit_total = credit_total
            .checked_add(line.credit_minor)
            .ok_or_else(|| invalid("authoritative Finance journal credit overflowed"))?;
        let totals = source_totals.entry(line.account_id).or_default();
        totals.0 = totals
            .0
            .checked_add(line.debit_minor)
            .ok_or_else(|| invalid("authoritative Finance account debit overflowed"))?;
        totals.1 = totals
            .1
            .checked_add(line.credit_minor)
            .ok_or_else(|| invalid("authoritative Finance account credit overflowed"))?;
    }
    if debit_total != credit_total {
        return Err(invalid(format!(
            "authoritative Finance journal {} is unbalanced",
            fact.fact_id()
        )));
    }
    let submitted_totals: BTreeMap<Uuid, (i64, i64)> = fact
        .lines()
        .iter()
        .map(|line| (line.account_id(), (line.debit_minor(), line.credit_minor())))
        .collect();
    if source_totals != submitted_totals {
        return Err(invalid(format!(
            "Finance close fact {} does not match authoritative journal lines",
            fact.fact_id()
        )));
    }
    Ok(())
}

async fn validate_authoritative_controls(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    period: &AccountingPeriod,
    input: &CloseInput,
) -> Result<FinanceCloseControlReconciliation, PersistenceError> {
    let initial_rows: Vec<CloseControlEvidenceRow> = sqlx::query_as(
        "SELECT holding_id, company_id, period_id, control_code, source_type,
                source_aggregate_id, source_version, evidence_ref, resolved,
                observed_at, payload
         FROM erpx_tenant.finance_close_control_evidence
         WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3 AND period_id=$4
         ORDER BY control_code",
    )
    .bind(holding_ref)
    .bind(period.holding_id)
    .bind(period.company_id)
    .bind(period.id)
    .fetch_all(&mut **tx)
    .await?;

    // Source publication serializes on the period row before it changes the
    // evidence row and appends its outbox event. The close transaction already
    // holds that period row FOR SHARE, so the second read is a stable source
    // snapshot and cannot race a newly inserted control.
    let rows: Vec<CloseControlEvidenceRow> = sqlx::query_as(
        "SELECT holding_id, company_id, period_id, control_code, source_type,
                source_aggregate_id, source_version, evidence_ref, resolved,
                observed_at, payload
         FROM erpx_tenant.finance_close_control_evidence
         WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3 AND period_id=$4
         ORDER BY control_code
         FOR SHARE",
    )
    .bind(holding_ref)
    .bind(period.holding_id)
    .bind(period.company_id)
    .bind(period.id)
    .fetch_all(&mut **tx)
    .await?;
    if rows != initial_rows {
        return Err(invalid(
            "Finance close control evidence changed during reconciliation",
        ));
    }

    let mut source_version = 0_u64;
    let mut unresolved_authoritative_control_count = 0_usize;
    for row in &rows {
        validate_authoritative_control_row(row, period)?;
        source_version = source_version.max(as_u64(row.6, "Finance close control source version")?);
        if !row.8 {
            unresolved_authoritative_control_count =
                unresolved_authoritative_control_count.saturating_add(1);
        }
        validate_authoritative_control_event(tx, holding_ref, row).await?;
    }

    if rows.len() != input.controls().len()
        || rows.iter().any(|row| {
            input
                .controls()
                .iter()
                .find(|control| control.code() == row.3)
                .is_none_or(|control| control.resolved() != row.8)
        })
    {
        return Err(invalid(
            "Finance close controls do not match authoritative control evidence",
        ));
    }
    for control in input.controls() {
        if !rows
            .iter()
            .any(|row| row.3 == control.code() && row.8 == control.resolved())
        {
            return Err(invalid(format!(
                "Finance close control {} has no authoritative evidence",
                control.code()
            )));
        }
    }
    Ok(FinanceCloseControlReconciliation {
        authoritative_control_count: rows.len(),
        submitted_control_count: input.controls().len(),
        unresolved_authoritative_control_count,
        source_version,
    })
}

async fn validate_authoritative_control_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    row: &CloseControlEvidenceRow,
) -> Result<(), PersistenceError> {
    let expected_aggregate_id =
        deterministic_control_aggregate_id_for_scope(holding_ref, row.0, row.1, row.2, &row.3);
    let events: Vec<CloseControlEvidenceEventRow> = sqlx::query_as(
        "SELECT aggregate_id, aggregate_version, event_type, payload
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2
           AND aggregate_id=$3 AND aggregate_version=$4
           AND event_type=$5
         ORDER BY event_id
         LIMIT 2
         FOR SHARE",
    )
    .bind(holding_ref)
    .bind(FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE)
    .bind(expected_aggregate_id)
    .bind(row.6)
    .bind(FINANCE_CLOSE_CONTROL_EVENT_TYPE)
    .fetch_all(&mut **tx)
    .await?;
    if events.len() != 1 {
        return Err(invalid(format!(
            "Finance close control {} has no unique authoritative source event",
            row.3
        )));
    }
    let (aggregate_id, aggregate_version, event_type, payload) = &events[0];
    if *aggregate_id != expected_aggregate_id
        || *aggregate_version != row.6
        || event_type != FINANCE_CLOSE_CONTROL_EVENT_TYPE
        || payload.get("aggregate_type")
            != Some(&Value::String(FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE.into()))
        || payload.get("version") != Some(&Value::Number(row.6.into()))
    {
        return Err(invalid(format!(
            "Finance close control {} source event envelope is invalid",
            row.3
        )));
    }

    let aggregate = payload
        .get("aggregate")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("Finance close control source event aggregate is invalid"))?;
    let source = aggregate
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("Finance close control source event source is invalid"))?;
    let control = aggregate
        .get("control")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("Finance close control source event control is invalid"))?;
    let evidence = aggregate
        .get("evidence")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("Finance close control source event evidence is invalid"))?;
    let expected_event_key = format!(
        "{holding_ref}:{FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE}:{}:{}:{}:{}:{}:{}",
        row.0, row.1, row.2, row.3, row.5, row.6,
    );
    let event_observed_at: DateTime<Utc> = evidence
        .get("observed_at")
        .cloned()
        .ok_or_else(|| invalid("Finance close control source event observation is missing"))
        .and_then(|value| {
            serde_json::from_value(value)
                .map_err(|_| invalid("Finance close control source event observation is invalid"))
        })?;
    if aggregate.get("event_key") != Some(&Value::String(expected_event_key))
        || source.get("type") != Some(&Value::String(row.4.clone()))
        || source.get("aggregate_id") != Some(&Value::String(row.5.to_string()))
        || source.get("version") != Some(&Value::Number(row.6.into()))
        || control.get("holding_id") != Some(&Value::String(row.0.to_string()))
        || control.get("company_id") != Some(&Value::String(row.1.to_string()))
        || control.get("period_id") != Some(&Value::String(row.2.to_string()))
        || control.get("code") != Some(&Value::String(row.3.clone()))
        || evidence.get("reference") != Some(&Value::String(row.7.clone()))
        || evidence.get("resolved") != Some(&Value::Bool(row.8))
        // PostgreSQL stores timestamptz at microsecond precision while the
        // source event retains the caller's chrono value. Compare at the
        // database precision so a valid source publication is not rejected
        // merely because its input carried sub-microsecond digits.
        || event_observed_at.timestamp_micros() != row.9.timestamp_micros()
        || evidence.get("payload") != Some(&row.10)
    {
        return Err(invalid(format!(
            "Finance close control {} source event does not reconcile to the authoritative row",
            row.3
        )));
    }
    Ok(())
}

fn validate_authoritative_control_row(
    row: &CloseControlEvidenceRow,
    period: &AccountingPeriod,
) -> Result<(), PersistenceError> {
    if row.0 != period.holding_id || row.1 != period.company_id || row.2 != period.id {
        return Err(invalid(
            "persisted Finance close control evidence identity does not match the period",
        ));
    }
    if row.6 <= 0 {
        return Err(invalid(
            "persisted Finance close control evidence source version is invalid",
        ));
    }
    FinanceCloseControlEvidence::new(
        row.0,
        row.1,
        row.2,
        row.3.clone(),
        row.4.clone(),
        row.5,
        row.7.clone(),
        row.8,
        row.9,
        row.10.clone(),
    )?;
    Ok(())
}

fn mutation_context(review: &FinanceCloseReview) -> Result<&CommandContext, PersistenceError> {
    match review.status() {
        CloseStatus::Open => Ok(review.opened_context()),
        CloseStatus::Review => review
            .reviewed_context()
            .ok_or_else(|| invalid("review close aggregate has no reviewer context")),
        CloseStatus::Closed => review
            .closed_context()
            .ok_or_else(|| invalid("closed close aggregate has no closer context")),
    }
}

fn validate_period(period: &AccountingPeriod) -> Result<(), PersistenceError> {
    period.validate().map_err(domain_error)?;
    if period.status == PeriodStatus::Closed {
        return Err(invalid("accounting period is already closed"));
    }
    Ok(())
}

fn validate_review_identity(
    review: &FinanceCloseReview,
    period: &AccountingPeriod,
) -> Result<(), PersistenceError> {
    if review.holding_id() != period.holding_id {
        return Err(invalid(
            "Finance close review holding does not match period",
        ));
    }
    if review.company_id() != period.company_id {
        return Err(invalid(
            "Finance close review company does not match period",
        ));
    }
    if review.period_id() != period.id {
        return Err(invalid("Finance close review period does not match period"));
    }
    Ok(())
}

fn validate_persisted_row(row: &CloseReviewRow, review_id: Uuid) -> Result<(), PersistenceError> {
    if row.0 <= 0 {
        return Err(invalid("persisted Finance close review version is invalid"));
    }
    if row.4 != review_id {
        return Err(invalid(
            "persisted Finance close review identity is invalid",
        ));
    }
    Ok(())
}

fn validate_persisted_identity(
    row: &CloseReviewRow,
    review: &FinanceCloseReview,
) -> Result<(), PersistenceError> {
    if row.2 != review.holding_id()
        || row.3 != review.company_id()
        || row.4 != review.period_id()
        || row.5 != close_status(review.status())
        || row.6 != mutation_context(review)?.idempotency_key.as_str()
        || row.7 != review.opened_by()
        || row.8.as_deref() != review.reviewed_by()
        || row.9.as_deref() != review.closed_by()
    {
        return Err(invalid(
            "persisted Finance close review columns do not match its payload",
        ));
    }
    Ok(())
}

fn close_status(status: CloseStatus) -> &'static str {
    match status {
        CloseStatus::Open => "open",
        CloseStatus::Review => "review",
        CloseStatus::Closed => "closed",
    }
}

fn validate_idempotency_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(invalid("Finance close review idempotency key is invalid"));
    }
    Ok(())
}

fn validate_control_evidence(
    evidence: &FinanceCloseControlEvidence,
) -> Result<(), PersistenceError> {
    for (value, field, max) in [
        (
            evidence.control_code.as_str(),
            "Finance close control code",
            MAX_CONTROL_CODE_BYTES,
        ),
        (
            evidence.source_type.as_str(),
            "Finance close control source type",
            128,
        ),
        (
            evidence.evidence_ref.as_str(),
            "Finance close control evidence reference",
            255,
        ),
    ] {
        if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(invalid(format!("{field} is invalid")));
        }
    }
    if evidence.holding_id.is_nil()
        || evidence.company_id.is_nil()
        || evidence.period_id.is_nil()
        || evidence.source_aggregate_id.is_nil()
    {
        return Err(invalid(
            "Finance close control evidence identifiers cannot be nil",
        ));
    }
    Ok(())
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
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

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn domain_error(error: impl std::fmt::Display) -> PersistenceError {
    invalid(error.to_string())
}

async fn validate_close_review_persistence_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    review_id: Uuid,
    review_version: u64,
    expected_payload: &Value,
) -> Result<(), PersistenceError> {
    let snapshot: Option<CloseReviewSnapshotEvidenceRow> = sqlx::query_as(
        "SELECT version, state, holding_ref
         FROM erpx_tenant.aggregate_snapshots
         WHERE aggregate_type=$1 AND aggregate_id=$2
         FOR SHARE",
    )
    .bind(CLOSE_REVIEW_AGGREGATE)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((snapshot_version, snapshot_state, snapshot_holding_ref)) = snapshot else {
        return Err(invalid(format!(
            "Finance close review version {review_version} requires an authoritative aggregate snapshot"
        )));
    };
    let snapshot_version = as_u64(snapshot_version, "Finance close review snapshot version")?;
    if snapshot_holding_ref != holding_ref
        || snapshot_version != review_version
        || snapshot_state != *expected_payload
    {
        return Err(invalid(format!(
            "Finance close review version {review_version} aggregate snapshot does not match its authoritative row"
        )));
    }

    let outbox_rows: Vec<CloseReviewOutboxRow> = sqlx::query_as(
        "SELECT event_type, payload
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1
           AND aggregate_type=$2
           AND aggregate_id=$3
           AND aggregate_version=$4
         ORDER BY event_id
         LIMIT 2
         FOR SHARE",
    )
    .bind(holding_ref)
    .bind(CLOSE_REVIEW_AGGREGATE)
    .bind(review_id)
    .bind(as_i64(review_version, "Finance close review version")?)
    .fetch_all(&mut **tx)
    .await?;
    validate_close_review_outbox_rows(review_version, expected_payload, &outbox_rows)
}

async fn ensure_close_review_next_version_is_unclaimed(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    review_id: Uuid,
    current_version: Option<u64>,
    next_version: u64,
) -> Result<(), PersistenceError> {
    let existing_snapshot: Option<i64> = sqlx::query_scalar(
        "SELECT version
         FROM erpx_tenant.aggregate_snapshots
         WHERE aggregate_type=$1 AND aggregate_id=$2
         FOR UPDATE",
    )
    .bind(CLOSE_REVIEW_AGGREGATE)
    .bind(review_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(existing_snapshot) = existing_snapshot {
        let existing_snapshot = as_u64(
            existing_snapshot,
            "existing Finance close review snapshot version",
        )?;
        if Some(existing_snapshot) != current_version {
            return Err(invalid(format!(
                "Finance close review next version {next_version} has conflicting aggregate snapshot evidence"
            )));
        }
    }

    let outbox_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1
           AND aggregate_type=$2
           AND aggregate_id=$3
           AND aggregate_version=$4",
    )
    .bind(holding_ref)
    .bind(CLOSE_REVIEW_AGGREGATE)
    .bind(review_id)
    .bind(as_i64(next_version, "Finance close review version")?)
    .fetch_one(&mut **tx)
    .await?;
    if outbox_event_count != 0 {
        return Err(invalid(format!(
            "Finance close review next version {next_version} already has authoritative outbox evidence"
        )));
    }
    Ok(())
}

fn validate_close_review_outbox_rows(
    review_version: u64,
    expected_payload: &Value,
    outbox_rows: &[CloseReviewOutboxRow],
) -> Result<(), PersistenceError> {
    if outbox_rows.len() != 1 {
        return Err(invalid(format!(
            "Finance close review version {review_version} requires exactly one authoritative outbox event, found {}",
            outbox_rows.len()
        )));
    }
    let (event_type, payload) = &outbox_rows[0];
    if event_type != CLOSE_REVIEW_EVENT {
        return Err(invalid(format!(
            "Finance close review version {review_version} outbox payload does not match its authoritative snapshot"
        )));
    }
    validate_close_review_outbox_evidence(
        review_version,
        expected_payload,
        std::slice::from_ref(payload),
    )
}

fn validate_close_review_outbox_evidence(
    review_version: u64,
    expected_payload: &Value,
    outbox_event_payloads: &[Value],
) -> Result<(), PersistenceError> {
    if outbox_event_payloads.len() != 1 {
        return Err(invalid(format!(
            "Finance close review version {review_version} requires exactly one authoritative outbox event, found {}",
            outbox_event_payloads.len()
        )));
    }
    let expected_event_payload = serde_json::json!({
        "aggregate_type": CLOSE_REVIEW_AGGREGATE,
        "aggregate": expected_payload,
        "version": review_version,
    });
    if outbox_event_payloads[0] != expected_event_payload {
        return Err(invalid(format!(
            "Finance close review version {review_version} outbox payload does not match its authoritative snapshot"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn constructor_rejects_unbounded_holding_references() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxFinanceCloseStore::new(pool.clone(), " ").is_err());
        assert!(SqlxFinanceCloseStore::new(pool.clone(), "hold\nref").is_err());
        assert!(SqlxFinanceCloseStore::new(pool, "holding-close-test").is_ok());
    }

    #[test]
    fn idempotency_key_bounds_are_fail_closed() {
        assert!(validate_idempotency_key("").is_err());
        assert!(validate_idempotency_key("close\nreview").is_err());
        assert!(validate_idempotency_key(&"x".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1)).is_err());
        assert!(validate_idempotency_key("close-review-1").is_ok());
    }

    #[test]
    fn idempotent_replay_requires_one_matching_outbox_snapshot() {
        let payload = serde_json::json!({"status": "review"});
        let envelope = serde_json::json!({
            "aggregate_type": CLOSE_REVIEW_AGGREGATE,
            "aggregate": payload,
            "version": 1,
        });
        assert!(validate_close_review_outbox_evidence(1, &payload, &[]).is_err());
        assert!(
            validate_close_review_outbox_evidence(
                1,
                &payload,
                &[envelope.clone(), serde_json::json!({"status": "closed"})]
            )
            .is_err()
        );
        assert!(
            validate_close_review_outbox_evidence(
                1,
                &payload,
                &[serde_json::json!({
                    "aggregate_type": CLOSE_REVIEW_AGGREGATE,
                    "aggregate": {"status": "closed"},
                    "version": 1,
                })]
            )
            .is_err()
        );
        assert!(
            validate_close_review_outbox_evidence(1, &payload, std::slice::from_ref(&payload))
                .is_err()
        );
        validate_close_review_outbox_evidence(
            1,
            &serde_json::json!({"status": "review"}),
            &[envelope],
        )
        .expect("the sole close review outbox event must match the snapshot");
    }

    #[test]
    fn close_review_history_rejects_wrong_event_type_and_duplicate_events() {
        let payload = serde_json::json!({"status": "review"});
        let envelope = serde_json::json!({
            "aggregate_type": CLOSE_REVIEW_AGGREGATE,
            "aggregate": payload,
            "version": 1,
        });
        assert!(
            validate_close_review_outbox_rows(
                1,
                &payload,
                &[("finance.close_review_saved.v2".into(), envelope.clone())]
            )
            .is_err()
        );
        assert!(
            validate_close_review_outbox_rows(
                1,
                &payload,
                &[
                    (CLOSE_REVIEW_EVENT.into(), envelope.clone()),
                    (CLOSE_REVIEW_EVENT.into(), envelope),
                ]
            )
            .is_err()
        );
    }
}
