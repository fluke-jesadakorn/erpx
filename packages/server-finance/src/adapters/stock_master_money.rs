//! Finance-owned Stock master settings and product price Money evidence.
use super::{
    postgres_inventory_consumption::{Quote, quote},
    postgres_journal_facts::embedded_currency_rules,
};
use crate::domain::company_money::{
    FinanceCollation, ManualRate, convert_currency_units, resolve_manual_rate,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use num_bigint::BigInt;
use serde::Serialize;
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct StockMasterProductMoneyEvidence {
    pub transaction_date: String,
    pub currency: String,
    #[cfg_attr(feature = "contracts", ts(type = "number"))]
    pub currency_scale: i32,
    pub amount_units: String,
    pub usd_units: String,
    pub fx_rate_id: Option<String>,
    pub used_latest_fallback: bool,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    evidence: StockMasterProductMoneyEvidence,
    digest: String,
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<Image>, Error> {
    let row=sqlx::query("SELECT CASE WHEN b.bounded THEN m.transaction_date::text END transaction_date,CASE WHEN b.bounded THEN m.currency END currency,m.currency_scale::int currency_scale,CASE WHEN b.bounded THEN m.amount_units::text END amount_units,CASE WHEN b.bounded THEN m.usd_units::text END usd_units,CASE WHEN b.bounded THEN m.fx_rate_id END fx_rate_id,m.used_latest_fallback,b.bounded,(m.currency_scale=2 AND m.amount_units>=0 AND m.usd_units>=0 AND m.captured_at BETWEEN 0 AND 9007199254740991 AND m.amount_kind='unit_price' AND m.original_source_type IS NULL AND m.original_source_id IS NULL AND m.original_amount_kind IS NULL AND m.carried_source_type IS NULL AND m.carried_source_id IS NULL AND m.carried_amount_kind IS NULL AND m.carried_allocation_offset_units IS NULL) ordinary,encode(sha256(convert_to(to_jsonb(m)::text,'UTF8')),'hex') digest FROM money_transaction_snapshots m CROSS JOIN LATERAL (SELECT coalesce(octet_length(m.fx_rate_id),0)+octet_length(m.currency)+octet_length(m.amount_units::text)+octet_length(m.usd_units::text)<=65536 bounded) b WHERE m.company_id=$1 AND m.source_type='commerce_product_revision' AND m.source_id=$2 LIMIT 2 FOR SHARE OF m").bind(company.as_str()).bind(id).fetch_all(c).await?;
    if row.len() > 1 {
        return Err(changed());
    }
    let Some(row) = row.into_iter().next() else {
        return Ok(None);
    };
    if !row.try_get::<bool, _>("bounded")? || !row.try_get::<bool, _>("ordinary")? {
        return Err(changed());
    }
    let text = |key| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(key)?.ok_or_else(changed)
    };
    Ok(Some(Image {
        evidence: StockMasterProductMoneyEvidence {
            transaction_date: text("transaction_date")?,
            currency: text("currency")?,
            currency_scale: row.try_get("currency_scale")?,
            amount_units: text("amount_units")?,
            usd_units: text("usd_units")?,
            fx_rate_id: row.try_get("fx_rate_id")?,
            used_latest_fallback: row.try_get("used_latest_fallback")?,
        },
        digest: row.try_get("digest")?,
    }))
}
pub struct PreparedStockMasterProductMoney {
    unit: Unit,
    company: CompanyId,
    id: String,
    quote: Quote,
    evidence: StockMasterProductMoneyEvidence,
    captured_at: i64,
}
impl PreparedStockMasterProductMoney {
    pub fn evidence(&self) -> &StockMasterProductMoneyEvidence {
        &self.evidence
    }
    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || quote(c, &self.company, &self.evidence.currency).await? != self.quote
            || image(c, &self.company, &self.id).await?.is_some()
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedStockMasterProductMoney, Error> {
        self.require_before(c).await?;
        // Compare the entire inserted physical row to the planned image, including
        // captured time and null provenance; a trigger must not become authority.
        let expected: String = sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('company_id',$1::text,'source_type','commerce_product_revision','source_id',$2::text,'amount_kind','unit_price','transaction_date',$3::date,'currency',$4::text,'currency_scale',2,'amount_units',$5::numeric,'usd_units',$6::numeric,'fx_rate_id',$7::text,'used_latest_fallback',$8::boolean,'captured_at',$9::bigint,'original_source_type',NULL,'original_source_id',NULL,'original_amount_kind',NULL,'carried_source_type',NULL,'carried_source_id',NULL,'carried_amount_kind',NULL,'carried_allocation_offset_units',NULL)::text,'UTF8')),'hex')")
            .bind(self.company.as_str()).bind(&self.id).bind(&self.evidence.transaction_date).bind(&self.evidence.currency).bind(&self.evidence.amount_units).bind(&self.evidence.usd_units).bind(&self.evidence.fx_rate_id).bind(self.evidence.used_latest_fallback).bind(self.captured_at).fetch_one(&mut *c).await?;
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)VALUES($1,'commerce_product_revision',$2,'unit_price',$3::date,$4,2,$5::numeric,$6::numeric,$7,$8,$9)")
            .bind(self.company.as_str()).bind(&self.id).bind(&self.evidence.transaction_date).bind(&self.evidence.currency).bind(&self.evidence.amount_units).bind(&self.evidence.usd_units).bind(&self.evidence.fx_rate_id).bind(self.evidence.used_latest_fallback).bind(self.captured_at).execute(&mut *c).await?;
        let captured = image(c, &self.company, &self.id)
            .await?
            .ok_or_else(changed)?;
        if captured.evidence != self.evidence || captured.digest != expected {
            return Err(changed());
        }
        let retained = RetainedStockMasterProductMoney {
            plan: self,
            image: captured,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}
pub struct RetainedStockMasterProductMoney {
    plan: PreparedStockMasterProductMoney,
    image: Image,
}
impl RetainedStockMasterProductMoney {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.plan.unit
            || quote(c, &self.plan.company, &self.plan.evidence.currency).await? != self.plan.quote
            || image(c, &self.plan.company, &self.plan.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Replay freezes the entire original source, including the empty set; it does
/// not demand that an old product has a native price snapshot.
pub struct CurrentStockMasterProductMoney {
    unit: Unit,
    company: CompanyId,
    id: String,
    image: Option<String>,
}
impl CurrentStockMasterProductMoney {
    /// Bounded raw PostgreSQL source image, absent when there were no rows.
    pub fn evidence(&self) -> Option<&str> {
        self.image.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || replay_image(c, &self.company, &self.id).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn replay_image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<String>, Error> {
    let row = sqlx::query("WITH rows AS MATERIALIZED (SELECT to_jsonb(m) value,m.captured_at FROM money_transaction_snapshots m WHERE m.company_id=$1 AND m.source_type='commerce_product_revision' AND m.source_id=$2 ORDER BY m.amount_kind LIMIT 20001), image AS (SELECT count(*) n, bool_and(captured_at BETWEEN -9007199254740991 AND 9007199254740991) safe, jsonb_agg(value)::text data FROM rows) SELECT n, safe, CASE WHEN n<=20000 AND octet_length(data)<=2097152 THEN data END data FROM image")
        .bind(company.as_str()).bind(id).fetch_one(c).await?;
    let n: i64 = row.try_get("n")?;
    if n == 0 {
        return Ok(None);
    }
    if row.try_get::<Option<bool>, _>("safe")? != Some(true) {
        return Err(changed());
    }
    Ok(Some(
        row.try_get::<Option<String>, _>("data")?
            .ok_or_else(changed)?,
    ))
}
pub async fn read_stock_master_product_money(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<CurrentStockMasterProductMoney, Error> {
    let unit = unit(c).await?;
    let id = format!("{id}:1");
    let image = replay_image(c, company, &id).await?;
    Ok(CurrentStockMasterProductMoney {
        unit,
        company: company.clone(),
        id,
        image,
    })
}
pub async fn prepare_stock_master_product_money(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    currency: &str,
    amount: i64,
    clock: &impl Clock,
) -> Result<PreparedStockMasterProductMoney, Error> {
    let unit = unit(c).await?;
    if !id.starts_with("products-") || id.len() > 1024 {
        return Err(changed());
    }
    let at = clock.now().get();
    if !(0..=9_007_199_254_740_991).contains(&at) {
        return Err(changed());
    }
    let instant = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(at) * 1_000_000)
        .map_err(|_| changed())?;
    let date = instant
        .date()
        .format(&time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(|_| changed())?;
    if !(0..=9_007_199_254_740_991).contains(&amount) {
        return Err(changed());
    }
    let scale: Option<i64> = sqlx::query_scalar("SELECT scale::bigint FROM company_currency_scales WHERE company_id=$1 AND currency=$2 FOR SHARE").bind(company.as_str()).bind(currency).fetch_optional(&mut *c).await?;
    let rules = embedded_currency_rules().map_err(|_| {
        BusinessError::invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    if rules.scale(currency, scale).map_err(|_| {
        BusinessError::invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })? != 2
    {
        return Err(BusinessError::invalid(
            "CURRENCY_SCALE_MISMATCH",
            "The product currency must use two decimal places.",
        )
        .into());
    }
    let quote = quote(c, company, currency).await?;
    if quote.scale != scale {
        return Err(changed());
    }
    let rates = quote
        .rates
        .iter()
        .map(|r| -> Result<ManualRate, Error> {
            let confirmed_at = r["confirmed_at"].as_i64().ok_or_else(changed)?;
            if !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&confirmed_at) {
                return Err(changed());
            }
            Ok(ManualRate {
                id: r["id"].as_str().ok_or_else(changed)?.into(),
                currency: r["currency"].as_str().ok_or_else(changed)?.into(),
                effective_date: r["effective_date"].as_str().ok_or_else(changed)?.into(),
                currency_per_usd: r["currency_per_usd"].as_str().ok_or_else(changed)?.into(),
                confirmed_at,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let collation = FinanceCollation::english().map_err(|_| changed())?;
    let rate = resolve_manual_rate(&rates, currency, &date, &collation).ok_or_else(|| {
        BusinessError::invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency/USD rate is required.",
        )
    })?;
    let usd = convert_currency_units(
        &BigInt::from(amount),
        2,
        2,
        &rate.rate.currency_per_usd,
        "1",
    )
    .map_err(|_| {
        BusinessError::invalid(
            "MONEY_OVERFLOW",
            "Money conversion exceeds its exact bounds.",
        )
    })?
    .to_string();
    if usd.len() > 38 {
        return Err(BusinessError::invalid(
            "MONEY_OVERFLOW",
            "Money evidence exceeds its numeric bounds.",
        )
        .into());
    }
    let evidence = StockMasterProductMoneyEvidence {
        transaction_date: date,
        currency: currency.into(),
        currency_scale: 2,
        amount_units: amount.to_string(),
        usd_units: usd,
        fx_rate_id: (rate.rate.id != "USD").then_some(rate.rate.id),
        used_latest_fallback: rate.used_latest_fallback,
    };
    let plan = PreparedStockMasterProductMoney {
        unit,
        company: company.clone(),
        id: format!("{id}:1"),
        quote,
        evidence,
        captured_at: at,
    };
    plan.require_before(c).await?;
    Ok(plan)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Stock master Finance source changed.",
    )
    .into()
}

/// Finance's view of a Commerce-owned settings row. The caller must prove that
/// the supplied row is the actual current Commerce source in this same unit.
pub struct CurrentStockMasterFinance {
    unit: Unit,
    company: CompanyId,
    chart: String,
    currency: String,
    image: String,
    active: bool,
    active_base_matches: bool,
}
impl CurrentStockMasterFinance {
    pub fn effective_matches_mapping(&self) -> bool {
        self.active
    }
    pub fn active_chart_base_matches_mapping(&self) -> bool {
        self.active_base_matches
    }
    pub fn currency(&self) -> &str {
        &self.currency
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || finance_image(c, &self.company, &self.chart, &self.currency).await?
                != (self.image.clone(), self.active, self.active_base_matches)
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire before any protected snapshot in the shared SERIALIZABLE transaction.
pub async fn lock_stock_master_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn finance_image(
    c: &mut PgConnection,
    company: &CompanyId,
    chart: &str,
    currency: &str,
) -> Result<(String, bool, bool), Error> {
    // Bound both number and size before returning owner data. Keep the whole
    // Finance source image so late changes cannot silently alter selection.
    let row = sqlx::query("WITH refs AS MATERIALIZED (SELECT 'chart' kind,to_jsonb(x) value FROM finance_charts x WHERE company_id=$1 AND id=$2 UNION ALL SELECT 'account',to_jsonb(x) FROM finance_accounts x WHERE company_id=$1 AND id IN (SELECT unnest(ARRAY[d.receivable_account_id,d.revenue_account_id,d.tax_account_id]) FROM finance_sales_defaults d WHERE d.company_id=$1 AND d.chart_id=$2 AND d.currency=$3 AND d.status='approved') UNION ALL SELECT 'default',to_jsonb(x) FROM finance_sales_defaults x WHERE company_id=$1 AND chart_id=$2 AND currency=$3 AND status='approved' UNION ALL SELECT 'settings',to_jsonb(x) FROM company_money_settings x WHERE company_id=$1 UNION ALL SELECT 'scale',to_jsonb(x) FROM company_currency_scales x WHERE company_id=$1 AND currency=$3 UNION ALL SELECT 'rate',to_jsonb(x) FROM company_fx_rates x WHERE company_id=$1 AND currency=$3), bounded AS (SELECT * FROM refs LIMIT 20001), image AS (SELECT count(*) n, coalesce(jsonb_agg(jsonb_build_array(kind,value) ORDER BY kind,value::text),'[]'::jsonb)::text data FROM bounded) SELECT CASE WHEN n<=20000 AND octet_length(data)<=2097152 THEN data END data, EXISTS(SELECT 1 FROM finance_charts WHERE company_id=$1 AND id=$2 AND status='active') active, EXISTS(SELECT 1 FROM finance_charts WHERE company_id=$1 AND id=$2 AND status='active' AND base_currency=$3) active_base_matches FROM image")
        .bind(company.as_str()).bind(chart).bind(currency).fetch_one(&mut *c).await?;
    let data: Option<String> = row.try_get("data")?;
    let data = data.ok_or_else(changed)?;
    // The reader joins the active mapping chart and left-joins a valid approved
    // Sales default for its chart/currency; selection is captured in the image.
    let selected: Option<String> = sqlx::query_scalar("SELECT to_jsonb(d)::text FROM finance_sales_defaults d WHERE d.company_id=$1 AND d.chart_id=$2 AND d.currency=$3 AND d.status='approved' AND EXISTS(SELECT 1 FROM finance_accounts a WHERE a.id=d.receivable_account_id AND a.company_id=d.company_id AND a.chart_id=d.chart_id AND a.currency=d.currency AND a.control_kind='receivable' AND a.status='active') AND EXISTS(SELECT 1 FROM finance_accounts a WHERE a.id=d.revenue_account_id AND a.company_id=d.company_id AND a.chart_id=d.chart_id AND a.currency=d.currency AND a.category='revenue' AND a.status='active') AND (d.tax_account_id IS NULL OR EXISTS(SELECT 1 FROM finance_accounts a WHERE a.id=d.tax_account_id AND a.company_id=d.company_id AND a.chart_id=d.chart_id AND a.currency=d.currency AND a.category='liability' AND a.control_kind='none' AND a.status='active')) LIMIT 1")
        .bind(company.as_str()).bind(chart).bind(currency).fetch_optional(c).await?;
    Ok((
        format!("{}:{}", data, selected.unwrap_or_default()),
        row.try_get("active")?,
        row.try_get("active_base_matches")?,
    ))
}
pub async fn read_stock_master_finance(
    c: &mut PgConnection,
    company: &CompanyId,
    chart: &str,
    currency: &str,
) -> Result<CurrentStockMasterFinance, Error> {
    let unit = unit(c).await?;
    let (image, active, active_base_matches) = finance_image(c, company, chart, currency).await?;
    Ok(CurrentStockMasterFinance {
        unit,
        company: company.clone(),
        chart: chart.into(),
        currency: currency.into(),
        image,
        active,
        active_base_matches,
    })
}

/// Chart-only Finance witness for workspace discovery. The Commerce owner must
/// separately prove the mapping and its original receipt in this same unit.
pub struct StockMasterWorkspaceChart {
    unit: Unit,
    company: CompanyId,
    chart: String,
    currency: String,
    image: Option<String>,
    ready: bool,
}
impl StockMasterWorkspaceChart {
    pub fn active_chart_base_matches_mapping(&self) -> bool {
        self.ready
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || workspace_chart_image(c, &self.company, &self.chart, &self.currency).await?
                != (self.image.clone(), self.ready)
        {
            return Err(changed());
        }
        Ok(())
    }
}
async fn workspace_chart_image(
    c: &mut PgConnection,
    company: &CompanyId,
    chart: &str,
    currency: &str,
) -> Result<(Option<String>, bool), Error> {
    let row = sqlx::query("SELECT CASE WHEN octet_length(to_jsonb(c)::text)<=2097152 THEN to_jsonb(c)::text END image,(c.status='active' AND c.base_currency=$3) ready FROM finance_charts c WHERE c.company_id=$1 AND c.id=$2")
        .bind(company.as_str()).bind(chart).bind(currency).fetch_optional(c).await?;
    match row {
        None => Ok((None, false)),
        Some(row) => Ok((
            Some(
                row.try_get::<Option<String>, _>("image")?
                    .ok_or_else(changed)?,
            ),
            row.try_get("ready")?,
        )),
    }
}
pub async fn read_stock_master_workspace_chart(
    c: &mut PgConnection,
    company: &CompanyId,
    chart: &str,
    currency: &str,
) -> Result<StockMasterWorkspaceChart, Error> {
    let unit = unit(c).await?;
    let (image, ready) = workspace_chart_image(c, company, chart, currency).await?;
    Ok(StockMasterWorkspaceChart {
        unit,
        company: company.clone(),
        chart: chart.into(),
        currency: currency.into(),
        image,
        ready,
    })
}
