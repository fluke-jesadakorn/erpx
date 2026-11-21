//! Native Finance controlled payment and receipt preparation for open Company
//! invoices that are not Field Service releases. The outer command owns current
//! IAM authority, purchase witnesses, the Workflow receipt and commit, exactly
//! as the Field-specific receipt path does; this adapter only prepares and
//! validates Finance-owned source facts.
use super::service_receipt_money::{
    PreparedServiceReceiptMoney, RetainedServiceReceiptMoney, prepare_service_receipt_money,
};
use crate::{
    application::journal::FinanceActor,
    domain::{money::units_to_decimal, service_receipt::service_receipt_amount},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericPaymentInput {
    pub kind: String,
    pub party_id: String,
    pub payment_number: String,
    pub payment_date: String,
    pub currency: String,
    pub amount: String,
    pub bank_account: String,
    pub settlement_invoice_id: String,
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original invoice changed. Refresh its source workflow.",
    )
    .into()
}
fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}
fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str, Error> {
    row.get(key).and_then(Value::as_str).ok_or_else(changed)
}
fn units(row: &Value, key: &str) -> Result<i64, Error> {
    row.get(key)
        .and_then(Value::as_i64)
        .filter(|value| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(value))
        .ok_or_else(changed)
}
async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("FINANCE_OWNER_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("FINANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}
/// Take the same Finance money/payment writer leases as the Field receipt path
/// before the authorizer's first snapshot. The generic source locks its own
/// invoice/party/account rows later with the existing `FOR SHARE` reads.
pub async fn lock_generic_payment_sources(c: &mut PgConnection) -> Result<(), Error> {
    super::service_receipt_money::lock_service_receipt_money_sources(c).await?;
    sqlx::query(
        "LOCK TABLE finance_payments,finance_invoices,finance_parties,finance_accounts,\
         finance_charts,finance_periods,finance_journals,finance_journal_lines,\
         finance_allocations,finance_bank_statements,finance_bank_transactions IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}
async fn invoice_image(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<Value, Error> {
    sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM finance_invoices i WHERE i.company_id=$1 AND i.id=$2 FOR SHARE",
    )
    .bind(actor.company_id.as_str())
    .bind(id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::invalid(
            "INVOICE_INVALID",
            "The selected invoice does not belong to this Company.",
        )
        .into()
    })
}
async fn master_images(
    c: &mut PgConnection,
    actor: &FinanceActor,
    input: &GenericPaymentInput,
) -> Result<(Value, Value), Error> {
    let party_kind = if input.kind == "receipt" {
        "customer"
    } else {
        "supplier"
    };
    let party: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_parties p WHERE p.id=$1 AND p.company_id=$2 \
         AND p.kind=$3 AND p.currency=$4 AND p.status='active' AND p.person_id IS NULL FOR SHARE",
    )
    .bind(&input.party_id)
    .bind(actor.company_id.as_str())
    .bind(party_kind)
    .bind(&input.currency)
    .fetch_optional(&mut *c)
    .await?;
    let bank: Option<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(a) FROM finance_accounts a WHERE a.id=$1 AND a.company_id=$2 \
         AND a.category='asset' AND a.control_kind='bank' AND a.currency=$3 AND a.status='active' FOR SHARE",
    )
    .bind(&input.bank_account)
    .bind(actor.company_id.as_str())
    .bind(&input.currency)
    .fetch_optional(&mut *c)
    .await?;
    Ok((
        party.ok_or_else(|| {
            invalid(
                "PAYMENT_SETUP_REQUIRED",
                "Select the customer or supplier and bank ledger account.",
            )
        })?,
        bank.ok_or_else(|| {
            invalid(
                "BANK_ACCOUNT_INVALID",
                "The selected ledger account is not an active bank control account in this currency.",
            )
        })?,
    ))
}
async fn payment_image(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<Option<Value>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_payments p WHERE p.id=$1 AND p.company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(actor.company_id.as_str())
    .fetch_optional(c)
    .await?)
}
fn expected_payment(
    actor: &FinanceActor,
    id: &str,
    input: &GenericPaymentInput,
    amount: i64,
    at: i64,
) -> Value {
    json!({
        "id": id,
        "company_id": actor.company_id.as_str(),
        "party_id": input.party_id,
        "kind": input.kind,
        "payment_number": input.payment_number,
        "payment_date": input.payment_date,
        "currency": input.currency,
        "amount_minor": amount,
        "outstanding_minor": amount,
        "bank_account": input.bank_account,
        "status": "draft",
        "journal_id": null,
        "version": 1,
        "created_by": actor.user_id.as_str(),
        "approved_by": null,
        "released_by": null,
        "created_at": at,
        "updated_at": at,
        "payment_slip_id": null,
        "fx_rate_micros": null,
        "base_amount_minor": null,
        "fx_journal_id": null,
        "withholding_assessment_id": null,
        "withheld_minor": 0
    })
}
pub struct PreparedGenericPayment {
    actor: FinanceActor,
    id: String,
    input: GenericPaymentInput,
    amount: i64,
    at: i64,
    invoice: Value,
    masters: (Value, Value),
    money: PreparedServiceReceiptMoney,
}
impl PreparedGenericPayment {
    pub fn input(&self) -> &GenericPaymentInput {
        &self.input
    }
    pub fn amount(&self) -> i64 {
        self.amount
    }
    pub fn result(&self) -> Result<Value, BusinessError> {
        let amount = units_to_decimal(&self.amount.into(), 2).map_err(|_| {
            BusinessError::invalid("MONEY_OVERFLOW", "Money exceeds supported precision.")
        })?;
        Ok(json!({
            "id": self.id,
            "amount": amount,
            "outstanding": amount,
            "status": "draft",
            "version": 1,
            "settlementInvoiceId": self.input.settlement_invoice_id
        }))
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if invoice_image(c, &self.actor, &self.input.settlement_invoice_id).await? != self.invoice
            || master_images(c, &self.actor, &self.input).await? != self.masters
            || payment_image(c, &self.actor, &self.id).await?.is_some()
        {
            return Err(changed());
        }
        self.money.require_before(c).await
    }
    pub async fn persist(self, c: &mut PgConnection) -> Result<RetainedGenericPayment, Error> {
        self.require_before(c).await?;
        let expected = expected_payment(&self.actor, &self.id, &self.input, self.amount, self.at);
        let money = self.money.persist(c).await?;
        sqlx::query(
            "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,\
             currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at)\
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$11)",
        )
        .bind(&self.id)
        .bind(self.actor.company_id.as_str())
        .bind(&self.input.party_id)
        .bind(&self.input.kind)
        .bind(&self.input.payment_number)
        .bind(&self.input.payment_date)
        .bind(&self.input.currency)
        .bind(self.amount)
        .bind(&self.input.bank_account)
        .bind(self.actor.user_id.as_str())
        .bind(self.at)
        .execute(&mut *c)
        .await?;
        let retained = RetainedGenericPayment {
            actor: self.actor,
            id: self.id,
            input: self.input,
            invoice: self.invoice,
            masters: self.masters,
            money,
            expected,
        };
        retained.require_current(c).await?;
        super::service_receipt_runtime::mark(
            c,
            retained.actor.company_id.as_str(),
            &retained.id,
            &[],
        )
        .await?;
        Ok(retained)
    }
}
pub struct RetainedGenericPayment {
    actor: FinanceActor,
    id: String,
    input: GenericPaymentInput,
    invoice: Value,
    masters: (Value, Value),
    money: RetainedServiceReceiptMoney,
    expected: Value,
}
impl RetainedGenericPayment {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if invoice_image(c, &self.actor, &self.input.settlement_invoice_id).await? != self.invoice
            || master_images(c, &self.actor, &self.input).await? != self.masters
            || self.money.require_current(c).await.is_err()
            || payment_image(c, &self.actor, &self.id).await?.as_ref() != Some(&self.expected)
        {
            return Err(changed());
        }
        Ok(())
    }
    pub fn result(&self) -> Result<Value, BusinessError> {
        let amount = units_to_decimal(&self.amount().into(), 2).map_err(|_| {
            BusinessError::invalid("MONEY_OVERFLOW", "Money exceeds supported precision.")
        })?;
        Ok(json!({
            "id": self.id,
            "amount": amount,
            "outstanding": amount,
            "status": "draft",
            "version": 1,
            "settlementInvoiceId": self.input.settlement_invoice_id
        }))
    }
    fn amount(&self) -> i64 {
        self.expected
            .get("amount_minor")
            .and_then(Value::as_i64)
            .unwrap_or_default()
    }
}
fn validate_invoice(
    kind: &str,
    invoice: &Value,
    party: &str,
    currency: &str,
    amount: i64,
) -> Result<(), Error> {
    let expected_kind = if kind == "receipt" {
        "receivable"
    } else {
        "payable"
    };
    let status = text(invoice, "status")?;
    let source = text(invoice, "source_type")?;
    if text(invoice, "party_id")? != party
        || text(invoice, "currency")? != currency
        || text(invoice, "kind")? != expected_kind
        || !matches!(status, "posted" | "partially_paid")
        || source == "expense_claim"
        || units(invoice, "outstanding_minor")? < amount
    {
        return Err(invalid(
            "PAYMENT_SETTLEMENT_SOURCE_INVALID",
            "Choose an open invoice for the same party, currency and payment amount.",
        ));
    }
    Ok(())
}
pub async fn prepare_generic_payment(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    input: &GenericPaymentInput,
    clock: &impl Clock,
) -> Result<PreparedGenericPayment, Error> {
    serializable(c).await?;
    if !matches!(input.kind.as_str(), "payment" | "receipt") {
        return Err(invalid(
            "PAYMENT_KIND_UNAVAILABLE",
            "Only controlled customer receipts and supplier payments are available.",
        ));
    }
    if input.party_id.is_empty()
        || input.bank_account.is_empty()
        || input.settlement_invoice_id.is_empty()
        || input.payment_number.trim().is_empty()
    {
        return Err(invalid(
            "PAYMENT_SETUP_REQUIRED",
            "Select the customer or supplier, bank ledger account and open invoice.",
        ));
    }
    let amount = service_receipt_amount(&input.amount)?;
    let mut input = input.clone();
    input.currency = input.currency.to_uppercase();
    let invoice = invoice_image(c, actor, &input.settlement_invoice_id).await?;
    validate_invoice(
        &input.kind,
        &invoice,
        &input.party_id,
        &input.currency,
        amount,
    )?;
    let masters = master_images(c, actor, &input).await?;
    let money = prepare_service_receipt_money(
        c,
        &actor.company_id,
        id,
        &input.payment_date,
        &input.currency,
        amount,
        clock,
    )
    .await?;
    let at = clock.now().get();
    if !(0..=9_007_199_254_740_991).contains(&at) {
        return Err(changed());
    }
    let plan = PreparedGenericPayment {
        actor: actor.clone(),
        id: id.into(),
        input,
        amount,
        at,
        invoice,
        masters,
        money,
    };
    plan.require_before(c).await?;
    Ok(plan)
}
