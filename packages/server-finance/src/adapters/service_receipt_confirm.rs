//! Finance-owned controlled receipt settlement. The outer composition retains
//! actual IAM/Field/Workflow/Files/Organization witnesses and commits the unit.
use super::{
    postgres_inventory_consumption::{Quote, quote},
    postgres_journal::PgJournalStore,
    service_receipt_money::{
        PreparedServiceReceiptMoney, ReceiptMoneyKind, RetainedServiceReceiptMoney,
        prepare_receipt_money_kind,
    },
    service_receipt_review::{CurrentServiceReceiptReview, read_service_receipt_review},
};
use crate::{
    application::journal::{FinanceActor, JournalDraft, JournalPreparation},
    domain::{
        company_money::{FinanceCollation, ManualRate, functional_fx_evidence},
        journal::{
            JournalError, JournalLineInput, convert_at_fx, functional_fx_micros, journal_minor,
        },
        money::units_to_decimal,
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptAllocationInput {
    pub invoice_id: String,
    pub amount: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReceiptConfirmInput {
    pub expected_version: i64,
    pub period_id: String,
    pub bank_transaction_id: String,
    pub bank_version: i64,
    pub bank_reference: String,
    pub proof: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_gain_account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fx_loss_account_id: Option<String>,
    pub allocations: Vec<ServiceReceiptAllocationInput>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_PAYMENT_SOURCE_CHANGED",
        "The controlled payment's original Service invoice changed. Refresh its source workflow.",
    )
    .into()
}
fn je(e: JournalError) -> Error {
    BusinessError::new(
        e.kind,
        e.code,
        "The controlled receipt ledger source is unavailable.",
    )
    .into()
}
fn n(v: &Value, k: &str) -> Result<i64, Error> {
    v[k].as_i64()
        .filter(|n| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(n))
        .ok_or_else(changed)
}
fn t<'a>(v: &'a Value, k: &str) -> Result<&'a str, Error> {
    v[k].as_str().ok_or_else(changed)
}
fn decimal(n: i64) -> Result<String, Error> {
    units_to_decimal(&n.into(), 2).map_err(|_| changed())
}
#[derive(Clone, PartialEq)]
struct Image {
    books: Vec<String>,
    payment: Value,
    invoice: Value,
    bank: Value,
    statement: Value,
    party: Value,
    accounts: Vec<Value>,
    chart: Value,
    allocations: Vec<Value>,
    money: Vec<String>,
}
async fn capture(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment: &str,
    invoice: &str,
    bank: &str,
    account_ids: &[String],
) -> Result<Image, Error> {
    let company = actor.company_id.as_str();
    let p: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_payments p WHERE company_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(company)
    .bind(payment)
    .fetch_one(&mut *c)
    .await?;
    let i: Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM finance_invoices i WHERE company_id=$1 AND id=$2 FOR UPDATE",
    )
    .bind(company)
    .bind(invoice)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::invalid(
            "INVOICE_INVALID",
            "Every selected invoice must belong to this company.",
        )
    })?;
    let b:Value=sqlx::query_scalar("SELECT to_jsonb(t) FROM finance_bank_transactions t WHERE company_id=$1 AND id=$2 FOR UPDATE").bind(company).bind(bank).fetch_optional(&mut *c).await?.ok_or_else(||BusinessError::invalid("SERVICE_PAYMENT_BANK_SOURCE_INVALID","Match the selected account, currency, date and exact bank amount."))?;
    let s: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM finance_bank_statements s WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(company)
    .bind(t(&b, "statement_id")?)
    .fetch_one(&mut *c)
    .await?;
    let party: Value = sqlx::query_scalar(
        "SELECT to_jsonb(p) FROM finance_parties p WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(company)
    .bind(t(&p, "party_id")?)
    .fetch_one(&mut *c)
    .await?;
    let accounts:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM finance_accounts a WHERE company_id=$1 AND id=ANY($2) ORDER BY id COLLATE \"C\" FOR SHARE").bind(company).bind(account_ids).fetch_all(&mut *c).await?;
    let bank_account = accounts
        .iter()
        .find(|a| a["id"] == p["bank_account"])
        .ok_or_else(|| {
            BusinessError::invalid(
                "BANK_ACCOUNT_INVALID",
                "The selected ledger account is not configured as a bank control account.",
            )
        })?;
    let chart: Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM finance_charts c WHERE company_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(company)
    .bind(t(bank_account, "chart_id")?)
    .fetch_one(&mut *c)
    .await?;
    let allocations:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a) FROM finance_allocations a WHERE payment_id=$1 OR (invoice_id=$2 AND company_id=$3) ORDER BY id COLLATE \"C\" FOR SHARE").bind(payment).bind(invoice).bind(company).fetch_all(&mut *c).await?;
    let money:Vec<String>=sqlx::query_scalar("SELECT to_jsonb(m)::text FROM money_transaction_snapshots m WHERE company_id=$1 AND ((source_type='finance_payment' AND source_id=$2) OR (source_type='finance_invoice' AND source_id=$3) OR (source_type='field_service_billing_release' AND source_id=$4)) ORDER BY to_jsonb(m)::text COLLATE \"C\" FOR SHARE")
 .bind(company).bind(payment).bind(invoice).bind(format!("field-billing-{}",invoice.strip_prefix("field-invoice-").ok_or_else(changed)?)).fetch_all(&mut *c).await?;
    let books:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(to_jsonb(r),(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY m.primary_account_id),'[]'::jsonb) FROM finance_book_account_mappings m WHERE m.rule_id=r.id))::text FROM finance_book_rules r WHERE company_id=$1 AND primary_chart_id=$2 ORDER BY r.id COLLATE \"C\" FOR SHARE").bind(company).bind(t(&chart,"id")?).fetch_all(&mut *c).await?;
    Ok(Image {
        books,
        payment: p,
        invoice: i,
        bank: b,
        statement: s,
        party,
        accounts,
        chart,
        allocations,
        money,
    })
}
async fn journal_image(c: &mut PgConnection, id: &str) -> Result<String, Error> {
    sqlx::query_scalar("SELECT jsonb_build_array(to_jsonb(j),(SELECT coalesce(jsonb_agg(to_jsonb(l) ORDER BY l.ordinal,l.id),'[]'::jsonb) FROM finance_journal_lines l WHERE l.journal_id=j.id),(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.amount_kind),'[]'::jsonb) FROM money_transaction_snapshots m WHERE m.company_id=j.company_id AND m.source_id=j.id))::text FROM finance_journals j WHERE id=$1 FOR SHARE").bind(id).fetch_optional(c).await?.ok_or_else(changed)
}
pub struct PreparedServiceReceiptConfirm {
    actor: FinanceActor,
    payment_id: String,
    invoice_id: String,
    input: ServiceReceiptConfirmInput,
    key: String,
    source: CurrentServiceReceiptReview,
    before: Image,
    account_ids: Vec<String>,
    quotes: (Quote, Quote),
    primary: JournalDraft,
    fx: Option<JournalDraft>,
    amount: i64,
    base_invoice: i64,
    base_payment: i64,
    difference: i64,
    rate: i64,
    money_settled: Option<PreparedServiceReceiptMoney>,
    money_bank: Option<PreparedServiceReceiptMoney>,
}
impl PreparedServiceReceiptConfirm {
    pub fn primary_draft(&self) -> &JournalDraft {
        &self.primary
    }
    pub fn fx_draft(&self) -> Option<&JournalDraft> {
        self.fx.as_ref()
    }
    pub fn bank_statement(&self) -> &Value {
        &self.before.statement
    }
    pub fn bank_row(&self) -> &Value {
        &self.before.bank
    }
    pub fn amount(&self) -> i64 {
        self.amount
    }
    pub fn difference(&self) -> i64 {
        self.difference
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.source.require_current(c).await?;
        if capture(
            c,
            &self.actor,
            &self.payment_id,
            &self.invoice_id,
            &self.input.bank_transaction_id,
            &self.account_ids,
        )
        .await?
            != self.before
            || quote(c, &self.actor.company_id, &self.primary.currency).await? != self.quotes.0
            || quote(
                c,
                &self.actor.company_id,
                t(&self.before.chart, "base_currency")?,
            )
            .await?
                != self.quotes.1
        {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn apply(
        mut self,
        c: &mut PgConnection,
        primary: &JournalPreparation,
        fx: Option<&JournalPreparation>,
        clock: &impl Clock,
    ) -> Result<RetainedServiceReceiptConfirm, Error> {
        self.require_before(c).await?;
        if primary.actor() != &self.actor
            || primary.fx() != self.rate
            || primary.draft() != &self.primary
            || fx.is_some() != self.fx.is_some()
        {
            return Err(changed());
        }
        if let Some(fx) = fx {
            if fx.actor() != &self.actor || fx.draft() != self.fx.as_ref().ok_or_else(changed)? {
                return Err(changed());
            }
        }
        let at = clock.now().get();
        if !(0..=9_007_199_254_740_991).contains(&at) {
            return Err(changed());
        }
        let before_flush: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(&mut *c)
                .await?;
        let mut expected = self.before.clone();
        expected.invoice["outstanding_minor"] =
            json!(n(&expected.invoice, "outstanding_minor")? - self.amount);
        expected.invoice["base_outstanding_minor"] =
            json!(n(&expected.invoice, "base_outstanding_minor")? - self.base_invoice);
        expected.invoice["status"] = json!(if expected.invoice["outstanding_minor"] == 0 {
            "paid"
        } else {
            "partially_paid"
        });
        expected.invoice["version"] = json!(n(&expected.invoice, "version")? + 1);
        expected.invoice["updated_at"] = json!(at);
        expected.payment["outstanding_minor"] = json!(0);
        expected.payment["status"] = json!("settled");
        expected.payment["journal_id"] = json!(primary.journal_id());
        expected.payment["fx_rate_micros"] = json!(self.rate);
        expected.payment["base_amount_minor"] = json!(self.base_payment);
        expected.payment["fx_journal_id"] =
            fx.map(|p| json!(p.journal_id())).unwrap_or(Value::Null);
        expected.payment["version"] = json!(self.input.expected_version + 1);
        expected.payment["updated_at"] = json!(at);
        expected.bank["match_status"] = json!("matched");
        expected.bank["matched_type"] = json!("payment");
        expected.bank["matched_id"] = json!(self.payment_id);
        expected.bank["version"] = json!(self.input.bank_version + 1);
        let allocation = json!({"id":format!("allocation-{}-0",self.key),"company_id":self.actor.company_id.as_str(),"payment_id":self.payment_id,"invoice_id":self.invoice_id,"amount_minor":self.amount,"base_invoice_minor":self.base_invoice,"base_payment_minor":self.base_payment,"realized_fx_minor":self.difference,"created_by":self.actor.user_id.as_str(),"created_at":at});
        expected.allocations.push(allocation);
        expected
            .allocations
            .sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        PgJournalStore::new(
            self.actor.holding_id.clone(),
            self.actor.company_id.clone(),
            c,
        )
        .persist(primary)
        .await
        .map_err(|e| match e {
            PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
            PgTransactionError::Business(e) => je(e),
        })?;
        if let Some(fx) = fx {
            PgJournalStore::new(
                self.actor.holding_id.clone(),
                self.actor.company_id.clone(),
                c,
            )
            .persist(fx)
            .await
            .map_err(|e| match e {
                PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
                PgTransactionError::Business(e) => je(e),
            })?;
        }
        sqlx::query("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(format!("allocation-{}-0",self.key)).bind(self.actor.company_id.as_str()).bind(&self.payment_id).bind(&self.invoice_id).bind(self.amount).bind(self.base_invoice).bind(self.base_payment).bind(self.difference).bind(self.actor.user_id.as_str()).bind(at).execute(&mut *c).await?;
        sqlx::query("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-$1,base_outstanding_minor=base_outstanding_minor-$2,status=CASE WHEN outstanding_minor=$1 THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5").bind(self.amount).bind(self.base_invoice).bind(at).bind(&self.invoice_id).bind(self.actor.company_id.as_str()).execute(&mut *c).await?;
        sqlx::query("UPDATE finance_payments SET outstanding_minor=0,status='settled',journal_id=$1,fx_rate_micros=$2,base_amount_minor=$3,fx_journal_id=$4,version=version+1,updated_at=$5 WHERE id=$6 AND company_id=$7").bind(primary.journal_id()).bind(self.rate).bind(self.base_payment).bind(fx.map(|p|p.journal_id())).bind(at).bind(&self.payment_id).bind(self.actor.company_id.as_str()).execute(&mut *c).await?;
        sqlx::query("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=$1,version=version+1 WHERE id=$2 AND company_id=$3").bind(&self.payment_id).bind(&self.input.bank_transaction_id).bind(self.actor.company_id.as_str()).execute(&mut *c).await?;
        let settled = self
            .money_settled
            .take()
            .ok_or_else(changed)?
            .persist(c)
            .await?;
        let bank = self
            .money_bank
            .take()
            .ok_or_else(changed)?
            .persist(c)
            .await?;
        expected.money.push(settled.row_image(c).await?);
        expected.money.push(bank.row_image(c).await?);
        expected.money.sort();
        let mut journals = vec![(
            primary.journal_id().to_owned(),
            journal_image(c, primary.journal_id()).await?,
        )];
        if let Some(fx) = fx {
            journals.push((
                fx.journal_id().to_owned(),
                journal_image(c, fx.journal_id()).await?,
            ));
        }
        let retained = RetainedServiceReceiptConfirm {
            plan: self,
            expected,
            settled,
            bank,
            journals,
            before_flush,
        };
        retained.require_current(c).await?;
        super::service_receipt_runtime::mark(
            c,
            retained.plan.actor.company_id.as_str(),
            &retained.plan.payment_id,
            &[retained.plan.input.bank_transaction_id.clone()],
        )
        .await?;
        Ok(retained)
    }
}
pub struct RetainedServiceReceiptConfirm {
    plan: PreparedServiceReceiptConfirm,
    expected: Image,
    settled: RetainedServiceReceiptMoney,
    bank: RetainedServiceReceiptMoney,
    journals: Vec<(String, String)>,
    before_flush: i64,
}
impl RetainedServiceReceiptConfirm {
    pub async fn require_finalized(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_current(c).await?;
        let after: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
                .fetch_one(&mut *c)
                .await?;
        for (id, _) in &self.journals {
            let valid: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM finance_financial_events e JOIN finance_journals j ON j.id=e.journal_id JOIN finance_charts c ON c.id=j.chart_id WHERE j.id=$1 AND e.id='journal-event-'||j.id AND e.company_id=j.company_id AND e.chart_id=j.chart_id AND e.period_id=j.period_id AND e.source_type=j.source_type AND e.source_id=j.source_id AND e.source_version=j.version AND e.effective_date=j.entry_date AND e.currency=j.currency AND e.base_currency=c.base_currency AND e.event_kind=CASE WHEN j.source_type='payment_fx' THEN 'fx_adjustment' ELSE 'settlement' END AND e.legacy_snapshot=0 AND e.recorded_at BETWEEN $2 AND $3 AND e.recorded_at BETWEEN 0 AND 9007199254740991 AND to_jsonb(e)=jsonb_build_object('id','journal-event-'||j.id,'company_id',j.company_id,'chart_id',j.chart_id,'period_id',j.period_id,'journal_id',j.id,'source_type',j.source_type,'source_id',j.source_id,'source_version',j.version,'effective_date',j.entry_date,'currency',j.currency,'base_currency',c.base_currency,'event_kind',CASE WHEN j.source_type='payment_fx' THEN 'fx_adjustment' ELSE 'settlement' END,'evidence_json',jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,'legacy_snapshot',0,'recorded_at',e.recorded_at))").bind(id).bind(self.before_flush).bind(after).fetch_one(&mut *c).await?;
            if !valid {
                return Err(changed());
            }
            // The deferred trigger selects the latest approved rule per secondary
            // chart. Verify the complete selected set and the projected line and
            // event evidence after the caller has flushed the constraints.
            let books_valid: bool = sqlx::query_scalar(r#"WITH primary_j AS (SELECT * FROM finance_journals WHERE id=$1), selected AS (
                SELECT DISTINCT ON (r.secondary_chart_id) r.* FROM finance_book_rules r,primary_j p
                WHERE r.company_id=p.company_id AND r.primary_chart_id=p.chart_id AND r.status='approved' AND r.effective_date<=p.entry_date
                ORDER BY r.secondary_chart_id,r.effective_date DESC
              ), actual AS (SELECT b.* FROM finance_book_postings b WHERE b.primary_journal_id=$1)
              SELECT (SELECT count(*) FROM selected)=(SELECT count(*) FROM actual)
                AND NOT EXISTS(SELECT 1 FROM selected r CROSS JOIN primary_j p LEFT JOIN actual b ON b.rule_id=r.id
                  LEFT JOIN finance_journals j ON j.id=b.secondary_journal_id
                  WHERE b.rule_id IS NULL OR b.company_id<>p.company_id OR b.source_version<>p.version
                    OR b.created_at NOT BETWEEN $2 AND $3 OR b.created_at NOT BETWEEN 0 AND 9007199254740991
                    OR to_jsonb(b) IS DISTINCT FROM jsonb_build_object('primary_journal_id',p.id,'secondary_journal_id','book-'||r.id||'-'||p.id,'rule_id',r.id,'company_id',p.company_id,'source_version',p.version,'created_at',b.created_at)
                    OR to_jsonb(j) IS DISTINCT FROM (to_jsonb(p)||jsonb_build_object('id','book-'||r.id||'-'||p.id,'chart_id',r.secondary_chart_id,'journal_number','book-'||r.id||'-'||p.id,'source_type','book_projection','source_id',p.id))
                    OR j.id IS DISTINCT FROM ('book-'||r.id||'-'||p.id) OR j.company_id IS DISTINCT FROM p.company_id
                    OR j.chart_id IS DISTINCT FROM r.secondary_chart_id OR j.period_id IS DISTINCT FROM p.period_id
                    OR j.journal_number IS DISTINCT FROM j.id OR j.entry_date IS DISTINCT FROM p.entry_date
                    OR j.currency IS DISTINCT FROM p.currency OR j.fx_rate_micros IS DISTINCT FROM p.fx_rate_micros
                    OR j.source_type IS DISTINCT FROM 'book_projection' OR j.source_id IS DISTINCT FROM p.id
                    OR j.memo IS DISTINCT FROM p.memo OR j.status IS DISTINCT FROM p.status OR j.is_opening IS DISTINCT FROM p.is_opening
                    OR j.version IS DISTINCT FROM p.version OR j.created_by IS DISTINCT FROM p.created_by
                    OR j.posted_by IS DISTINCT FROM p.posted_by OR j.posted_at IS DISTINCT FROM p.posted_at
                    OR j.reversed_by IS DISTINCT FROM p.reversed_by OR j.reversed_at IS DISTINCT FROM p.reversed_at
                    OR j.created_at IS DISTINCT FROM p.created_at OR j.updated_at IS DISTINCT FROM p.updated_at
                    OR (SELECT count(*) FROM finance_journal_lines l WHERE l.journal_id=j.id)<>(SELECT count(*) FROM finance_journal_lines l WHERE l.journal_id=p.id)
                    OR EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_book_account_mappings m ON m.rule_id=r.id AND m.primary_account_id=l.account_id
                      LEFT JOIN finance_journal_lines x ON x.journal_id=j.id AND x.ordinal=l.ordinal
                      WHERE l.journal_id=p.id AND (to_jsonb(x) IS DISTINCT FROM (to_jsonb(l)||jsonb_build_object('id',j.id||'-'||l.ordinal,'journal_id',j.id,'account_id',m.secondary_account_id)) OR x.id IS DISTINCT FROM (j.id||'-'||l.ordinal) OR x.account_id IS DISTINCT FROM m.secondary_account_id
                        OR (x.description,x.debit_minor,x.credit_minor,x.base_debit_minor,x.base_credit_minor,x.dimension_json) IS DISTINCT FROM (l.description,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json)))
                    OR NOT EXISTS(SELECT 1 FROM finance_financial_events e JOIN finance_charts chart ON chart.id=j.chart_id WHERE e.journal_id=j.id AND e.id='journal-event-'||j.id
                      AND e.company_id=j.company_id AND e.chart_id=j.chart_id AND e.period_id=j.period_id AND e.source_type=j.source_type AND e.source_id=j.source_id
                      AND e.source_version=j.version AND e.effective_date=j.entry_date AND e.currency=j.currency AND e.base_currency=chart.base_currency
                      AND e.event_kind='recognition' AND e.legacy_snapshot=0 AND e.recorded_at BETWEEN $2 AND $3 AND e.recorded_at BETWEEN 0 AND 9007199254740991 AND to_jsonb(e)=jsonb_build_object('id','journal-event-'||j.id,'company_id',j.company_id,'chart_id',j.chart_id,'period_id',j.period_id,'journal_id',j.id,'source_type',j.source_type,'source_id',j.source_id,'source_version',j.version,'effective_date',j.entry_date,'currency',j.currency,'base_currency',chart.base_currency,'event_kind','recognition','evidence_json',jsonb_build_object('journal',to_jsonb(j),'lines',COALESCE((SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),'[]'::jsonb))::text,'legacy_snapshot',0,'recorded_at',e.recorded_at)))"#).bind(id).bind(self.before_flush).bind(after).fetch_one(&mut *c).await?;
            if !books_valid {
                return Err(changed());
            }
        }
        Ok(())
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let p = &self.plan;
        for (id, image) in &self.journals {
            if journal_image(c, id).await? != *image {
                return Err(changed());
            }
        }
        self.settled.require_current(c).await?;
        self.bank.require_current(c).await?;
        if capture(
            c,
            &p.actor,
            &p.payment_id,
            &p.invoice_id,
            &p.input.bank_transaction_id,
            &p.account_ids,
        )
        .await?
            != self.expected
            || quote(c, &p.actor.company_id, &p.primary.currency).await? != p.quotes.0
            || quote(c, &p.actor.company_id, t(&p.before.chart, "base_currency")?).await?
                != p.quotes.1
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn prepare_service_receipt_confirm(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment_id: &str,
    invoice_id: &str,
    key: &str,
    input: &ServiceReceiptConfirmInput,
    clock: &impl Clock,
) -> Result<PreparedServiceReceiptConfirm, Error> {
    let source = read_service_receipt_review(c, actor, payment_id).await?;
    let row = source.row();
    if row["status"] != "released" || n(row, "version")? != input.expected_version {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PAYMENT_NOT_CONFIRMABLE",
            "Refresh the released payment before confirming it.",
        )
        .into());
    }
    if input.allocations.len() != 1 || input.allocations[0].invoice_id != invoice_id {
        return Err(changed());
    }
    let amount = journal_minor(&input.allocations[0].amount, 2).map_err(je)?;
    if amount <= 0 || amount != n(row, "outstanding_minor")? {
        return Err(BusinessError::invalid(
            "ALLOCATION_TOTAL_MISMATCH",
            "Allocation amounts must equal the payment amount.",
        )
        .into());
    }
    let invoice_account: String = sqlx::query_scalar(
        "SELECT debit_account_id FROM finance_invoices WHERE id=$1 AND company_id=$2",
    )
    .bind(invoice_id)
    .bind(actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::invalid(
            "INVOICE_INVALID",
            "Every selected invoice must belong to this company.",
        )
    })?;
    let mut account_ids = vec![t(row, "bank_account")?.into(), invoice_account];
    account_ids.extend(input.fx_gain_account_id.iter().cloned());
    account_ids.extend(input.fx_loss_account_id.iter().cloned());
    account_ids.sort();
    account_ids.dedup();
    let before = capture(
        c,
        actor,
        payment_id,
        invoice_id,
        &input.bank_transaction_id,
        &account_ids,
    )
    .await?;
    let invoice = &before.invoice;
    let bank = &before.bank;
    if invoice["party_id"] != row["party_id"]
        || invoice["currency"] != row["currency"]
        || invoice["kind"] != "receivable"
        || !matches!(t(invoice, "status")?, "posted" | "partially_paid")
        || n(invoice, "outstanding_minor")? < amount
    {
        return Err(BusinessError::invalid("INVOICE_ALLOCATION_MISMATCH","Invoices must be open, compatible with the payment, and have enough outstanding balance.").into());
    }
    if !matches!(t(bank, "match_status")?, "unmatched" | "proposed")
        || n(bank, "version")? != input.bank_version
    {
        return Err(BusinessError::new(FailureKind::Conflict,"BANK_EVIDENCE_CHANGED","The bank transaction changed or was claimed by another workflow. Refresh its evidence.").into());
    }
    if before.statement["bank_account"] != row["bank_account"]
        || bank["currency"] != row["currency"]
        || n(bank, "amount_minor")? != n(row, "amount_minor")?
        || t(bank, "transaction_date")? < t(row, "payment_date")?
    {
        return Err(BusinessError::invalid(
            "SERVICE_PAYMENT_BANK_SOURCE_INVALID",
            "Match the selected account, currency, date and exact bank amount.",
        )
        .into());
    }
    let bank_account = before
        .accounts
        .iter()
        .find(|a| a["id"] == row["bank_account"])
        .ok_or_else(changed)?;
    if bank_account["category"] != "asset"
        || bank_account["control_kind"] != "bank"
        || bank_account["status"] != "active"
    {
        return Err(BusinessError::invalid(
            "BANK_ACCOUNT_INVALID",
            "The selected ledger account is not configured as a bank control account.",
        )
        .into());
    }
    let control = before
        .accounts
        .iter()
        .find(|a| a["id"] == invoice["debit_account_id"])
        .ok_or_else(changed)?;
    if [bank_account, control].iter().any(|a| {
        a["currency"] != row["currency"]
            || a["chart_id"] != before.chart["id"]
            || a["status"] != "active"
    }) {
        return Err(BusinessError::invalid(
            "PAYMENT_LEDGER_MISMATCH",
            "Bank and control accounts must use one active chart and currency.",
        )
        .into());
    }
    let currency = t(row, "currency")?;
    let base = t(&before.chart, "base_currency")?;
    let date = t(bank, "transaction_date")?;
    let quotes = (
        quote(c, &actor.company_id, currency).await?,
        quote(c, &actor.company_id, base).await?,
    );
    let rate = if let Some(fx) = input.fx_rate.as_deref().filter(|v| !v.is_empty()) {
        functional_fx_micros(currency, base, Some(fx)).map_err(je)?
    } else {
        let rates = quotes
            .0
            .rates
            .iter()
            .chain(quotes.1.rates.iter())
            .map(|r| -> Result<ManualRate, Error> {
                Ok(ManualRate {
                    id: t(r, "id")?.into(),
                    currency: t(r, "currency")?.into(),
                    effective_date: t(r, "effective_date")?.into(),
                    currency_per_usd: t(r, "currency_per_usd")?.into(),
                    confirmed_at: n(r, "confirmed_at")?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        functional_fx_evidence(
            &rates,
            currency,
            base,
            date,
            &FinanceCollation::english().map_err(|_| changed())?,
        )
        .map_err(je)?
        .micros
    };
    let base_payment = convert_at_fx(amount, rate).map_err(je)?;
    let outstanding = n(invoice, "outstanding_minor")?;
    let base_outstanding = n(invoice, "base_outstanding_minor")?;
    let base_invoice = if amount == outstanding {
        base_outstanding
    } else {
        i64::try_from(
            (i128::from(base_outstanding) * i128::from(amount) + i128::from(outstanding / 2))
                / i128::from(outstanding),
        )
        .map_err(|_| changed())?
    };
    let difference = base_payment.checked_sub(base_invoice).ok_or_else(changed)?;
    let line = |account: String,
                debit: i64,
                credit: i64,
                description: String|
     -> Result<JournalLineInput, Error> {
        Ok(JournalLineInput {
            account_id: account,
            debit: decimal(debit)?,
            credit: decimal(credit)?,
            description: Some(description),
            dimensions: None,
        })
    };
    let primary = JournalDraft {
        chart_id: t(&before.chart, "id")?.into(),
        period_id: input.period_id.clone(),
        entry_date: date.into(),
        currency: currency.into(),
        fx_rate: input.fx_rate.clone(),
        memo: format!(
            "receipt {}: {}",
            t(row, "payment_number")?,
            input.bank_reference
        ),
        source_type: "payment".into(),
        source_id: Some(payment_id.into()),
        is_opening: false,
        lines: vec![
            line(
                t(invoice, "debit_account_id")?.into(),
                0,
                amount,
                format!("Allocate {}", t(invoice, "invoice_number")?),
            )?,
            line(
                t(row, "bank_account")?.into(),
                amount,
                0,
                input.bank_reference.clone(),
            )?,
        ],
    };
    let fx = if difference != 0 {
        let gain = difference > 0;
        let id = if gain {
            input.fx_gain_account_id.as_ref().ok_or_else(|| {
                BusinessError::invalid(
                    "FX_GAIN_ACCOUNT_REQUIRED",
                    "Select the functional-currency realized FX gain account.",
                )
            })?
        } else {
            input.fx_loss_account_id.as_ref().ok_or_else(|| {
                BusinessError::invalid(
                    "FX_LOSS_ACCOUNT_REQUIRED",
                    "Select the functional-currency realized FX loss account.",
                )
            })?
        };
        let a = before
            .accounts
            .iter()
            .find(|a| a["id"].as_str() == Some(id.as_str()))
            .ok_or_else(changed)?;
        if a["currency"] != before.chart["base_currency"]
            || a["chart_id"] != before.chart["id"]
            || a["status"] != "active"
            || a["category"].as_str() != Some(if gain { "revenue" } else { "expense" })
        {
            return Err(BusinessError::invalid("FX_ACCOUNT_INVALID","Realized gain and loss accounts must use the ledger functional currency and matching account category.").into());
        }
        let delta = difference.checked_abs().ok_or_else(changed)?;
        Some(JournalDraft {
            chart_id: primary.chart_id.clone(),
            period_id: input.period_id.clone(),
            entry_date: date.into(),
            currency: base.into(),
            fx_rate: None,
            memo: format!("Realized FX for {}", t(row, "payment_number")?),
            source_type: "payment_fx".into(),
            source_id: Some(payment_id.into()),
            is_opening: false,
            lines: vec![
                line(
                    t(invoice, "debit_account_id")?.into(),
                    if gain { delta } else { 0 },
                    if gain { 0 } else { delta },
                    format!("Realized FX {}", t(invoice, "invoice_number")?),
                )?,
                line(
                    id.clone(),
                    if gain { 0 } else { delta },
                    if gain { delta } else { 0 },
                    if gain {
                        "Realized foreign exchange gain"
                    } else {
                        "Realized foreign exchange loss"
                    }
                    .into(),
                )?,
            ],
        })
    } else {
        None
    };
    let money_settled = Some(
        prepare_receipt_money_kind(
            c,
            &actor.company_id,
            payment_id,
            date,
            currency,
            amount,
            clock,
            ReceiptMoneyKind::Settled,
        )
        .await?,
    );
    let money_bank = Some(
        prepare_receipt_money_kind(
            c,
            &actor.company_id,
            payment_id,
            date,
            currency,
            amount,
            clock,
            ReceiptMoneyKind::BankSettled,
        )
        .await?,
    );
    let plan = PreparedServiceReceiptConfirm {
        actor: actor.clone(),
        payment_id: payment_id.into(),
        invoice_id: invoice_id.into(),
        input: input.clone(),
        key: key.into(),
        source,
        before,
        account_ids,
        quotes,
        primary,
        fx,
        amount,
        base_invoice,
        base_payment,
        difference,
        rate,
        money_settled,
        money_bank,
    };
    plan.require_before(c).await?;
    Ok(plan)
}

/// Typed internal schema normalization. Raw HTTP logical UTF-16/key ordering is
/// still the inbound adapter's responsibility; no pre-authorized DTO is created.
/// Flush Finance's derived books before globally switching deferred checks to
/// immediate. The book trigger inserts a posted header followed by its lines;
/// its financial-event/final-line constraints must observe that complete child,
/// exactly as at a normal deferred commit. No constraint is disabled.
pub async fn flush_service_receipt_confirm_effects(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("SET CONSTRAINTS finance_book_projection IMMEDIATE")
        .execute(&mut *c)
        .await?;
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(c)
        .await?;
    Ok(())
}

pub fn normalize_service_receipt_confirm_input(
    mut input: ServiceReceiptConfirmInput,
) -> Result<ServiceReceiptConfirmInput, BusinessError> {
    let bad = || BusinessError::invalid("VALIDATION_FAILED", "Invalid payment confirmation.");
    let safe = |n| (1..=9_007_199_254_740_991_i64).contains(&n);
    input.bank_reference = crate::domain::money::js_trim(&input.bank_reference).into();
    input.proof = crate::domain::money::js_trim(&input.proof).into();
    let length = |s: &str| min_max_len(s);
    if !safe(input.expected_version)
        || !safe(input.bank_version)
        || input.period_id.is_empty()
        || input.bank_transaction_id.is_empty()
        || !(2..=160).contains(&length(&input.bank_reference))
        || !(3..=1000).contains(&length(&input.proof))
        || input
            .fx_gain_account_id
            .as_ref()
            .is_some_and(|s| s.is_empty())
        || input
            .fx_loss_account_id
            .as_ref()
            .is_some_and(|s| s.is_empty())
        || input.allocations.is_empty()
        || input.allocations.len() > 100
        || input.attachment_set_ids.len() > 20
        || input.attachment_set_ids.iter().any(|s| s.is_empty())
    {
        return Err(bad());
    }
    for a in &input.allocations {
        let raw = a.amount.strip_prefix('-').unwrap_or(&a.amount);
        let (whole, fraction) = raw
            .split_once('.')
            .map_or((raw, None), |(a, b)| (a, Some(b)));
        if a.invoice_id.is_empty()
            || whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|v| {
                v.is_empty() || v.len() > 6 || !v.bytes().all(|b| b.is_ascii_digit())
            })
        {
            return Err(bad());
        }
    }
    Ok(input)
}
fn min_max_len(value: &str) -> usize {
    value.encode_utf16().count()
}
