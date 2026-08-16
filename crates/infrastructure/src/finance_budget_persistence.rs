//! SQLx persistence for company-scoped Finance budgets and commitment
//! controls. General-ledger actuals remain authoritative in the Finance
//! journal context; this adapter only owns budget facts and immutable source
//! commitment references.

use erpx_domain::finance_budget::{BudgetControlDecision, BudgetStatus, FinanceBudget};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::{begin_scoped, write_finance_source_event};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
struct CommitmentRecord {
    commitment_id: Uuid,
    budget_id: Uuid,
    company_id: Uuid,
    amount_minor: i64,
    account_id: Uuid,
    period: i16,
    idempotency_key: String,
    actual_minor: i64,
    source_ref: String,
    decision_budget_minor: i64,
    decision_new_commitment_minor: i64,
    decision_remaining_minor: i64,
    payload: Value,
}

type CommitmentOutboxRecord = (String, Value);

type BudgetRow = (
    Uuid,
    String,
    Uuid,
    Uuid,
    i16,
    String,
    String,
    i64,
    String,
    Option<String>,
    Option<i64>,
    Value,
);

#[derive(Debug, Clone)]
pub struct SqlxFinanceBudgetStore {
    write_pool: PgPool,
    holding_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetCommitmentRequest {
    pub commitment_id: Uuid,
    pub budget_id: Uuid,
    pub account_id: Uuid,
    pub period: u8,
    pub amount_minor: i64,
    pub source_ref: String,
    pub actual_minor: i64,
    pub idempotency_key: String,
}

impl SqlxFinanceBudgetStore {
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

    pub async fn save_budget(
        &self,
        budget: &FinanceBudget,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        budget
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if expected_version.is_none() && budget.status != BudgetStatus::Draft {
            return Err(PersistenceError::InvalidData(
                "a new Finance budget must start in draft".into(),
            ));
        }
        let payload = serde_json::to_value(budget)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let current_row: Option<BudgetRow> = sqlx::query_as(
            "SELECT budget_id, holding_ref, holding_id, company_id, fiscal_year,
                    currency, status, version, idempotency_key, approved_by,
                    approval_limit_minor, payload
             FROM erpx_tenant.finance_budgets
             WHERE budget_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(budget.id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        let current_budget = current_row
            .as_ref()
            .map(|row| validate_persisted_budget_row(row, &self.holding_ref, budget.id))
            .transpose()?;
        let current = current_row.as_ref().map(|row| row.7);
        if let (Some(current_row), Some(current_budget)) = (&current_row, &current_budget)
            && current_budget.context.idempotency_key == budget.context.idempotency_key
        {
            if current_row.11 != payload {
                return Err(PersistenceError::IdempotencyConflict {
                    key: budget.context.idempotency_key.clone(),
                });
            }
            let current_version = u64::try_from(current_row.7).map_err(|_| {
                PersistenceError::InvalidData("persisted Finance budget version is invalid".into())
            })?;
            validate_budget_replay_evidence(
                &mut tx,
                &self.holding_ref,
                budget.id,
                current_version,
                &payload,
            )
            .await?;
            tx.commit().await?;
            return Ok(current_version);
        }
        let version = i64_version(budget.version)?;
        let affected = match (current, expected_version) {
            (None, None) if budget.version == 1 => sqlx::query(
                "INSERT INTO erpx_tenant.finance_budgets
                     (budget_id, holding_ref, holding_id, company_id, fiscal_year,
                     currency, status, version, idempotency_key, approved_by,
                     approval_limit_minor, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            )
            .bind(budget.id)
            .bind(&self.holding_ref)
            .bind(budget.holding_id)
            .bind(budget.company_id)
            .bind(i16::try_from(budget.fiscal_year).map_err(|_| {
                PersistenceError::InvalidData("budget fiscal year is invalid".into())
            })?)
            .bind(&budget.currency)
            .bind(budget_status(budget.status))
            .bind(version)
            .bind(&budget.context.idempotency_key)
            .bind(&budget.approved_by)
            .bind(budget.approval_limit_minor)
            .bind(&payload)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
            (Some(actual), Some(expected)) if actual == i64_version(expected)? => {
                let current_budget = current_budget.as_ref().ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "budget row disappeared during version check".into(),
                    )
                })?;
                validate_budget_transition(current_budget, budget)?;
                let expected_next = expected.checked_add(1).ok_or_else(|| {
                    PersistenceError::InvalidData("budget version overflow".into())
                })?;
                if budget.version != expected_next {
                    return Err(PersistenceError::OptimisticConcurrency {
                        aggregate_type: "finance_budget".into(),
                        aggregate_id: budget.id,
                        expected: Some(expected),
                        actual: Some(u64::try_from(actual).unwrap_or(u64::MAX)),
                    });
                }
                sqlx::query(
                    "UPDATE erpx_tenant.finance_budgets
                     SET holding_id=$2, company_id=$3, fiscal_year=$4, currency=$5,
                         status=$6, version=$7, idempotency_key=$8, approved_by=$9,
                         approval_limit_minor=$10, payload=$11, updated_at=now()
                     WHERE budget_id=$1 AND holding_ref=$12 AND version=$13",
                )
                .bind(budget.id)
                .bind(budget.holding_id)
                .bind(budget.company_id)
                .bind(i16::try_from(budget.fiscal_year).map_err(|_| {
                    PersistenceError::InvalidData("budget fiscal year is invalid".into())
                })?)
                .bind(&budget.currency)
                .bind(budget_status(budget.status))
                .bind(version)
                .bind(&budget.context.idempotency_key)
                .bind(&budget.approved_by)
                .bind(budget.approval_limit_minor)
                .bind(&payload)
                .bind(&self.holding_ref)
                .bind(actual)
                .execute(&mut *tx)
                .await?
                .rows_affected()
            }
            (actual, expected) => {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: "finance_budget".into(),
                    aggregate_id: budget.id,
                    expected,
                    actual: actual.map(|value| u64::try_from(value).unwrap_or(u64::MAX)),
                });
            }
        };
        if affected != 1 {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "finance_budget".into(),
                aggregate_id: budget.id,
                expected: expected_version,
                actual: Some(budget.version),
            });
        }
        write_finance_source_event(
            &mut tx,
            &self.holding_ref,
            "finance_budget",
            budget.id,
            budget.version,
            &payload,
            "finance.budget_saved.v1",
        )
        .await?;
        tx.commit().await?;
        Ok(budget.version)
    }

    pub async fn get_budget(
        &self,
        budget_id: Uuid,
    ) -> Result<Option<FinanceBudget>, PersistenceError> {
        if budget_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Finance budget ID is required".into(),
            ));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let row: Option<BudgetRow> = sqlx::query_as(
            "SELECT budget_id, holding_ref, holding_id, company_id, fiscal_year,
                    currency, status, version, idempotency_key, approved_by,
                    approval_limit_minor, payload
             FROM erpx_tenant.finance_budgets
             WHERE budget_id=$1 AND holding_ref=$2",
        )
        .bind(budget_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(|row| validate_persisted_budget_row(&row, &self.holding_ref, budget_id))
            .transpose()
    }

    pub async fn check_control(
        &self,
        budget_id: Uuid,
        account_id: Uuid,
        period: u8,
        actual_minor: i64,
        proposed_commitment_minor: i64,
    ) -> Result<BudgetControlDecision, PersistenceError> {
        let budget = self
            .get_budget(budget_id)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "finance budget",
                id: budget_id,
            })?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let authoritative_actual =
            authoritative_actual_minor(&mut tx, &self.holding_ref, &budget, account_id, period)
                .await?;
        if actual_minor != authoritative_actual {
            return Err(PersistenceError::InvalidData(
                "budget actuals must match posted Finance journal facts".into(),
            ));
        }
        let committed: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor), 0)::BIGINT
             FROM erpx_tenant.finance_budget_commitments
             WHERE budget_id=$1 AND holding_ref=$2 AND account_id=$3 AND period=$4",
        )
        .bind(budget_id)
        .bind(&self.holding_ref)
        .bind(account_id)
        .bind(i16::from(period))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        let requested_total = committed
            .checked_add(proposed_commitment_minor)
            .ok_or_else(|| PersistenceError::InvalidData("budget commitment overflow".into()))?;
        budget
            .control_check(account_id, period, actual_minor, requested_total)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))
    }

    pub async fn record_commitment(
        &self,
        request: &BudgetCommitmentRequest,
    ) -> Result<BudgetControlDecision, PersistenceError> {
        if request.commitment_id == Uuid::nil() {
            return Err(PersistenceError::InvalidData(
                "budget commitment ID is required".into(),
            ));
        }
        validate_commitment_reference(&request.source_ref, "source_ref")?;
        validate_commitment_reference(&request.idempotency_key, "idempotency_key")?;
        if request.amount_minor <= 0 {
            return Err(PersistenceError::InvalidData(
                "budget commitment amount must be positive".into(),
            ));
        }
        if !(1..=12).contains(&request.period) || request.account_id == Uuid::nil() {
            return Err(PersistenceError::InvalidData(
                "budget commitment account and period are invalid".into(),
            ));
        }
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        let control_lock = format!(
            "erpx.finance-budget:{}:{}:{}:{}",
            self.holding_ref, request.budget_id, request.account_id, request.period
        );
        let source_lock = format!(
            "erpx.finance-budget-source:{}:{}",
            self.holding_ref, request.source_ref
        );
        let idempotency_lock = format!(
            "erpx.finance-budget-idempotency:{}:{}",
            self.holding_ref, request.idempotency_key
        );
        let mut locks = [control_lock, source_lock, idempotency_lock];
        locks.sort_unstable();
        for lock in locks {
            sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
                .bind(lock)
                .execute(&mut *tx)
                .await?;
        }
        let row: BudgetRow = sqlx::query_as(
            "SELECT budget_id, holding_ref, holding_id, company_id, fiscal_year,
                    currency, status, version, idempotency_key, approved_by,
                    approval_limit_minor, payload
             FROM erpx_tenant.finance_budgets
             WHERE budget_id=$1 AND holding_ref=$2 FOR UPDATE",
        )
        .bind(request.budget_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "finance budget",
            id: request.budget_id,
        })?;
        let budget = validate_persisted_budget_row(&row, &self.holding_ref, request.budget_id)?;
        let existing: Vec<CommitmentRecord> = sqlx::query_as(
            "SELECT commitment_id, budget_id, company_id, amount_minor, account_id, period,
                    idempotency_key, actual_minor, source_ref,
                    decision_budget_minor, decision_new_commitment_minor,
                    decision_remaining_minor, payload
             FROM erpx_tenant.finance_budget_commitments
             WHERE holding_ref=$1 AND (source_ref=$2 OR idempotency_key=$3)",
        )
        .bind(&self.holding_ref)
        .bind(&request.source_ref)
        .bind(&request.idempotency_key)
        .fetch_all(&mut *tx)
        .await?;
        if existing.len() > 1 {
            return Err(PersistenceError::IdempotencyConflict {
                key: request.idempotency_key.clone(),
            });
        }
        if let Some(existing) = existing.into_iter().next() {
            let decision = BudgetControlDecision {
                budget_id: request.budget_id,
                account_id: request.account_id,
                period: request.period,
                budget_minor: existing.decision_budget_minor,
                actual_minor: request.actual_minor,
                new_commitment_minor: existing.decision_new_commitment_minor,
                remaining_minor: existing.decision_remaining_minor,
                allowed: true,
            };
            let commitment_payload = build_commitment_payload(request, &decision);
            if existing.commitment_id != request.commitment_id
                || existing.budget_id != request.budget_id
                || existing.company_id != budget.company_id
                || existing.amount_minor != request.amount_minor
                || existing.account_id != request.account_id
                || existing.period != i16::from(request.period)
                || existing.idempotency_key != request.idempotency_key
                || existing.actual_minor != request.actual_minor
                || existing.source_ref != request.source_ref
                || existing.payload != commitment_payload
            {
                return Err(PersistenceError::IdempotencyConflict {
                    key: request.idempotency_key.clone(),
                });
            }
            validate_commitment_outbox_evidence(
                &mut tx,
                &self.holding_ref,
                request.commitment_id,
                &commitment_payload,
            )
            .await?;
            tx.commit().await?;
            return Ok(decision);
        }
        let authoritative_actual = authoritative_actual_minor(
            &mut tx,
            &self.holding_ref,
            &budget,
            request.account_id,
            request.period,
        )
        .await?;
        if request.actual_minor != authoritative_actual {
            return Err(PersistenceError::InvalidData(
                "budget actuals must match posted Finance journal facts".into(),
            ));
        }
        let committed: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor), 0)::BIGINT
             FROM erpx_tenant.finance_budget_commitments
             WHERE budget_id=$1 AND holding_ref=$2 AND account_id=$3 AND period=$4",
        )
        .bind(request.budget_id)
        .bind(&self.holding_ref)
        .bind(request.account_id)
        .bind(i16::from(request.period))
        .fetch_one(&mut *tx)
        .await?;
        let requested_total = committed
            .checked_add(request.amount_minor)
            .ok_or_else(|| PersistenceError::InvalidData("budget commitment overflow".into()))?;
        let decision = budget
            .control_check(
                request.account_id,
                request.period,
                authoritative_actual,
                requested_total,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if !decision.allowed {
            return Err(PersistenceError::InvalidData(
                "budget control rejected commitment".into(),
            ));
        }
        let commitment_payload = build_commitment_payload(request, &decision);
        let inserted = sqlx::query(
            "INSERT INTO erpx_tenant.finance_budget_commitments
                (commitment_id, budget_id, holding_ref, company_id, account_id,
                 period, amount_minor, actual_minor, source_ref, idempotency_key,
                 decision_budget_minor, decision_new_commitment_minor,
                 decision_remaining_minor, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
             ON CONFLICT (holding_ref, source_ref) DO NOTHING",
        )
        .bind(request.commitment_id)
        .bind(request.budget_id)
        .bind(&self.holding_ref)
        .bind(budget.company_id)
        .bind(request.account_id)
        .bind(i16::from(request.period))
        .bind(request.amount_minor)
        .bind(request.actual_minor)
        .bind(&request.source_ref)
        .bind(&request.idempotency_key)
        .bind(decision.budget_minor)
        .bind(decision.new_commitment_minor)
        .bind(decision.remaining_minor)
        .bind(&commitment_payload)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            let existing: Option<CommitmentRecord> = sqlx::query_as(
                "SELECT commitment_id, budget_id, company_id, amount_minor, account_id, period,
                        idempotency_key, actual_minor, source_ref,
                        decision_budget_minor, decision_new_commitment_minor,
                        decision_remaining_minor, payload
                 FROM erpx_tenant.finance_budget_commitments
                 WHERE holding_ref=$1 AND source_ref=$2",
            )
            .bind(&self.holding_ref)
            .bind(&request.source_ref)
            .fetch_optional(&mut *tx)
            .await?;
            let expected_record = CommitmentRecord {
                commitment_id: request.commitment_id,
                budget_id: request.budget_id,
                company_id: budget.company_id,
                amount_minor: request.amount_minor,
                account_id: request.account_id,
                period: i16::from(request.period),
                idempotency_key: request.idempotency_key.clone(),
                actual_minor: request.actual_minor,
                source_ref: request.source_ref.clone(),
                decision_budget_minor: decision.budget_minor,
                decision_new_commitment_minor: decision.new_commitment_minor,
                decision_remaining_minor: decision.remaining_minor,
                payload: commitment_payload.clone(),
            };
            if existing.as_ref() != Some(&expected_record) {
                return Err(PersistenceError::IdempotencyConflict {
                    key: request.idempotency_key.clone(),
                });
            }
        } else {
            write_finance_source_event(
                &mut tx,
                &self.holding_ref,
                "finance_budget_commitment",
                request.commitment_id,
                1,
                &commitment_payload,
                "finance.budget_commitment_recorded.v1",
            )
            .await?;
        }
        tx.commit().await?;
        Ok(decision)
    }
}

async fn authoritative_actual_minor(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    budget: &FinanceBudget,
    account_id: Uuid,
    period: u8,
) -> Result<i64, PersistenceError> {
    let (debit_text, credit_text, invalid_count, matching_line_count): (String, String, i64, i64) =
        sqlx::query_as(
            "SELECT
             COALESCE(SUM(CASE
                 WHEN line.value->>'account_id' = $7
                      AND line.value->>'debit_minor' ~ '^[0-9]{1,19}$'
                     THEN (line.value->>'debit_minor')::NUMERIC
                 ELSE 0::NUMERIC
             END), 0)::TEXT AS debit_minor,
             COALESCE(SUM(CASE
                 WHEN line.value->>'account_id' = $7
                      AND line.value->>'credit_minor' ~ '^[0-9]{1,19}$'
                     THEN (line.value->>'credit_minor')::NUMERIC
                 ELSE 0::NUMERIC
             END), 0)::TEXT AS credit_minor,
             COUNT(*) FILTER (
                 WHERE line.value->>'account_id' = $7
                   AND (
                       NOT (line.value ? 'debit_minor')
                       OR line.value->>'debit_minor' !~ '^[0-9]{1,19}$'
                       OR NOT (line.value ? 'credit_minor')
                       OR line.value->>'credit_minor' !~ '^[0-9]{1,19}$'
                   )
               ) AS invalid_count,
             COUNT(*) FILTER (
                 WHERE line.value->>'account_id' = $7
             ) AS matching_line_count
         FROM erpx_tenant.finance_posted_journals AS journal
         CROSS JOIN LATERAL jsonb_array_elements(
             CASE
                 WHEN jsonb_typeof(journal.payload #> '{entry,lines}') = 'array'
                     THEN journal.payload #> '{entry,lines}'
                 ELSE '[]'::jsonb
             END
         ) AS line(value)
         WHERE journal.holding_ref=$1
           AND journal.holding_id=$2
           AND journal.company_id=$3
           AND journal.currency=$4
           AND EXTRACT(YEAR FROM journal.posted_at)::INT=$5
           AND EXTRACT(MONTH FROM journal.posted_at)::INT=$6",
        )
        .bind(holding_ref)
        .bind(budget.holding_id)
        .bind(budget.company_id)
        .bind(&budget.currency)
        .bind(i32::from(budget.fiscal_year))
        .bind(i32::from(period))
        .bind(account_id.to_string())
        .fetch_one(&mut **tx)
        .await?;
    if matching_line_count == 0 {
        return Ok(0);
    }
    if invalid_count > 0 {
        return Err(PersistenceError::InvalidData(
            "posted Finance journal facts contain invalid account amounts".into(),
        ));
    }
    let debit = debit_text.parse::<i64>().map_err(|_| {
        PersistenceError::InvalidData("posted Finance actuals exceed integer range".into())
    })?;
    let credit = credit_text.parse::<i64>().map_err(|_| {
        PersistenceError::InvalidData("posted Finance actuals exceed integer range".into())
    })?;
    let (account_count, normal_balance): (i64, Option<String>) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT, MIN(normal_balance)
           FROM erpx_tenant.finance_chart_accounts
          WHERE holding_ref=$1 AND company_id=$2 AND account_id=$3",
    )
    .bind(holding_ref)
    .bind(budget.company_id)
    .bind(account_id)
    .fetch_one(&mut **tx)
    .await?;
    if account_count != 1 {
        return Err(PersistenceError::InvalidData(
            "budget actuals require exactly one authoritative Finance account".into(),
        ));
    }
    let normal_balance = normal_balance.ok_or_else(|| {
        PersistenceError::InvalidData(
            "authoritative Finance account normal balance is missing".into(),
        )
    })?;
    let total = match normal_balance.as_str() {
        "debit" => debit.checked_sub(credit),
        "credit" => credit.checked_sub(debit),
        _ => None,
    }
    .ok_or_else(|| {
        PersistenceError::InvalidData(
            "authoritative Finance account normal balance is invalid".into(),
        )
    })?;
    if total < 0 {
        return Err(PersistenceError::InvalidData(
            "posted Finance actuals are negative for the account normal balance".into(),
        ));
    }
    Ok(total)
}

fn budget_status(status: BudgetStatus) -> &'static str {
    match status {
        BudgetStatus::Draft => "draft",
        BudgetStatus::Submitted => "submitted",
        BudgetStatus::Approved => "approved",
        BudgetStatus::Locked => "locked",
        BudgetStatus::Closed => "closed",
    }
}

fn validate_persisted_budget_row(
    row: &BudgetRow,
    holding_ref: &str,
    budget_id: Uuid,
) -> Result<FinanceBudget, PersistenceError> {
    let (
        row_budget_id,
        row_holding_ref,
        row_holding_id,
        row_company_id,
        row_fiscal_year,
        row_currency,
        row_status,
        row_version,
        row_idempotency_key,
        row_approved_by,
        row_approval_limit_minor,
        payload,
    ) = row;
    if *row_budget_id != budget_id || row_holding_ref != holding_ref {
        return Err(PersistenceError::InvalidData(
            "persisted Finance budget identity does not match its requested scope".into(),
        ));
    }
    let budget: FinanceBudget = serde_json::from_value(payload.clone())?;
    budget
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let fiscal_year = i16::try_from(budget.fiscal_year)
        .map_err(|_| PersistenceError::InvalidData("budget fiscal year is invalid".into()))?;
    let version = i64_version(budget.version)?;
    if budget.id != *row_budget_id
        || budget.holding_id != *row_holding_id
        || budget.company_id != *row_company_id
        || fiscal_year != *row_fiscal_year
        || row_currency.trim_end() != budget.currency
        || row_status != budget_status(budget.status)
        || version != *row_version
        || row_idempotency_key != &budget.context.idempotency_key
        || row_approved_by != &budget.approved_by
        || row_approval_limit_minor != &budget.approval_limit_minor
    {
        return Err(PersistenceError::InvalidData(
            "persisted Finance budget columns do not match their authoritative payload".into(),
        ));
    }
    Ok(budget)
}

fn validate_budget_transition(
    current: &FinanceBudget,
    candidate: &FinanceBudget,
) -> Result<(), PersistenceError> {
    current
        .validate()
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    if current.id != candidate.id
        || current.holding_id != candidate.holding_id
        || current.company_id != candidate.company_id
        || current.currency != candidate.currency
        || current.fiscal_year != candidate.fiscal_year
        || current.created_by != candidate.created_by
        || current.lines != candidate.lines
    {
        return Err(PersistenceError::InvalidData(
            "Finance budget identity and lines are immutable across lifecycle transitions".into(),
        ));
    }
    let mut expected = current.clone();
    match candidate.status {
        BudgetStatus::Draft => {
            return Err(PersistenceError::InvalidData(
                "Finance budgets cannot transition back to draft".into(),
            ));
        }
        BudgetStatus::Submitted => {
            expected
                .submit(&candidate.context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        }
        BudgetStatus::Approved => {
            let approval_limit = candidate.approval_limit_minor.ok_or_else(|| {
                PersistenceError::InvalidData("approval limit is required".into())
            })?;
            expected
                .approve(&candidate.context, approval_limit)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        }
        BudgetStatus::Locked => {
            expected
                .lock(&candidate.context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        }
        BudgetStatus::Closed => {
            expected
                .close(&candidate.context)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        }
    }
    if expected.status != candidate.status
        || expected.version != candidate.version
        || expected.approved_by != candidate.approved_by
        || expected.approval_limit_minor != candidate.approval_limit_minor
    {
        return Err(PersistenceError::InvalidData(
            "Finance budget lifecycle evidence does not match the domain transition".into(),
        ));
    }
    Ok(())
}

fn i64_version(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData("budget version exceeds database range".into()))
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(
            "holding reference is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_commitment_reference(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::InvalidData(format!(
            "Finance budget commitment {field} is invalid"
        )));
    }
    Ok(())
}

async fn validate_budget_replay_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    budget_id: Uuid,
    version: u64,
    payload: &Value,
) -> Result<(), PersistenceError> {
    let events = sqlx::query(
        "SELECT aggregate_version, event_type, payload
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_type='finance_budget'
            AND aggregate_id=$2 AND aggregate_version=$3
          FOR SHARE",
    )
    .bind(holding_ref)
    .bind(budget_id)
    .bind(i64_version(version)?)
    .fetch_all(&mut **tx)
    .await?;
    if events.len() != 1 {
        return Err(PersistenceError::InvalidData(format!(
            "Finance budget replay requires exactly one outbox event, found {}",
            events.len()
        )));
    }
    let event = &events[0];
    let expected_payload = serde_json::json!({
        "aggregate_type": "finance_budget",
        "aggregate": payload,
        "version": version,
    });
    let event_version =
        u64::try_from(event.try_get::<i64, _>("aggregate_version")?).map_err(|_| {
            PersistenceError::InvalidData("Finance budget event version is invalid".into())
        })?;
    if event_version != version
        || event.try_get::<String, _>("event_type")? != "finance.budget_saved.v1"
        || event.try_get::<Value, _>("payload")? != expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "Finance budget replay has divergent outbox evidence".into(),
        ));
    }
    Ok(())
}

fn validate_commitment_outbox_cardinality(
    commitment_id: Uuid,
    outbox_event_count: i64,
) -> Result<(), PersistenceError> {
    if commitment_id.is_nil() {
        return Err(PersistenceError::InvalidData(
            "budget commitment ID is required".into(),
        ));
    }
    if outbox_event_count != 1 {
        return Err(PersistenceError::InvalidData(format!(
            "Finance budget commitment requires exactly one authoritative outbox event, found {outbox_event_count}"
        )));
    }
    Ok(())
}

async fn validate_commitment_outbox_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    commitment_id: Uuid,
    commitment_payload: &Value,
) -> Result<(), PersistenceError> {
    let rows: Vec<CommitmentOutboxRecord> = sqlx::query_as(
        "SELECT event_type, payload
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1
            AND aggregate_type='finance_budget_commitment'
            AND aggregate_id=$2
            AND aggregate_version=1
          FOR SHARE",
    )
    .bind(holding_ref)
    .bind(commitment_id)
    .fetch_all(&mut **tx)
    .await?;
    validate_commitment_outbox_rows(commitment_id, commitment_payload, &rows)
}

fn validate_commitment_outbox_rows(
    commitment_id: Uuid,
    commitment_payload: &Value,
    rows: &[CommitmentOutboxRecord],
) -> Result<(), PersistenceError> {
    validate_commitment_outbox_cardinality(commitment_id, rows.len() as i64)?;
    let expected_payload = serde_json::json!({
        "aggregate_type": "finance_budget_commitment",
        "aggregate": commitment_payload,
        "version": 1,
    });
    let (event_type, payload) = &rows[0];
    if event_type != "finance.budget_commitment_recorded.v1" || payload != &expected_payload {
        return Err(PersistenceError::InvalidData(
            "Finance budget commitment outbox evidence does not match the persisted commitment"
                .into(),
        ));
    }
    Ok(())
}

fn build_commitment_payload(
    request: &BudgetCommitmentRequest,
    decision: &BudgetControlDecision,
) -> Value {
    serde_json::json!({
        "commitment_id": request.commitment_id,
        "budget_id": request.budget_id,
        "account_id": request.account_id,
        "period": request.period,
        "amount_minor": request.amount_minor,
        "actual_minor": request.actual_minor,
        "decision_budget_minor": decision.budget_minor,
        "decision_new_commitment_minor": decision.new_commitment_minor,
        "decision_remaining_minor": decision.remaining_minor,
        "source_ref": request.source_ref,
        "idempotency_key": request.idempotency_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn constructor_rejects_empty_holding() {
        assert!(
            SqlxFinanceBudgetStore::new(
                PgPool::connect_lazy("postgres://unused").expect("lazy"),
                "",
            )
            .is_err()
        );
    }

    #[test]
    fn commitment_replay_rejects_missing_or_duplicate_outbox_evidence() {
        let commitment_id = Uuid::new_v4();
        assert!(validate_commitment_outbox_cardinality(commitment_id, 0).is_err());
        assert!(validate_commitment_outbox_cardinality(commitment_id, 2).is_err());
        validate_commitment_outbox_cardinality(commitment_id, 1)
            .expect("one commitment outbox event is the only valid cardinality");
    }

    #[test]
    fn commitment_replay_rejects_mismatched_outbox_type_or_payload() {
        let commitment_id = Uuid::new_v4();
        let payload = serde_json::json!({"commitment_id": commitment_id});
        let expected = serde_json::json!({
            "aggregate_type": "finance_budget_commitment",
            "aggregate": payload,
            "version": 1,
        });
        let valid = vec![(
            "finance.budget_commitment_recorded.v1".into(),
            expected.clone(),
        )];
        validate_commitment_outbox_rows(commitment_id, &payload, &valid)
            .expect("exact commitment outbox evidence");

        let wrong_type = vec![("finance.budget_saved.v1".into(), expected.clone())];
        assert!(validate_commitment_outbox_rows(commitment_id, &payload, &wrong_type).is_err());

        let wrong_payload = vec![(
            "finance.budget_commitment_recorded.v1".into(),
            serde_json::json!({
                "aggregate_type": "finance_budget_commitment",
                "aggregate": {"commitment_id": Uuid::new_v4()},
                "version": 1,
            }),
        )];
        assert!(validate_commitment_outbox_rows(commitment_id, &payload, &wrong_payload).is_err());
    }

    #[test]
    fn commitment_references_are_bounded_and_control_free() {
        assert!(validate_commitment_reference("purchase-order:42", "source_ref").is_ok());
        assert!(validate_commitment_reference("", "source_ref").is_err());
        assert!(validate_commitment_reference("\nsource", "source_ref").is_err());
        assert!(validate_commitment_reference(&"x".repeat(256), "idempotency_key").is_err());
    }
}
