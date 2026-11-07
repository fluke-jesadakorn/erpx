//! Native standalone customer-receipt detail, export and issue. Field-owned
//! receipts retain their existing native Service interface; this slice handles
//! the remaining Company receipts and keeps the immutable issued copy.
use super::sales_invoice_create::{
    SalesInvoiceCommandActor, authority_json, fingerprint, receipt,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesReceiptIssueInput {
    pub payment_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesReceiptRead {
    pub sha256: String,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub receipt: Value,
    #[cfg_attr(feature = "contracts", ts(type = "Record<string, unknown>"))]
    #[cfg_attr(feature = "contracts", schema(value_type = Object))]
    pub snapshot: Value,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesReceiptExport {
    pub content: String,
    pub sha256: String,
    pub document_number: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SalesReceiptIssueResult {
    pub id: String,
}

fn unavailable() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_SALES_RECEIPT_UNAVAILABLE",
        "The customer receipt is unavailable.",
    )
    .into()
}

fn not_found() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "RECEIPT_NOT_FOUND",
        "Receipt not found in this workspace.",
    )
    .into()
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

pub async fn lock_sales_receipt_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE finance_customer_receipts,finance_payments,finance_journals,\
         finance_allocations,finance_invoices,finance_parties,finance_periods,\
         finance_sales_number_sequences IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

fn parse(value: String) -> Result<Value, Error> {
    serde_json::from_str(&value).map_err(|_| unavailable())
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

fn money(value: &str) -> Result<String, Error> {
    let units = value
        .parse::<i128>()
        .map_err(|_| unavailable())?;
    let negative = units < 0;
    let absolute = units.abs();
    Ok(format!(
        "{}{}.{:02}",
        if negative { "-" } else { "" },
        absolute / 100,
        absolute % 100
    ))
}

fn render_receipt(snapshot: &Value) -> Result<String, Error> {
    let text = |key: &str| -> Result<&str, Error> {
        snapshot
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(unavailable)
    };
    let allocations = snapshot
        .get("allocations")
        .and_then(Value::as_array)
        .ok_or_else(unavailable)?;
    let mut rows = String::new();
    let mut allocated = 0i128;
    for allocation in allocations {
        let number = allocation
            .get("invoiceNumber")
            .and_then(Value::as_str)
            .ok_or_else(unavailable)?;
        let amount = allocation
            .get("amountMinor")
            .and_then(Value::as_str)
            .ok_or_else(unavailable)?;
        allocated = allocated
            .checked_add(amount.parse::<i128>().map_err(|_| unavailable())?)
            .ok_or_else(unavailable)?;
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            escape_html(number),
            money(amount)?
        ));
    }
    let amount = text("amountMinor")?;
    let total = amount.parse::<i128>().map_err(|_| unavailable())?;
    if total > allocated {
        rows.push_str(&format!(
            "<tr><td>Unallocated customer receipt</td><td>{}</td></tr>",
            money(&(total - allocated).to_string())?
        ));
    }
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Receipt {}</title><style>body{{font-family:Arial,sans-serif;color:#172033;margin:40px}}header{{display:flex;justify-content:space-between}}h1{{font-size:30px}}p{{line-height:1.6}}table{{width:100%;border-collapse:collapse;margin:30px 0}}td,th{{padding:12px;border-bottom:1px solid #ddd;text-align:left}}td:last-child,th:last-child{{text-align:right}}.total{{text-align:right;font-size:22px}}footer{{font-size:12px;color:#555}}@page{{size:A4;margin:15mm}}@media print{{body{{margin:0}}}}</style></head><body><header><div><h2>{}</h2><p>Received from<br><strong>{}</strong><br>{}</p></div><div><h1>Receipt</h1><p>{}<br>{}</p></div></header><table><thead><tr><th>Invoice</th><th>Amount ({})</th></tr></thead><tbody>{}</tbody></table><p class=\"total\">Received {} {}</p><p>Payment reference: {}</p><footer>This receipt records posted customer money. VAT documents are issued separately. Issued copies retain their original contents; check the receipt record for subsequent cancellation.</footer></body></html>",
        escape_html(text("documentNumber")?),
        escape_html(text("companyName")?),
        escape_html(text("customerName")?),
        escape_html(text("customerCode")?),
        escape_html(text("documentNumber")?),
        escape_html(text("date")?),
        escape_html(text("currency")?),
        rows,
        escape_html(text("currency")?),
        money(amount)?,
        escape_html(text("bankReference")?),
    ))
}

const RECEIPT_SCOPE: &str = "($3::text IS NULL AND $4::text IS NULL) OR (\
    EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=r.payment_id) \
    AND NOT EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id \
      WHERE a.payment_id=r.payment_id AND NOT (\
        EXISTS(SELECT 1 FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id \
          WHERE f.invoice_id=i.id AND o.company_id=i.company_id \
            AND ($3::text IS NULL OR o.branch_id=$3) \
            AND ($4::text IS NULL OR o.department_id=$4)))))";

pub async fn read_customer_receipt(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    id: &str,
) -> Result<Option<SalesReceiptRead>, Error> {
    serializable(c).await?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT r.id,r.company_id,r.payment_id,r.document_number,r.receipt_date,r.currency,\
         r.amount_minor,r.snapshot_json,r.export_sha256,r.issued_by,r.issued_at,\
         p.status payment_status,p.version payment_version,\
         erpx_finance_field_payment_owned(r.payment_id) field_owned \
         FROM finance_customer_receipts r \
         JOIN finance_payments p ON p.id=r.payment_id AND p.company_id=r.company_id \
         WHERE r.id=$1 AND r.company_id=$2 AND ({RECEIPT_SCOPE})"
    )))
    .bind(id)
    .bind(company)
    .bind(branch)
    .bind(department)
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.try_get::<bool, _>("field_owned")? {
        return Ok(None);
    }
    let snapshot = parse(row.try_get("snapshot_json")?)?;
    let payment_status: String = row.try_get("payment_status")?;
    let status = if matches!(payment_status.as_str(), "cancelled" | "reversed" | "voided") {
        "cancelled"
    } else {
        "issued"
    };
    let receipt = json!({
        "id": row.try_get::<String, _>("id")?,
        "company_id": row.try_get::<String, _>("company_id")?,
        "payment_id": row.try_get::<String, _>("payment_id")?,
        "document_number": row.try_get::<String, _>("document_number")?,
        "receipt_date": row.try_get::<String, _>("receipt_date")?,
        "currency": row.try_get::<String, _>("currency")?,
        "amount_minor": row.try_get::<i64, _>("amount_minor")?,
        "export_sha256": row.try_get::<String, _>("export_sha256")?,
        "issued_by": row.try_get::<String, _>("issued_by")?,
        "issued_at": row.try_get::<i64, _>("issued_at")?,
        "payment_status": payment_status,
        "payment_version": row.try_get::<i64, _>("payment_version")?,
    });
    Ok(Some(SalesReceiptRead {
        sha256: row.try_get("export_sha256")?,
        receipt,
        snapshot,
        status: status.into(),
    }))
}

pub async fn export_customer_receipt(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    department: Option<&str>,
    id: &str,
) -> Result<Option<SalesReceiptExport>, Error> {
    let Some(_) = read_customer_receipt(c, company, branch, department, id).await? else {
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT export_html,export_sha256,document_number FROM finance_customer_receipts \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(not_found)?;
    Ok(Some(SalesReceiptExport {
        content: row.try_get("export_html")?,
        sha256: row.try_get("export_sha256")?,
        document_number: row.try_get("document_number")?,
    }))
}

pub async fn issue_customer_receipt(
    c: &mut PgConnection,
    actor: &SalesInvoiceCommandActor,
    command: &CommandKey,
    payment_id: &str,
    now: i64,
) -> Result<SalesReceiptIssueResult, Error> {
    serializable(c).await?;
    if payment_id.is_empty()
        || payment_id.len() > 200
        || actor.company_id.is_empty()
        || actor.user_id.is_empty()
        || actor.assignment_id.is_empty()
        || actor.branch_id.is_some()
        || actor.department_id.is_some()
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "COMPANY_SCOPE_REQUIRED",
            "Use a Company Finance assignment to issue a customer receipt.",
        )
        .into());
    }
    let field_owned: bool = sqlx::query_scalar("SELECT erpx_finance_field_payment_owned($1)")
        .bind(payment_id)
        .fetch_one(&mut *c)
        .await?;
    if field_owned {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "NATIVE_FINANCE_RECEIPT_REQUIRED",
            "Use the native Field receipt interface.",
        )
        .into());
    }
    let input = SalesReceiptIssueInput {
        payment_id: payment_id.into(),
    };
    let input_json = serde_json::to_string(&input).map_err(|_| unavailable())?;
    let operation = "finance.customer-receipt.issue";
    let request_hash = fingerprint(actor, operation, payment_id, &input_json);
    let authority = authority_json(actor);
    let existing = sqlx::query(
        "SELECT request_hash,result_json FROM command_receipts WHERE command_id=$1 FOR UPDATE",
    )
    .bind(command.as_str())
    .fetch_optional(&mut *c)
    .await?;
    if let Some(row) = existing {
        let saved: String = row.try_get("request_hash")?;
        if saved != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command key belongs to a different request.",
            )
            .into());
        }
        let raw: Option<String> = row.try_get("result_json")?;
        return raw
            .and_then(|value| serde_json::from_str::<SalesReceiptIssueResult>(&value).ok())
            .ok_or_else(unavailable);
    }
    let already: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_customer_receipts WHERE payment_id=$1 AND company_id=$2",
    )
    .bind(payment_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    if let Some(id) = already {
        let result = SalesReceiptIssueResult { id };
        receipt(
            c,
            actor,
            &authority,
            command.as_str(),
            operation,
            &request_hash,
            payment_id,
            &serde_json::to_value(&result).map_err(|_| unavailable())?,
            1,
            now,
        )
        .await?;
        return Ok(result);
    }
    let payment = sqlx::query(
        "SELECT p.id,p.payment_number,p.payment_date,p.currency,p.amount_minor,p.version,p.journal_id,\
         party.name customer_name,party.code customer_code \
         FROM finance_payments p \
         JOIN finance_journals j ON j.id=p.journal_id \
         JOIN finance_parties party ON party.id=p.party_id \
         WHERE p.id=$1 AND p.company_id=$2 AND p.kind='receipt' AND p.status='settled' \
           AND j.status='posted' AND NOT erpx_finance_field_payment_owned(p.id) \
         FOR UPDATE OF p",
    )
    .bind(payment_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::Conflict,
            "POSTED_RECEIPT_REQUIRED",
            "A settled customer receipt with a posted journal is required.",
        )
    })?;
    let after_lock: Option<String> = sqlx::query_scalar(
        "SELECT id FROM finance_customer_receipts WHERE payment_id=$1 AND company_id=$2",
    )
    .bind(payment_id)
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?;
    if let Some(id) = after_lock {
        let result = SalesReceiptIssueResult { id };
        receipt(
            c,
            actor,
            &authority,
            command.as_str(),
            operation,
            &request_hash,
            payment_id,
            &serde_json::to_value(&result).map_err(|_| unavailable())?,
            1,
            now,
        )
        .await?;
        return Ok(result);
    }
    let payment_date: String = payment.try_get("payment_date")?;
    let currency: String = payment.try_get("currency")?;
    let amount_minor: i64 = payment.try_get("amount_minor")?;
    let payment_version: i64 = payment.try_get("version")?;
    let payment_number: String = payment.try_get("payment_number")?;
    let journal_id: String = payment.try_get("journal_id")?;
    let customer_name: String = payment.try_get("customer_name")?;
    let customer_code: String = payment.try_get("customer_code")?;
    let allocations = sqlx::query(
        "SELECT i.id invoice_id,i.invoice_number,a.amount_minor \
         FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id \
         WHERE a.payment_id=$1 AND a.company_id=$2 ORDER BY a.created_at,a.id",
    )
    .bind(payment_id)
    .bind(&actor.company_id)
    .fetch_all(&mut *c)
    .await?;
    let company_name: String = sqlx::query_scalar(
        "SELECT display_name FROM organization_nodes WHERE id=$1 AND kind='company'",
    )
    .bind(&actor.company_id)
    .fetch_optional(&mut *c)
    .await?
    .unwrap_or_else(|| actor.company_id.clone());
    let number: String = sqlx::query_scalar("SELECT erpx_sales_document_number($1,'RC',$2)")
        .bind(&actor.company_id)
        .bind(&payment_date)
        .fetch_one(&mut *c)
        .await?;
    let allocation_values = allocations
        .iter()
        .map(|row| {
            Ok(json!({
                "invoiceId": row.try_get::<String, _>("invoice_id")?,
                "invoiceNumber": row.try_get::<String, _>("invoice_number")?,
                "amountMinor": row.try_get::<i64, _>("amount_minor")?.to_string(),
            }))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let snapshot = json!({
        "schema": "finance.customer-receipt.v1",
        "documentNumber": number,
        "companyName": company_name,
        "customerName": customer_name,
        "customerCode": customer_code,
        "date": payment_date,
        "currency": currency,
        "amountMinor": amount_minor.to_string(),
        "paymentId": payment_id,
        "paymentVersion": payment_version,
        "bankReference": payment_number,
        "journalId": journal_id,
        "allocations": allocation_values,
    });
    let html = render_receipt(&snapshot)?;
    let sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(html.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let id = format!("customer-receipt-{payment_id}");
    sqlx::query(
        "INSERT INTO finance_customer_receipts(id,company_id,payment_id,document_number,receipt_date,\
         currency,amount_minor,snapshot_json,export_html,export_sha256,issued_by,issued_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
    )
    .bind(&id)
    .bind(&actor.company_id)
    .bind(payment_id)
    .bind(&number)
    .bind(&payment_date)
    .bind(&currency)
    .bind(amount_minor)
    .bind(snapshot.to_string())
    .bind(&html)
    .bind(&sha256)
    .bind(&actor.user_id)
    .bind(now)
    .execute(&mut *c)
    .await?;
    let result = SalesReceiptIssueResult { id };
    receipt(
        c,
        actor,
        &authority,
        command.as_str(),
        operation,
        &request_hash,
        payment_id,
        &serde_json::to_value(&result).map_err(|_| unavailable())?,
        1,
        now,
    )
    .await?;
    Ok(result)
}
