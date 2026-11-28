//! Day-close report calculation over one transaction's ordered source snapshot.
//!
//! `sources_json` must be the JavaScript `JSON.stringify` byte representation of
//! the nine source arrays in the retained report's property and row order. In
//! particular PostgreSQL JSONB key sorting and serde's map order cannot be
//! substituted for that wire representation: existing prepared closes store a
//! digest of these exact bytes. The transaction adapter must also obtain all
//! cross-owner rows from their owning ports in the same unit of work.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosDaySaleSource {
    /// JSON fields in the database's POS sale column order.
    pub row_json: String,
    pub reversal_entry_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PosDayOwnedSources {
    /// POS sales rows, before PMS room_charge_id and reversal fields are appended.
    pub sales: Vec<PosDaySaleSource>,
    pub shifts: Vec<String>,
    pub payments: Vec<String>,
    pub refunds: Vec<String>,
    pub refund_payments: Vec<String>,
    /// Company/date candidates: caller must filter using the actual Commerce
    /// branch warehouse witness, preserving their id order.
    pub stock_move_candidates: Vec<String>,
    pub kitchen: Vec<String>,
    pub pending_cash: Vec<String>,
    pub pending_payouts: Vec<String>,
    pub currency: Option<String>,
}

pub struct DayReportInput {
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub currency: String,
    pub sources_json: String,
    pub cost_minor: i64,
    pub waste_minor: i64,
    pub pending_payouts: Value,
    pub stripe_clearing_minor: i64,
}

pub struct DayReport {
    pub value: Value,
    /// Input to SHA-256, retained to verify compatibility with existing closes.
    pub digest_input: String,
    pub digest: String,
}

fn bad_source() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_SOURCE_INVALID",
        "The POS day report source could not be verified.",
    )
}
fn field<'a>(row: &'a Value, key: &str) -> &'a Value {
    row.get(key).unwrap_or(&Value::Null)
}
fn text(row: &Value, key: &str) -> String {
    match field(row, key) {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}
fn number(row: &Value, key: &str) -> Result<i64, BusinessError> {
    match field(row, key) {
        Value::Null => Ok(0),
        Value::Number(n) => n.as_i64().ok_or_else(bad_source),
        Value::String(s) if s.is_empty() => Ok(0),
        Value::String(s) => s.parse().map_err(|_| bad_source()),
        Value::Bool(b) => Ok(i64::from(*b)),
        _ => Err(bad_source()),
    }
}
fn add(a: i64, b: i64) -> Result<i64, BusinessError> {
    let n = a.checked_add(b).ok_or_else(bad_source)?;
    if n.unsigned_abs() > 9_007_199_254_740_991 {
        return Err(bad_source());
    }
    Ok(n)
}
fn sum(rows: &[Value], key: &str) -> Result<i64, BusinessError> {
    rows.iter()
        .try_fold(0, |total, row| add(total, number(row, key)?))
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn array<'a>(sources: &'a Value, key: &str) -> Result<&'a [Value], BusinessError> {
    sources
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(bad_source)
}
fn totals_json(
    fields: &[(&str, i64)],
    received: &[(String, i64)],
    returned: &[(String, i64)],
) -> Result<String, BusinessError> {
    let mut parts: Vec<String> = fields.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
    for (key, values) in [("received", received), ("returned", returned)] {
        let mut entries = Vec::new();
        for (method, amount) in values {
            entries.push(format!(
                "{}:{amount}",
                serde_json::to_string(method).map_err(|_| bad_source())?
            ));
        }
        parts.push(format!("\"{key}\":{{{}}}", entries.join(",")));
    }
    Ok(format!("{{{}}}", parts.join(",")))
}
fn aggregate_methods(rows: &[Value]) -> Result<Vec<(String, i64)>, BusinessError> {
    let mut out: Vec<(String, i64)> = Vec::new();
    for row in rows.iter().filter(|r| text(r, "state") == "succeeded") {
        let key = text(row, "method");
        let amount = number(row, "amount_minor")?;
        if let Some((_, value)) = out.iter_mut().find(|(method, _)| method == &key) {
            *value = add(*value, amount)?;
        } else {
            out.push((key, add(0, amount)?));
        }
    }
    Ok(out)
}
fn block(blockers: &mut Vec<Value>, code: &str, row: &Value, message: String) {
    blockers.push(json!({"code":code,"id":text(row,"id"),"message":message}));
}

/// Calculate the full retained POS day report and hash its legacy-compatible
/// digest input. All source rows and cross-owner facts must be transaction-bound.
pub fn calculate_day_report(
    input: DayReportInput,
    hash: impl FnOnce(&str) -> String,
) -> Result<DayReport, BusinessError> {
    let s: Value = serde_json::from_str(&input.sources_json).map_err(|_| bad_source())?;
    let sales = array(&s, "sales")?;
    let shifts = array(&s, "shifts")?;
    let payments = array(&s, "payments")?;
    let refunds = array(&s, "refunds")?;
    let refund_payments = array(&s, "refundPayments")?;
    let _stock_moves = array(&s, "stockMoves")?;
    let journals = array(&s, "journals")?;
    let kitchen = array(&s, "kitchen")?;
    let pending_cash = array(&s, "pendingCash")?;
    if s.as_object().is_none_or(|o| o.len() != 9) || !input.pending_payouts.is_array() {
        return Err(bad_source());
    }
    let mut blockers = Vec::new();
    for row in sales
        .iter()
        .filter(|r| ["open", "held", "payment_pending"].contains(&text(r, "state").as_str()))
    {
        block(
            &mut blockers,
            "OPEN_SALE",
            row,
            format!("จัดการบิล {} ให้เสร็จ", text(row, "number")),
        );
    }
    for row in sales.iter().filter(|r| {
        text(r, "state") == "completed"
            && !truthy(field(r, "room_charge_id"))
            && !truthy(field(r, "invoice_id"))
    }) {
        block(
            &mut blockers,
            "SALE_POSTING",
            row,
            "บิลขายที่สำเร็จต้องมีใบแจ้งหนี้และ journal ต้นทาง".into(),
        );
    }
    for row in shifts.iter().filter(|r| text(r, "state") != "reviewed") {
        block(
            &mut blockers,
            "SHIFT_REVIEW",
            row,
            "ปิดกะและให้ผู้ตรวจอีกคนตรวจยอดเงินสด".into(),
        );
    }
    for row in payments
        .iter()
        .filter(|r| ["requested", "pending", "exception"].contains(&text(r, "state").as_str()))
    {
        block(
            &mut blockers,
            if text(row, "method") == "transfer" {
                "TRANSFER_EVIDENCE"
            } else {
                "PROVIDER_POSTING"
            },
            row,
            "ยืนยันหลักฐานรับเงินและลงบัญชีให้สำเร็จ".into(),
        );
    }
    for row in payments
        .iter()
        .filter(|r| text(r, "state") == "succeeded" && !truthy(field(r, "finance_payment_id")))
    {
        block(
            &mut blockers,
            "RECEIPT_POSTING",
            row,
            "รับเงินสำเร็จแต่ยังไม่มีใบรับเงิน Finance".into(),
        );
    }
    for row in refunds.iter().filter(|r| {
        ["requested", "approved", "processing", "exception"].contains(&text(r, "state").as_str())
    }) {
        block(
            &mut blockers,
            "REFUND_PENDING",
            row,
            "จัดการรายการคืนเงินค้าง".into(),
        );
    }
    for row in kitchen {
        block(
            &mut blockers,
            "KITCHEN_PENDING",
            row,
            "ส่งมอบหรือยกเลิกรายการครัวพร้อมบันทึกของเสีย".into(),
        );
    }
    for row in journals.iter().filter(|r| text(r, "status") != "posted") {
        block(
            &mut blockers,
            "JOURNAL_PENDING",
            row,
            "Finance ต้องลงบัญชีรายการต้นทาง".into(),
        );
    }
    for row in pending_cash {
        block(
            &mut blockers,
            "CASH_REVIEW",
            row,
            "อนุมัติเงินเข้า/ออกลิ้นชัก".into(),
        );
    }
    let completed: Vec<_> = sales
        .iter()
        .filter(|r| text(r, "state") == "completed" && !truthy(field(r, "room_charge_id")))
        .cloned()
        .collect();
    let completed_refunds: Vec<_> = refunds
        .iter()
        .filter(|r| text(r, "state") == "completed" && text(r, "kind") == "sale")
        .cloned()
        .collect();
    let room: Vec<_> = sales
        .iter()
        .filter(|r| {
            text(r, "state") == "completed"
                && truthy(field(r, "room_charge_id"))
                && !truthy(field(r, "reversal_entry_id"))
        })
        .cloned()
        .collect();
    let subtotal = sum(&completed, "subtotal_minor")?;
    let discount = sum(&completed, "discount_minor")?;
    let tax = sum(&completed, "tax_minor")?;
    let refund = sum(&completed_refunds, "amount_minor")?;
    let refund_tax = sum(&completed_refunds, "tax_minor")?;
    let received = aggregate_methods(payments)?;
    let returned = aggregate_methods(refund_payments)?;
    let bills = i64::try_from(completed.len()).map_err(|_| bad_source())?;
    let gross = add(subtotal, discount)?;
    let net = add(sum(&completed, "total_minor")?, -refund)?;
    let net_tax = add(tax, -refund_tax)?;
    let variance = sum(shifts, "variance_minor")?;
    let room_total = sum(&room, "total_minor")?;
    let digest_totals = totals_json(
        &[
            ("bills", bills),
            ("grossMinor", gross),
            ("discountMinor", discount),
            ("salesMinor", subtotal),
            ("taxMinor", tax),
            ("refundMinor", refund),
            ("netMinor", net),
            ("costMinor", input.cost_minor),
            ("wasteMinor", input.waste_minor),
            ("cashVarianceMinor", variance),
            ("roomChargeMinor", room_total),
        ],
        &received,
        &returned,
    )?;
    let digest_input = format!(
        "{{\"sources\":{},\"totals\":{}}}",
        input.sources_json, digest_totals
    );
    let digest = hash(&digest_input);
    if digest.is_empty() {
        return Err(bad_source());
    }
    let received_json: Value =
        serde_json::from_str(&totals_json(&[], &received, &returned)?).map_err(|_| bad_source())?;
    let totals = json!({"bills":bills,"grossMinor":gross,"discountMinor":discount,"salesMinor":subtotal,"taxMinor":tax,"refundMinor":refund,"refundTaxMinor":refund_tax,"netTaxMinor":net_tax,"netMinor":net,"costMinor":input.cost_minor,"wasteMinor":input.waste_minor,"cashVarianceMinor":variance,"roomChargeMinor":room_total,"received":received_json["received"],"returned":received_json["returned"]});
    let value = json!({"companyId":input.company_id,"branchId":input.branch_id,"businessDate":input.business_date,"currency":input.currency,"totals":totals,"blockers":blockers,"sources":s,"pendingPayouts":input.pending_payouts,"stripeClearingMinor":input.stripe_clearing_minor,"digest":digest});
    Ok(DayReport {
        value,
        digest_input,
        digest,
    })
}
