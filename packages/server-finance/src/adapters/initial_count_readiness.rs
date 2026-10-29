//! Read-only Finance readiness for the original Service first Count.
//! The Commerce owner retains the actual mapping and its currentness; this
//! component supplies facts only and never grants Stock or Finance authority.
use crate::adapters::initial_count_finance::InitialCountFinanceBasis;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId};
use serde::Serialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, PartialEq, Eq)]
struct Unit(String, i32, String, String);
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    let (a,b,d,e)=sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?;
    Ok(Unit(a, b, d, e))
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
#[derive(Clone, PartialEq, Eq)]
struct Image {
    witness: String,
    period_ids: Vec<String>,
    exact: String,
    basis: Option<InitialCountFinanceBasis>,
}
/// Opaque same-unit snapshot. `basis` and `period` are facts, not permission.
pub struct CurrentInitialCountReadiness {
    holding: HoldingId,
    company: CompanyId,
    chart: Option<String>,
    inventory: Option<String>,
    date: String,
    transaction: Unit,
    image: Image,
}
impl CurrentInitialCountReadiness {
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
    pub fn basis(&self) -> Option<&InitialCountFinanceBasis> {
        self.image.basis.as_ref()
    }
    pub fn period_id(&self) -> Option<&str> {
        match self.image.period_ids.as_slice() {
            [id] => Some(id),
            _ => None,
        }
    }
    pub fn period_count(&self) -> usize {
        self.image.period_ids.len()
    }
    pub fn context_image(&self) -> &str {
        &self.image.exact
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.transaction
            || capture(
                c,
                &self.company,
                self.chart.as_deref(),
                self.inventory.as_deref(),
                &self.date,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Call only after Finance and all other owners' utility locks have been taken.
/// `chart`/`inventory` come from a separately retained Commerce witness; they
/// are not inbound authorization or evidence of an active Service mapping.
pub async fn read_initial_count_readiness(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    chart: Option<&str>,
    inventory: Option<&str>,
    date: &str,
) -> Result<CurrentInitialCountReadiness, Error> {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    if date.len() != 10 || time::Date::parse(date, fmt).is_err() {
        return Err(BusinessError::invalid("INVALID_COUNT_DATE", "Use a valid Count date.").into());
    }
    let transaction = unit(c).await?;
    let image = capture(c, company, chart, inventory, date).await?;
    Ok(CurrentInitialCountReadiness {
        holding: holding.clone(),
        company: company.clone(),
        chart: chart.map(str::to_owned),
        inventory: inventory.map(str::to_owned),
        date: date.into(),
        transaction,
        image,
    })
}
#[derive(Serialize)]
struct Period<'a> {
    id: &'a str,
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    chart: Option<&str>,
    inventory: Option<&str>,
    date: &str,
) -> Result<Image, Error> {
    // Preserve the exact unsorted order of the retained `SELECT id` query for its
    // fallback JSON.stringify image. Limit bytes/count in SQL before transfer.
    let bounded: bool = sqlx::query_scalar("SELECT count(*)<=20000 AND COALESCE(sum(octet_length(id)),0)<=4194304 AND COALESCE(bool_and(octet_length(id)<=65536),true) FROM finance_periods WHERE company_id=$1 AND state='open' AND start_date<=$2 AND end_date>=$3").bind(company.as_str()).bind(date).bind(date).fetch_one(&mut *c).await?;
    if !bounded {
        return Err(limit());
    }
    // Execute exactly the original un-ordered SELECT after the bound check.
    // The same serializable snapshot and pre-acquired table lease cover both.
    let period_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM finance_periods WHERE company_id=$1 AND state='open' AND start_date<=$2 AND end_date>=$3").bind(company.as_str()).bind(date).bind(date).fetch_all(&mut *c).await?;
    // Keep complete original Finance rows, not a reduced projection or only the
    // successful join, so changes in missing/unavailable states invalidate it.
    let witness:Option<String>=sqlx::query_scalar(r#"WITH charts AS MATERIALIZED (SELECT * FROM finance_charts WHERE company_id=$1 AND id=$2), accounts AS MATERIALIZED (SELECT * FROM finance_accounts WHERE company_id=$1 AND (id=$3 OR (chart_id=$2 AND code='5400')) ORDER BY id), periods AS MATERIALIZED (SELECT * FROM finance_periods WHERE company_id=$1 AND state='open' AND start_date<=$4 AND end_date>=$4 ORDER BY id LIMIT 20001), image AS (SELECT jsonb_build_object('charts',COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM charts c),'[]'::jsonb),'accounts',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM accounts a),'[]'::jsonb),'periods',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM periods p),'[]'::jsonb)) v) SELECT CASE WHEN (SELECT count(*) FROM periods)<=20000 AND NOT EXISTS(SELECT 1 FROM charts c WHERE octet_length(to_jsonb(c)::text)>65536) AND NOT EXISTS(SELECT 1 FROM accounts a WHERE octet_length(to_jsonb(a)::text)>65536) AND NOT EXISTS(SELECT 1 FROM periods p WHERE octet_length(to_jsonb(p)::text)>65536) AND octet_length(v::text)<=4194304 THEN v::text END FROM image"#).bind(company.as_str()).bind(chart).bind(inventory).bind(date).fetch_one(&mut *c).await?;
    let witness = witness.ok_or_else(limit)?;
    let row=sqlx::query(r#"WITH src AS (SELECT c.id chart_id,c.base_currency currency,i.id inventory_id,a.id variance_id,jsonb_build_object('chart',to_jsonb(c),'inventory',to_jsonb(i),'variance',to_jsonb(a),'periods',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM finance_periods p WHERE p.company_id=c.company_id AND p.state='open' AND p.start_date<=$4 AND p.end_date>=$4),'[]'::jsonb))::text image FROM finance_charts c JOIN finance_accounts i ON i.id=$3 AND i.company_id=c.company_id AND i.chart_id=c.id AND i.currency=c.base_currency AND i.category='asset' AND i.control_kind='none' AND i.status='active' JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=c.company_id AND a.currency=c.base_currency AND a.category='expense' AND a.code='5400' AND a.status='active' WHERE c.id=$2 AND c.company_id=$1 AND c.status='active') SELECT chart_id,currency,inventory_id,variance_id,CASE WHEN octet_length(image)<=4194304 THEN image END image FROM src"#).bind(company.as_str()).bind(chart).bind(inventory).bind(date).fetch_optional(&mut *c).await?;
    let (basis, exact) = if let Some(row) = row {
        let exact: String = row
            .try_get::<Option<String>, _>("image")?
            .ok_or_else(limit)?;
        if exact.len() > 4194304 {
            return Err(limit());
        }
        (
            Some(InitialCountFinanceBasis {
                company_id: company.clone(),
                chart_id: row.try_get("chart_id")?,
                inventory_account_id: row.try_get("inventory_id")?,
                variance_account_id: row.try_get("variance_id")?,
                currency: row.try_get("currency")?,
            }),
            exact,
        )
    } else {
        let periods: Vec<_> = period_ids.iter().map(|id| Period { id }).collect();
        (
            None,
            format!(
                "[null,{}]",
                serde_json::to_string(&periods).map_err(|_| limit())?
            ),
        )
    };
    Ok(Image {
        witness,
        period_ids,
        exact,
        basis,
    })
}
