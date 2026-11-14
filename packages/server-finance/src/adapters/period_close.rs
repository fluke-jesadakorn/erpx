//! Native Company Finance period close.
//!
//! Owns the close readiness checks, control-account reconciliation, statutory
//! VAT evidence, immutable close evidence rows, period transitions and the
//! caller-owned command receipt in one SERIALIZABLE transaction. Prepare,
//! cancel, approve and reopen preserve independent-person controls through the
//! current Account/IAM authority supplied by composition.
use super::sales_invoice_create::{SalesInvoiceCommandActor, authority_json, fingerprint, receipt};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

const INVOICE_HISTORY_CTE: &str = r#"WITH history_cutoff AS (
  SELECT $1::text company_id, $2::text as_of
), historical_invoice_balances AS (
  SELECT i.*, a.chart_id,
    COALESCE(j.entry_date,i.invoice_date) recognition_date,
    i.total_minor
      - COALESCE(settlements.transaction_minor,0)
      - COALESCE(credits.transaction_minor,0)
      - COALESCE(billing_credits.transaction_minor,0)
      - COALESCE(opening_reversal.transaction_minor,0)
      + COALESCE(refunds.transaction_minor,0) historical_outstanding_minor,
    (CASE WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END)
      - COALESCE(settlements.base_minor,0)
      - COALESCE(credits.base_minor,0)
      - COALESCE(billing_credits.base_minor,0)
      - COALESCE(opening_reversal.base_minor,0)
      + COALESCE(refunds.base_minor,0)
      + COALESCE(revaluation.base_minor,0) historical_base_outstanding_minor,
    (j.id IS NULL OR COALESCE(settlements.legacy_payment_evidence,false)) legacy_date_evidence
  FROM finance_invoices i
  JOIN history_cutoff h ON h.company_id=i.company_id
  JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END AND a.company_id=i.company_id
  LEFT JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id
  LEFT JOIN LATERAL (
    SELECT SUM(x.amount_minor) transaction_minor,BOOL_OR(pj.id IS NULL) legacy_payment_evidence,
      SUM(CASE WHEN x.base_invoice_minor=0 AND i.fx_rate_micros=1000000 THEN x.amount_minor ELSE x.base_invoice_minor END) base_minor
    FROM finance_allocations x
    JOIN finance_payments p ON p.id=x.payment_id AND p.company_id=i.company_id
    LEFT JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_payment_reversals pr ON pr.payment_id=p.id
    LEFT JOIN finance_journals rj ON rj.id=pr.reversal_journal_id AND rj.company_id=i.company_id
    WHERE x.invoice_id=i.id AND COALESCE(pj.entry_date,p.payment_date)<=h.as_of
      AND (pj.status IN ('posted','reversed') OR (pj.id IS NULL AND p.status='settled'))
      AND (rj.id IS NULL OR rj.entry_date>h.as_of)
  ) settlements ON true
  LEFT JOIN LATERAL (
    SELECT SUM(r.total_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=cj.id OR l.journal_id IN (
          SELECT fj.id FROM finance_journals fj WHERE fj.id=r.fx_journal_id AND fj.company_id=i.company_id
            AND fj.status IN ('posted','reversed') AND fj.entry_date<=h.as_of)) AND l.account_id=a.id)) base_minor
    FROM (SELECT "id","company_id","branch_id","order_id","fulfillment_id","invoice_id","kind","number","return_date","reason","status","subtotal_minor","tax_minor","total_minor","stock_journal_id","credit_journal_id","version","requested_by","approved_by","created_at","updated_at","credited_minor","refund_minor","base_credit_minor","base_credited_minor","base_refund_minor","fx_journal_id","goods_fx_journal_id" FROM commerce_returns) r
    JOIN finance_journals cj ON cj.id=r.credit_journal_id AND cj.company_id=i.company_id
    WHERE r.invoice_id=i.id AND cj.status IN ('posted','reversed') AND cj.entry_date<=h.as_of
  ) credits ON true
  LEFT JOIN LATERAL (
    SELECT SUM(c.total_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(l.base_credit_minor-l.base_debit_minor),0) FROM finance_journal_lines l WHERE l.journal_id=cj.id AND l.account_id=a.id)) base_minor
    FROM finance_billing_corrections c JOIN finance_journals cj ON cj.id=c.credit_journal_id AND cj.company_id=i.company_id
    WHERE c.invoice_id=i.id AND c.company_id=i.company_id AND c.status='posted' AND cj.status='posted' AND cj.entry_date<=h.as_of
  ) billing_credits ON true
  LEFT JOIN LATERAL (
    SELECT SUM(CASE WHEN rj.id IS NOT NULL THEN i.total_minor ELSE 0 END) transaction_minor,
      SUM(CASE WHEN rj.id IS NULL THEN 0 WHEN i.base_total_minor=0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END) base_minor
    FROM finance_journals rj WHERE rj.company_id=i.company_id AND rj.source_type='reversal'
      AND rj.source_id=j.id AND rj.status IN ('posted','reversed') AND rj.entry_date<=h.as_of
      AND i.source_type='opening_import'
  ) opening_reversal ON true
  LEFT JOIN LATERAL (
    SELECT SUM(p.amount_minor) transaction_minor,
      SUM((SELECT COALESCE(SUM(CASE WHEN i.kind='receivable' THEN l.base_debit_minor-l.base_credit_minor ELSE l.base_credit_minor-l.base_debit_minor END),0)
        FROM finance_journal_lines l WHERE (l.journal_id=pj.id OR l.journal_id=rfj.id) AND l.account_id=a.id)) base_minor
    FROM finance_refund_obligations o JOIN finance_refund_instructions ri ON ri.obligation_id=o.id
    JOIN finance_payments p ON p.id=ri.payment_id AND p.company_id=i.company_id
    JOIN finance_journals pj ON pj.id=p.journal_id AND pj.company_id=i.company_id
    LEFT JOIN finance_journals rfj ON rfj.id=ri.fx_journal_id AND rfj.company_id=i.company_id AND rfj.status IN ('posted','reversed') AND rfj.entry_date<=h.as_of
    WHERE o.invoice_id=i.id AND pj.status IN ('posted','reversed') AND pj.entry_date<=h.as_of
  ) refunds ON true
  LEFT JOIN LATERAL (
    SELECT SUM(l.adjustment_minor) base_minor FROM finance_fx_revaluation_lines l
    JOIN finance_fx_revaluation_runs r ON r.id=l.run_id AND r.company_id=i.company_id
    JOIN finance_periods rp ON rp.id=r.period_id
    WHERE l.invoice_id=i.id AND r.status='posted' AND rp.end_date<=h.as_of
  ) revaluation ON true
  WHERE COALESCE(j.entry_date,i.invoice_date)<=h.as_of
    AND (j.status IN ('posted','reversed') OR (j.id IS NULL AND i.status IN ('posted','partially_paid','paid')))
)"#;

const PENDING_VAT_REVIEW_SQL: &str = r#"WITH scope AS (
  SELECT company_id,statutory_chart_id,effective_date,$1::text end_date FROM finance_accounting_profiles WHERE company_id=$2 AND status='approved'
) SELECT COUNT(*) value FROM (
  SELECT i.id FROM scope s JOIN finance_invoices i ON i.company_id=s.company_id JOIN finance_journals j ON j.id=i.journal_id
  WHERE j.chart_id=s.statutory_chart_id AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s JOIN (SELECT "id","company_id","branch_id","order_id","fulfillment_id","invoice_id","kind","number","return_date","reason","status","subtotal_minor","tax_minor","total_minor","stock_journal_id","credit_journal_id","version","requested_by","approved_by","created_at","updated_at","credited_minor","refund_minor","base_credit_minor","base_credited_minor","base_refund_minor","fx_journal_id","goods_fx_journal_id" FROM commerce_returns) r ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT r.id FROM scope s JOIN finance_billing_corrections r ON r.company_id=s.company_id JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
  WHERE r.status='posted' AND j.chart_id=s.statutory_chart_id AND j.status='posted' AND j.entry_date BETWEEN s.effective_date AND s.end_date AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')
  UNION ALL SELECT c.id FROM scope s JOIN finance_vat_corrections c ON c.company_id=s.company_id AND c.chart_id=s.statutory_chart_id
    JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.status='prepared' AND t.accounting_date<=s.end_date
) pending"#;

const EXPANSION_CLOSE_SQL: &str = r#"WITH boundary AS (SELECT $1::text company,$2::text start,$3::text finish),pending AS (
 SELECT 'revenue recognition' kind,r.id FROM finance_recognition_runs r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.entry_date<=p.finish
 UNION ALL SELECT 'billing correction',r.id FROM finance_billing_corrections r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.entry_date<=p.finish
 UNION ALL SELECT 'asset depreciation',r.id FROM finance_asset_policy_runs r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.entry_date<=p.finish
 UNION ALL SELECT 'financing payment',r.id FROM finance_instrument_runs r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.input_json::jsonb->>'entryDate'<=p.finish
 UNION ALL SELECT 'financing agreement',r.id FROM finance_instrument_editions r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.content_json::jsonb->>'startDate'<=p.finish
 UNION ALL SELECT 'book policy adjustment',r.id FROM finance_book_adjustments r,boundary p WHERE r.company_id=p.company AND r.status='draft' AND r.content_json::jsonb->>'entryDate'<=p.finish
 UNION ALL SELECT 'zero usage review',r.id FROM finance_business_billing_runs r,boundary p WHERE r.company_id=p.company AND r.no_charge_status='draft' AND r.service_from<=p.finish AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=r.id)
 UNION ALL SELECT 'scheduled draft',r.id FROM finance_due_drafts r,boundary p WHERE r.company_id=p.company AND r.status IN ('scheduled','blocked') AND COALESCE(r.input_json::jsonb->>'entryDate',r.input_json::jsonb->>'invoiceDate')<=p.finish
 UNION ALL SELECT 'due revenue recognition',b.id||':'||(item->>'key') FROM finance_business_billing_runs b JOIN finance_invoices i ON i.id=b.invoice_id JOIN finance_journals posted ON posted.id=i.journal_id AND posted.company_id=i.company_id AND posted.status IN ('posted','reversed') CROSS JOIN LATERAL jsonb_array_elements(b.result_json::jsonb->'items') item,boundary p
 WHERE b.company_id=p.company AND i.status IN ('posted','partially_paid','paid') AND posted.entry_date<=p.finish AND item->>'date'<=p.finish AND (item->>'amount')::numeric>0 AND NOT EXISTS(SELECT 1 FROM finance_billing_corrections c WHERE c.billing_run_id=b.id AND c.status='posted' AND c.entry_date<=p.finish)
 AND NOT EXISTS(SELECT 1 FROM finance_recognition_runs r WHERE r.billing_run_id=b.id AND r.item_key=item->>'key' AND (r.status IN ('draft','posted') OR r.status='reversed' AND EXISTS(SELECT 1 FROM finance_journals rev WHERE rev.id=r.reversal_journal_id AND rev.entry_date>p.finish)) AND r.entry_date<=p.finish)
 UNION ALL SELECT 'due asset depreciation',a.id FROM finance_asset_policy_editions r JOIN finance_assets a ON a.id=r.asset_id,boundary p WHERE r.company_id=p.company AND r.status='active' AND a.status='in_service' AND a.in_service_date<=p.finish
 AND NOT EXISTS(SELECT 1 FROM finance_asset_policy_runs run WHERE run.asset_id=a.id AND run.status='draft' AND run.entry_date<=p.finish)
 AND ((SELECT count(*) FROM finance_asset_policy_runs run WHERE run.asset_id=a.id AND (run.status='posted' OR run.status='reversed' AND run.reversal_entry_date>p.finish) AND run.entry_date<=p.finish)+(SELECT count(*) FROM finance_asset_depreciations old WHERE old.asset_id=a.id AND old.entry_date<=p.finish))<LEAST(a.useful_life_months,((extract(year FROM p.finish::date)::integer-extract(year FROM a.in_service_date::date)::integer)*12+extract(month FROM p.finish::date)::integer-extract(month FROM a.in_service_date::date)::integer+1))
 UNION ALL SELECT 'due financing payment',r.id||':'||(item->>'ordinal') FROM finance_instrument_editions r CROSS JOIN LATERAL jsonb_array_elements(r.schedule_json::jsonb->'rows') item,boundary p WHERE r.company_id=p.company AND r.status='active' AND item->>'dueDate'<=p.finish
 AND NOT EXISTS(SELECT 1 FROM finance_instrument_runs run WHERE run.instrument_id=r.id AND run.status IN ('draft','posted') AND run.ordinal=(item->>'ordinal')::bigint AND run.input_json::jsonb->>'entryDate'<=p.finish)
 UNION ALL SELECT 'due contract billing',r.id||':'||to_char(month,'YYYY-MM') FROM finance_business_contracts r CROSS JOIN boundary p CROSS JOIN LATERAL generate_series(date_trunc('month',r.start_date::date),date_trunc('month',LEAST(r.end_date,p.finish)::date),interval '1 month') month
 WHERE r.company_id=p.company AND r.status='active' AND r.start_date<=p.finish AND (r.content_json::jsonb->>'cycle'!='once' OR to_char(month,'YYYY-MM')=substr(r.start_date,1,7))
 AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs b LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.root_id=r.root_id AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) AND b.billing_key=CASE WHEN r.content_json::jsonb->>'cycle'='once' THEN 'once' ELSE to_char(month,'YYYY-MM') END AND (i.status IN ('draft','in_review','posted','partially_paid','paid') OR b.no_charge_status IN ('draft','approved')))
) SELECT kind,id FROM pending ORDER BY kind,id"#;

const VAT_RECONCILIATION_SQL: &str = r#"WITH tax_accounts AS (
      SELECT DISTINCT i.tax_account_id id,CASE WHEN i.kind='receivable' THEN 'output' ELSE 'input' END direction
      FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.company_id=$1 AND j.chart_id=$2 AND j.entry_date BETWEEN $3 AND $4 AND i.tax_account_id IS NOT NULL
    ) SELECT a.id,a.direction,ac.code,ac.name,
      COALESCE((SELECT SUM(CASE WHEN a.direction='output' THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END)
        FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=a.id AND j.company_id=$5 AND j.chart_id=$6 AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN $7 AND $8),0) ledger_minor,
      COALESCE((SELECT SUM(t.tax_minor) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=$9 AND t.chart_id=$10 AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN $11 AND $12),0) register_minor,
      COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.tax_account_id=a.id AND t.company_id=$13 AND t.chart_id=$14 AND t.direction=a.direction AND t.status='approved' AND t.accounting_date BETWEEN $15 AND $16),'[]'::jsonb)::text register_evidence_json
      FROM tax_accounts a JOIN finance_accounts ac ON ac.id=a.id ORDER BY a.id,a.direction"#;

const VAT_MISSING_SQL: &str = r#"SELECT COUNT(*) FROM (SELECT i.id FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id
    WHERE i.company_id=$1 AND j.chart_id=$2 AND j.status IN ('posted','reversed') AND j.entry_date BETWEEN $3 AND $4 AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')
    UNION ALL SELECT r.id FROM (SELECT "id","company_id","branch_id","order_id","fulfillment_id","invoice_id","kind","number","return_date","reason","status","subtotal_minor","tax_minor","total_minor","stock_journal_id","credit_journal_id","version","requested_by","approved_by","created_at","updated_at","credited_minor","refund_minor","base_credit_minor","base_credited_minor","base_refund_minor","fx_journal_id","goods_fx_journal_id" FROM commerce_returns) r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=$5 AND j.chart_id=$6 AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN $7 AND $8 AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')
    UNION ALL SELECT r.id FROM finance_billing_corrections r JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=r.credit_journal_id
    WHERE r.company_id=$9 AND j.chart_id=$10 AND r.status='posted' AND j.status='posted' AND j.entry_date BETWEEN $11 AND $12 AND i.tax_account_id IS NOT NULL
    AND NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')) pending"#;

const VAT_PENDING_CORRECTIONS_SQL: &str =
    "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=$1 AND c.chart_id=$2 AND c.status='prepared' AND t.accounting_date<=$3";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeriodCloseCancelInput {
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeriodCloseProofInput {
    pub proof: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<i64>,
    #[serde(default)]
    pub attachment_set_ids: Vec<String>,
}

/// Retained close-workpaper attachment contract from the TypeScript command.
pub const PERIOD_CLOSE_WORKPAPER_TARGET_TYPE: &str = "finance_period";
pub const PERIOD_CLOSE_WORKPAPER_PURPOSE_KEY: &str = "close_workpaper";
pub const PERIOD_CLOSE_WORKPAPER_COMPONENT_KIND: &str = "action";
pub const PERIOD_CLOSE_WORKPAPER_COMPONENT_REF: &str = "close-approval";
pub const PERIOD_CLOSE_WORKPAPER_BINDING_TYPE: &str = "finance_period_close";
pub const PERIOD_CLOSE_WORKPAPER_MAX_SETS: usize = 20;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeriodCloseEvidenceRequest {
    pub set_ids: Vec<String>,
    pub target_id: String,
    pub expected_target_version: i64,
    pub command_id: String,
    pub binding_type: String,
    pub binding_id: String,
}

/// The compiled composition supplies the shared Records/Files business-evidence
/// port bound to this exact serializable Finance unit. Files prepares and
/// revalidates the retained workpaper binding; Finance never binds file state
/// itself and never falls back to a TypeScript path.
pub trait PgPeriodCloseEvidence: Send {
    type Prepared: Send;
    fn prepare(
        &mut self,
        c: &mut PgConnection,
        request: PeriodCloseEvidenceRequest,
    ) -> impl Future<Output = Result<Self::Prepared, Error>> + Send;
    fn require_current(
        &mut self,
        c: &mut PgConnection,
        prepared: &Self::Prepared,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct CloseReconciliation {
    pub kind: String,
    pub account_id: String,
    pub account_code: String,
    pub account_name: String,
    pub chart_name: String,
    pub currency: String,
    pub ledger_minor: i64,
    pub source_minor: i64,
    pub difference_minor: i64,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub source: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct VatCloseEvidence {
    pub profile_id: String,
    pub chart_id: String,
    pub effective_date: String,
    pub end_date: String,
    #[cfg_attr(feature = "contracts", ts(type = "Array<Record<string, unknown>>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Vec<Object>))]
    pub rows: Vec<Value>,
    pub missing: i64,
    pub pending_corrections: i64,
    pub ready: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PeriodCloseReadiness {
    pub draft_journals: i64,
    pub draft_invoices: i64,
    pub unmatched_bank: i64,
    pub open_manufacturing: i64,
    pub pending_calculations: i64,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PeriodCloseWorkspace {
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub period: Value,
    pub readiness: PeriodCloseReadiness,
    pub pending_tax: i64,
    pub pending_source_tasks: i64,
    pub supplier_bills: i64,
    pub supplier_bank: i64,
    pub refunds: i64,
    pub reconciliations: Vec<CloseReconciliation>,
    pub vat: Option<VatCloseEvidence>,
    pub reconciliation_ready: bool,
    pub blockers: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PeriodCloseResult {
    pub id: String,
    pub state: String,
    pub version: i64,
    pub reconciliation_hash: String,
    pub readiness: PeriodCloseReadiness,
    pub reconciliations: Vec<CloseReconciliation>,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
}

fn not_found(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::NotFound, code, message).into()
}

fn forbidden(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Forbidden, code, message).into()
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_PERIOD_CLOSE_UNAVAILABLE",
        "The Finance period close is unavailable.",
    )
    .into()
}

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *c)
        .await?;
    Ok(())
}

fn require_company(actor: &SalesInvoiceCommandActor) -> Result<(), Error> {
    if actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
        || actor.iam_json.trim().is_empty()
        || actor.session_id.is_empty()
    {
        return Err(forbidden(
            "COMPANY_FINANCE_REQUIRED",
            "Use a Company Finance assignment for period close.",
        ));
    }
    Ok(())
}

async fn replay<T: serde::de::DeserializeOwned>(
    c: &mut PgConnection,
    command: &CommandKey,
    operation: &str,
    request_hash: &str,
) -> Result<Option<T>, Error> {
    let saved = sqlx::query(
        "SELECT operation,request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = saved else {
        return Ok(None);
    };
    let saved_operation: String = row.try_get("operation")?;
    let saved_hash: String = row.try_get("request_hash")?;
    if saved_operation != operation || saved_hash != request_hash {
        return Err(conflict(
            "IDEMPOTENCY_CONFLICT",
            "This command key belongs to a different request.",
        ));
    }
    let raw: Option<String> = row.try_get("result_json")?;
    raw.and_then(|value| serde_json::from_str::<T>(&value).ok())
        .map(Some)
        .ok_or_else(unavailable)
}

fn json_row(raw: String) -> Result<Value, Error> {
    serde_json::from_str(&raw).map_err(|_| unavailable())
}

fn digest(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_string(value).unwrap_or_default().as_bytes())
    )
}

async fn period_row(
    c: &mut PgConnection,
    company: &str,
    period_id: &str,
    lock: bool,
) -> Result<Value, Error> {
    let suffix = if lock { " FOR UPDATE" } else { "" };
    let raw = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
        "SELECT to_jsonb(p)::text FROM finance_periods p WHERE p.id=$1 AND p.company_id=$2{suffix}"
    )))
    .bind(period_id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?;
    raw.map(json_row)
        .transpose()?
        .ok_or_else(|| not_found("PERIOD_NOT_FOUND", "Accounting period not found."))
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn number(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or_default()
}

async fn count(
    c: &mut PgConnection,
    sql: &str,
    binds: &[&str],
) -> Result<i64, Error> {
    let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        query = query.bind(*bind);
    }
    query.fetch_one(&mut *c).await.map_err(Into::into)
}

async fn readiness(
    c: &mut PgConnection,
    company: &str,
    period_id: &str,
    period: &Value,
) -> Result<PeriodCloseReadiness, Error> {
    let start = text(period, "start_date");
    let end = text(period, "end_date");
    let draft_journals = count(
        c,
        "SELECT COUNT(*) FROM finance_journals WHERE company_id=$1 AND (period_id=$2 OR entry_date BETWEEN $3 AND $4) AND status IN ('draft','in_review')",
        &[company, period_id, &start, &end],
    )
    .await?;
    let draft_invoices = count(
        c,
        "SELECT COUNT(*) FROM finance_invoices WHERE company_id=$1 AND status IN ('draft','in_review') AND invoice_date BETWEEN $2 AND $3",
        &[company, &start, &end],
    )
    .await?;
    let unmatched_bank = count(
        c,
        "SELECT COUNT(*) FROM finance_bank_transactions WHERE company_id=$1 AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN $2 AND $3",
        &[company, &start, &end],
    )
    .await?;
    let open_manufacturing = count(
        c,
        "SELECT COUNT(*) FROM manufacturing_orders WHERE company_id=$1 AND status IN ('reserved','in_progress','awaiting_qc')",
        &[company],
    )
    .await?;
    let pending_calculations = count(
        c,
        "SELECT COUNT(*) FROM finance_calculation_runs WHERE company_id=$1 AND period_id=$2 AND status IN ('prepared','reviewed','stale')",
        &[company, period_id],
    )
    .await?;
    Ok(PeriodCloseReadiness {
        draft_journals,
        draft_invoices,
        unmatched_bank,
        open_manufacturing,
        pending_calculations,
    })
}

async fn pending_tax(c: &mut PgConnection, company: &str, end: &str) -> Result<i64, Error> {
    count(c, PENDING_VAT_REVIEW_SQL, &[end, company]).await
}

async fn expansion_pending(
    c: &mut PgConnection,
    company: &str,
    start: &str,
    end: &str,
) -> Result<i64, Error> {
    let rows = sqlx::query(EXPANSION_CLOSE_SQL)
        .bind(company)
        .bind(start)
        .bind(end)
        .fetch_all(&mut *c)
        .await?;
    Ok(rows.len() as i64)
}

async fn supplier_bills(c: &mut PgConnection, company: &str, end: &str) -> Result<i64, Error> {
    count(
        c,
        "SELECT COUNT(*) FROM procurement_bill_matches WHERE company_id=$1 AND status IN ('review','approved','invoiced') AND invoice_date<=$2",
        &[company, end],
    )
    .await
}

async fn supplier_bank(c: &mut PgConnection, company: &str, end: &str) -> Result<i64, Error> {
    count(
        c,
        "SELECT COUNT(*) FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=$1 AND i.status IN ('exported','unknown') AND b.planned_date<=$2",
        &[company, end],
    )
    .await
}

async fn refund_pending(c: &mut PgConnection, company: &str, end: &str) -> Result<(i64, i64), Error> {
    let returns = count(
        c,
        "SELECT COUNT(*) FROM commerce_returns WHERE company_id=$1 AND status='approved' AND return_date<=$2",
        &[company, end],
    )
    .await?;
    let instructions = count(
        c,
        "SELECT COUNT(*) FROM finance_refund_instructions WHERE company_id=$1 AND status IN ('exported','unknown') AND planned_date<=$2",
        &[company, end],
    )
    .await?;
    Ok((returns, instructions))
}

async fn control_accounts(c: &mut PgConnection, company: &str) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(a)::text || '|' || to_jsonb(c)::text FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id \
         WHERE a.company_id=$1 AND a.status='active' AND c.status='active' AND a.control_kind IN ('receivable','payable','bank') \
         ORDER BY c.name,a.code,a.id",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    rows.into_iter()
        .map(|row| {
            let (account, chart) = row
                .split_once('|')
                .ok_or_else(|| unavailable())?;
            let mut account = json_row(account.to_owned())?;
            let chart = json_row(chart.to_owned())?;
            if let Some(object) = account.as_object_mut() {
                object.insert("chart_name".into(), chart.get("name").cloned().unwrap_or(Value::Null));
                object.insert(
                    "base_currency".into(),
                    chart.get("base_currency").cloned().unwrap_or(Value::Null),
                );
            }
            Ok(account)
        })
        .collect()
}

async fn primary_book_account(
    c: &mut PgConnection,
    company: &str,
    account_id: &str,
    end: &str,
) -> Result<Option<String>, Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT m.primary_account_id FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id \
         WHERE r.company_id=$1 AND r.status='approved' AND m.secondary_account_id=$2 AND r.effective_date<=$3 \
         ORDER BY r.effective_date DESC LIMIT 1",
    )
    .bind(company)
    .bind(account_id)
    .bind(end)
    .fetch_optional(&mut *c)
    .await
    .map_err(Into::into)
}

async fn ledger_signed(
    c: &mut PgConnection,
    company: &str,
    account_id: &str,
    end: &str,
) -> Result<i64, Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0)::bigint FROM finance_journal_lines l \
         JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=$1 AND l.account_id=$2 \
         AND j.status IN ('posted','reversed') AND j.entry_date<=$3",
    )
    .bind(company)
    .bind(account_id)
    .bind(end)
    .fetch_one(&mut *c)
    .await
    .map_err(Into::into)
}

async fn bank_closing(
    c: &mut PgConnection,
    company: &str,
    account_id: &str,
    end: &str,
) -> Result<Option<Value>, Error> {
    let raw = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(s)::text FROM finance_bank_statements s WHERE s.company_id=$1 AND s.bank_account=$2 \
         AND s.statement_date<=$3 ORDER BY s.statement_date DESC,s.created_at DESC LIMIT 1",
    )
    .bind(company)
    .bind(account_id)
    .bind(end)
    .fetch_optional(&mut *c)
    .await?;
    raw.map(json_row).transpose()
}

async fn subledger_closing(
    c: &mut PgConnection,
    company: &str,
    period_id: &str,
    end: &str,
    kind: &str,
    base_currency: &str,
    account_id: &str,
) -> Result<(i64, i64, i64, i64), Error> {
    let sql = format!(
        r#"{INVOICE_HISTORY_CTE}
SELECT COALESCE(SUM(i.historical_base_outstanding_minor),0)::bigint value,
  COUNT(*)::bigint invoice_count,
  COUNT(*) FILTER(WHERE i.currency<>$3 AND i.historical_outstanding_minor>0 AND NOT EXISTS(
    SELECT 1 FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id
    WHERE l.invoice_id=i.id AND r.period_id=$4 AND r.status='posted'))::bigint foreign_open_count,
  COUNT(*) FILTER(WHERE i.legacy_date_evidence)::bigint legacy_date_count
FROM historical_invoice_balances i
WHERE i.kind=$5 AND CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END=$6"#
    );
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(company)
        .bind(end)
        .bind(base_currency)
        .bind(period_id)
        .bind(kind)
        .bind(account_id)
        .fetch_one(&mut *c)
        .await?;
    Ok((
        row.try_get::<i64, _>("value")?,
        row.try_get::<i64, _>("invoice_count")?,
        row.try_get::<i64, _>("foreign_open_count")?,
        row.try_get::<i64, _>("legacy_date_count")?,
    ))
}

async fn vat_evidence(
    c: &mut PgConnection,
    company: &str,
    end: &str,
) -> Result<Option<VatCloseEvidence>, Error> {
    let profile_raw = sqlx::query_scalar::<_, String>(
        "SELECT to_jsonb(p)::text FROM finance_accounting_profiles p WHERE p.company_id=$1 AND p.status='approved' \
         AND p.effective_date<=$2 ORDER BY p.effective_date DESC,p.id DESC LIMIT 1",
    )
    .bind(company)
    .bind(end)
    .fetch_optional(&mut *c)
    .await?;
    let Some(profile_raw) = profile_raw else {
        return Ok(None);
    };
    let profile = json_row(profile_raw)?;
    let profile_id = text(&profile, "id");
    let chart_id = text(&profile, "statutory_chart_id");
    let effective_date = text(&profile, "effective_date");
    let rows = sqlx::query_scalar::<_, String>(VAT_RECONCILIATION_SQL)
        .bind(company)
        .bind(&chart_id)
        .bind(&effective_date)
        .bind(end)
        .bind(company)
        .bind(&chart_id)
        .bind(&effective_date)
        .bind(end)
        .bind(company)
        .bind(&chart_id)
        .bind(&effective_date)
        .bind(end)
        .bind(company)
        .bind(&chart_id)
        .bind(&effective_date)
        .bind(end)
        .fetch_all(&mut *c)
        .await?;
    let rows = rows
        .into_iter()
        .map(|raw| {
            let value = json_row(raw)?;
            Ok(value)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let missing = count(
        c,
        VAT_MISSING_SQL,
        &[
            company,
            &chart_id,
            &effective_date,
            end,
            company,
            &chart_id,
            &effective_date,
            end,
            company,
            &chart_id,
            &effective_date,
            end,
        ],
    )
    .await?;
    let pending_corrections = count(
        c,
        VAT_PENDING_CORRECTIONS_SQL,
        &[company, &chart_id, end],
    )
    .await?;
    let ready = missing == 0
        && pending_corrections == 0
        && rows.iter().all(|row| {
            row.get("ledger_minor").and_then(Value::as_i64) == row.get("register_minor").and_then(Value::as_i64)
        });
    Ok(Some(VatCloseEvidence {
        profile_id,
        chart_id,
        effective_date,
        end_date: end.to_owned(),
        rows,
        missing,
        pending_corrections,
        ready,
    }))
}

async fn reconciliation(
    c: &mut PgConnection,
    company: &str,
    period: &Value,
) -> Result<(Vec<CloseReconciliation>, Option<VatCloseEvidence>, bool), Error> {
    let period_id = text(period, "id");
    let end = text(period, "end_date");
    let mut rows = Vec::new();
    for account in control_accounts(c, company).await? {
        let kind = text(&account, "control_kind");
        let account_id = text(&account, "id");
        let base_currency = text(&account, "base_currency");
        let source_account_id = primary_book_account(c, company, &account_id, &end)
            .await?
            .unwrap_or_else(|| account_id.clone());
        let signed = ledger_signed(c, company, &account_id, &end).await?;
        let ledger = if kind == "payable" { -signed } else { signed };
        let (source_minor, source) = if kind == "bank" {
            let statement = bank_closing(c, company, &source_account_id, &end).await?;
            let source_minor = statement
                .as_ref()
                .map(|statement| {
                    let closing = number(statement, "closing_minor");
                    let base = number(statement, "base_closing_minor");
                    let rate = statement
                        .get("closing_fx_rate_micros")
                        .and_then(Value::as_i64)
                        .unwrap_or(1_000_000);
                    if base == 0 && closing != 0 && rate == 1_000_000 {
                        closing
                    } else {
                        base
                    }
                })
                .unwrap_or_default();
            let evidence_complete = statement
                .as_ref()
                .map(|statement| text(statement, "statement_date") == end)
                .unwrap_or(false);
            (
                source_minor,
                json!({
                    "sourceAccountId": source_account_id,
                    "statementId": statement.as_ref().map(|value| value.get("id").cloned().unwrap_or(Value::Null)),
                    "statementNumber": statement.as_ref().map(|value| value.get("statement_number").cloned().unwrap_or(Value::Null)),
                    "statementDate": statement.as_ref().map(|value| value.get("statement_date").cloned().unwrap_or(Value::Null)),
                    "transactionClosingMinor": statement.as_ref().map(|value| value.get("closing_minor").cloned().unwrap_or(Value::Null)),
                    "closingFxRateMicros": statement.as_ref().map(|value| value.get("closing_fx_rate_micros").cloned().unwrap_or(Value::Null)),
                    "periodEnd": end,
                    "evidenceComplete": evidence_complete,
                }),
            )
        } else {
            let (value, invoices, foreign, legacy) =
                subledger_closing(c, company, &period_id, &end, &kind, &base_currency, &source_account_id).await?;
            (
                value,
                json!({
                    "sourceAccountId": source_account_id,
                    "invoiceCount": invoices,
                    "foreignOpenCount": foreign,
                    "legacyDateCount": legacy,
                    "asOf": end,
                }),
            )
        };
        rows.push(CloseReconciliation {
            kind,
            account_id,
            account_code: text(&account, "code"),
            account_name: text(&account, "name"),
            chart_name: text(&account, "chart_name"),
            currency: text(&account, "currency"),
            ledger_minor: ledger,
            source_minor,
            difference_minor: ledger - source_minor,
            source,
        });
    }
    let vat = vat_evidence(c, company, &end).await?;
    let ready = rows.iter().all(|row| {
        row.difference_minor == 0
            && row.source.get("foreignOpenCount").and_then(Value::as_i64).unwrap_or_default() == 0
            && row.source.get("legacyDateCount").and_then(Value::as_i64).unwrap_or_default() == 0
            && (row.kind != "bank"
                || row.source.get("evidenceComplete").and_then(Value::as_bool) == Some(true))
    }) && vat.as_ref().map(|value| value.ready).unwrap_or(true);
    Ok((rows, vat, ready))
}

fn hash_input(rows: &[CloseReconciliation], vat: &Option<VatCloseEvidence>) -> String {
    let value = json!({
        "rows": rows.iter().map(|row| json!({
            "kind": row.kind,
            "accountId": row.account_id,
            "currency": row.currency,
            "ledgerMinor": row.ledger_minor,
            "sourceMinor": row.source_minor,
            "differenceMinor": row.difference_minor,
            "source": row.source,
        })).collect::<Vec<_>>(),
        "vat": vat,
    });
    digest(&value)
}

fn blockers(readiness: &PeriodCloseReadiness, pending_tax: i64, source_tasks: i64, ready: bool) -> i64 {
    readiness.draft_journals
        + readiness.draft_invoices
        + readiness.unmatched_bank
        + readiness.open_manufacturing
        + readiness.pending_calculations
        + pending_tax
        + source_tasks
        + i64::from(!ready)
}

async fn workspace_counts(
    c: &mut PgConnection,
    company: &str,
    period: &Value,
) -> Result<(i64, i64, i64, i64, i64, i64), Error> {
    let start = text(period, "start_date");
    let end = text(period, "end_date");
    let source_tasks = expansion_pending(c, company, &start, &end).await?;
    let tax = pending_tax(c, company, &end).await?;
    let bills = supplier_bills(c, company, &end).await?;
    let bank = supplier_bank(c, company, &end).await?;
    let (returns, instructions) = refund_pending(c, company, &end).await?;
    Ok((source_tasks, tax, bills, bank, returns, instructions + returns))
}

pub async fn read_period_close(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    period_id: &str,
) -> Result<PeriodCloseWorkspace, Error> {
    require_company(actor)?;
    let period = period_row(c, &actor.company_id, period_id, false).await?;
    let readiness = readiness(c, &actor.company_id, period_id, &period).await?;
    let (source_tasks, tax, bills, bank, _returns, refunds) =
        workspace_counts(c, &actor.company_id, &period).await?;
    let (reconciliations, vat, ready) = reconciliation(c, &actor.company_id, &period).await?;
    let blockers = blockers(&readiness, tax, source_tasks, ready);
    Ok(PeriodCloseWorkspace {
        period,
        readiness,
        pending_tax: tax,
        pending_source_tasks: source_tasks,
        supplier_bills: bills,
        supplier_bank: bank,
        refunds,
        reconciliations,
        vat,
        reconciliation_ready: ready,
        blockers,
    })
}

pub async fn begin_period_close(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    period_id: &str,
    now: i64,
) -> Result<PeriodCloseResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    let operation = "finance.period.begin-close";
    let input_json = json!({"periodId": period_id}).to_string();
    let request_hash = fingerprint(actor, operation, period_id, &input_json);
    if let Some(saved) = replay::<PeriodCloseResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let period = period_row(c, &actor.company_id, period_id, true).await?;
    if text(&period, "state") != "open" {
        return Err(conflict(
            "INVALID_PERIOD_TRANSITION",
            "Only an open period can begin close.",
        ));
    }
    let version = number(&period, "close_version");
    let start = text(&period, "start_date");
    let end = text(&period, "end_date");
    let readiness = readiness(c, &actor.company_id, period_id, &period).await?;
    if readiness.draft_journals
        + readiness.draft_invoices
        + readiness.unmatched_bank
        + readiness.open_manufacturing
        + readiness.pending_calculations
        > 0
    {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve draft journals, draft invoices, unmatched bank rows, manufacturing orders and calculation runs before beginning close.",
        ));
    }
    if pending_tax(c, &actor.company_id, &end).await? > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Review source VAT documents in the statutory ledger before beginning close.",
        ));
    }
    if expansion_pending(c, &actor.company_id, &start, &end).await? > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve Finance source tasks before beginning close.",
        ));
    }
    if supplier_bills(c, &actor.company_id, &end).await? > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve supplier invoice matching and AP posting before beginning close.",
        ));
    }
    if supplier_bank(c, &actor.company_id, &end).await? > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve exported supplier bank instructions before beginning close.",
        ));
    }
    let (returns, instructions) = refund_pending(c, &actor.company_id, &end).await?;
    if returns > 0 || instructions > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Complete return adjustments and resolve exported refund instructions before beginning close.",
        ));
    }
    let (reconciliations, vat, ready) = reconciliation(c, &actor.company_id, &period).await?;
    if vat.as_ref().is_some_and(|evidence| !evidence.ready) {
        return Err(conflict(
            "CLOSE_VAT_RECONCILIATION_FAILED",
            "Reconcile the reviewed statutory VAT register with every originating VAT account before beginning close.",
        ));
    }
    if !ready {
        return Err(conflict(
            "CLOSE_RECONCILIATION_FAILED",
            "Reconcile, retain dated source journals or revalue control accounts before beginning close.",
        ));
    }
    let next_version = version + 1;
    let reconciliation_hash = hash_input(&reconciliations, &vat);
    let check_source = digest(&json!({
        "periodId": period_id,
        "closeVersion": version,
        "readiness": readiness,
        "reconciliation": reconciliation_hash,
    }));
    for (check, value) in [
        ("draftJournals", readiness.draft_journals),
        ("draftInvoices", readiness.draft_invoices),
        ("unmatchedBank", readiness.unmatched_bank),
        ("openManufacturing", readiness.open_manufacturing),
        ("pendingCalculations", readiness.pending_calculations),
    ] {
        sqlx::query(
            "INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,'pass',$5,$6,$7,$8) ON CONFLICT(period_id,check_key,source_version) DO NOTHING",
        )
        .bind(&actor.company_id)
        .bind(period_id)
        .bind(check)
        .bind(&check_source)
        .bind(value)
        .bind(json!({"count": value}).to_string())
        .bind(now)
        .bind(&actor.user_id)
        .execute(&mut *c)
        .await?;
    }
    if let Some(vat) = &vat {
        sqlx::query(
            "INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) \
             VALUES(gen_random_uuid()::text,$1,$2,'statutory-vat',$3,'pass',0,$4,$5,$6) ON CONFLICT(period_id,check_key,source_version) DO NOTHING",
        )
        .bind(&actor.company_id)
        .bind(period_id)
        .bind(&check_source)
        .bind(serde_json::to_string(vat).map_err(|_| unavailable())?)
        .bind(now)
        .bind(&actor.user_id)
        .execute(&mut *c)
        .await?;
    }
    for row in &reconciliations {
        sqlx::query(
            "INSERT INTO finance_close_reconciliations(id,company_id,period_id,close_version,kind,account_id,currency,ledger_minor,source_minor,difference_minor,source_json,checked_at,checked_by) \
             VALUES(gen_random_uuid()::text,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(&actor.company_id)
        .bind(period_id)
        .bind(next_version)
        .bind(&row.kind)
        .bind(&row.account_id)
        .bind(&row.currency)
        .bind(row.ledger_minor)
        .bind(row.source_minor)
        .bind(row.difference_minor)
        .bind(serde_json::to_string(&row.source).map_err(|_| unavailable())?)
        .bind(now)
        .bind(&actor.user_id)
        .execute(&mut *c)
        .await?;
    }
    sqlx::query(
        "UPDATE finance_periods SET state='closing',close_started_by=$1,close_version=close_version+1,reconciliation_hash=$2 WHERE id=$3 AND company_id=$4 AND state='open' AND close_version=$5",
    )
    .bind(&actor.user_id)
    .bind(&reconciliation_hash)
    .bind(period_id)
    .bind(&actor.company_id)
    .bind(version)
    .execute(&mut *c)
    .await?;
    let result = PeriodCloseResult {
        id: period_id.to_owned(),
        state: "closing".into(),
        version: next_version,
        reconciliation_hash,
        readiness,
        reconciliations,
    };
    let authority = authority_json(actor);
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        period_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn cancel_period_close(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    period_id: &str,
    input: &PeriodCloseCancelInput,
    can_approve: bool,
    now: i64,
) -> Result<PeriodCloseResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if input.reason.trim().len() < 3 || input.reason.trim().len() > 2000 || input.expected_version < 1 {
        return Err(invalid(
            "INVALID_CLOSE_CANCELLATION",
            "Use a close version and a reason to cancel the preparation.",
        ));
    }
    let operation = "finance.period.cancel-close";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, period_id, &input_json);
    if let Some(saved) = replay::<PeriodCloseResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let period = period_row(c, &actor.company_id, period_id, true).await?;
    if text(&period, "state") != "closing" {
        return Err(conflict(
            "PERIOD_NOT_CLOSING",
            "Only a prepared close can be cancelled.",
        ));
    }
    if text(&period, "close_started_by") != actor.user_id && !can_approve {
        return Err(forbidden(
            "CLOSE_CANCELLATION_FORBIDDEN",
            "The preparer or a Finance approver must cancel the preparation.",
        ));
    }
    if number(&period, "close_version") != input.expected_version {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The close changed. Refresh and review again.",
        ));
    }
    let next_version = input.expected_version + 1;
    sqlx::query(
        "UPDATE finance_periods SET state='open',close_started_by=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=$1 AND company_id=$2 AND state='closing' AND close_version=$3",
    )
    .bind(period_id)
    .bind(&actor.company_id)
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    let result = PeriodCloseResult {
        id: period_id.to_owned(),
        state: "open".into(),
        version: next_version,
        reconciliation_hash: String::new(),
        readiness: PeriodCloseReadiness::default(),
        reconciliations: Vec::new(),
    };
    let authority = authority_json(actor);
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        period_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}

pub async fn close_period_with_evidence<E: PgPeriodCloseEvidence>(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    period_id: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
    now: i64,
    evidence: &mut E,
) -> Result<PeriodCloseResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if !can_approve {
        return Err(forbidden(
            "FINANCE_APPROVAL_REQUIRED",
            "Use a current Finance approver to close the period.",
        ));
    }
    if input.proof.trim().is_empty() || input.proof.trim().len() > 2000 {
        return Err(invalid(
            "INVALID_CLOSE_PROOF",
            "Record the review evidence before approving the close.",
        ));
    }
    if input.attachment_set_ids.len() > PERIOD_CLOSE_WORKPAPER_MAX_SETS {
        return Err(invalid(
            "INVALID_CLOSE_ATTACHMENTS",
            "Attach no more than twenty close workpapers.",
        ));
    }
    let mut set_ids = Vec::new();
    for id in &input.attachment_set_ids {
        if !set_ids.iter().any(|selected| selected == id) {
            set_ids.push(id.clone());
        }
    }
    let operation = "finance.period.close";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, period_id, &input_json);
    // A replay returns the retained result before any file binding is prepared.
    if let Some(saved) = replay::<PeriodCloseResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let period = period_row(c, &actor.company_id, period_id, true).await?;
    if text(&period, "state") != "closing" {
        return Err(conflict(
            "INVALID_PERIOD_TRANSITION",
            "Begin the close before approving it.",
        ));
    }
    if text(&period, "close_started_by") == actor.user_id {
        return Err(conflict(
            "CLOSE_APPROVAL_SEPARATION",
            "A different person must approve the period close.",
        ));
    }
    if input
        .expected_version
        .is_some_and(|version| version != number(&period, "close_version"))
    {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The close changed. Refresh and review again.",
        ));
    }
    let version = number(&period, "close_version");
    let next_version = version + 1;
    let prepared = if set_ids.is_empty() {
        None
    } else {
        Some(
            evidence
                .prepare(
                    c,
                    PeriodCloseEvidenceRequest {
                        set_ids,
                        target_id: period_id.to_owned(),
                        expected_target_version: version,
                        command_id: command.as_str().to_owned(),
                        binding_type: PERIOD_CLOSE_WORKPAPER_BINDING_TYPE.into(),
                        binding_id: format!("{period_id}:{next_version}"),
                    },
                )
                .await?,
        )
    };
    let start = text(&period, "start_date");
    let end = text(&period, "end_date");
    let readiness = readiness(c, &actor.company_id, period_id, &period).await?;
    if readiness.draft_journals
        + readiness.draft_invoices
        + readiness.unmatched_bank
        + readiness.open_manufacturing
        + readiness.pending_calculations
        > 0
    {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve draft journals, draft invoices, unmatched bank rows, manufacturing orders and calculation runs before approving the close.",
        ));
    }
    if pending_tax(c, &actor.company_id, &end).await? > 0
        || expansion_pending(c, &actor.company_id, &start, &end).await? > 0
        || supplier_bills(c, &actor.company_id, &end).await? > 0
        || supplier_bank(c, &actor.company_id, &end).await? > 0
    {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Resolve source evidence and supplier requirements before approving the close.",
        ));
    }
    let (returns, instructions) = refund_pending(c, &actor.company_id, &end).await?;
    if returns > 0 || instructions > 0 {
        return Err(conflict(
            "CLOSE_NOT_READY",
            "Complete return adjustments and refund instructions before approving the close.",
        ));
    }
    let (reconciliations, vat, ready) = reconciliation(c, &actor.company_id, &period).await?;
    let reconciliation_hash = hash_input(&reconciliations, &vat);
    if !ready || reconciliation_hash != text(&period, "reconciliation_hash") {
        return Err(conflict(
            "CLOSE_RECONCILIATION_STALE",
            "Ledger, subledger, or bank evidence changed after preparation. Reopen the period and prepare the close again.",
        ));
    }
    let changed = sqlx::query(
        "UPDATE finance_periods SET state='closed',closed_by=$1,closed_at=$2,close_version=close_version+1 WHERE id=$3 AND company_id=$4 AND state='closing' AND close_version=$5",
    )
    .bind(&actor.user_id)
    .bind(now)
    .bind(period_id)
    .bind(&actor.company_id)
    .bind(version)
    .execute(&mut *c)
    .await?;
    if changed.rows_affected() != 1 {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The close changed. Refresh and review again.",
        ));
    }
    let result = PeriodCloseResult {
        id: period_id.to_owned(),
        state: "closed".into(),
        version: next_version,
        reconciliation_hash,
        readiness,
        reconciliations,
    };
    let authority = authority_json(actor);
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        period_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next_version,
        now,
    )
    .await?;
    if let Some(prepared) = prepared.as_ref() {
        // Files re-reads the exact bound sets and links after every close
        // effect; drift aborts the whole serializable unit.
        evidence.require_current(c, prepared).await?;
    }
    Ok(result)
}

/// Transitional owner bridge for assemblies that have not yet mounted the
/// shared Files workpaper port. It fails closed for nonempty workpapers and
/// never falls back to TypeScript; the mounted composition uses
/// [`close_period_with_evidence`] with the real Files adapter.
pub async fn close_period(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    period_id: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
    now: i64,
) -> Result<PeriodCloseResult, Error> {
    close_period_with_evidence(
        c,
        actor,
        command,
        period_id,
        input,
        can_approve,
        now,
        &mut UnmountedPeriodCloseEvidence,
    )
    .await
}

struct UnmountedPeriodCloseEvidence;

impl PgPeriodCloseEvidence for UnmountedPeriodCloseEvidence {
    type Prepared = ();

    async fn prepare(
        &mut self,
        _c: &mut PgConnection,
        _request: PeriodCloseEvidenceRequest,
    ) -> Result<Self::Prepared, Error> {
        Err(BusinessError::new(
            FailureKind::Unavailable,
            "FINANCE_PERIOD_CLOSE_EVIDENCE_UNAVAILABLE",
            "The shared Files workpaper binding is not mounted for this Finance close.",
        )
        .into())
    }

    async fn require_current(
        &mut self,
        _c: &mut PgConnection,
        _prepared: &Self::Prepared,
    ) -> Result<(), Error> {
        Ok(())
    }
}

pub async fn reopen_period(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    period_id: &str,
    input: &PeriodCloseProofInput,
    can_approve: bool,
    now: i64,
) -> Result<PeriodCloseResult, Error> {
    serializable(c).await?;
    require_company(actor)?;
    if !can_approve {
        return Err(forbidden(
            "FINANCE_APPROVAL_REQUIRED",
            "Use a current Finance approver to reopen the period.",
        ));
    }
    if input.proof.trim().is_empty() || input.proof.trim().len() > 2000 {
        return Err(invalid(
            "INVALID_CLOSE_PROOF",
            "Record the review evidence before reopening the period.",
        ));
    }
    if !input.attachment_set_ids.is_empty() {
        return Err(invalid(
            "ATTACHMENT_ACTION_UNSUPPORTED",
            "Close workpapers bind only to the close approval, not to reopening a period.",
        ));
    }
    let operation = "finance.period.reopen";
    let input_json = serde_json::to_string(input).map_err(|_| unavailable())?;
    let request_hash = fingerprint(actor, operation, period_id, &input_json);
    if let Some(saved) = replay::<PeriodCloseResult>(c, command, operation, &request_hash).await? {
        return Ok(saved);
    }
    let period = period_row(c, &actor.company_id, period_id, true).await?;
    if text(&period, "state") != "closed" {
        return Err(not_found("PERIOD_NOT_CLOSED", "Closed period not found."));
    }
    if text(&period, "closed_by") == actor.user_id {
        return Err(conflict(
            "CLOSE_APPROVAL_SEPARATION",
            "A different approver must reopen this period.",
        ));
    }
    let version = input
        .expected_version
        .unwrap_or_else(|| number(&period, "close_version"));
    if version != number(&period, "close_version") {
        return Err(conflict(
            "VERSION_CONFLICT",
            "The close changed. Refresh and review again.",
        ));
    }
    let next_version = version + 1;
    sqlx::query(
        "UPDATE finance_periods SET state='open',closed_by=NULL,closed_at=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=$1 AND company_id=$2 AND state='closed' AND close_version=$3",
    )
    .bind(period_id)
    .bind(&actor.company_id)
    .bind(version)
    .execute(&mut *c)
    .await?;
    let result = PeriodCloseResult {
        id: period_id.to_owned(),
        state: "open".into(),
        version: next_version,
        reconciliation_hash: String::new(),
        readiness: PeriodCloseReadiness::default(),
        reconciliations: Vec::new(),
    };
    let authority = authority_json(actor);
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        period_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        next_version,
        now,
    )
    .await?;
    Ok(result)
}
