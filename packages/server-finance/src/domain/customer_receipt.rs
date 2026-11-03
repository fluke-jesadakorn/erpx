//! Immutable customer receipt rendering, preserving the retained Finance HTML.
use erpx_server_shared::public::BusinessError;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerReceiptAllocation {
    pub invoice_id: String,
    pub invoice_number: String,
    pub amount_minor: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerReceiptSnapshot {
    pub schema: String,
    pub document_number: String,
    pub company_name: String,
    pub customer_name: String,
    pub customer_code: String,
    pub date: String,
    pub currency: String,
    pub amount_minor: String,
    pub payment_id: String,
    pub payment_version: i64,
    pub bank_reference: String,
    pub journal_id: String,
    pub allocations: Vec<CustomerReceiptAllocation>,
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_RECEIPT_INVALID",
        "Use the actual posted receipt and its retained allocations.",
    )
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn units(value: &str) -> Result<BigInt, BusinessError> {
    if value.len() > 64 {
        return Err(invalid());
    }
    value.parse().map_err(|_| invalid())
}
fn money(value: &BigInt) -> String {
    format!("{}.{:0>2}", value / 100_u8, (value % 100_u8).to_string())
}
pub fn render_customer_receipt(
    snapshot: &CustomerReceiptSnapshot,
) -> Result<String, BusinessError> {
    if snapshot.schema != "finance.customer-receipt.v1" || snapshot.allocations.len() > 1000 {
        return Err(invalid());
    }
    let total_units = units(&snapshot.amount_minor)?;
    let mut allocated = BigInt::from(0);
    let mut rows = String::new();
    for row in &snapshot.allocations {
        let amount = units(&row.amount_minor)?;
        allocated += &amount;
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            escape(&row.invoice_number),
            money(&amount)
        ));
    }
    if total_units > allocated {
        rows.push_str(&format!(
            "<tr><td>Unallocated customer receipt</td><td>{}</td></tr>",
            money(&(&total_units - allocated))
        ));
    }
    let header = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Receipt {documentNumber}</title><style>body{{font-family:Arial,sans-serif;color:#172033;margin:40px}}header{{display:flex;justify-content:space-between}}h1{{font-size:30px}}p{{line-height:1.6}}table{{width:100%;border-collapse:collapse;margin:30px 0}}td,th{{padding:12px;border-bottom:1px solid #ddd;text-align:left}}td:last-child,th:last-child{{text-align:right}}.total{{text-align:right;font-size:22px}}footer{{font-size:12px;color:#555}}@page{{size:A4;margin:15mm}}@media print{{body{{margin:0}}}}</style></head><body><header><div><h2>{companyName}</h2><p>Received from<br><strong>{customerName}</strong><br>{customerCode}</p></div><div><h1>Receipt</h1><p>{documentNumber}<br>{date}</p></div></header><table><thead><tr><th>Invoice</th><th>Amount ({currency})</th></tr></thead><tbody>",
        companyName = escape(&snapshot.company_name),
        currency = escape(&snapshot.currency),
        customerCode = escape(&snapshot.customer_code),
        customerName = escape(&snapshot.customer_name),
        date = escape(&snapshot.date),
        documentNumber = escape(&snapshot.document_number)
    );
    let footer = format!(
        "</tbody></table><p class=\"total\">Received {currency} {total}</p><p>Payment reference: {bankReference}</p><footer>This receipt records posted customer money. VAT documents are issued separately. Issued copies retain their original contents; check the receipt record for subsequent cancellation.</footer></body></html>",
        bankReference = escape(&snapshot.bank_reference),
        currency = escape(&snapshot.currency),
        total = money(&total_units)
    );
    let html = header + &rows + &footer;
    if html.len() > 2 * 1024 * 1024 {
        return Err(invalid());
    }
    Ok(html)
}
