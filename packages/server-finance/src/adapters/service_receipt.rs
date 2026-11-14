//! Finance-owned controlled receipt creation. The outer native command retains
//! current IAM, purchase and Field lineage and owns commit/replay/audit/outbox.
use crate::{
    adapters::{
        field_service_invoice_read::{
            CurrentFinanceServiceInvoiceWorkspace, lock_service_invoice_sources,
            read_service_invoice_workspace,
        },
        service_receipt_money::{
            PreparedServiceReceiptMoney, RetainedServiceReceiptMoney,
            lock_service_receipt_money_sources, prepare_service_receipt_money,
        },
    },
    application::journal::FinanceActor,
    domain::service_receipt::*,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptCreateInput {
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
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
    .into()
}
/// Acquire writer leases before the first source snapshot; a SHARE read then
/// payment/snapshot upgrade must not deadlock across different Companies.
pub async fn lock_service_receipt_sources(c: &mut PgConnection) -> Result<(), Error> {
    lock_service_receipt_money_sources(c).await?;
    sqlx::query("LOCK TABLE finance_payments IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    lock_service_invoice_sources(c).await
}
async fn master_images(
    c: &mut PgConnection,
    actor: &FinanceActor,
    input: &ServiceReceiptCreateInput,
) -> Result<(Value, Value), Error> {
    let party:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(p) FROM finance_parties p WHERE id=$1 AND company_id=$2 AND kind='customer' AND currency=$3 AND status='active' AND person_id IS NULL FOR SHARE")
        .bind(&input.party_id).bind(actor.company_id.as_str()).bind(&input.currency).fetch_optional(&mut *c).await?;
    let bank:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM finance_accounts a WHERE id=$1 AND company_id=$2 AND category='asset' AND control_kind='bank' AND currency=$3 AND status='active' FOR SHARE")
        .bind(&input.bank_account).bind(actor.company_id.as_str()).bind(&input.currency).fetch_optional(&mut *c).await?;
    Ok((party.ok_or_else(changed)?, bank.ok_or_else(changed)?))
}
async fn payment_image(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<Option<Value>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_payments p WHERE id=$1 AND company_id=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(actor.company_id.as_str())
    .fetch_optional(c)
    .await?)
}
pub struct PreparedServiceReceiptCreate {
    actor: FinanceActor,
    id: String,
    input: ServiceReceiptCreateInput,
    amount: i64,
    at: i64,
    invoice: CurrentFinanceServiceInvoiceWorkspace,
    masters: (Value, Value),
    money: PreparedServiceReceiptMoney,
}
impl PreparedServiceReceiptCreate {
    pub fn input(&self) -> &ServiceReceiptCreateInput {
        &self.input
    }
    pub fn amount(&self) -> i64 {
        self.amount
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.invoice.require_current(c).await?;
        self.money.require_before(c).await?;
        if master_images(c, &self.actor, &self.input).await? != self.masters
            || payment_image(c, &self.actor, &self.id).await?.is_some()
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn persist(
        self,
        c: &mut PgConnection,
    ) -> Result<RetainedServiceReceiptCreate, Error> {
        self.require_before(c).await?;
        let expected = json!({"id":self.id,"company_id":self.actor.company_id.as_str(),"party_id":self.input.party_id,"kind":"receipt","payment_number":self.input.payment_number,"payment_date":self.input.payment_date,"currency":self.input.currency,"amount_minor":self.amount,"outstanding_minor":self.amount,"bank_account":self.input.bank_account,"status":"draft","journal_id":null,"version":1,"created_by":self.actor.user_id.as_str(),"approved_by":null,"released_by":null,"created_at":self.at,"updated_at":self.at,"payment_slip_id":null,"fx_rate_micros":null,"base_amount_minor":null,"fx_journal_id":null,"withholding_assessment_id":null,"withheld_minor":0});
        let money = self.money.persist(c).await?;
        sqlx::query("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at)VALUES($1,$2,$3,'receipt',$4,$5,$6,$7,$7,$8,$9,$10,$10)")
            .bind(&self.id).bind(self.actor.company_id.as_str()).bind(&self.input.party_id).bind(&self.input.payment_number).bind(&self.input.payment_date).bind(&self.input.currency).bind(self.amount).bind(&self.input.bank_account).bind(self.actor.user_id.as_str()).bind(self.at).execute(&mut *c).await?;
        let retained = RetainedServiceReceiptCreate {
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
pub struct RetainedServiceReceiptCreate {
    actor: FinanceActor,
    id: String,
    input: ServiceReceiptCreateInput,
    invoice: CurrentFinanceServiceInvoiceWorkspace,
    masters: (Value, Value),
    money: RetainedServiceReceiptMoney,
    expected: Value,
}
impl RetainedServiceReceiptCreate {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.invoice.require_current(c).await?;
        self.money.require_current(c).await?;
        if master_images(c, &self.actor, &self.input).await? != self.masters
            || payment_image(c, &self.actor, &self.id).await?.as_ref() != Some(&self.expected)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn prepare_service_receipt_create(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    input: &ServiceReceiptCreateInput,
    clock: &impl Clock,
) -> Result<PreparedServiceReceiptCreate, Error> {
    if input.party_id.is_empty() || input.bank_account.is_empty() {
        return Err(BusinessError::invalid(
            "PAYMENT_SETUP_REQUIRED",
            "Select the customer or supplier and bank ledger account.",
        )
        .into());
    }
    if input.kind != "receipt" {
        return Err(BusinessError::invalid(
            "PAYMENT_SETTLEMENT_SOURCE_INVALID",
            "Choose an open invoice for the same party, currency and payment amount.",
        )
        .into());
    }
    let amount = service_receipt_amount(&input.amount)?;
    let mut input = input.clone();
    input.currency = input.currency.to_uppercase();
    let invoice = read_service_invoice_workspace(c, actor, &input.settlement_invoice_id).await?;
    let row = &invoice.data().invoice;
    let text = |k| row.get(k).and_then(Value::as_str).ok_or_else(changed);
    validate_service_receipt_invoice(
        &ServiceReceiptInvoiceFacts {
            party: text("party_id")?,
            currency: text("currency")?,
            kind: text("kind")?,
            status: text("status")?,
            source_type: text("source_type")?,
            outstanding: row
                .get("outstanding_minor")
                .and_then(Value::as_i64)
                .ok_or_else(changed)?,
        },
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
    let plan = PreparedServiceReceiptCreate {
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
