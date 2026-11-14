//! Finance-owned controlled receipt readback. Source and current permission /
//! purchase guards belong to the caller's native selected composition.
use crate::application::journal::FinanceActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
const SQL: &str = r#"
WITH workspace AS (
 SELECT jsonb_build_object(
 'payment',to_jsonb(p)||jsonb_build_object('party_code',f.code,'party_name',f.name,'settlement_invoice_id',$3::text),
 'allocations',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.created_at) FROM (
   SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id AND i.company_id=a.company_id WHERE a.payment_id=p.id AND a.company_id=p.company_id) a),'[]'::jsonb),
 'reversal',(SELECT to_jsonb(r) FROM finance_payment_reversals r WHERE r.payment_id=p.id),
 'periods',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.start_date DESC) FROM (SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=p.company_id AND state='open') r),'[]'::jsonb),
 'invoices',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r)-'due_date' ORDER BY r.due_date,r.invoice_number) FROM (SELECT id,invoice_number,kind,outstanding_minor,status,version,due_date FROM finance_invoices WHERE company_id=p.company_id AND party_id=p.party_id AND currency=p.currency AND kind=CASE WHEN p.kind='receipt' THEN 'receivable' ELSE 'payable' END AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim' AND id=$3) r),'[]'::jsonb) ELSE '[]'::jsonb END,
 'bankTransactions',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.transaction_date DESC,r.external_id) FROM (
   SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.company_id=p.company_id AND s.bank_account=p.bank_account AND t.currency=p.currency AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=p.amount_minor AND t.transaction_date>=p.payment_date AND t.statement_id=ANY($5::text[])) r),'[]'::jsonb) ELSE '[]'::jsonb END,
 'fxAccounts',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.code) FROM (
   SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id AND c.company_id=bank.company_id JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=bank.company_id WHERE bank.id=p.bank_account AND bank.company_id=p.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')) r),'[]'::jsonb) ELSE '[]'::jsonb END
 ) image FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id AND f.company_id=p.company_id WHERE p.company_id=$1 AND p.id=$2 AND p.kind IN ('payment','receipt')
) SELECT CASE WHEN octet_length(image::text)<=2097152 THEN image END FROM workspace
"#;
const GENERIC_SQL: &str = r#"
WITH workspace AS (
 SELECT jsonb_build_object(
 'payment',to_jsonb(p)||jsonb_build_object('party_code',f.code,'party_name',f.name,'settlement_invoice_id',$3::text),
 'allocations',COALESCE((SELECT jsonb_agg(to_jsonb(a) ORDER BY a.created_at) FROM (
   SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id AND i.company_id=a.company_id WHERE a.payment_id=p.id AND a.company_id=p.company_id) a),'[]'::jsonb),
 'reversal',(SELECT to_jsonb(r) FROM finance_payment_reversals r WHERE r.payment_id=p.id),
 'periods',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.start_date DESC) FROM (SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=p.company_id AND state='open') r),'[]'::jsonb),
 'invoices',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r)-'due_date' ORDER BY r.due_date,r.invoice_number) FROM (SELECT id,invoice_number,kind,outstanding_minor,status,version,due_date FROM finance_invoices WHERE company_id=p.company_id AND party_id=p.party_id AND currency=p.currency AND kind=CASE WHEN p.kind='receipt' THEN 'receivable' ELSE 'payable' END AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim' AND id=$3) r),'[]'::jsonb) ELSE '[]'::jsonb END,
 'bankTransactions',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.transaction_date DESC,r.external_id) FROM (
   SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.company_id=p.company_id AND s.bank_account=p.bank_account AND t.currency=p.currency AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -(p.amount_minor-p.withheld_minor) END AND t.transaction_date>=p.payment_date) r),'[]'::jsonb) ELSE '[]'::jsonb END,
 'fxAccounts',CASE WHEN $4::boolean THEN COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.code) FROM (
   SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id AND c.company_id=bank.company_id JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=bank.company_id WHERE bank.id=p.bank_account AND bank.company_id=p.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')) r),'[]'::jsonb) ELSE '[]'::jsonb END
 ) image FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id AND f.company_id=p.company_id WHERE p.company_id=$1 AND p.id=$2 AND p.kind IN ('payment','receipt')
) SELECT CASE WHEN octet_length(image::text)<=2097152 THEN image END FROM workspace
"#;
async fn unit(c: &mut PgConnection) -> Result<(String, i32, String, String), Error> {
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
async fn image(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    invoice: &str,
    confirm: bool,
    statements: &[String],
) -> Result<Value, Error> {
    let value: Option<Option<Value>> = sqlx::query_scalar(SQL)
        .bind(company)
        .bind(id)
        .bind(invoice)
        .bind(confirm)
        .bind(statements)
        .fetch_optional(c)
        .await?;
    let value = value.flatten().ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "PAYMENT_NOT_FOUND",
            "Payment not found in this company.",
        )
    })?;
    fn safe(value: &Value) -> bool {
        match value {
            Value::Number(n) => {
                n.as_i64()
                    .is_some_and(|v| v.unsigned_abs() <= 9_007_199_254_740_991)
                    || n.as_u64().is_some_and(|v| v <= 9_007_199_254_740_991)
            }
            Value::Array(rows) => rows.iter().all(safe),
            Value::Object(row) => row.values().all(safe),
            _ => true,
        }
    }
    if !safe(&value) {
        return Err(PersistenceError::new("FINANCE_RECEIPT_WORKSPACE_WIRE_INVALID").into());
    }
    Ok(value)
}
pub struct CurrentServiceReceiptWorkspace {
    unit: (String, i32, String, String),
    company: String,
    id: String,
    invoice: String,
    confirm: bool,
    statements: Vec<String>,
    data: Value,
    generic: bool,
}
impl CurrentServiceReceiptWorkspace {
    pub fn data(&self) -> &Value {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let current = if self.generic {
            generic_image(c, &self.company, &self.id, &self.invoice, self.confirm).await?
        } else {
            image(
                c,
                &self.company,
                &self.id,
                &self.invoice,
                self.confirm,
                &self.statements,
            )
            .await?
        };
        if unit(c).await? != self.unit || current != self.data {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SERVICE_PAYMENT_SOURCE_CHANGED",
                "The controlled receipt workspace changed.",
            )
            .into());
        }
        Ok(())
    }
}
/// Invoice identity, confirmation admission and statement IDs must be supplied
/// from current native owner ports, never directly from an inbound request.
pub async fn read_service_receipt_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    invoice: &str,
    can_confirm: bool,
    statements: &[String],
) -> Result<CurrentServiceReceiptWorkspace, Error> {
    let unit = unit(c).await?;
    let data = image(
        c,
        actor.company_id.as_str(),
        id,
        invoice,
        can_confirm,
        statements,
    )
    .await?;
    let current = CurrentServiceReceiptWorkspace {
        unit,
        company: actor.company_id.as_str().into(),
        id: id.into(),
        invoice: invoice.into(),
        confirm: can_confirm,
        statements: statements.into(),
        data,
        generic: false,
    };
    current.require_current(c).await?;
    Ok(current)
}

async fn generic_image(
    c: &mut PgConnection,
    company: &str,
    id: &str,
    invoice: &str,
    confirm: bool,
) -> Result<Value, Error> {
    let value: Option<Option<Value>> = sqlx::query_scalar(GENERIC_SQL)
        .bind(company)
        .bind(id)
        .bind(invoice)
        .bind(confirm)
        .bind(Vec::<String>::new())
        .fetch_optional(c)
        .await?;
    let value = value.flatten().ok_or_else(|| {
        BusinessError::new(FailureKind::NotFound, "PAYMENT_NOT_FOUND", "Payment not found in this company.")
    })?;
    fn safe(value: &Value) -> bool {
        match value {
            Value::Number(n) => n.as_i64().is_some_and(|v| v.unsigned_abs() <= 9_007_199_254_740_991)
                || n.as_u64().is_some_and(|v| v <= 9_007_199_254_740_991),
            Value::Array(rows) => rows.iter().all(safe),
            Value::Object(row) => row.values().all(safe),
            _ => true,
        }
    }
    if !safe(&value) {
        return Err(PersistenceError::new("FINANCE_RECEIPT_WORKSPACE_WIRE_INVALID").into());
    }
    Ok(value)
}
/// Generic payment workspace: matching unmatched bank rows are selected by
/// account, currency, amount and date, not by a Field import receipt.
pub async fn read_generic_payment_workspace(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
    invoice: &str,
    can_confirm: bool,
) -> Result<CurrentServiceReceiptWorkspace, Error> {
    let unit = unit(c).await?;
    let data = generic_image(c, actor.company_id.as_str(), id, invoice, can_confirm).await?;
    let current = CurrentServiceReceiptWorkspace {
        unit,
        company: actor.company_id.as_str().into(),
        id: id.into(),
        invoice: invoice.into(),
        confirm: can_confirm,
        statements: Vec::new(),
        data,
        generic: true,
    };
    current.require_current(c).await?;
    Ok(current)
}

/// Minimal persisted routing facts only. A consuming native command must still
/// prove current Field lineage, permission and all transaction invariants.
pub struct ServicePaymentRoutingFacts {
    pub kind: String,
    pub creator: String,
    pub field_allocation_marker: bool,
}
pub async fn service_payment_routing_facts(
    c: &mut PgConnection,
    actor: &FinanceActor,
    id: &str,
) -> Result<ServicePaymentRoutingFacts, Error> {
    unit(c).await?;
    let row:Option<(String,String,bool)>=sqlx::query_as("SELECT p.kind,p.created_by,EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=p.id AND i.source_type='field_service') FROM finance_payments p WHERE p.company_id=$1 AND p.id=$2 FOR SHARE OF p").bind(actor.company_id.as_str()).bind(id).fetch_optional(c).await?;
    let (kind, creator, field_allocation_marker) = row.ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "PAYMENT_NOT_FOUND",
            "Payment not found in this company.",
        )
    })?;
    Ok(ServicePaymentRoutingFacts {
        kind,
        creator,
        field_allocation_marker,
    })
}
