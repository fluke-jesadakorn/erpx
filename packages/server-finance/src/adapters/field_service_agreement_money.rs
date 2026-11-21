//! Exact Finance-owned visit-rate snapshot; no Finance actor or Inventory grant.
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
pub struct AgreementMoneyEvidence {
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
    evidence: AgreementMoneyEvidence,
    digest: String,
}
async fn image(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<Image>, Error> {
    let row=sqlx::query("SELECT CASE WHEN b.bounded THEN m.transaction_date::text END transaction_date,CASE WHEN b.bounded THEN m.currency END currency,m.currency_scale::int currency_scale,CASE WHEN b.bounded THEN m.amount_units::text END amount_units,CASE WHEN b.bounded THEN m.usd_units::text END usd_units,CASE WHEN b.bounded THEN m.fx_rate_id END fx_rate_id,m.used_latest_fallback,b.bounded,(m.currency_scale=2 AND m.amount_units>=0 AND m.usd_units>=0 AND m.captured_at>=0 AND m.original_source_type IS NULL AND m.original_source_id IS NULL AND m.original_amount_kind IS NULL AND m.carried_source_type IS NULL AND m.carried_source_id IS NULL AND m.carried_amount_kind IS NULL AND m.carried_allocation_offset_units IS NULL) ordinary,encode(sha256(convert_to(to_jsonb(m)::text,'UTF8')),'hex') digest FROM money_transaction_snapshots m CROSS JOIN LATERAL (SELECT coalesce(octet_length(m.fx_rate_id),0)+octet_length(m.currency)+octet_length(m.amount_units::text)+octet_length(m.usd_units::text)<=65536 bounded) b WHERE m.company_id=$1 AND m.source_type='field_service_agreement' AND m.source_id=$2 AND m.amount_kind='visit_rate' FOR SHARE OF m").bind(company.as_str()).bind(id).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if !row.try_get::<bool, _>("bounded")? || !row.try_get::<bool, _>("ordinary")? {
        return Err(changed());
    }
    let text = |key| -> Result<String, Error> {
        row.try_get::<Option<String>, _>(key)?.ok_or_else(changed)
    };
    Ok(Some(Image {
        evidence: AgreementMoneyEvidence {
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
pub async fn lock_agreement_money_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE company_currency_scales,company_fx_rates IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE money_transaction_snapshots IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
/// Historical readback freezes only the immutable Money source, not today's FX quote.
pub async fn lock_agreement_money_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE money_transaction_snapshots IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub struct PreparedAgreementMoney {
    unit: Unit,
    company: CompanyId,
    id: String,
    quote: Quote,
    evidence: AgreementMoneyEvidence,
    captured_at: i64,
}
impl PreparedAgreementMoney {
    pub fn evidence(&self) -> &AgreementMoneyEvidence {
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
    pub async fn persist(self, c: &mut PgConnection) -> Result<RetainedAgreementMoney, Error> {
        self.require_before(c).await?;
        // Compare the entire inserted physical row to the planned image, including
        // captured time and null provenance; a trigger must not become authority.
        let expected: String = sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('company_id',$1::text,'source_type','field_service_agreement','source_id',$2::text,'amount_kind','visit_rate','transaction_date',$3::date,'currency',$4::text,'currency_scale',2,'amount_units',$5::numeric,'usd_units',$6::numeric,'fx_rate_id',$7::text,'used_latest_fallback',$8::boolean,'captured_at',$9::bigint,'original_source_type',NULL,'original_source_id',NULL,'original_amount_kind',NULL,'carried_source_type',NULL,'carried_source_id',NULL,'carried_amount_kind',NULL,'carried_allocation_offset_units',NULL)::text,'UTF8')),'hex')")
            .bind(self.company.as_str()).bind(&self.id).bind(&self.evidence.transaction_date).bind(&self.evidence.currency).bind(&self.evidence.amount_units).bind(&self.evidence.usd_units).bind(&self.evidence.fx_rate_id).bind(self.evidence.used_latest_fallback).bind(self.captured_at).fetch_one(&mut *c).await?;
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)VALUES($1,'field_service_agreement',$2,'visit_rate',$3::date,$4,2,$5::numeric,$6::numeric,$7,$8,$9)")
            .bind(self.company.as_str()).bind(&self.id).bind(&self.evidence.transaction_date).bind(&self.evidence.currency).bind(&self.evidence.amount_units).bind(&self.evidence.usd_units).bind(&self.evidence.fx_rate_id).bind(self.evidence.used_latest_fallback).bind(self.captured_at).execute(&mut *c).await?;
        let captured = image(c, &self.company, &self.id)
            .await?
            .ok_or_else(changed)?;
        if captured.evidence != self.evidence || captured.digest != expected {
            return Err(changed());
        }
        let retained = RetainedAgreementMoney {
            plan: self,
            image: captured,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}
pub struct RetainedAgreementMoney {
    plan: PreparedAgreementMoney,
    image: Image,
}
impl RetainedAgreementMoney {
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
pub struct CurrentAgreementMoney {
    unit: Unit,
    company: CompanyId,
    id: String,
    image: Image,
}
impl CurrentAgreementMoney {
    pub fn evidence(&self) -> &AgreementMoneyEvidence {
        &self.image.evidence
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || image(c, &self.company, &self.id).await?.as_ref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_agreement_money(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<CurrentAgreementMoney>, Error> {
    let unit = unit(c).await?;
    Ok(image(c, company, id)
        .await?
        .map(|image| CurrentAgreementMoney {
            unit,
            company: company.clone(),
            id: id.into(),
            image,
        }))
}
pub async fn prepare_agreement_money(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
    date: &str,
    currency: &str,
    amount: i64,
    clock: &impl Clock,
) -> Result<PreparedAgreementMoney, Error> {
    let unit = unit(c).await?;
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
            "The Agreement currency must use two decimal places.",
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
            Ok(ManualRate {
                id: r["id"].as_str().ok_or_else(changed)?.into(),
                currency: r["currency"].as_str().ok_or_else(changed)?.into(),
                effective_date: r["effective_date"].as_str().ok_or_else(changed)?.into(),
                currency_per_usd: r["currency_per_usd"].as_str().ok_or_else(changed)?.into(),
                confirmed_at: r["confirmed_at"].as_i64().ok_or_else(changed)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let collation = FinanceCollation::english().map_err(|_| changed())?;
    let rate = resolve_manual_rate(&rates, currency, date, &collation).ok_or_else(|| {
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
    let at = clock.now().get();
    if !(0..=9_007_199_254_740_991).contains(&at) {
        return Err(changed());
    }
    let evidence = AgreementMoneyEvidence {
        transaction_date: date.into(),
        currency: currency.into(),
        currency_scale: 2,
        amount_units: amount.to_string(),
        usd_units: usd,
        fx_rate_id: (rate.rate.id != "USD").then_some(rate.rate.id),
        used_latest_fallback: rate.used_latest_fallback,
    };
    let plan = PreparedAgreementMoney {
        unit,
        company: company.clone(),
        id: id.into(),
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
        "FIELD_SERVICE_MONEY_CHANGED",
        "The original Agreement Money source changed.",
    )
    .into()
}
