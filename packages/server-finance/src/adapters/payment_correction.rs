//! Native Finance payment allocation and independent reversal. The caller owns
//! current IAM, the purchase witness, the Workflow receipt and the transaction;
//! this adapter owns the Finance source guards and writer transitions.
use crate::{application::journal::FinanceActor, domain::service_receipt::service_receipt_amount};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentAllocationInput {
    pub invoice_id: String,
    pub amount: String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentReversalInput {
    pub expected_version: i64,
    pub period_id: String,
    pub entry_date: String,
    pub reason: String,
}

fn invalid(code: &'static str, message: &'static str) -> Error {
    BusinessError::invalid(code, message).into()
}

fn conflict(code: &'static str, message: &'static str) -> Error {
    BusinessError::new(FailureKind::Conflict, code, message).into()
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

/// Take the ordered Finance payment/invoice/journal writer leases before the
/// authorizer's first snapshot. Shared table gates match the retained command
/// order without taking ownership outside this transaction.
pub async fn lock_payment_correction_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_payments,finance_invoices,finance_journals,\
         finance_journal_lines,finance_allocations,finance_payment_reversals,\
         finance_periods,finance_bank_transactions,finance_billing_corrections,\
         command_receipts,audit_events,outbox_events IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

/// Allocate part or all of a settled payment to one open invoice. The payment
/// journal and the target invoice must both already be posted in the same
/// Company and currency; the command receipt is written by the caller.
pub async fn allocate_payment(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment_id: &str,
    input: &PaymentAllocationInput,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    serializable(c).await?;
    let amount = service_receipt_amount(&input.amount)?;
    if amount <= 0 || input.expected_version <= 0 || input.invoice_id.is_empty() {
        return Err(invalid("INVALID_ALLOCATION", "Allocation must be positive."));
    }
    let reserved: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=$1 \
         AND status IN ('reserved','exported','unknown'))",
    )
    .bind(&input.invoice_id)
    .fetch_one(&mut *c)
    .await?;
    if reserved {
        return Err(conflict(
            "PAYMENT_SUPPLIER_RESERVED",
            "The invoice is reserved by a supplier payment instruction.",
        ));
    }
    // The retained target assertion keeps the payment journal posted, the
    // invoice open, the party/currency/kind aligned and both outstanding
    // balances sufficient after the expected payment version check.
    let targets: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_payments p \
         JOIN finance_invoices i ON i.id=$1 \
         JOIN finance_journals j ON j.id=p.journal_id \
         WHERE p.id=$2 AND p.company_id=$3 \
         AND i.company_id=p.company_id AND i.source_type!='expense_claim' \
         AND p.party_id=i.party_id AND p.currency=i.currency \
         AND p.version=$4 AND p.status IN ('released','settled') \
         AND j.status='posted' AND i.status IN ('posted','partially_paid') \
         AND ((p.kind='receipt' AND i.kind='receivable') OR (p.kind='payment' AND i.kind='payable')) \
         AND p.outstanding_minor>=$5 AND i.outstanding_minor>=$5)",
    )
    .bind(&input.invoice_id)
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .bind(input.expected_version)
    .bind(amount)
    .fetch_one(&mut *c)
    .await?;
    if !targets {
        return Err(conflict(
            "PAYMENT_ALLOCATION_INVALID",
            "The payment and invoice no longer permit this allocation.",
        ));
    }
    let history: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_payments p \
         JOIN finance_journals pj ON pj.id=p.journal_id \
         JOIN finance_periods period ON period.id=pj.period_id \
         JOIN finance_charts c ON c.id=pj.chart_id \
         JOIN finance_invoices i ON i.id=$1 \
         JOIN finance_journals ij ON ij.id=i.journal_id \
         WHERE p.id=$2 AND p.company_id=$3 \
         AND period.company_id=p.company_id AND period.state='open' \
         AND pj.entry_date>=ij.entry_date AND pj.chart_id=ij.chart_id \
         AND p.currency=c.base_currency AND i.fx_rate_micros=1000000 \
         AND COALESCE(p.fx_rate_micros,1000000)=1000000 \
         AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=pj.id \
         AND l.account_id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END))",
    )
    .bind(&input.invoice_id)
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    if !history {
        return Err(conflict(
            "PAYMENT_HISTORY_INVALID",
            "The payment journal cannot allocate this invoice history.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,\
         base_invoice_minor,base_payment_minor,created_by,created_at) \
         VALUES($1,$2,$3,$4,$5,$5,$5,$6,$7)",
    )
    .bind(format!("allocation-{key}"))
    .bind(actor.company_id.as_str())
    .bind(payment_id)
    .bind(&input.invoice_id)
    .bind(amount)
    .bind(actor.user_id.as_str())
    .bind(now)
    .execute(&mut *c)
    .await?;
    let paid = sqlx::query(
        "UPDATE finance_payments SET outstanding_minor=outstanding_minor-$1,\
         status=CASE WHEN outstanding_minor=$1 THEN 'settled' ELSE status END,\
         version=version+1,updated_at=$2 \
         WHERE id=$3 AND company_id=$4 AND version=$5 \
         AND outstanding_minor>=$1 AND status IN ('released','settled')",
    )
    .bind(amount)
    .bind(now)
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if paid.rows_affected() != 1 {
        return Err(conflict(
            "PAYMENT_SOURCE_CHANGED",
            "The payment changed before the allocation committed.",
        ));
    }
    let invoiced = sqlx::query(
        "UPDATE finance_invoices SET outstanding_minor=outstanding_minor-$1,\
         base_outstanding_minor=GREATEST(0,base_outstanding_minor-$1),\
         status=CASE WHEN outstanding_minor=$1 THEN 'paid' ELSE 'partially_paid' END,\
         version=version+1,updated_at=$2 \
         WHERE id=$3 AND company_id=$4 AND outstanding_minor>=$1",
    )
    .bind(amount)
    .bind(now)
    .bind(&input.invoice_id)
    .bind(actor.company_id.as_str())
    .execute(&mut *c)
    .await?;
    if invoiced.rows_affected() != 1 {
        return Err(conflict(
            "INVOICE_BALANCE_CONFLICT",
            "The invoice balance changed before the allocation committed.",
        ));
    }
    let payment_version: i64 = sqlx::query_scalar(
        "SELECT version FROM finance_payments WHERE id=$1 AND company_id=$2",
    )
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    let invoice_version: i64 = sqlx::query_scalar(
        "SELECT version FROM finance_invoices WHERE id=$1 AND company_id=$2",
    )
    .bind(&input.invoice_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    let allocated = crate::domain::money::units_to_decimal(&amount.into(), 2)
        .map_err(|_| invalid("MONEY_OVERFLOW", "Money exceeds supported precision."))?;
    Ok(json!({
        "paymentId": payment_id,
        "invoiceId": input.invoice_id,
        "allocated": allocated,
        "paymentVersion": payment_version,
        "invoiceVersion": invoice_version
    }))
}

struct ReversalPayment {
    created_by: String,
    status: String,
    version: i64,
    journal_id: String,
    journal_status: String,
    journal_entry_date: String,
    fx_journal_id: Option<String>,
}

/// Reverse a settled, journal-posted payment by a different finance approver.
/// Restores every allocated invoice, posts a balanced reversal journal (and FX
/// reversal when present), unmatches the bank row, voids the payment and
/// records the correction. The caller writes the command receipt.
pub async fn reverse_payment(
    c: &mut PgConnection,
    actor: &FinanceActor,
    payment_id: &str,
    input: &PaymentReversalInput,
    key: &str,
    now: i64,
) -> Result<Value, Error> {
    serializable(c).await?;
    if input.expected_version <= 0
        || input.period_id.is_empty()
        || input.entry_date.is_empty()
        || input.reason.trim().len() < 3
    {
        return Err(invalid(
            "PAYMENT_REVERSAL_INVALID",
            "A reversal needs an open period, an entry date and a reason.",
        ));
    }
    let row = sqlx::query(
        "SELECT p.created_by,p.status,p.version,p.journal_id,p.fx_journal_id,p.amount_minor,\
         j.status journal_status,j.entry_date journal_entry_date \
         FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id \
         AND j.company_id=p.company_id WHERE p.id=$1 AND p.company_id=$2 FOR UPDATE OF p,j",
    )
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "POSTED_PAYMENT_NOT_FOUND",
            "A posted payment with accounting evidence was not found.",
        )
    })?;
    use sqlx::Row;
    let payment = ReversalPayment {
        created_by: row.try_get("created_by")?,
        status: row.try_get("status")?,
        version: row.try_get("version")?,
        journal_id: row.try_get("journal_id")?,
        journal_status: row.try_get("journal_status")?,
        journal_entry_date: row.try_get("journal_entry_date")?,
        fx_journal_id: row.try_get("fx_journal_id")?,
    };
    if payment.created_by == actor.user_id.as_str() {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "INDEPENDENT_CORRECTION_REQUIRED",
            "A different finance approver must reverse the payment.",
        )
        .into());
    }
    if payment.status != "settled" || payment.journal_status != "posted" {
        return Err(conflict(
            "PAYMENT_NOT_REVERSIBLE",
            "Only a settled payment with a posted journal can be reversed.",
        ));
    }
    if payment.version != input.expected_version {
        return Err(conflict(
            "PAYMENT_SOURCE_CHANGED",
            "The payment changed before the correction committed.",
        ));
    }
    if input.entry_date < payment.journal_entry_date {
        return Err(invalid(
            "REVERSAL_BEFORE_SOURCE",
            "The reversal cannot precede the payment date.",
        ));
    }
    let matched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM finance_bank_transactions WHERE company_id=$1 \
         AND matched_type='payment' AND matched_id=$2 AND match_status='matched'",
    )
    .bind(actor.company_id.as_str())
    .bind(payment_id)
    .fetch_one(&mut *c)
    .await?;
    if matched > 1 {
        return Err(conflict(
            "PAYMENT_BANK_MATCH_CONFLICT",
            "More than one bank row matches this payment.",
        ));
    }
    let period: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_periods WHERE id=$1 AND company_id=$2 \
         AND state='open' AND start_date<=$3 AND end_date>=$3)",
    )
    .bind(&input.period_id)
    .bind(actor.company_id.as_str())
    .bind(&input.entry_date)
    .fetch_one(&mut *c)
    .await?;
    if !period {
        return Err(conflict(
            "REVERSAL_PERIOD_CLOSED",
            "Select an open Finance period that contains the reversal date.",
        ));
    }
    let reversed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_payment_reversals WHERE payment_id=$1)",
    )
    .bind(payment_id)
    .fetch_one(&mut *c)
    .await?;
    if reversed {
        return Err(conflict(
            "PAYMENT_ALREADY_REVERSED",
            "This payment already has a correction.",
        ));
    }
    let credited: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM finance_billing_corrections c \
         JOIN finance_allocations a ON a.invoice_id=c.invoice_id \
         WHERE a.payment_id=$1 AND c.company_id=$2 AND c.status='posted')",
    )
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    if credited {
        return Err(conflict(
            "PAYMENT_CREDITED_SOURCE_REVIEW",
            "Reconcile the credited bill and its refund before correcting the original receipt.",
        ));
    }
    let allocations = sqlx::query(
        "SELECT a.id,a.invoice_id,a.amount_minor,a.base_invoice_minor,\
         i.outstanding_minor,i.base_outstanding_minor,i.total_minor,i.base_total_minor,i.source_type \
         FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id \
         AND i.company_id=a.company_id WHERE a.payment_id=$1 AND a.company_id=$2 \
         ORDER BY a.id FOR UPDATE OF a,i",
    )
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .fetch_all(&mut *c)
    .await?;
    if allocations.is_empty() {
        return Err(conflict(
            "PAYMENT_ALLOCATION_REQUIRED",
            "Only an allocated payment can be corrected here.",
        ));
    }
    struct Allocation {
        id: String,
        invoice_id: String,
        amount: i64,
        base_amount: i64,
        outstanding: i64,
        total: i64,
        source_type: String,
    }
    let allocations = allocations
        .iter()
        .map(|row| {
            Ok::<_, sqlx::Error>(Allocation {
                id: row.try_get("id")?,
                invoice_id: row.try_get("invoice_id")?,
                amount: row.try_get("amount_minor")?,
                base_amount: row
                    .try_get::<Option<i64>, _>("base_invoice_minor")?
                    .unwrap_or(row.try_get("amount_minor")?),
                outstanding: row.try_get("outstanding_minor")?,
                total: row.try_get("total_minor")?,
                source_type: row.try_get("source_type")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for allocation in &allocations {
        if matches!(allocation.source_type.as_str(), "expense_claim" | "pms") {
            return Err(conflict(
                "REVERSE_AT_SOURCE",
                "Reverse this payment from its originating module.",
            ));
        }
        if allocation.outstanding + allocation.amount > allocation.total {
            return Err(conflict(
                "INVOICE_BALANCE_CONFLICT",
                "The allocated invoice balance no longer permits this reversal.",
            ));
        }
    }
    let reversal = format!("payment-reversal-{key}");
    let fx_reversal = payment
        .fx_journal_id
        .as_ref()
        .map(|_| format!("payment-fx-reversal-{key}"));
    let journal = sqlx::query(
        "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,\
         entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,\
         posted_by,posted_at,created_at,updated_at) \
         SELECT $1,company_id,chart_id,$2,$3,$4,currency,fx_rate_micros,'payment_reversal',$5,$6,\
         'posted',$7,$7,$8,$8,$8 FROM finance_journals WHERE id=$9 AND company_id=$10",
    )
    .bind(&reversal)
    .bind(&input.period_id)
    .bind(format!("PAY-REV-{key}"))
    .bind(&input.entry_date)
    .bind(payment_id)
    .bind(&input.reason)
    .bind(actor.user_id.as_str())
    .bind(now)
    .bind(&payment.journal_id)
    .bind(actor.company_id.as_str())
    .execute(&mut *c)
    .await?;
    if journal.rows_affected() != 1 {
        return Err(conflict(
            "PAYMENT_JOURNAL_UNAVAILABLE",
            "The payment journal is no longer posted.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,\
         credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) \
         SELECT $1||'-'||ordinal,$1,account_id,description,credit_minor,debit_minor,\
         base_credit_minor,base_debit_minor,dimension_json,ordinal \
         FROM finance_journal_lines WHERE journal_id=$2",
    )
    .bind(&reversal)
    .bind(&payment.journal_id)
    .execute(&mut *c)
    .await?;
    let reversed_journal = sqlx::query(
        "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=$1,\
         reversed_at=$2,updated_at=$2 WHERE id=$3 AND company_id=$4 AND status='posted'",
    )
    .bind(actor.user_id.as_str())
    .bind(now)
    .bind(&payment.journal_id)
    .bind(actor.company_id.as_str())
    .execute(&mut *c)
    .await?;
    if reversed_journal.rows_affected() != 1 {
        return Err(conflict(
            "PAYMENT_JOURNAL_UNAVAILABLE",
            "The payment journal changed before the correction committed.",
        ));
    }
    if let (Some(fx_journal), Some(fx_reversal)) = (&payment.fx_journal_id, &fx_reversal) {
        sqlx::query(
            "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,\
             entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,\
             posted_by,posted_at,created_at,updated_at) \
             SELECT $1,company_id,chart_id,$2,$3,$4,currency,fx_rate_micros,\
             'payment_fx_reversal',$5,$6,'posted',$7,$7,$8,$8,$8 \
             FROM finance_journals WHERE id=$9 AND company_id=$10 AND status='posted'",
        )
        .bind(fx_reversal)
        .bind(&input.period_id)
        .bind(format!("PAY-FX-REV-{key}"))
        .bind(&input.entry_date)
        .bind(payment_id)
        .bind(&input.reason)
        .bind(actor.user_id.as_str())
        .bind(now)
        .bind(fx_journal)
        .bind(actor.company_id.as_str())
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,\
             credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) \
             SELECT $1||'-'||ordinal,$1,account_id,description,credit_minor,debit_minor,\
             base_credit_minor,base_debit_minor,dimension_json,ordinal \
             FROM finance_journal_lines WHERE journal_id=$2",
        )
        .bind(fx_reversal)
        .bind(fx_journal)
        .execute(&mut *c)
        .await?;
        sqlx::query(
            "UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=$1,\
             reversed_at=$2,updated_at=$2 WHERE id=$3 AND company_id=$4 AND status='posted'",
        )
        .bind(actor.user_id.as_str())
        .bind(now)
        .bind(fx_journal)
        .bind(actor.company_id.as_str())
        .execute(&mut *c)
        .await?;
    }
    for allocation in &allocations {
        let restored = sqlx::query(
            "UPDATE finance_invoices SET outstanding_minor=outstanding_minor+$1,\
             base_outstanding_minor=base_outstanding_minor+$2,\
             status=CASE WHEN outstanding_minor+$1=total_minor THEN 'posted' ELSE 'partially_paid' END,\
             version=version+1,updated_at=$3 \
             WHERE id=$4 AND company_id=$5 AND outstanding_minor=$6",
        )
        .bind(allocation.amount)
        .bind(allocation.base_amount)
        .bind(now)
        .bind(&allocation.invoice_id)
        .bind(actor.company_id.as_str())
        .bind(allocation.outstanding)
        .execute(&mut *c)
        .await?;
        if restored.rows_affected() != 1 {
            return Err(conflict(
                "INVOICE_BALANCE_CONFLICT",
                "An allocated invoice changed before the correction committed.",
            ));
        }
        // Keep the allocation identifier in the error surface when a future
        // close/commerce guard needs the exact row. The current guard does not.
        let _ = &allocation.id;
    }
    sqlx::query(
        "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,\
         matched_id=NULL,version=version+1 WHERE company_id=$1 AND matched_type='payment' \
         AND matched_id=$2 AND match_status='matched'",
    )
    .bind(actor.company_id.as_str())
    .bind(payment_id)
    .execute(&mut *c)
    .await?;
    let voided = sqlx::query(
        "UPDATE finance_payments SET status='voided',version=version+1,updated_at=$1 \
         WHERE id=$2 AND company_id=$3 AND version=$4 AND status='settled'",
    )
    .bind(now)
    .bind(payment_id)
    .bind(actor.company_id.as_str())
    .bind(input.expected_version)
    .execute(&mut *c)
    .await?;
    if voided.rows_affected() != 1 {
        return Err(conflict(
            "PAYMENT_SOURCE_CHANGED",
            "The payment changed before the correction committed.",
        ));
    }
    sqlx::query(
        "INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,\
         fx_reversal_journal_id,reason,reversed_by,reversed_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(format!("payment-correction-{key}"))
    .bind(payment_id)
    .bind(&reversal)
    .bind(&fx_reversal)
    .bind(&input.reason)
    .bind(actor.user_id.as_str())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(json!({
        "id": payment_id,
        "status": "voided",
        "version": input.expected_version + 1,
        "reversalJournalId": reversal,
        "fxReversalJournalId": fx_reversal,
        "restoredAllocations": allocations.len()
    }))
}
