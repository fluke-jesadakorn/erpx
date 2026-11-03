//! Finance-owned immutable receipt issue. The consuming native confirmation
//! retains payment/allocation/journal sources and Organization's Company name.
use crate::{application::journal::FinanceActor, domain::customer_receipt::*};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use erpx_server_workflow::public::commands::CommandJson;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_RECEIPT_SOURCE_CHANGED",
        "The posted receipt source changed before its immutable copy was retained.",
    )
    .into()
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
pub async fn lock_customer_receipt_sources(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("LOCK TABLE finance_customer_receipts,finance_sales_number_sequences IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}
pub struct RetainedCustomerReceipt {
    unit: Unit,
    company: String,
    id: String,
    expected: Value,
}
impl RetainedCustomerReceipt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let image:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(r) FROM finance_customer_receipts r WHERE id=$1 AND company_id=$2 FOR SHARE").bind(&self.id).bind(&self.company).fetch_optional(&mut *c).await?;
        if unit(c).await? != self.unit || image.as_ref() != Some(&self.expected) {
            return Err(changed());
        }
        Ok(())
    }
}
/// A new confirmation stages exactly one issued copy. Exact command replay is
/// resolved by Workflow before this effect; this port never invents a replay.
pub async fn issue_customer_receipt(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment_id: &str,
    company_name: &str,
    clock: &impl Clock,
) -> Result<RetainedCustomerReceipt, Error> {
    let unit = unit(c).await?;
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_customer_receipts WHERE payment_id=$1)",
    )
    .bind(payment_id)
    .fetch_one(&mut *c)
    .await?;
    if existing {
        return Err(changed());
    }
    let source:Option<Option<Value>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(p)::text)+octet_length(to_jsonb(party)::text)<=65536 THEN to_jsonb(p)||jsonb_build_object('customer_name',party.name,'customer_code',party.code) END FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id JOIN finance_parties party ON party.id=p.party_id AND party.company_id=p.company_id WHERE p.id=$1 AND p.company_id=$2 AND p.kind='receipt' AND p.status='settled' AND j.status='posted' FOR UPDATE OF p").bind(payment_id).bind(actor.company_id.as_str()).fetch_optional(&mut *c).await?;
    let source = source.flatten().ok_or_else(changed)?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('invoiceId',i.id,'invoiceNumber',i.invoice_number,'amountMinor',a.amount_minor::text) FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id AND i.company_id=a.company_id WHERE a.payment_id=$1 AND a.company_id=$2 ORDER BY a.created_at,a.id LIMIT 1001 FOR SHARE OF a,i").bind(payment_id).bind(actor.company_id.as_str()).fetch_all(&mut *c).await?;
    if rows.len() > 1000 {
        return Err(changed());
    }
    let text = |key| source[key].as_str().map(str::to_owned).ok_or_else(changed);
    let date = text("payment_date")?;
    let number: String = sqlx::query_scalar("SELECT erpx_sales_document_number($1,'RC',$2)")
        .bind(actor.company_id.as_str())
        .bind(&date)
        .fetch_one(&mut *c)
        .await?;
    let snapshot = CustomerReceiptSnapshot {
        schema: "finance.customer-receipt.v1".into(),
        document_number: number.clone(),
        company_name: company_name.into(),
        customer_name: text("customer_name")?,
        customer_code: text("customer_code")?,
        date: date.clone(),
        currency: text("currency")?,
        amount_minor: source["amount_minor"]
            .as_i64()
            .ok_or_else(changed)?
            .to_string(),
        payment_id: payment_id.into(),
        payment_version: source["version"].as_i64().ok_or_else(changed)?,
        bank_reference: text("payment_number")?,
        journal_id: text("journal_id")?,
        allocations: rows
            .iter()
            .map(|r| {
                Ok(CustomerReceiptAllocation {
                    invoice_id: r["invoiceId"].as_str().ok_or_else(changed)?.into(),
                    invoice_number: r["invoiceNumber"].as_str().ok_or_else(changed)?.into(),
                    amount_minor: r["amountMinor"].as_str().ok_or_else(changed)?.into(),
                })
            })
            .collect::<Result<Vec<_>, Error>>()?,
    };
    let html = render_customer_receipt(&snapshot)?;
    let hash = format!("{:x}", Sha256::digest(html.as_bytes()));
    let json = CommandJson::from_serializable(&snapshot)?;
    let at = clock.now().get();
    if !(0..=9_007_199_254_740_991).contains(&at) {
        return Err(changed());
    }
    let id = format!("customer-receipt-{payment_id}");
    let expected = json!({"id":id,"company_id":actor.company_id.as_str(),"payment_id":payment_id,"document_number":number,"receipt_date":date,"currency":snapshot.currency,"amount_minor":source["amount_minor"],"snapshot_json":json.as_str(),"export_html":html,"export_sha256":hash,"issued_by":actor.user_id.as_str(),"issued_at":at});
    sqlx::query("INSERT INTO finance_customer_receipts(id,company_id,payment_id,document_number,receipt_date,currency,amount_minor,snapshot_json,export_html,export_sha256,issued_by,issued_at)VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(&id).bind(actor.company_id.as_str()).bind(payment_id).bind(&number).bind(&date).bind(&snapshot.currency).bind(source["amount_minor"].as_i64().ok_or_else(changed)?).bind(json.as_str()).bind(&html).bind(&hash).bind(actor.user_id.as_str()).bind(at).execute(&mut *c).await?;
    let retained = RetainedCustomerReceipt {
        unit,
        company: actor.company_id.as_str().into(),
        id,
        expected,
    };
    retained.require_current(c).await?;
    Ok(retained)
}

/// Immutable issued copy and its current payment status on the caller's unit.
/// The caller must additionally retain IAM, purchase and Field lineage proofs.
pub struct CurrentCustomerReceipt {
    retained: RetainedCustomerReceipt,
    payment: Value,
    pub payment_id: String,
    pub detail: Value,
    pub export: Value,
}
impl CurrentCustomerReceipt {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.retained.require_current(c).await?;
        let image: Option<Value> = sqlx::query_scalar("SELECT to_jsonb(p) FROM finance_payments p WHERE id=$1 AND company_id=$2 FOR SHARE").bind(&self.payment_id).bind(&self.retained.company).fetch_optional(c).await?;
        if image.as_ref() != Some(&self.payment) { return Err(changed()); }
        Ok(())
    }
}
pub async fn read_customer_receipt(c: &mut PgConnection, actor: &FinanceActor, id: &str) -> Result<CurrentCustomerReceipt, Error> {
    let unit = unit(c).await?;
    let row: Option<(Value,Value)> = sqlx::query_as("SELECT to_jsonb(r),to_jsonb(p) FROM finance_customer_receipts r JOIN finance_payments p ON p.id=r.payment_id AND p.company_id=r.company_id WHERE r.id=$1 AND r.company_id=$2 FOR SHARE OF r,p").bind(id).bind(actor.company_id.as_str()).fetch_optional(&mut *c).await?;
    let (image,payment) = row.ok_or_else(||BusinessError::new(FailureKind::NotFound,"RECEIPT_NOT_FOUND","Receipt not found in this workspace."))?;
    let text = |key| image[key].as_str().ok_or_else(changed);
    let payment_id = text("payment_id")?.to_owned();
    let html = text("export_html")?;
    let hash = text("export_sha256")?;
    let json = text("snapshot_json")?;
    if html.len()>2*1024*1024 || json.len()>2*1024*1024 { return Err(changed()); }
    let snapshot: CustomerReceiptSnapshot = serde_json::from_str(json).map_err(|_|changed())?;
    if snapshot.payment_id!=payment_id || snapshot.document_number!=text("document_number")? ||
       snapshot.date!=text("receipt_date")? || snapshot.currency!=text("currency")? ||
       snapshot.amount_minor!=image["amount_minor"].as_i64().ok_or_else(changed)?.to_string() ||
       payment["id"]!=payment_id || payment["kind"]!="receipt" ||
       snapshot.payment_version<1 || snapshot.payment_version>payment["version"].as_i64().ok_or_else(changed)? ||
       format!("{:x}",Sha256::digest(html.as_bytes()))!=hash || render_customer_receipt(&snapshot)?!=html {
        return Err(changed());
    }
    let mut receipt=image.clone();
    receipt.as_object_mut().ok_or_else(changed)?.remove("export_html");
    receipt["payment_status"]=payment["status"].clone();
    receipt["payment_version"]=payment["version"].clone();
    let status=if matches!(payment["status"].as_str(),Some("cancelled"|"reversed"|"voided")) {"cancelled"} else {"issued"};
    let detail=json!({"sha256":hash,"receipt":receipt,"snapshot":snapshot,"status":status});
    let export=json!({"content":html,"sha256":hash,"document_number":text("document_number")?});
    let current=CurrentCustomerReceipt{retained:RetainedCustomerReceipt{unit,company:actor.company_id.as_str().into(),id:id.into(),expected:image},payment,payment_id,detail,export};
    current.require_current(c).await?;
    Ok(current)
}
/// Only a persisted Company receipt can select its payment owner. Never use
/// a client source flag or infer that a missing receipt belongs to legacy.
pub async fn customer_receipt_payment(c:&mut PgConnection,actor:&FinanceActor,id:&str)->Result<String,Error>{
    unit(c).await?;
    sqlx::query_scalar("SELECT r.payment_id FROM finance_customer_receipts r JOIN finance_payments p ON p.id=r.payment_id AND p.company_id=r.company_id WHERE r.id=$1 AND r.company_id=$2 FOR SHARE OF r,p").bind(id).bind(actor.company_id.as_str()).fetch_optional(c).await?.ok_or_else(||BusinessError::new(FailureKind::NotFound,"RECEIPT_NOT_FOUND","Receipt not found in this workspace.").into())
}
