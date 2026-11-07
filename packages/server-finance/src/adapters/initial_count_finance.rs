//! Finance source witness for the Service first physical Count. The caller must
//! independently retain the Commerce mapping, owner authority and source proof.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use serde_json::Value;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitialCountFinanceBasis {
    pub company_id: CompanyId,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub variance_account_id: String,
    pub currency: String,
}
#[derive(Clone, Eq, PartialEq)]
struct Image {
    chart: Vec<String>,
    accounts: Vec<String>,
    periods: Vec<String>,
    closed_periods: Vec<String>,
    scale: Vec<String>,
}
#[derive(Clone, Eq, PartialEq)]
struct Transaction {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}
/// Opaque facts retain complete Finance rows and their transaction identity.
/// They provide no owner, mapping or actor authorization.
pub struct CurrentInitialCountFinance {
    holding: HoldingId,
    basis: InitialCountFinanceBasis,
    date: String,
    transaction: Transaction,
    image: Image,
    period: String,
}
impl CurrentInitialCountFinance {
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
    pub fn basis(&self) -> &InitialCountFinanceBasis {
        &self.basis
    }
    pub fn count_date(&self) -> &str {
        &self.date
    }
    pub fn period_id(&self) -> &str {
        &self.period
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction
            || capture(c, &self.basis, &self.date).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Lock before the first protected snapshot, following the retained source's
/// write intent. The caller owns transaction, retry, receipt and final guards.
pub async fn lock_initial_count_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    // Do not take a SERIALIZABLE snapshot before a potentially waiting lock.
    // The first source read proves isolation and transaction identity after all
    // the owner's utility locks have been acquired.
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_periods,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE finance_journals,finance_journal_lines,finance_book_postings,finance_financial_events,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE").execute(&mut *c).await?;
    Ok(())
}
/// Read only after the caller has retained the actual Commerce mapping and
/// acquired all owner locks in their agreed order. `basis` is not a grant.
pub async fn read_initial_count_finance(
    c: &mut PgConnection,
    holding: &HoldingId,
    basis: &InitialCountFinanceBasis,
    date: &str,
) -> Result<CurrentInitialCountFinance, Error> {
    require_unit(c).await?;
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    if date.len() != 10 || time::Date::parse(date, fmt).is_err() {
        return Err(BusinessError::invalid("INVALID_COUNT_DATE", "Use a valid Count date.").into());
    }
    let transaction = transaction(c).await?;
    let image = capture(c, basis, date).await?;
    let chart: Vec<Value> = image
        .chart
        .iter()
        .map(|s| serde_json::from_str(s).map_err(|_| limit()))
        .collect::<Result<_, _>>()?;
    let accounts: Vec<Value> = image
        .accounts
        .iter()
        .map(|s| serde_json::from_str(s).map_err(|_| limit()))
        .collect::<Result<_, _>>()?;
    let valid = chart.len() == 1
        && accounts.len() == 2
        && basis.inventory_account_id != basis.variance_account_id
        && chart[0]["status"] == "active"
        && chart[0]["base_currency"] == basis.currency
        && accounts.iter().any(|a| {
            a["id"] == basis.inventory_account_id
                && a["chart_id"] == basis.chart_id
                && a["currency"] == basis.currency
                && a["category"] == "asset"
                && a["control_kind"] == "none"
                && a["status"] == "active"
        })
        && accounts.iter().any(|a| {
            a["id"] == basis.variance_account_id
                && a["chart_id"] == basis.chart_id
                && a["currency"] == basis.currency
                && a["category"] == "expense"
                && a["code"] == "5400"
                && a["status"] == "active"
        });
    if !valid {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "VARIANCE_MAPPING_REQUIRED",
            "Configure the stock count variance account (5400) in the operations ledger.",
        )
        .into());
    }
    let [period] = image.periods.as_slice() else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PERIOD_UNAVAILABLE",
            "Exactly one open period must cover the count date.",
        )
        .into());
    };
    let period: Value = serde_json::from_str(period).map_err(|_| limit())?;
    let period = period["id"].as_str().ok_or_else(changed)?.to_owned();
    Ok(CurrentInitialCountFinance {
        holding: holding.clone(),
        basis: basis.clone(),
        date: date.into(),
        transaction,
        image,
        period,
    })
}
async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    let r = sqlx::query("SELECT current_database() d, pg_backend_pid() p, pg_current_xact_id()::text x, pg_postmaster_start_time()::text s").fetch_one(c).await?;
    Ok(Transaction {
        database: r.try_get("d")?,
        backend: r.try_get("p")?,
        xid: r.try_get("x")?,
        postmaster: r.try_get("s")?,
    })
}
async fn capture(
    c: &mut PgConnection,
    b: &InitialCountFinanceBasis,
    date: &str,
) -> Result<Image, Error> {
    async fn rows(
        c: &mut PgConnection,
        sql: &'static str,
        company: &str,
        ids: Vec<&str>,
        date: &str,
    ) -> Result<Vec<String>, Error> {
        // PostgreSQL checks the complete bounded set before returning any row
        // text to the client. Preserve original ordered jsonb text verbatim.
        let bounded = format!(
            "WITH source AS MATERIALIZED (SELECT image, row_number() OVER () ordinal FROM ({sql}) AS raw(image)), stats AS (SELECT count(*) n, COALESCE(sum(octet_length(image)),0) bytes, COALESCE(bool_and(image IS NOT NULL AND octet_length(image)<=65536),true) valid FROM source) SELECT CASE WHEN n<=20000 AND bytes<=4194304 AND valid THEN (SELECT COALESCE(array_agg(image ORDER BY ordinal), ARRAY[]::text[]) FROM source) END FROM stats"
        );
        let rows: Option<Vec<String>> = sqlx::query_scalar(sqlx::AssertSqlSafe(bounded))
            .bind(company)
            .bind(ids)
            .bind(date)
            .fetch_one(c)
            .await?;
        rows.ok_or_else(limit)
    }
    let ids = || {
        vec![
            b.chart_id.as_str(),
            b.inventory_account_id.as_str(),
            b.variance_account_id.as_str(),
            b.currency.as_str(),
        ]
    };
    let chart = rows(c, "SELECT CASE WHEN pg_column_size(to_jsonb(t))<=65536 THEN to_jsonb(t)::text END FROM finance_charts t WHERE t.company_id=$1 AND t.id=($2::text[])[1] AND $3::text<>'' FOR SHARE OF t", b.company_id.as_str(), ids(), date).await?;
    let accounts = rows(c, "SELECT CASE WHEN pg_column_size(to_jsonb(t))<=65536 THEN to_jsonb(t)::text END FROM finance_accounts t WHERE t.company_id=$1 AND t.id=ANY(($2::text[])[2:3]) AND $3::text<>'' ORDER BY t.id FOR SHARE OF t", b.company_id.as_str(), ids(), date).await?;
    let periods = rows(c, "SELECT CASE WHEN pg_column_size(to_jsonb(t))<=65536 THEN to_jsonb(t)::text END FROM finance_periods t WHERE t.company_id=$1 AND t.state='open' AND t.start_date<=$3 AND t.end_date>=$3 AND cardinality($2::text[])>0 ORDER BY t.id LIMIT 20001 FOR SHARE OF t", b.company_id.as_str(), ids(), date).await?;
    let closed_periods = rows(c, "SELECT CASE WHEN pg_column_size(to_jsonb(t))<=65536 THEN to_jsonb(t)::text END FROM finance_periods t WHERE t.company_id=$1 AND t.state IN ('closing','closed') AND t.start_date<=$3 AND t.end_date>=$3 AND cardinality($2::text[])>0 ORDER BY t.id LIMIT 20001 FOR SHARE OF t", b.company_id.as_str(), ids(), date).await?;
    let scale = rows(c, "SELECT CASE WHEN pg_column_size(to_jsonb(t))<=65536 THEN to_jsonb(t)::text END FROM company_currency_scales t WHERE t.company_id=$1 AND t.currency=($2::text[])[4] AND $3::text<>'' FOR SHARE OF t", b.company_id.as_str(), ids(), date).await?;
    Ok(Image {
        chart,
        accounts,
        periods,
        closed_periods,
        scale,
    })
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Count posting references changed.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "COUNT_FINANCE_SOURCE_LIMIT",
        "The Finance source exceeds its bounded snapshot budget.",
    )
    .into()
}
