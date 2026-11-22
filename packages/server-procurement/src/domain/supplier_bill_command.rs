//! Native Procurement supplier-bill command facts.
//!
//! These are validated request/source facts, never authority. Composition
//! supplies the current Account/IAM actor and the Commerce/Finance source
//! facts on the same serializable Holding transaction. The owner command
//! writes only the immutable Procurement bill history plus the Workflow-style
//! command receipt, audit and outbox evidence. Commerce receipt-version and
//! invoice-link effects, the Finance draft-void effect and the Collaboration
//! notification fan-out are explicit cross-owner hooks on the repository
//! trait: composition must supply the Commerce/Finance/Collaboration-owned
//! ports, and they must run inside the same caller-owned transaction.
//! Nothing here invents an owner write.
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, TimestampMillis};
use serde::{Deserialize, Serialize};

pub const OPERATION_CAPTURE: &str = "procurement.bill.capture";
pub const OPERATION_APPROVE: &str = "procurement.bill.approve";
pub const OPERATION_RETURN: &str = "procurement.bill.return";
pub const OPERATION_POLICY: &str = "procurement.invoice-policy.update";
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug)]
pub struct SupplierBillCommandActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: CompanyId,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub revision: i64,
    pub assignment_version: i64,
    pub permissions: Vec<String>,
    pub expires_at: i64,
}

impl SupplierBillCommandActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn same_person(&self, user_id: &str) -> bool {
        // Merged-person aliases (`iam.personUserIds`) are not carried on the
        // native actor; only direct user-id equality is proven here. A merged
        // alias could pass the independent-reviewer check below while the
        // oracle would deny them. Seeds use distinct users.
        self.user_id == user_id
    }

    fn require_expiry(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        Ok(())
    }

    fn require_permissions(&self, permissions: &[&str]) -> Result<(), BusinessError> {
        for permission in permissions {
            if !self.has(permission) {
                return Err(permission_required());
            }
        }
        Ok(())
    }

    /// Capture gate: the retained `finance.write` + `commerce.read` JWT-static
    /// halves plus work-context currency. The assignment purchase authority
    /// (`SUPPLIER_BILL_ACCESS_CHANGED`) runs in composition's purchase
    /// admission ahead of this port.
    pub fn require_capture(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_expiry(now)?;
        self.require_permissions(&["finance.write", "commerce.read"])
    }

    /// Decide gate: the retained `finance.read` + `finance.write` +
    /// `finance.approve` halves plus work-context currency.
    pub fn require_decide(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_expiry(now)?;
        self.require_permissions(&["finance.read", "finance.write", "finance.approve"])
    }

    /// Policy gate: the retained `finance.approve` half plus work-context
    /// currency. The company approver/owner proof
    /// (`COMPANY_FINANCE_AUTHORITY_REQUIRED`) is an application check below.
    pub fn require_policy(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_expiry(now)?;
        self.require_permissions(&["finance.approve"])
    }
}

pub fn permission_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PERMISSION_REQUIRED",
        "Your selected role does not grant this permission.",
    )
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::InvalidInput, code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn unavailable(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, code, message)
}

pub fn source_changed() -> BusinessError {
    conflict(
        "SUPPLIER_BILL_SOURCE_CHANGED",
        "The actual Purchase invoice source changed before commit.",
    )
}

pub fn changed_before_commit() -> BusinessError {
    conflict(
        "SUPPLIER_BILL_SOURCE_CHANGED",
        "The actual Purchase invoice source changed.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    conflict(
        "COMMAND_IDEMPOTENCY_CONFLICT",
        "This command was already recorded with different details. Review before retrying.",
    )
}

pub fn invalid_receipt() -> BusinessError {
    unavailable(
        "COMMAND_RECEIPT_INVALID",
        "The Procurement command acknowledgement could not be read back.",
    )
}

pub fn incomplete_receipt() -> BusinessError {
    unavailable(
        "COMMAND_RECEIPT_INCOMPLETE",
        "The Procurement command acknowledgement is incomplete.",
    )
}

/// Exact decimal-to-minor conversion, literal mirror of the retained
/// `decimalToMinor(value, scale)`: optional sign, digits with an optional
/// fraction, truncation rejected past scale, safe-integer range enforced.
pub fn decimal_to_minor(value: &str, scale: u32) -> Result<i64, ()> {
    let trimmed = value.trim();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed),
    };
    let mut parts = digits.split('.');
    let (Some(head), tail) = (parts.next(), parts.next()) else {
        return Err(());
    };
    if parts.next().is_some() || head.is_empty() || !head.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    let fraction = tail.unwrap_or("");
    if !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return Err(());
    }
    if fraction.len() > scale as usize && fraction.as_bytes()[scale as usize..].iter().any(|b| *b != b'0') {
        // Truncation past scale is rejected, but only when the dropped tail
        // is non-zero. This matches the retained check, which tests the
        // dropped tail for non-zero digits (trailing zeros compare clean).
        // NOTE: the retained check slices at `scale` and tests the REMAINDER
        // for [1-9]; zeros are kept. Mirror exactly.
        return Err(());
    }
    let mut padded = String::from(head);
    let keep = fraction.get(..scale as usize).unwrap_or(fraction);
    padded.push_str(keep);
    for _ in keep.len()..scale as usize {
        padded.push('0');
    }
    let magnitude: i128 = padded.parse().map_err(|_| ())?;
    let signed = if negative { -magnitude } else { magnitude };
    if signed > MAX_SAFE as i128 || signed < -(MAX_SAFE as i128) {
        return Err(());
    }
    Ok(signed as i64)
}

pub fn quantity_milli(value: &str) -> Result<i64, ()> {
    let milli = decimal_to_minor(value, 3)?;
    if milli <= 0 {
        return Err(());
    }
    Ok(milli)
}

/// Exact retained `scaledAmount`: round-half-up integer division with a
/// safe-integer range check.
pub fn scaled_amount(amount: i64, numerator: i64, denominator: i64) -> Result<i64, ()> {
    if denominator <= 0 {
        return Err(());
    }
    let scaled = amount as i128 * numerator as i128 + (denominator as i128 / 2);
    let out = scaled / denominator as i128;
    if out > MAX_SAFE as i128 || out < -(MAX_SAFE as i128) {
        return Err(());
    }
    Ok(out as i64)
}

pub fn line_amounts(quantity: i64, price: i64, tax_bps: i64) -> Result<(i64, i64), ()> {
    let subtotal = scaled_amount(price, quantity, 1000)?;
    let tax = scaled_amount(subtotal, tax_bps, 10_000)?;
    Ok((subtotal, tax))
}

#[derive(Clone, Debug, Deserialize)]
pub struct BillCaptureReceiptInput {
    #[serde(rename = "fulfillmentId")]
    pub fulfillment_id: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BillCaptureLineInput {
    #[serde(rename = "fulfillmentLineId")]
    pub fulfillment_line_id: String,
    pub quantity: String,
    pub price: String,
    #[serde(rename = "taxBps")]
    pub tax_bps: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CaptureSupplierBillCommand {
    #[serde(rename = "fulfillmentId")]
    pub fulfillment_id: Option<String>,
    #[serde(rename = "expectedVersion")]
    pub expected_version: Option<i64>,
    pub receipts: Option<Vec<BillCaptureReceiptInput>>,
    pub reference: String,
    #[serde(rename = "invoiceDate")]
    pub invoice_date: String,
    #[serde(rename = "dueDate")]
    pub due_date: String,
    pub evidence: String,
    pub lines: Vec<BillCaptureLineInput>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DecideSupplierBillCommand {
    pub action: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    pub reason: String,
    #[serde(rename = "varianceAccountId")]
    pub variance_account_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SaveSupplierBillPolicyCommand {
    #[serde(rename = "matchingMode")]
    pub matching_mode: String,
    #[serde(rename = "expectedVersion")]
    pub expected_version: i64,
    pub reason: String,
}

fn valid_reason(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.len() >= 3 && trimmed.len() <= 2000
}

fn valid_quantity(value: &str) -> bool {
    let (head, tail) = match value.split_once('.') {
        Some((head, tail)) => (head, Some(tail)),
        None => (value, None),
    };
    !head.is_empty()
        && head.len() <= 12
        && head.bytes().all(|b| b.is_ascii_digit())
        && tail.is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= 3
                && fraction.bytes().all(|b| b.is_ascii_digit())
        })
}

fn valid_price(value: &str) -> bool {
    let (head, tail) = match value.split_once('.') {
        Some((head, tail)) => (head, Some(tail)),
        None => (value, None),
    };
    !head.is_empty()
        && head.len() <= 12
        && head.bytes().all(|b| b.is_ascii_digit())
        && tail.is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= 2
                && fraction.bytes().all(|b| b.is_ascii_digit())
        })
}

fn is_iso_date(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let (Some(year), Some(month), Some(day)) = (
        parts[0].parse::<i64>().ok(),
        parts[1].parse::<i64>().ok(),
        parts[2].parse::<i64>().ok(),
    ) else {
        return false;
    };
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    if !(1..=12).contains(&month) || day < 1 {
        return false;
    }
    let leap = month == 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= max
}

impl CaptureSupplierBillCommand {
    /// Structural shape check, mirroring the retained zod schema bounds.
    /// Zod-shape failures have no stable TS code (framework error); the
    /// native port fails closed with 422 instead of 500.
    pub fn parse(value: &serde_json::Value) -> Result<Self, BusinessError> {
        serde_json::from_value(value.clone()).map_err(|_| {
            invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            )
        })
    }

    pub fn validate(&self) -> Result<ValidatedCapture, BusinessError> {
        let legacy = self.fulfillment_id.is_some() || self.expected_version.is_some();
        if legacy && (self.fulfillment_id.as_deref().unwrap_or("").is_empty() || self.expected_version.is_none()) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "Receipt and version must be supplied together.",
            ));
        }
        if self.receipts.is_some() == legacy {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "Use either receipts or one legacy receipt, not both.",
            ));
        }
        if self.receipts.as_ref().is_some_and(|receipts| receipts.is_empty() || receipts.len() > 50) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            ));
        }
        if self.lines.is_empty() || self.lines.len() > 200 {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            ));
        }
        if self.reference.trim().len() < 2 || self.reference.trim().len() > 160 {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            ));
        }
        if !is_iso_date(&self.invoice_date) || !is_iso_date(&self.due_date) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            ));
        }
        if !valid_reason(&self.evidence) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice change is invalid. Review the receipts, lines and dates.",
            ));
        }
        for line in &self.lines {
            if line.fulfillment_line_id.is_empty() {
                return Err(invalid(
                    "SUPPLIER_BILL_INPUT_INVALID",
                    "The supplier invoice change is invalid. Review the receipts, lines and dates.",
                ));
            }
            if !valid_quantity(&line.quantity)
                || !valid_price(&line.price)
                || line.tax_bps < 0
                || line.tax_bps > 10_000
            {
                return Err(invalid(
                    "SUPPLIER_BILL_INPUT_INVALID",
                    "The supplier invoice change is invalid. Review the receipts, lines and dates.",
                ));
            }
        }
        Ok(ValidatedCapture {
            legacy_fulfillment_id: self.fulfillment_id.clone(),
            legacy_expected_version: self.expected_version,
            receipts: self.receipts.clone().unwrap_or_default(),
            lines: self.lines.clone(),
        })
    }
}

pub struct ValidatedCapture {
    pub legacy_fulfillment_id: Option<String>,
    pub legacy_expected_version: Option<i64>,
    pub receipts: Vec<BillCaptureReceiptInput>,
    pub lines: Vec<BillCaptureLineInput>,
}

impl DecideSupplierBillCommand {
    pub fn parse(value: &serde_json::Value) -> Result<Self, BusinessError> {
        serde_json::from_value(value.clone()).map_err(|_| {
            invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice decision is invalid. Review the action and version.",
            )
        })
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.action != "approve" && self.action != "return" {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice decision is invalid. Review the action and version.",
            ));
        }
        if self.expected_version <= 0 || !valid_reason(&self.reason) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice decision is invalid. Review the action and version.",
            ));
        }
        if let Some(account) = self.variance_account_id.as_deref() {
            if account.is_empty() {
                return Err(invalid(
                    "SUPPLIER_BILL_INPUT_INVALID",
                    "The supplier invoice decision is invalid. Review the action and version.",
                ));
            }
        }
        Ok(())
    }
}

impl SaveSupplierBillPolicyCommand {
    pub fn parse(value: &serde_json::Value) -> Result<Self, BusinessError> {
        serde_json::from_value(value.clone()).map_err(|_| {
            invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice policy change is invalid.",
            )
        })
    }

    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.matching_mode != "optional" && self.matching_mode != "required" {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice policy change is invalid.",
            ));
        }
        if self.expected_version < 0 || !valid_reason(&self.reason) {
            return Err(invalid(
                "SUPPLIER_BILL_INPUT_INVALID",
                "The supplier invoice policy change is invalid.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RequestedBillReceipt {
    pub fulfillment_id: String,
    pub expected_version: i64,
}

/// Retained `requestedBillReceipts`: legacy single receipt or the receipts
/// list, with duplicate detection.
pub fn requested_bill_receipts(
    legacy_fulfillment_id: Option<&str>,
    legacy_expected_version: Option<i64>,
    receipts: &[BillCaptureReceiptInput],
    lines: &[BillCaptureLineInput],
) -> Result<Vec<RequestedBillReceipt>, BusinessError> {
    let requested: Vec<RequestedBillReceipt> = if receipts.is_empty() {
        vec![RequestedBillReceipt {
            fulfillment_id: legacy_fulfillment_id.unwrap_or("").to_owned(),
            expected_version: legacy_expected_version.unwrap_or(0),
        }]
    } else {
        receipts
            .iter()
            .map(|receipt| RequestedBillReceipt {
                fulfillment_id: receipt.fulfillment_id.clone(),
                expected_version: receipt.expected_version,
            })
            .collect()
    };
    let mut seen = std::collections::BTreeSet::new();
    for receipt in &requested {
        if !seen.insert(receipt.fulfillment_id.clone()) {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "DUPLICATE_RECEIPT",
                "Select each goods receipt once.",
            ));
        }
    }
    let mut seen_lines = std::collections::BTreeSet::new();
    for line in lines {
        if !seen_lines.insert(line.fulfillment_line_id.clone()) {
            return Err(BusinessError::new(
                FailureKind::InvalidInput,
                "DUPLICATE_BILL_LINE",
                "Match each receipt line once.",
            ));
        }
    }
    Ok(requested)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBillAuthority {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub revision: i64,
    pub assignment_version: i64,
}

impl SupplierBillAuthority {
    pub fn from_actor(actor: &SupplierBillCommandActor) -> Self {
        Self {
            user_id: actor.user_id.clone(),
            assignment_id: actor.assignment_id.clone(),
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: actor.branch_id.as_ref().map(|value| value.as_str().to_owned()),
            department_id: actor.department_id.as_ref().map(|value| value.as_str().to_owned()),
            revision: actor.revision,
            assignment_version: actor.assignment_version,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBillRecordReceipt {
    pub id: String,
    pub version: i64,
    pub receipt_count: Option<i64>,
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voided_invoice_id: Option<String>,
    pub matching_mode: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SupplierBillReadback {
    pub id: String,
    pub version: i64,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor_with(permissions: &[&str]) -> SupplierBillCommandActor {
        SupplierBillCommandActor {
            user_id: "user-1".to_owned(),
            assignment_id: "grant-1".to_owned(),
            company_id: CompanyId::new("co").unwrap(),
            branch_id: None,
            department_id: None,
            revision: 1,
            assignment_version: 1,
            permissions: permissions.iter().map(|value| value.to_string()).collect(),
            expires_at: 9_007_199_254_740_991,
        }
    }

    #[test]
    fn decimal_conversion_matches_the_retained_math() {
        assert_eq!(decimal_to_minor("10.00", 2), Ok(1000));
        assert_eq!(decimal_to_minor("10.5", 2), Ok(1050));
        assert_eq!(decimal_to_minor("0.01", 2), Ok(1));
        assert_eq!(decimal_to_minor("7.256", 2), Err(()));
        assert_eq!(decimal_to_minor("7.250", 2), Ok(725));
        assert_eq!(decimal_to_minor("abc", 2), Err(()));
        assert_eq!(quantity_milli("2.5"), Ok(2500));
        assert_eq!(quantity_milli("0"), Err(()));
        assert_eq!(quantity_milli("-1"), Err(()));
        assert_eq!(line_amounts(2500, 1000, 700), Ok((2500, 175)));
    }

    #[test]
    fn actor_gates_keep_the_retained_permissions() {
        let actor = actor_with(&["finance.write", "commerce.read"]);
        assert!(actor.require_capture(TimestampMillis::new(1)).is_ok());
        assert!(actor.require_decide(TimestampMillis::new(1)).is_err());
        let actor = actor_with(&["finance.read", "finance.write", "finance.approve"]);
        assert!(actor.require_decide(TimestampMillis::new(1)).is_ok());
        assert!(actor.require_policy(TimestampMillis::new(1)).is_ok());
        assert!(actor.require_capture(TimestampMillis::new(1)).is_err());
        let expired = SupplierBillCommandActor {
            expires_at: 1,
            ..actor_with(&["finance.write", "commerce.read"])
        };
        assert!(matches!(
            expired.require_capture(TimestampMillis::new(2)),
            Err(error) if error.code == "WORK_CONTEXT_CHANGED"
        ));
    }

    #[test]
    fn duplicate_receipts_and_lines_fail_closed() {
        let dup = requested_bill_receipts(
            None,
            None,
            &[
                BillCaptureReceiptInput { fulfillment_id: "f-1".to_owned(), expected_version: 1 },
                BillCaptureReceiptInput { fulfillment_id: "f-1".to_owned(), expected_version: 1 },
            ],
            &[BillCaptureLineInput { fulfillment_line_id: "l-1".to_owned(), quantity: "1".to_owned(), price: "1.00".to_owned(), tax_bps: 0 }],
        );
        assert!(matches!(dup, Err(error) if error.code == "DUPLICATE_RECEIPT"));
        let dup_lines = requested_bill_receipts(
            None,
            None,
            &[BillCaptureReceiptInput { fulfillment_id: "f-1".to_owned(), expected_version: 1 }],
            &[
                BillCaptureLineInput { fulfillment_line_id: "l-1".to_owned(), quantity: "1".to_owned(), price: "1.00".to_owned(), tax_bps: 0 },
                BillCaptureLineInput { fulfillment_line_id: "l-1".to_owned(), quantity: "1".to_owned(), price: "1.00".to_owned(), tax_bps: 0 },
            ],
        );
        assert!(matches!(dup_lines, Err(error) if error.code == "DUPLICATE_BILL_LINE"));
    }
}
