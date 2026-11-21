use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{CompanyId, FailureKind, HoldingId};
use sqlx::{Connection, PgConnection};

use super::postgres_journal_facts::current_transaction_witness;
use crate::application::journal::{JournalPreparation, JournalStatus};
use crate::domain::journal::JournalError;

/// Writer within a caller-owned Holding transaction. Construction does not authorize a command.
/// Composition selects the Holding connection from trusted runtime bindings and retains the
/// Identity request lease until the enclosing owner transaction commits or rolls back.
pub struct PgJournalStore<'c> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
}

impl<'c> PgJournalStore<'c> {
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
        }
    }

    /// Verify every persisted draft field and line on the original transaction.
    /// The caller invokes this after Workflow audit/outbox effects and before
    /// the final current-authority check; this never commits.
    pub async fn retain_manual_draft(
        &mut self,
        plan: &JournalPreparation,
    ) -> Result<(), PgTransactionError<JournalError>> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let witness = plan
            .transaction_witness
            .as_ref()
            .ok_or_else(|| PersistenceError::new("JOURNAL_TRANSACTION_WITNESS_REQUIRED"))?;
        if &current_transaction_witness(self.connection).await? != witness
            || plan.actor.holding_id != self.holding_id
            || plan.actor.company_id != self.company_id
            || !matches!(plan.result.status, JournalStatus::Draft)
            || plan.draft.source_type != "manual"
            || plan.draft.source_id.is_some()
            || plan.draft.is_opening
        {
            return Err(conflict(
                "JOURNAL_SOURCE_CHANGED",
                "The prepared manual journal changed.",
            ));
        }
        let draft = &plan.draft;
        let expected = serde_json::json!({"id":plan.journal_id,"company_id":self.company_id.as_str(),"chart_id":draft.chart_id,"period_id":draft.period_id,"journal_number":plan.number,"entry_date":draft.entry_date,"currency":draft.currency,"fx_rate_micros":plan.fx,"source_type":"manual","source_id":null,"memo":draft.memo,"status":"draft","is_opening":0,"version":1,"created_by":plan.actor.user_id.as_str(),"posted_by":null,"posted_at":null,"reversed_by":null,"reversed_at":null,"created_at":plan.now.get(),"updated_at":plan.now.get()});
        let actual: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT to_jsonb(j) FROM finance_journals j WHERE id=$1 AND company_id=$2 FOR SHARE",
        )
        .bind(&plan.journal_id)
        .bind(self.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        if actual.as_ref() != Some(&expected) {
            return Err(conflict(
                "JOURNAL_SOURCE_CHANGED",
                "The prepared manual journal changed.",
            ));
        }
        let rows: Vec<serde_json::Value> = sqlx::query_scalar("SELECT to_jsonb(l) FROM finance_journal_lines l WHERE journal_id=$1 ORDER BY ordinal,id FOR SHARE")
            .bind(&plan.journal_id).fetch_all(&mut *self.connection).await?;
        if rows.len() != plan.amounts.lines.len() {
            return Err(conflict(
                "JOURNAL_SOURCE_CHANGED",
                "The prepared manual journal changed.",
            ));
        }
        for (index, (row, line)) in rows.iter().zip(&plan.amounts.lines).enumerate() {
            let dimensions = serde_json::to_string(&line.dimensions.clone().unwrap_or_default())
                .map_err(|_| PersistenceError::new("JOURNAL_DIMENSION_ENCODING_FAILED"))?;
            let expected = serde_json::json!({"id":format!("{}-{index}",plan.journal_id),"journal_id":plan.journal_id,"account_id":line.account_id,"description":line.description.as_deref().unwrap_or(""),"debit_minor":line.debit,"credit_minor":line.credit,"base_debit_minor":line.base_debit,"base_credit_minor":line.base_credit,"dimension_json":dimensions,"ordinal":index});
            if row != &expected {
                return Err(conflict(
                    "JOURNAL_SOURCE_CHANGED",
                    "The prepared manual journal changed.",
                ));
            }
        }
        let unexpected: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM money_transaction_snapshots WHERE company_id=$1 AND source_type='finance_journal' AND source_id=$2) OR EXISTS(SELECT 1 FROM finance_book_postings WHERE company_id=$1 AND (primary_journal_id=$2 OR secondary_journal_id=$2)) OR EXISTS(SELECT 1 FROM finance_financial_events WHERE company_id=$1 AND journal_id=$2)")
            .bind(self.company_id.as_str()).bind(&plan.journal_id).fetch_one(&mut *self.connection).await?;
        if unexpected {
            return Err(conflict(
                "JOURNAL_SOURCE_CHANGED",
                "The prepared manual journal changed.",
            ));
        }
        Ok(())
    }

    /// Persists no independent receipt or commit: the caller must commit source work,
    /// command receipt, audit and outbox atomically on this same connection.
    pub async fn persist(
        &mut self,
        plan: &JournalPreparation,
    ) -> Result<(), PgTransactionError<JournalError>> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let isolation = sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
        }
        if plan.actor.holding_id != self.holding_id || plan.actor.company_id != self.company_id {
            return Err(PgTransactionError::Business(JournalError {
                kind: FailureKind::Forbidden,
                code: "JOURNAL_SCOPE_MISMATCH",
                message: "The prepared journal belongs to another Holding or Company.".to_owned(),
            }));
        }
        let witness = plan
            .transaction_witness
            .as_ref()
            .ok_or_else(|| PersistenceError::new("JOURNAL_TRANSACTION_WITNESS_REQUIRED"))?;
        if &current_transaction_witness(self.connection).await? != witness {
            return Err(conflict(
                "JOURNAL_TRANSACTION_CHANGED",
                "Prepare and persist the journal in the same owner transaction.",
            ));
        }
        let input = &plan.draft;
        let company = self.company_id.as_str();
        let period = sqlx::query_scalar::<_, String>("SELECT id FROM finance_periods WHERE id=$1 AND company_id=$2 AND state='open' AND start_date<=$3 AND end_date>=$3 FOR SHARE")
            .bind(&input.period_id).bind(company).bind(&input.entry_date).fetch_optional(&mut *self.connection).await?;
        let covered = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=$1 AND state IN ('closed','closing') AND $2 BETWEEN start_date AND end_date)")
            .bind(company).bind(&input.entry_date).fetch_one(&mut *self.connection).await?;
        if period.is_none() || covered {
            return Err(conflict(
                "PERIOD_CLOSED",
                "The accounting period does not accept postings.",
            ));
        }
        let chart = sqlx::query_scalar::<_, String>("SELECT base_currency FROM finance_charts WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE")
            .bind(&input.chart_id).bind(company).fetch_optional(&mut *self.connection).await?;
        if chart.as_deref() != Some(&plan.chart.base_currency) {
            return Err(conflict(
                "CHART_CHANGED",
                "The active chart changed. Prepare the journal again.",
            ));
        }
        let accounts = sqlx::query_scalar::<_, String>("SELECT id FROM finance_accounts WHERE company_id=$1 AND chart_id=$2 AND status='active' AND id=ANY($3) FOR SHARE")
            .bind(company).bind(&input.chart_id).bind(&plan.amounts.account_ids).fetch_all(&mut *self.connection).await?;
        if accounts.len() != plan.amounts.account_ids.len() {
            return Err(conflict(
                "ACCOUNT_INVALID",
                "Every posting account must be active and belong to this chart.",
            ));
        }
        if input.is_opening {
            let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=$1 AND chart_id=$2 AND is_opening=1 AND status IN ('draft','in_review','posted'))")
                .bind(company).bind(&input.chart_id).fetch_one(&mut *self.connection).await?;
            if exists {
                return Err(conflict(
                    "OPENING_BALANCE_EXISTS",
                    "Reverse or finish the existing opening balance before importing another one.",
                ));
            }
        }
        let status = match plan.result.status {
            JournalStatus::Draft => "draft",
            JournalStatus::Posted => "posted",
        };
        let posted_by = matches!(plan.result.status, JournalStatus::Posted)
            .then_some(plan.actor.user_id.as_str());
        let posted_at = posted_by.map(|_| plan.now.get());
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)")
            .bind(&plan.journal_id).bind(company).bind(&input.chart_id).bind(&input.period_id).bind(&plan.number).bind(&input.entry_date)
            .bind(&input.currency).bind(plan.fx).bind(&input.source_type).bind(&input.source_id).bind(&input.memo).bind(i64::from(input.is_opening))
            .bind(status).bind(plan.actor.user_id.as_str()).bind(posted_by).bind(posted_at).bind(plan.now.get()).bind(plan.now.get()).execute(&mut *self.connection).await?;
        for (index, line) in plan.amounts.lines.iter().enumerate() {
            let dimensions = serde_json::to_string(&line.dimensions.clone().unwrap_or_default())
                .map_err(|_| PersistenceError::new("JOURNAL_DIMENSION_ENCODING_FAILED"))?;
            sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                .bind(format!("{}-{index}",plan.journal_id)).bind(&plan.journal_id).bind(&line.account_id).bind(line.description.as_deref().unwrap_or(""))
                .bind(line.debit).bind(line.credit).bind(line.base_debit).bind(line.base_credit).bind(dimensions)
                .bind(i64::try_from(index).map_err(|_| PersistenceError::new("JOURNAL_LINE_LIMIT"))?).execute(&mut *self.connection).await?;
        }
        Ok(())
    }
}

fn conflict(code: &'static str, message: &'static str) -> PgTransactionError<JournalError> {
    PgTransactionError::Business(JournalError {
        kind: FailureKind::Conflict,
        code,
        message: message.to_owned(),
    })
}
