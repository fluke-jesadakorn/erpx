//! Field Service billing's Finance-owned draft invoice and Money effects.
//! Facts are data, not authority: the caller must retain the real Service owner proof.
use super::{
    postgres_inventory_consumption::{Quote, quote},
    postgres_journal_facts::embedded_currency_rules,
};
use crate::domain::company_money::{
    FinanceCollation, ManualRate, convert_currency_units, resolve_manual_rate,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, TimestampMillis, UserId,
};
use num_bigint::BigInt;
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingInvoiceInput {
    pub company_id: CompanyId,
    pub owner_key: CommandKey,
    pub order_id: String,
    pub customer_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub invoice_date: String,
    pub due_date: String,
    pub actor_id: UserId,
    pub created_at: TimestampMillis,
}
/// The implementation must retain actual Service order, actor, and release
/// proofs in the same transaction; scalar input is never an authorization.
pub trait PgBillingOwnerSource: Sync {
    fn input(&self) -> &BillingInvoiceInput;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn require_after(&self, c: &mut PgConnection)
    -> impl Future<Output = Result<(), Error>> + Send;
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillingInvoiceEvidence {
    pub invoice_id: String,
    pub release_id: String,
    pub currency: String,
    pub amount_units: String,
    pub usd_units: String,
    pub fx_rate_id: Option<String>,
    pub used_latest_fallback: bool,
}
#[derive(Clone, Eq, PartialEq)]
struct Accounts {
    debit: String,
    credit: String,
    digest: String,
}
async fn accounts(
    c: &mut PgConnection,
    i: &BillingInvoiceInput,
) -> Result<Option<Accounts>, Error> {
    let row=sqlx::query("SELECT CASE WHEN b.bounded THEN ar.id END debit,CASE WHEN b.bounded THEN rev.id END credit,b.bounded,encode(sha256(convert_to(jsonb_build_array(to_jsonb(ar),to_jsonb(rev),to_jsonb(ch))::text,'UTF8')),'hex') digest FROM finance_accounts ar JOIN finance_charts ch ON ch.id=ar.chart_id AND ch.company_id=ar.company_id JOIN finance_accounts rev ON rev.chart_id=ar.chart_id AND rev.company_id=ar.company_id CROSS JOIN LATERAL (SELECT octet_length(ar.id)::bigint+octet_length(rev.id)::bigint<=2097152 AS bounded) b WHERE ar.company_id=$1 AND ar.status='active' AND ar.control_kind='receivable' AND ch.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules secondary WHERE secondary.secondary_chart_id=ch.id AND secondary.company_id=ch.company_id AND secondary.status='approved') AND rev.status='active' AND rev.category='revenue' AND ar.currency=$2 AND rev.currency=$2 AND ch.base_currency=$2 ORDER BY ar.code,rev.code,ch.id,ar.id,rev.id LIMIT 1 FOR SHARE OF ar,rev,ch").bind(i.company_id.as_str()).bind(&i.currency).fetch_optional(c).await?;
    let Some(r) = row else { return Ok(None) };
    if !r.try_get::<bool, _>("bounded")? {
        return Err(changed());
    }
    Ok(Some(Accounts {
        debit: r
            .try_get::<Option<String>, _>("debit")?
            .ok_or_else(changed)?,
        credit: r
            .try_get::<Option<String>, _>("credit")?
            .ok_or_else(changed)?,
        digest: r.try_get("digest")?,
    }))
}
pub async fn lock_billing_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE finance_accounts,finance_charts,finance_book_rules,company_currency_scales,company_fx_rates IN SHARE MODE").execute(&mut *c).await?;
    sqlx::query("LOCK TABLE finance_invoices,money_transaction_snapshots IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
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
        "FIELD_SERVICE_BILLING_FINANCE_CHANGED",
        "The original Service billing Finance source changed.",
    )
    .into()
}
async fn digest(
    c: &mut PgConnection,
    table: &str,
    company: &CompanyId,
    id: &str,
    kind: &str,
) -> Result<Option<String>, Error> {
    match table {
      "invoice"=>Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(i)::text,'UTF8')),'hex') FROM finance_invoices i WHERE i.company_id=$1 AND i.id=$2 FOR SHARE OF i").bind(company.as_str()).bind(id).fetch_optional(c).await?),
      "money"=>Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(m)::text,'UTF8')),'hex') FROM money_transaction_snapshots m WHERE m.company_id=$1 AND m.source_id=$2 AND m.source_type=$3 AND m.amount_kind=CASE WHEN $3='finance_invoice' THEN 'draft:v1' ELSE 'invoice_total' END FOR SHARE OF m").bind(company.as_str()).bind(id).bind(kind).fetch_optional(c).await?),
      _=>Err(changed()),
    }
}
pub struct PreparedBillingInvoice {
    unit: Unit,
    input: BillingInvoiceInput,
    accounts: Accounts,
    quote: Quote,
    evidence: BillingInvoiceEvidence,
    money_at: i64,
}
pub struct RetainedBillingInvoice {
    plan: PreparedBillingInvoice,
    invoice_digest: String,
    release_money_digest: String,
    invoice_money_digest: String,
}
impl PreparedBillingInvoice {
    pub fn evidence(&self) -> &BillingInvoiceEvidence {
        &self.evidence
    }
    pub async fn require_before<S: PgBillingOwnerSource>(
        &self,
        c: &mut PgConnection,
        owner: &S,
    ) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || owner.input() != &self.input
            || accounts(c, &self.input).await?.as_ref() != Some(&self.accounts)
            || quote(c, &self.input.company_id, &self.input.currency).await? != self.quote
            || digest(
                c,
                "invoice",
                &self.input.company_id,
                &self.evidence.invoice_id,
                "",
            )
            .await?
            .is_some()
            || digest(
                c,
                "money",
                &self.input.company_id,
                &self.evidence.release_id,
                "field_service_billing_release",
            )
            .await?
            .is_some()
            || digest(
                c,
                "money",
                &self.input.company_id,
                &self.evidence.invoice_id,
                "finance_invoice",
            )
            .await?
            .is_some()
        {
            return Err(changed());
        }
        owner.require_before(c).await
    }
    pub async fn apply<S: PgBillingOwnerSource>(
        self,
        c: &mut PgConnection,
        owner: &S,
    ) -> Result<RetainedBillingInvoice, Error> {
        self.require_before(c, owner).await?;
        let expected_release = expected_money(c, &self, false).await?;
        let expected_invoice_money = expected_money(c, &self, true).await?;
        let expected_invoice = expected_invoice(c, &self).await?;
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at) VALUES($1,'field_service_billing_release',$2,'invoice_total',$3::date,$4,2,$5::numeric,$6::numeric,$7,$8,$9)").bind(self.input.company_id.as_str()).bind(&self.evidence.release_id).bind(&self.input.invoice_date).bind(&self.input.currency).bind(&self.evidence.amount_units).bind(&self.evidence.usd_units).bind(&self.evidence.fx_rate_id).bind(self.evidence.used_latest_fallback).bind(self.money_at).execute(&mut *c).await?;
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,created_by,created_at,updated_at) VALUES($1,$2,$3,'receivable',$4,$5,$6,$7,$8,0,$8,$8,$9,$10,'field_service',$11,'draft',$12,$13,$13)").bind(&self.evidence.invoice_id).bind(self.input.company_id.as_str()).bind(&self.input.customer_id).bind(format!("FS-{}",self.input.order_id)).bind(&self.input.invoice_date).bind(&self.input.due_date).bind(&self.input.currency).bind(self.input.amount_minor).bind(&self.accounts.debit).bind(&self.accounts.credit).bind(&self.input.order_id).bind(self.input.actor_id.as_str()).bind(self.input.created_at.get()).execute(&mut *c).await?;
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,carried_source_type,carried_source_id,carried_amount_kind) SELECT company_id,'finance_invoice',$1,'draft:v1',transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at,CASE WHEN amount_units>0 THEN source_type END,CASE WHEN amount_units>0 THEN source_id END,CASE WHEN amount_units>0 THEN amount_kind END FROM money_transaction_snapshots WHERE company_id=$2 AND source_type='field_service_billing_release' AND source_id=$3 AND amount_kind='invoice_total'").bind(&self.evidence.invoice_id).bind(self.input.company_id.as_str()).bind(&self.evidence.release_id).execute(&mut *c).await?;
        let retained = RetainedBillingInvoice {
            plan: self,
            invoice_digest: expected_invoice,
            release_money_digest: expected_release,
            invoice_money_digest: expected_invoice_money,
        };
        retained.require_finance(c).await?;
        owner.require_before(c).await?;
        Ok(retained)
    }
}
impl RetainedBillingInvoice {
    pub fn evidence(&self) -> &BillingInvoiceEvidence {
        &self.plan.evidence
    }
    async fn require_finance(&self, c: &mut PgConnection) -> Result<(), Error> {
        let p = &self.plan;
        if unit(c).await? != p.unit
            || accounts(c, &p.input).await?.as_ref() != Some(&p.accounts)
            || quote(c, &p.input.company_id, &p.input.currency).await? != p.quote
            || digest(
                c,
                "invoice",
                &p.input.company_id,
                &p.evidence.invoice_id,
                "",
            )
            .await?
            .as_deref()
                != Some(&self.invoice_digest)
            || digest(
                c,
                "money",
                &p.input.company_id,
                &p.evidence.release_id,
                "field_service_billing_release",
            )
            .await?
            .as_deref()
                != Some(&self.release_money_digest)
            || digest(
                c,
                "money",
                &p.input.company_id,
                &p.evidence.invoice_id,
                "finance_invoice",
            )
            .await?
            .as_deref()
                != Some(&self.invoice_money_digest)
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_current<S: PgBillingOwnerSource>(
        &self,
        c: &mut PgConnection,
        owner: &S,
    ) -> Result<(), Error> {
        if owner.input() != &self.plan.input {
            return Err(changed());
        }
        self.require_finance(c).await?;
        owner.require_after(c).await
    }
}
pub async fn prepare_billing_invoice<S: PgBillingOwnerSource>(
    c: &mut PgConnection,
    owner: &S,
    clock: &impl Clock,
) -> Result<PreparedBillingInvoice, Error> {
    let unit = unit(c).await?;
    owner.require_before(c).await?;
    let input = owner.input().clone();
    if !(0..=SAFE).contains(&input.amount_minor) || !(0..=SAFE).contains(&input.created_at.get()) {
        return Err(changed());
    }
    let accounts=accounts(c,&input).await?.ok_or_else(||BusinessError::invalid("FIELD_SERVICE_FINANCE_SETUP_REQUIRED","Configure an active receivable control account and revenue account in Finance before releasing field service billing."))?;
    let scale:Option<i64>=sqlx::query_scalar("SELECT scale::bigint FROM company_currency_scales WHERE company_id=$1 AND currency=$2 FOR SHARE").bind(input.company_id.as_str()).bind(&input.currency).fetch_optional(&mut *c).await?;
    let missing_scale = || {
        BusinessError::invalid(
            "CURRENCY_SCALE_REQUIRED",
            "Use a configured ISO currency scale.",
        )
    };
    let rules = embedded_currency_rules().map_err(|_| missing_scale())?;
    if rules
        .scale(&input.currency, scale)
        .map_err(|_| missing_scale())?
        != 2
    {
        return Err(BusinessError::invalid(
            "CURRENCY_SCALE_MISMATCH",
            "The Service invoice currency must use two decimal places.",
        )
        .into());
    }
    let quote = quote(c, &input.company_id, &input.currency).await?;
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
    let rate = resolve_manual_rate(
        &rates,
        &input.currency,
        &input.invoice_date,
        &FinanceCollation::english().map_err(|_| changed())?,
    )
    .ok_or_else(|| {
        BusinessError::invalid(
            "MANUAL_RATE_REQUIRED",
            "A confirmed manual currency/USD rate is required.",
        )
    })?;
    let usd = convert_currency_units(
        &BigInt::from(input.amount_minor),
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
    let money_at = clock.now().get();
    if !(0..=SAFE).contains(&money_at) {
        return Err(changed());
    }
    let evidence = BillingInvoiceEvidence {
        invoice_id: format!("field-invoice-{}", input.owner_key.as_str()),
        release_id: format!("field-billing-{}", input.owner_key.as_str()),
        currency: input.currency.clone(),
        amount_units: input.amount_minor.to_string(),
        usd_units: usd,
        fx_rate_id: (rate.rate.id != "USD").then_some(rate.rate.id),
        used_latest_fallback: rate.used_latest_fallback,
    };
    let plan = PreparedBillingInvoice {
        unit,
        input,
        accounts,
        quote,
        evidence,
        money_at,
    };
    plan.require_before(c, owner).await?;
    Ok(plan)
}
async fn expected_money(
    c: &mut PgConnection,
    p: &PreparedBillingInvoice,
    invoice: bool,
) -> Result<String, Error> {
    let id = if invoice {
        &p.evidence.invoice_id
    } else {
        &p.evidence.release_id
    };
    let source = if invoice {
        "finance_invoice"
    } else {
        "field_service_billing_release"
    };
    let kind = if invoice { "draft:v1" } else { "invoice_total" };
    let carried = invoice && p.input.amount_minor > 0;
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('company_id',$1::text,'source_type',$2::text,'source_id',$3::text,'amount_kind',$4::text,'transaction_date',$5::date,'currency',$6::text,'currency_scale',2,'amount_units',$7::numeric,'usd_units',$8::numeric,'fx_rate_id',$9::text,'used_latest_fallback',$10::boolean,'captured_at',$11::bigint,'original_source_type',NULL,'original_source_id',NULL,'original_amount_kind',NULL,'carried_source_type',CASE WHEN $12::boolean THEN 'field_service_billing_release'::text END,'carried_source_id',CASE WHEN $12::boolean THEN $13::text END,'carried_amount_kind',CASE WHEN $12::boolean THEN 'invoice_total'::text END,'carried_allocation_offset_units',NULL)::text,'UTF8')),'hex')").bind(p.input.company_id.as_str()).bind(source).bind(id).bind(kind).bind(&p.input.invoice_date).bind(&p.input.currency).bind(&p.evidence.amount_units).bind(&p.evidence.usd_units).bind(&p.evidence.fx_rate_id).bind(p.evidence.used_latest_fallback).bind(p.money_at).bind(carried).bind(&p.evidence.release_id).fetch_one(c).await?)
}
async fn expected_invoice(
    c: &mut PgConnection,
    p: &PreparedBillingInvoice,
) -> Result<String, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(jsonb_build_object('id',$1::text,'company_id',$2::text,'party_id',$3::text,'kind','receivable','invoice_number',$4::text,'invoice_date',$5::text,'due_date',$6::text,'currency',$7::text,'subtotal_minor',$8::bigint,'tax_minor',0,'total_minor',$8::bigint,'outstanding_minor',$8::bigint,'tax_code',NULL,'debit_account_id',$9::text,'credit_account_id',$10::text,'source_type','field_service','source_id',$11::text,'status','draft','journal_id',NULL,'version',1,'created_by',$12::text,'approved_by',NULL,'posted_at',NULL,'created_at',$13::bigint,'updated_at',$13::bigint,'tax_account_id',NULL,'fx_rate_micros',1000000,'base_total_minor',0,'base_outstanding_minor',0,'line_mode','historical_header','line_revision',1)::text,'UTF8')),'hex')").bind(&p.evidence.invoice_id).bind(p.input.company_id.as_str()).bind(&p.input.customer_id).bind(format!("FS-{}",p.input.order_id)).bind(&p.input.invoice_date).bind(&p.input.due_date).bind(&p.input.currency).bind(p.input.amount_minor).bind(&p.accounts.debit).bind(&p.accounts.credit).bind(&p.input.order_id).bind(p.input.actor_id.as_str()).bind(p.input.created_at.get()).fetch_one(c).await?)
}
