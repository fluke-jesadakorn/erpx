//! PostgreSQL execution for the native R3 Manufacturing posting-setup
//! prerequisites read. This is the only layer that executes SQL for the
//! missing `GET /production/posting-setup/prerequisites` branch: it reuses the
//! existing native posting-setup configuration/chart/account read and adds the
//! retained active-period and company-FX-rate projections. No authority,
//! purchased-package admission, lock, `contextKey` or command write happens
//! here; composition owns those facts on its Holding transaction.
use crate::adapters::native_r3_posting_setup_read::posting_setup_read_in_unit;
use crate::application::native_r3_posting_prereq::{
    posting_prerequisites, PrereqFuture, PrereqRepository,
};
use crate::domain::native_r3_posting_setup::PostingSetupActor;
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde_json::Value;
use sqlx::{types::Json, PgConnection, Row as SqlRow};

type Error = PgTransactionError<BusinessError>;

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_POSTING_PREREQUISITES_UNAVAILABLE",
        "The Manufacturing posting prerequisites are unavailable.",
    )
    .into()
}

/// `$1` company. The retained `POSTING_FINANCE_PERIODS_SQL` selects
/// `close_version version` and `to_jsonb(p)::text source`; `source` is kept for
/// the composition-owned `contextKey` image. Dates are formatted for the
/// retained `String(...).slice(0,10)` projection.
pub const PERIODS_SQL: &str = r#"
SELECT COALESCE(jsonb_agg(jsonb_build_object(
  'id', q.id, 'name', q.name,
  'startDate', to_char(q.start_date, 'YYYY-MM-DD'),
  'endDate', to_char(q.end_date, 'YYYY-MM-DD'),
  'state', q.state, 'version', q.close_version, 'source', q.source
) ORDER BY q.id), '[]'::jsonb) AS periods
FROM (SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version,to_jsonb(p)::text source
      FROM finance_periods p WHERE p.company_id=$1) q
"#;

/// `$1` company. The retained `POSTING_COMPANY_FX_RATES_SQL` casts
/// `currency_per_usd::text` before the TS trailing-zero trim; the owner
/// projection reproduces the cast here and the domain trims it.
pub const RATES_SQL: &str = r#"
SELECT COALESCE(jsonb_agg(jsonb_build_object(
  'id', q.id, 'currency', q.currency,
  'effectiveDate', to_char(q.effective_date, 'YYYY-MM-DD'),
  'currencyPerUsd', q.currency_per_usd, 'confirmedAt', q.confirmed_at,
  'source', q.source
) ORDER BY q.id), '[]'::jsonb) AS rates
FROM (SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text currency_per_usd,
             r.confirmed_at,to_jsonb(r)::text source
      FROM company_fx_rates r WHERE r.company_id=$1) q
"#;

pub struct PgPostingPrereqRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgPostingPrereqRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn periods(&mut self, company: &CompanyId) -> Result<Value, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(PERIODS_SQL)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        let data: Json<Value> = row.try_get("periods")?;
        Ok(data.0)
    }

    pub async fn rates(&mut self, company: &CompanyId) -> Result<Value, Error> {
        if company.as_str().is_empty() {
            return Err(unavailable());
        }
        let row = sqlx::query(RATES_SQL)
            .bind(company.as_str())
            .fetch_one(&mut *self.connection)
            .await?;
        let data: Json<Value> = row.try_get("rates")?;
        Ok(data.0)
    }
}

impl PrereqRepository for PgPostingPrereqRead<'_> {
    type Error = Error;

    fn periods<'a>(&'a mut self, company: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error> {
        Box::pin(async move { PgPostingPrereqRead::periods(self, company).await })
    }

    fn rates<'a>(&'a mut self, company: &'a CompanyId) -> PrereqFuture<'a, Value, Self::Error> {
        Box::pin(async move { PgPostingPrereqRead::rates(self, company).await })
    }
}

/// Runs the retained owner read on the caller-owned Holding connection.
/// Composition supplies the Finance authority, purchased admission, locks and
/// `contextKey`; this helper never opens a transaction or writes.
pub async fn posting_prereq_in_unit(
    connection: &mut PgConnection,
    actor: &PostingSetupActor,
    now: TimestampMillis,
) -> Result<Value, Error> {
    let base = posting_setup_read_in_unit(connection, actor, now).await?;
    let mut repository = PgPostingPrereqRead::new(connection);
    posting_prerequisites(actor, base, &mut repository, now).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_match_the_retained_period_and_rate_reads() {
        assert!(PERIODS_SQL.contains("FROM finance_periods p WHERE p.company_id=$1"));
        assert!(PERIODS_SQL.contains("p.close_version,to_jsonb(p)::text source"));
        assert!(PERIODS_SQL.contains("'startDate', to_char(q.start_date, 'YYYY-MM-DD')"));
        assert!(PERIODS_SQL.contains("'version', q.close_version"));
        assert!(RATES_SQL.contains("FROM company_fx_rates r WHERE r.company_id=$1"));
        assert!(RATES_SQL.contains("r.currency_per_usd::text currency_per_usd"));
        assert!(RATES_SQL.contains("'effectiveDate', to_char(q.effective_date, 'YYYY-MM-DD')"));
        assert!(RATES_SQL.contains("'currencyPerUsd', q.currency_per_usd"));
        assert!(RATES_SQL.contains("'confirmedAt', q.confirmed_at"));
    }
}
