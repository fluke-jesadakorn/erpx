//! Exact Field controlled-receipt bank import, staged on the caller-owned unit.
use super::{
    postgres_inventory_consumption::{Quote, quote},
    postgres_journal_facts::embedded_currency_rules,
    service_receipt_review::{CurrentServiceReceiptReview, read_service_receipt_review},
};
use crate::{
    application::journal::FinanceActor,
    domain::{
        company_money::{FinanceCollation, ManualRate, functional_fx_evidence},
        journal::{JournalError, convert_at_fx, functional_fx_micros, journal_minor},
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptBankRow {
    pub external_id: String,
    pub transaction_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub amount: String,
    pub currency: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptBankInput {
    pub settlement_payment_id: String,
    pub bank_account: String,
    pub statement_number: String,
    pub statement_date: String,
    pub opening: String,
    pub closing: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closing_fx_rate: Option<String>,
    pub source_hash: String,
    pub rows: Vec<ServiceReceiptBankRow>,
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
    .into()
}
fn bank_source() -> Error {
    BusinessError::invalid("SERVICE_PAYMENT_BANK_SOURCE_INVALID","Import one exact bank receipt for the released Service payment's account, currency, date and amount.").into()
}
fn journal_error(e: JournalError) -> Error {
    BusinessError::new(
        e.kind,
        e.code,
        match e.code {
            "INVALID_MONEY" => "Money must be a decimal string.",
            "MONEY_OVERFLOW" => "Money amount is too large.",
            "MANUAL_RATE_REQUIRED" => "Confirm company manual USD rates before posting.",
            _ => "Use a valid confirmed functional currency rate.",
        },
    )
    .into()
}
/// Call before any snapshot in the owning composition, after its shared Money
/// and payment writer leases. This also serializes cross-statement external IDs.
pub async fn lock_service_receipt_bank_sources(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query(
        "LOCK TABLE finance_bank_statements,finance_bank_transactions IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
async fn masters(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<(Value, Value, bool), Error> {
    let a:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM finance_accounts a WHERE id=$1 AND company_id=$2 AND category='asset' AND control_kind IN ('none','bank') AND status='active' FOR SHARE").bind(id).bind(actor.company_id.as_str()).fetch_optional(&mut *c).await?;
    let a=a.ok_or_else(||BusinessError::invalid("BANK_ACCOUNT_INVALID","Select an active cash or bank account from the primary book. Secondary books use the same business statement."))?;
    let chart = a["chart_id"].as_str().ok_or_else(changed)?;
    let b:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(c) FROM finance_charts c WHERE id=$1 AND company_id=$2 AND status='active' FOR SHARE").bind(chart).bind(actor.company_id.as_str()).fetch_optional(&mut *c).await?;
    let secondary:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_book_rules WHERE company_id=$1 AND secondary_chart_id=$2 AND status='approved')").bind(actor.company_id.as_str()).bind(chart).fetch_one(&mut *c).await?;
    Ok((a, b.ok_or_else(changed)?, secondary))
}
async fn duplicate(
    c: &mut PgConnection,
    actor: &FinanceActor,
    input: &ServiceReceiptBankInput,
    digest: &str,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=$1 AND bank_account=$2 AND (statement_number=$3 OR source_hash=$4)) OR EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=$1 AND s.bank_account=$2 AND t.external_id=$5)")
 .bind(actor.company_id.as_str()).bind(&input.bank_account).bind(&input.statement_number).bind(digest).bind(&input.rows[0].external_id).fetch_one(c).await?)
}
pub struct PreparedServiceReceiptBank {
    actor: FinanceActor,
    input: ServiceReceiptBankInput,
    key: String,
    source: CurrentServiceReceiptReview,
    masters: (Value, Value, bool),
    quotes: (Quote, Quote),
    opening: i64,
    closing: i64,
    amount: i64,
    rate: i64,
    base_closing: i64,
    digest: String,
    at: i64,
}
pub struct RetainedServiceReceiptBank {
    plan: PreparedServiceReceiptBank,
    master_after: (Value, Value, bool),
    statement: Value,
    row: Value,
}
impl PreparedServiceReceiptBank {
    pub fn statement_id(&self) -> String {
        format!("statement-{}", self.key)
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_current(c).await?;
        if masters(c, &self.actor, &self.input.bank_account).await? != self.masters
            || quote(
                c,
                &self.actor.company_id,
                &self.masters.0["currency"].as_str().ok_or_else(changed)?,
            )
            .await?
                != self.quotes.0
            || quote(
                c,
                &self.actor.company_id,
                self.masters.1["base_currency"]
                    .as_str()
                    .ok_or_else(changed)?,
            )
            .await?
                != self.quotes.1
            || duplicate(c, &self.actor, &self.input, &self.digest).await?
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn persist(self, c: &mut PgConnection) -> Result<RetainedServiceReceiptBank, Error> {
        self.require_before(c).await?;
        let id = self.statement_id();
        let row = &self.input.rows[0];
        let statement = json!({"id":id,"company_id":self.actor.company_id.as_str(),"bank_account":self.input.bank_account,"statement_number":self.input.statement_number,"statement_date":self.input.statement_date,"opening_minor":self.opening,"closing_minor":self.closing,"status":"imported","source_hash":self.digest,"version":1,"imported_by":self.actor.user_id.as_str(),"created_at":self.at,"closing_fx_rate_micros":self.rate,"base_closing_minor":self.base_closing});
        let expected_row = json!({"id":format!("bankrow-{}-0",self.key),"statement_id":id,"company_id":self.actor.company_id.as_str(),"transaction_date":row.transaction_date,"external_id":row.external_id,"description":row.description.as_deref().unwrap_or(""),"amount_minor":self.amount,"currency":row.currency.to_uppercase(),"match_status":"unmatched","matched_type":null,"matched_id":null,"version":1});
        let mut after = self.masters.clone();
        if after.0["control_kind"] == "none" {
            after.0["control_kind"] = json!("bank");
            after.0["is_control"] = json!(1);
            after.0["version"] = json!(
                after.0["version"]
                    .as_i64()
                    .and_then(|v| v.checked_add(1))
                    .ok_or_else(changed)?
            );
            after.0["updated_at"] = json!(self.at);
        }
        sqlx::query("UPDATE finance_accounts SET is_control=1,control_kind='bank',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND control_kind='none'").bind(self.at).bind(&self.input.bank_account).bind(self.actor.company_id.as_str()).execute(&mut *c).await?;
        sqlx::query("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(&id).bind(self.actor.company_id.as_str()).bind(&self.input.bank_account).bind(&self.input.statement_number).bind(&self.input.statement_date).bind(self.opening).bind(self.closing).bind(self.rate).bind(self.base_closing).bind(&self.digest).bind(self.actor.user_id.as_str()).bind(self.at).execute(&mut *c).await?;
        sqlx::query("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency)VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(format!("bankrow-{}-0",self.key)).bind(&id).bind(self.actor.company_id.as_str()).bind(&row.transaction_date).bind(&row.external_id).bind(row.description.as_deref().unwrap_or("")).bind(self.amount).bind(row.currency.to_uppercase()).execute(&mut *c).await?;
        let retained = RetainedServiceReceiptBank {
            plan: self,
            master_after: after,
            statement,
            row: expected_row,
        };
        retained.require_current(c).await?;
        super::service_receipt_runtime::mark(
            c,
            retained.plan.actor.company_id.as_str(),
            &retained.plan.input.settlement_payment_id,
            &[format!("bankrow-{}-0", retained.plan.key)],
        )
        .await?;
        Ok(retained)
    }
}
impl RetainedServiceReceiptBank {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let p = &self.plan;
        p.source.require_current(c).await?;
        let statement:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(s) FROM finance_bank_statements s WHERE id=$1 AND company_id=$2 FOR SHARE").bind(p.statement_id()).bind(p.actor.company_id.as_str()).fetch_optional(&mut *c).await?;
        let row:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(t) FROM finance_bank_transactions t WHERE id=$1 AND company_id=$2 FOR SHARE").bind(format!("bankrow-{}-0",p.key)).bind(p.actor.company_id.as_str()).fetch_optional(&mut *c).await?;
        if statement.as_ref() != Some(&self.statement)
            || row.as_ref() != Some(&self.row)
            || masters(c, &p.actor, &p.input.bank_account).await? != self.master_after
            || quote(
                c,
                &p.actor.company_id,
                p.masters.0["currency"].as_str().ok_or_else(changed)?,
            )
            .await?
                != p.quotes.0
            || quote(
                c,
                &p.actor.company_id,
                p.masters.1["base_currency"].as_str().ok_or_else(changed)?,
            )
            .await?
                != p.quotes.1
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn prepare_service_receipt_bank(
    c: &mut PgConnection,
    actor: &FinanceActor,
    key: &str,
    input: &ServiceReceiptBankInput,
    clock: &impl Clock,
) -> Result<PreparedServiceReceiptBank, Error> {
    let source = read_service_receipt_review(c, actor, &input.settlement_payment_id).await?;
    let payment = source.row();
    if payment["status"] != "released"
        || input.rows.len() != 1
        || payment["bank_account"].as_str() != Some(input.bank_account.as_str())
    {
        return Err(bank_source());
    }
    let row = &input.rows[0];
    let amount = journal_minor(&row.amount, 2).map_err(journal_error)?;
    if payment["currency"].as_str() != Some(row.currency.to_uppercase().as_str())
        || payment["amount_minor"].as_i64() != Some(amount)
        || row.transaction_date.as_str() < payment["payment_date"].as_str().ok_or_else(changed)?
    {
        return Err(bank_source());
    }
    let m = masters(c, actor, &input.bank_account).await?;
    if m.2 {
        return Err(BusinessError::invalid("BANK_ACCOUNT_INVALID","Select an active cash or bank account from the primary book. Secondary books use the same business statement.").into());
    }
    let currency = m.0["currency"].as_str().ok_or_else(changed)?;
    let base = m.1["base_currency"].as_str().ok_or_else(changed)?;
    let quotes = (
        quote(c, &actor.company_id, currency).await?,
        quote(c, &actor.company_id, base).await?,
    );
    let rules = embedded_currency_rules().map_err(|_| changed())?;
    for (currency, q) in [(currency, &quotes.0), (base, &quotes.1)] {
        if rules.scale(currency, q.scale).map_err(|_| {
            BusinessError::invalid(
                "CURRENCY_SCALE_REQUIRED",
                "Configure the currency's minor units.",
            )
        })? != 2
        {
            return Err(BusinessError::invalid("FINANCE_CURRENCY_SCALE_UNSUPPORTED","This Finance ledger currently accepts currencies with two minor-unit decimals. Historical amounts retain their recorded scale; use a supported currency for a new document.").into());
        }
    }
    let opening = journal_minor(&input.opening, 2).map_err(journal_error)?;
    let closing = journal_minor(&input.closing, 2).map_err(journal_error)?;
    if i128::from(opening) + i128::from(amount) != i128::from(closing) {
        return Err(BusinessError::invalid(
            "STATEMENT_UNBALANCED",
            "Opening balance plus movements must equal the closing balance.",
        )
        .into());
    }
    if row.currency.to_uppercase() != currency || row.transaction_date > input.statement_date {
        return Err(BusinessError::invalid(
            "STATEMENT_ROW_INVALID",
            "Rows must use the bank account currency and cannot follow the statement date.",
        )
        .into());
    }
    let rate = if let Some(explicit) = input.closing_fx_rate.as_deref().filter(|v| !v.is_empty()) {
        functional_fx_micros(currency, base, Some(explicit)).map_err(journal_error)?
    } else {
        let rates = quotes
            .0
            .rates
            .iter()
            .chain(quotes.1.rates.iter())
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
        functional_fx_evidence(
            &rates,
            currency,
            base,
            &input.statement_date,
            &FinanceCollation::english().map_err(|_| changed())?,
        )
        .map_err(journal_error)?
        .micros
    };
    let base_closing = convert_at_fx(closing, rate).map_err(journal_error)?;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fingerprint<'a> {
        bank_account: &'a str,
        statement_number: &'a str,
        opening: i64,
        closing: i64,
        closing_fx_rate: i64,
        rows: &'a [ServiceReceiptBankRow],
    }
    let wire = CommandJson::from_serializable(&Fingerprint {
        bank_account: &input.bank_account,
        statement_number: &input.statement_number,
        opening,
        closing,
        closing_fx_rate: rate,
        rows: &input.rows,
    })?;
    let digest = format!("{:x}", Sha256::digest(wire.as_str().as_bytes()));
    let plan = PreparedServiceReceiptBank {
        actor: actor.clone(),
        input: input.clone(),
        key: key.into(),
        source,
        masters: m,
        quotes,
        opening,
        closing,
        amount,
        rate,
        base_closing,
        digest,
        at: clock.now().get(),
    };
    plan.require_before(c).await?;
    Ok(plan)
}
