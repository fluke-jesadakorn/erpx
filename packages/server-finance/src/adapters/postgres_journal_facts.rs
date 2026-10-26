use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{Clock, CompanyId, FailureKind, TimestampMillis};
use serde::Deserialize;
use sqlx::{Connection, PgConnection, Row};

use crate::application::journal::{
    FinanceActor, FunctionalFxEvidence, JournalAuthority, JournalChart, JournalDraft,
    JournalPreparationPorts, JournalTransactionWitness,
};
use crate::domain::company_money::{
    CurrencyDefinition, CurrencyRules, FinanceCollation, ManualRate, functional_fx_evidence,
};
use crate::domain::journal::JournalError;
use crate::domain::money::FinancePolicyError;

impl From<JournalError> for PgTransactionError<JournalError> {
    fn from(error: JournalError) -> Self {
        Self::Business(error)
    }
}

pub(crate) async fn current_transaction_witness(
    connection: &mut PgConnection,
) -> Result<JournalTransactionWitness, PgTransactionError<JournalError>> {
    let row = sqlx::query("SELECT current_database() AS database_name,pg_backend_pid() AS backend_id,pg_current_xact_id()::text AS transaction_id").fetch_one(connection).await?;
    let database: String = row.try_get("database_name")?;
    let backend: i32 = row.try_get("backend_id")?;
    let transaction: String = row.try_get("transaction_id")?;
    Ok(JournalTransactionWitness(format!(
        "{}:{database}:{backend}:{transaction}",
        database.len()
    )))
}

pub fn embedded_currency_rules() -> Result<CurrencyRules, FinancePolicyError> {
    #[derive(Deserialize)]
    struct Catalogue {
        currencies: Vec<CurrencyDefinition>,
    }
    let catalogue: Catalogue =
        serde_json::from_str(include_str!("../../../catalogue/src/iso-4217-current.json"))
            .map_err(|_| FinancePolicyError("Invalid currency catalogue.".to_owned()))?;
    CurrencyRules::new(catalogue.currencies)
}

/// Request-bound facts over the caller's existing transaction. No default authority exists.
pub struct PgJournalPreparation<'c, 'p, A, C> {
    connection: &'c mut PgConnection,
    actor: &'p FinanceActor,
    authority: &'p mut A,
    clock: &'p C,
    currencies: &'p CurrencyRules,
    collation: &'p FinanceCollation,
}

impl<'c, 'p, A, C> PgJournalPreparation<'c, 'p, A, C> {
    pub fn new(
        connection: &'c mut PgConnection,
        actor: &'p FinanceActor,
        authority: &'p mut A,
        clock: &'p C,
        currencies: &'p CurrencyRules,
        collation: &'p FinanceCollation,
    ) -> Result<Self, PgTransactionError<JournalError>> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        Ok(Self {
            connection,
            actor,
            authority,
            clock,
            currencies,
            collation,
        })
    }
    fn scope(&self, actor: &FinanceActor) -> Result<(), PgTransactionError<JournalError>> {
        if actor != self.actor {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "JOURNAL_SCOPE_MISMATCH",
                message: "The journal facts belong to another request context.".to_owned(),
            }
            .into());
        }
        Ok(())
    }
    fn company(&self, company: &CompanyId) -> Result<(), PgTransactionError<JournalError>> {
        if company != &self.actor.company_id {
            return Err(JournalError {
                kind: FailureKind::Forbidden,
                code: "JOURNAL_SCOPE_MISMATCH",
                message: "The journal facts belong to another Company.".to_owned(),
            }
            .into());
        }
        Ok(())
    }
}

impl<A, C> JournalPreparationPorts for PgJournalPreparation<'_, '_, A, C>
where
    A: JournalAuthority<Error = PgTransactionError<JournalError>> + Send,
    C: Clock + Sync,
{
    type Error = PgTransactionError<JournalError>;
    async fn transaction_witness(
        &mut self,
    ) -> Result<Option<JournalTransactionWitness>, Self::Error> {
        current_transaction_witness(self.connection).await.map(Some)
    }
    async fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> Result<(), Self::Error> {
        self.scope(actor)?;
        self.authority.authorize(actor, draft).await
    }
    async fn chart(
        &mut self,
        company: &CompanyId,
        chart: &str,
    ) -> Result<Option<JournalChart>, Self::Error> {
        self.company(company)?;
        let row = sqlx::query("SELECT id,base_currency FROM finance_charts WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE")
            .bind(chart).bind(company.as_str()).fetch_optional(&mut *self.connection).await?;
        row.map(|row| -> Result<JournalChart, sqlx::Error> {
            Ok(JournalChart {
                id: row.try_get("id")?,
                base_currency: row.try_get("base_currency")?,
            })
        })
        .transpose()
        .map_err(Into::into)
    }
    async fn ensure_period(
        &mut self,
        actor: &FinanceActor,
        period: &str,
        date: &str,
    ) -> Result<(), Self::Error> {
        self.scope(actor)?;
        let row = sqlx::query("SELECT start_date,end_date,state FROM finance_periods WHERE id=$1 AND company_id=$2 FOR SHARE")
            .bind(period).bind(actor.company_id.as_str()).fetch_optional(&mut *self.connection).await?
            .ok_or_else(|| JournalError { kind: FailureKind::NotFound, code: "PERIOD_NOT_FOUND", message: "Accounting period not found.".to_owned() })?;
        let start: String = row.try_get("start_date")?;
        let end: String = row.try_get("end_date")?;
        let state: String = row.try_get("state")?;
        if date < start.as_str() || date > end.as_str() {
            return Err(JournalError::new(
                "DATE_OUTSIDE_PERIOD",
                "Entry date is outside the accounting period.",
            )
            .into());
        }
        if state != "open" {
            return Err(conflict(
                "PERIOD_CLOSED",
                "The accounting period does not accept postings.",
            ));
        }
        let covered = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM finance_periods WHERE company_id=$1 AND state IN ('closed','closing') AND $2 BETWEEN start_date AND end_date)")
            .bind(actor.company_id.as_str()).bind(date).fetch_one(&mut *self.connection).await?;
        if covered {
            return Err(conflict(
                "PERIOD_CLOSED",
                "This date is covered by a closed or closing Company period. Use the next open date or independently reopen the period.",
            ));
        }
        Ok(())
    }
    async fn money_scale(
        &mut self,
        company: &CompanyId,
        currency: &str,
    ) -> Result<u8, Self::Error> {
        self.company(company)?;
        let scale = sqlx::query_scalar::<_, i64>(
            "SELECT scale::bigint FROM company_currency_scales WHERE company_id=$1 AND currency=$2",
        )
        .bind(company.as_str())
        .bind(currency)
        .fetch_optional(&mut *self.connection)
        .await?;
        self.currencies
            .scale(currency, scale)
            .map_err(|error| JournalError::new("CURRENCY_SCALE_REQUIRED", error.0).into())
    }
    async fn functional_fx(
        &mut self,
        actor: &FinanceActor,
        currency: &str,
        base: &str,
        date: &str,
    ) -> Result<FunctionalFxEvidence, Self::Error> {
        self.scope(actor)?;
        if currency == base {
            return functional_fx_evidence(&[], currency, base, date, self.collation)
                .map_err(Into::into);
        }
        let currencies = vec![currency.to_owned(), base.to_owned()];
        let rows = sqlx::query("SELECT id,currency,effective_date::text AS effective_date,currency_per_usd::text AS currency_per_usd,confirmed_at FROM company_fx_rates WHERE company_id=$1 AND currency=ANY($2) ORDER BY effective_date DESC,confirmed_at DESC,id DESC")
            .bind(actor.company_id.as_str()).bind(currencies).fetch_all(&mut *self.connection).await?;
        let rates: Vec<ManualRate> = rows
            .into_iter()
            .map(|row| {
                let quote: String = row.try_get("currency_per_usd")?;
                let quote = if quote.contains('.') {
                    quote.trim_end_matches('0').trim_end_matches('.').to_owned()
                } else {
                    quote
                };
                Ok(ManualRate {
                    id: row.try_get("id")?,
                    currency: row.try_get("currency")?,
                    effective_date: row.try_get("effective_date")?,
                    currency_per_usd: quote,
                    confirmed_at: row.try_get("confirmed_at")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?;
        functional_fx_evidence(&rates, currency, base, date, self.collation).map_err(Into::into)
    }
    async fn accounts(
        &mut self,
        company: &CompanyId,
        chart: &str,
        ids: &[String],
    ) -> Result<Vec<String>, Self::Error> {
        self.company(company)?;
        sqlx::query_scalar("SELECT id FROM finance_accounts WHERE company_id=$1 AND chart_id=$2 AND status='active' AND id=ANY($3) FOR SHARE")
            .bind(company.as_str()).bind(chart).bind(ids).fetch_all(&mut *self.connection).await.map_err(Into::into)
    }
    async fn opening_exists(
        &mut self,
        company: &CompanyId,
        chart: &str,
    ) -> Result<bool, Self::Error> {
        self.company(company)?;
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=$1 AND chart_id=$2 AND is_opening=1 AND status IN ('draft','in_review','posted'))")
            .bind(company.as_str()).bind(chart).fetch_one(&mut *self.connection).await.map_err(Into::into)
    }
    fn now(&mut self) -> TimestampMillis {
        self.clock.now()
    }
}

fn conflict(code: &'static str, message: &'static str) -> PgTransactionError<JournalError> {
    JournalError {
        kind: FailureKind::Conflict,
        code,
        message: message.to_owned(),
    }
    .into()
}
