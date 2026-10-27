//! Finance-owned facts and immutable stock-cost evidence on the caller's unit.
//! No Commerce/Inventory/source-event SQL, actor grant, transaction or retry.
use crate::adapters::postgres_journal_facts::embedded_currency_rules;
use crate::domain::company_money::{
    FinanceCollation, ManualRate, convert_currency_units, resolve_manual_rate,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, RecordId,
};
use futures_util::TryStreamExt;
use num_bigint::BigInt;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
const SAFE: i64 = 9_007_199_254_740_991;
const ROW_BYTES: usize = 65_536;
const RATE_BYTES: usize = 4 * 1024 * 1024;

/// Data from a separately retained Commerce owner proof, never authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryConsumptionMapping {
    pub company_id: CompanyId,
    pub chart_id: RecordId,
    pub inventory_account_id: RecordId,
    pub cogs_account_id: RecordId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionFinanceConfig {
    pub chart_id: RecordId,
    pub currency: String,
    pub base_currency: String,
    pub inventory_account_id: RecordId,
    pub cogs_account_id: RecordId,
}
#[derive(Clone, Eq, PartialEq)]
struct Transaction {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}
#[derive(Clone, Eq, PartialEq)]
struct Image {
    chart: Vec<Value>,
    accounts: Vec<Value>,
    periods: Vec<Value>,
}
/// Opaque same-transaction Finance facts. Does not prove the Commerce mapping,
/// source event, personal permission, release, purchase, stock or Quality facts.
pub struct CurrentConsumptionFinance {
    holding: HoldingId,
    mapping: InventoryConsumptionMapping,
    date: String,
    transaction: Transaction,
    image: Image,
    config: Option<ConsumptionFinanceConfig>,
    periods: Vec<RecordId>,
}
impl CurrentConsumptionFinance {
    // Internal bounded snapshot retention lets the owning Inventory source keep
    // its first Finance proof. This is deliberately not a public Clone grant.
    fn retain_for_money(&self) -> Self {
        Self {
            holding: self.holding.clone(),
            mapping: self.mapping.clone(),
            date: self.date.clone(),
            transaction: self.transaction.clone(),
            image: self.image.clone(),
            config: self.config.clone(),
            periods: self.periods.clone(),
        }
    }
    pub fn holding_id(&self) -> &HoldingId {
        &self.holding
    }
    pub fn mapping(&self) -> &InventoryConsumptionMapping {
        &self.mapping
    }
    pub fn entry_date(&self) -> &str {
        &self.date
    }
    pub fn config(&self) -> Option<&ConsumptionFinanceConfig> {
        self.config.as_ref()
    }
    pub fn period_ids(&self) -> &[RecordId] {
        &self.periods
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction
            || capture(c, &self.mapping, &self.date).await? != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
    /// Preserve B0 ordering: account validity is the later mapping guard, not an
    /// additional early error from the chart/company configuration lookup.
    pub async fn require_posting_mapping(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current(c).await?;
        let Some(config) = &self.config else {
            return Err(mapping_error());
        };
        let account = |id: &RecordId| self.image.accounts.iter().find(|a| a["id"] == id.as_str());
        let valid = |row: Option<&Value>, categories: &[&str]| {
            row.is_some_and(|a| {
                a["company_id"] == self.mapping.company_id.as_str()
                    && a["chart_id"] == config.chart_id.as_str()
                    && a["status"] == "active"
                    && a["currency"] == config.base_currency
                    && categories.iter().any(|category| a["category"] == *category)
            })
        };
        if self.mapping.inventory_account_id == self.mapping.cogs_account_id
            || !valid(account(&self.mapping.inventory_account_id), &["asset"])
            || !valid(
                account(&self.mapping.cogs_account_id),
                &["expense", "other_expense"],
            )
        {
            return Err(mapping_error());
        }
        Ok(())
    }
}
/// Utility locks only, before the FIRST protected snapshot in the caller's
/// owner bundle. Compatible Money writers remain allowed; no row is a grant.
pub async fn lock_inventory_consumption_finance_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_charts,finance_accounts,finance_periods,company_currency_scales,company_fx_rates IN SHARE MODE").execute(&mut*c).await?;
    sqlx::query("LOCK TABLE money_transaction_snapshots IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_inventory_consumption_finance(
    c: &mut PgConnection,
    holding: &HoldingId,
    mapping: &InventoryConsumptionMapping,
    date: &str,
) -> Result<CurrentConsumptionFinance, Error> {
    require_date(date)?;
    require_unit(c).await?;
    lock_inventory_consumption_finance_sources(c).await?;
    let transaction = transaction(c).await?;
    let image = capture(c, mapping, date).await?;
    let config = image
        .chart
        .first()
        .map(|chart| {
            let currency = string(chart, "base_currency")?.to_owned();
            Ok::<_, Error>(ConsumptionFinanceConfig {
                chart_id: mapping.chart_id.clone(),
                currency: currency.clone(),
                base_currency: currency,
                inventory_account_id: mapping.inventory_account_id.clone(),
                cogs_account_id: mapping.cogs_account_id.clone(),
            })
        })
        .transpose()?;
    let periods = image
        .periods
        .iter()
        .map(|p| RecordId::new(string(p, "id")?).map_err(Into::into))
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(CurrentConsumptionFinance {
        holding: holding.clone(),
        mapping: mapping.clone(),
        date: date.into(),
        transaction,
        image,
        config,
        periods,
    })
}
fn require_date(date: &str) -> Result<(), BusinessError> {
    let format = time::macros::format_description!("[year]-[month]-[day]");
    if date.len() != 10
        || !date.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
        || time::Date::parse(date, format).is_err()
    {
        return Err(BusinessError::invalid(
            "INVALID_CONSUMPTION_DATE",
            "Use a valid ISO inventory entry date.",
        ));
    }
    Ok(())
}
async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_TRANSACTION_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    let row=sqlx::query("SELECT current_database() d,pg_backend_pid() p,pg_current_xact_id()::text x,pg_postmaster_start_time()::text s").fetch_one(c).await?;
    Ok(Transaction {
        database: row.try_get("d")?,
        backend: row.try_get("p")?,
        xid: row.try_get("x")?,
        postmaster: row.try_get("s")?,
    })
}
async fn rows(
    c: &mut PgConnection,
    query: &'static str,
    mapping: &InventoryConsumptionMapping,
    date: &str,
) -> Result<Vec<Value>, Error> {
    let mut stream = sqlx::query(query)
        .bind(mapping.company_id.as_str())
        .bind(vec![
            mapping.chart_id.as_str(),
            mapping.inventory_account_id.as_str(),
            mapping.cogs_account_id.as_str(),
        ])
        .bind(date)
        .fetch(c);
    let mut result = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = stream.try_next().await? {
        let text: Option<String> = row.try_get("image")?;
        let text = text.ok_or_else(limit)?;
        bytes = bytes.checked_add(text.len()).ok_or_else(limit)?;
        if text.len() > ROW_BYTES || bytes > RATE_BYTES || result.len() >= 20_000 {
            return Err(limit());
        }
        result.push(serde_json::from_str(&text).map_err(|_| limit())?);
    }
    Ok(result)
}
async fn capture(
    c: &mut PgConnection,
    m: &InventoryConsumptionMapping,
    date: &str,
) -> Result<Image, Error> {
    let chart=rows(c,"SELECT CASE WHEN pg_column_size(jsonb_build_object('id',t.id,'company_id',t.company_id,'base_currency',t.base_currency,'status',t.status,'version',t.version))<=65536 THEN jsonb_build_object('id',t.id,'company_id',t.company_id,'base_currency',t.base_currency,'status',t.status,'version',t.version)::text END image FROM finance_charts t WHERE t.company_id=$1 AND t.id=($2::text[])[1] AND $3::text<>'' FOR SHARE OF t",m,date).await?;
    let accounts=rows(c,"SELECT CASE WHEN pg_column_size(jsonb_build_object('id',t.id,'company_id',t.company_id,'chart_id',t.chart_id,'currency',t.currency,'category',t.category,'status',t.status,'version',t.version,'is_control',t.is_control,'control_kind',t.control_kind))<=65536 THEN jsonb_build_object('id',t.id,'company_id',t.company_id,'chart_id',t.chart_id,'currency',t.currency,'category',t.category,'status',t.status,'version',t.version,'is_control',t.is_control,'control_kind',t.control_kind)::text END image FROM finance_accounts t WHERE t.company_id=$1 AND t.id=ANY(($2::text[])[2:3]) AND $3::text<>'' ORDER BY t.id FOR SHARE OF t",m,date).await?;
    // More than one matching period is already a denial. Three bounded rows are
    // enough to preserve that fact without retaining unrelated Company history.
    let periods=rows(c,"SELECT CASE WHEN pg_column_size(jsonb_build_object('id',t.id,'company_id',t.company_id,'start_date',t.start_date,'end_date',t.end_date,'state',t.state,'close_version',t.close_version))<=65536 THEN jsonb_build_object('id',t.id,'company_id',t.company_id,'start_date',t.start_date,'end_date',t.end_date,'state',t.state,'close_version',t.close_version)::text END image FROM finance_periods t WHERE t.company_id=$1 AND t.state='open' AND t.start_date<=$3 AND t.end_date>=$3 AND cardinality($2::text[])>0 ORDER BY t.id LIMIT 3 FOR SHARE OF t",m,date).await?;
    Ok(Image {
        chart,
        accounts,
        periods,
    })
}
fn string<'a>(row: &'a Value, key: &str) -> Result<&'a str, Error> {
    row[key].as_str().ok_or_else(changed)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_FINANCE_SOURCE_CHANGED",
        "The retained Finance configuration, period, quote or transaction changed.",
    )
    .into()
}
fn mapping_error() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PART_FINANCE_MAPPING_CHANGED",
        "Review the functional-currency inventory and service expense mapping.",
    )
    .into()
}
fn limit() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_FINANCE_SOURCE_LIMIT",
        "The scoped Finance source exceeds its bounded row or aggregate budget.",
    )
    .into()
}

#[derive(Clone, Eq, PartialEq)]
struct Quote {
    rates: Vec<Value>,
    scale: Option<i64>,
}
async fn quote(c: &mut PgConnection, company: &CompanyId, currency: &str) -> Result<Quote, Error> {
    let scale:Option<i64>=sqlx::query_scalar("SELECT scale::bigint FROM company_currency_scales WHERE company_id=$1 AND currency=$2 FOR SHARE").bind(company.as_str()).bind(currency).fetch_optional(&mut*c).await?;
    let mut stream=sqlx::query("SELECT CASE WHEN pg_column_size(jsonb_build_object('id',r.id,'company_id',r.company_id,'currency',r.currency,'effective_date',r.effective_date,'currency_per_usd',r.currency_per_usd::text,'confirmed_at',r.confirmed_at))<=65536 THEN jsonb_build_object('id',r.id,'company_id',r.company_id,'currency',r.currency,'effective_date',r.effective_date,'currency_per_usd',r.currency_per_usd::text,'confirmed_at',r.confirmed_at)::text END image FROM company_fx_rates r WHERE company_id=$1 AND currency=$2 ORDER BY r.id LIMIT 20001 FOR SHARE OF r").bind(company.as_str()).bind(currency).fetch(c);
    let mut rates = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = stream.try_next().await? {
        let text: Option<String> = row.try_get("image")?;
        let text = text.ok_or_else(limit)?;
        bytes = bytes.checked_add(text.len()).ok_or_else(limit)?;
        if text.len() > ROW_BYTES || bytes > RATE_BYTES || rates.len() >= 20_000 {
            return Err(limit());
        }
        rates.push(serde_json::from_str(&text).map_err(|_| limit())?);
    }
    Ok(Quote { rates, scale })
}
/// Prepared cost evidence is not a source-event grant. The owning command must
/// retain its actual event, stock/lot/Quality, journal, receipt and audit guards.
pub struct PreparedConsumptionMoney {
    source: CurrentConsumptionFinance,
    quote: Quote,
    source_id: RecordId,
    amount: i64,
    usd: String,
    rate_id: Option<String>,
    fallback: bool,
    captured_at: i64,
}
impl PreparedConsumptionMoney {
    pub fn usd_units(&self) -> &str {
        &self.usd
    }
    pub fn amount_units(&self) -> i64 {
        self.amount
    }
    pub fn rate_id(&self) -> Option<&str> {
        self.rate_id.as_deref()
    }
    pub fn used_latest_fallback(&self) -> bool {
        self.fallback
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_posting_mapping(c).await?;
        let currency = &self
            .source
            .config
            .as_ref()
            .ok_or_else(mapping_error)?
            .currency;
        if quote(c, &self.source.mapping.company_id, currency).await? != self.quote {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn persist(self, c: &mut PgConnection) -> Result<RetainedConsumptionMoney, Error> {
        self.require_current(c).await?;
        let currency = &self
            .source
            .config
            .as_ref()
            .ok_or_else(mapping_error)?
            .currency;
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)VALUES($1,'operational_inventory_consumption',$2,'stock_cost',$3::date,$4,2,$5::numeric,$6::numeric,$7,$8,$9)")
   .bind(self.source.mapping.company_id.as_str()).bind(self.source_id.as_str()).bind(&self.source.date).bind(currency).bind(self.amount.to_string()).bind(&self.usd).bind(&self.rate_id).bind(self.fallback).bind(self.captured_at).execute(&mut*c).await?;
        let row = money_row(c, &self).await?.ok_or_else(changed)?;
        let expected = json!({
            "company_id": self.source.mapping.company_id.as_str(),
            "source_type": "operational_inventory_consumption", "source_id": self.source_id.as_str(),
            "amount_kind": "stock_cost", "transaction_date": self.source.date,
            "currency": currency, "currency_scale": 2, "amount_units": self.amount.to_string(),
            "usd_units": self.usd, "fx_rate_id": self.rate_id, "used_latest_fallback": self.fallback,
            "captured_at": self.captured_at, "original_source_type": null, "original_source_id": null,
            "original_amount_kind": null, "carried_source_type": null, "carried_source_id": null,
            "carried_amount_kind": null, "carried_allocation_offset_units": null,
        });
        if expected
            .as_object()
            .expect("fixed object")
            .iter()
            .any(|(key, value)| row.get(key) != Some(value))
        {
            return Err(changed());
        }
        let retained = RetainedConsumptionMoney { plan: self, row };
        retained.require_current(c).await?;
        Ok(retained)
    }
}
pub struct RetainedConsumptionMoney {
    plan: PreparedConsumptionMoney,
    row: Value,
}
impl RetainedConsumptionMoney {
    pub fn usd_units(&self) -> &str {
        &self.plan.usd
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.plan.require_current(c).await?;
        if money_row(c, &self.plan).await?.as_ref() != Some(&self.row) {
            return Err(changed());
        }
        Ok(())
    }
}
async fn money_row(
    c: &mut PgConnection,
    plan: &PreparedConsumptionMoney,
) -> Result<Option<Value>, Error> {
    // NUMERIC(38,0) must remain decimal strings even beyond JSON's integer
    // range. Never let a one-unit evidence mutation disappear into an f64.
    let text:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN pg_column_size(to_jsonb(m))<=65536 THEN (to_jsonb(m)||jsonb_build_object('amount_units',m.amount_units::text,'usd_units',m.usd_units::text,'carried_allocation_offset_units',m.carried_allocation_offset_units::text))::text END FROM money_transaction_snapshots m WHERE company_id=$1 AND source_type='operational_inventory_consumption' AND source_id=$2 AND amount_kind='stock_cost' FOR SHARE OF m").bind(plan.source.mapping.company_id.as_str()).bind(plan.source_id.as_str()).fetch_optional(c).await?;
    text.map(|v| {
        v.ok_or_else(limit)
            .and_then(|s| serde_json::from_str(&s).map_err(|_| limit()))
    })
    .transpose()
}
pub async fn prepare_consumption_stock_cost_money(
    c: &mut PgConnection,
    source: &CurrentConsumptionFinance,
    source_id: RecordId,
    amount_units: i64,
    clock: &impl Clock,
) -> Result<PreparedConsumptionMoney, Error> {
    source.require_posting_mapping(c).await?;
    prepare_stock_cost_money_from_current(c, source, source_id, amount_units, clock).await
}

/// Only the exact Finance-owned parts draft bridge uses this internal phase.
/// B0 selects/validates the Money quote during preparation BEFORE executing the
/// mapping assertion. The resulting proof still requires that mapping on its
/// normal require_current/persist path, at the owner's effect position.
pub(crate) async fn prepare_owned_parts_stock_cost_money(
    c: &mut PgConnection,
    source: &CurrentConsumptionFinance,
    source_id: RecordId,
    amount_units: i64,
    clock: &impl Clock,
) -> Result<PreparedConsumptionMoney, Error> {
    source.require_current(c).await?;
    prepare_stock_cost_money_from_current(c, source, source_id, amount_units, clock).await
}

async fn prepare_stock_cost_money_from_current(
    c: &mut PgConnection,
    source: &CurrentConsumptionFinance,
    source_id: RecordId,
    amount_units: i64,
    clock: &impl Clock,
) -> Result<PreparedConsumptionMoney, Error> {
    if source.periods.len() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PART_POSTING_PERIOD_REQUIRED",
            "Choose a date in one open Finance period.",
        )
        .into());
    }
    if !(1..=SAFE).contains(&amount_units) {
        return Err(BusinessError::invalid(
            "PART_VALUATION_REQUIRED",
            "Reconcile positive safe spare valuation before posting its service cost.",
        )
        .into());
    }
    let currency = &source.config.as_ref().ok_or_else(mapping_error)?.currency;
    let quote = quote(c, &source.mapping.company_id, currency).await?;
    let rules = embedded_currency_rules().map_err(|_| {
        BusinessError::invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })?;
    if rules.scale(currency, quote.scale).map_err(|_| {
        BusinessError::invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    })? != 2
    {
        return Err(BusinessError::invalid(
            "CURRENCY_SCALE_MISMATCH",
            "Inventory stock cost must use the configured two-decimal currency scale.",
        )
        .into());
    }
    let rates = quote
        .rates
        .iter()
        .map(|r| {
            Ok(ManualRate {
                id: string(r, "id")?.into(),
                currency: string(r, "currency")?.into(),
                effective_date: string(r, "effective_date")?.into(),
                currency_per_usd: string(r, "currency_per_usd")?.into(),
                confirmed_at: r["confirmed_at"].as_i64().ok_or_else(changed)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let collation = FinanceCollation::english().map_err(|_| {
        BusinessError::invalid(
            "MONEY_COLLATION_UNAVAILABLE",
            "The pinned Finance collation is unavailable.",
        )
    })?;
    let rate =
        resolve_manual_rate(&rates, currency, &source.date, &collation).ok_or_else(|| {
            BusinessError::invalid(
                "MANUAL_RATE_REQUIRED",
                "A confirmed manual currency/USD rate is required.",
            )
        })?;
    let usd = convert_currency_units(
        &BigInt::from(amount_units),
        2,
        2,
        &rate.rate.currency_per_usd,
        "1",
    )
    .map_err(|_| {
        BusinessError::invalid(
            "MONEY_OVERFLOW",
            "The stock-cost conversion exceeds its exact numeric bounds.",
        )
    })?
    .to_string();
    if usd.trim_start_matches('-').len() > 38 {
        return Err(BusinessError::invalid(
            "MONEY_OVERFLOW",
            "Money evidence exceeds NUMERIC(38,0).",
        )
        .into());
    }
    let captured_at = clock.now().get();
    if !(0..=SAFE).contains(&captured_at) {
        return Err(BusinessError::invalid(
            "INVALID_CAPTURE_TIME",
            "Use a safe current capture time.",
        )
        .into());
    }
    Ok(PreparedConsumptionMoney {
        source: source.retain_for_money(),
        quote,
        source_id,
        amount: amount_units,
        usd,
        rate_id: (rate.rate.id != "USD").then_some(rate.rate.id),
        fallback: rate.used_latest_fallback,
        captured_at,
    })
}
