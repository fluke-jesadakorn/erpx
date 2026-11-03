//! Controlled Field receipt decisions. Identity comparisons are supplied only by
//! the current IAM witness; these facts are never deserialized from HTTP.
use super::money::{decimal_to_units, js_trim};
use erpx_server_shared::public::{BusinessError, FailureKind};
use num_bigint::BigInt;
fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}
pub fn service_receipt_amount(value: &str) -> Result<i64, BusinessError> {
    let units = decimal_to_units(value, 2).map_err(|e| {
        let code = if e.0.contains("decimal") {
            "INVALID_MONEY"
        } else if e.0.contains("places") {
            "MONEY_SCALE"
        } else {
            "MONEY_OVERFLOW"
        };
        error(
            FailureKind::InvalidInput,
            code,
            if e.0.contains("places") {
                "Money supports at most 2 decimal places."
            } else {
                "Money must be a decimal string."
            },
        )
    })?;
    if units > BigInt::from(9_007_199_254_740_991_i64)
        || units < BigInt::from(-9_007_199_254_740_991_i64)
    {
        return Err(error(
            FailureKind::InvalidInput,
            "MONEY_OVERFLOW",
            "Money amount is too large.",
        ));
    }
    if units <= BigInt::from(0) {
        return Err(error(
            FailureKind::InvalidInput,
            "INVALID_PAYMENT_AMOUNT",
            "Payment amount must be greater than zero.",
        ));
    }
    Ok(units.try_into().expect("bounded integer minor units"))
}
pub struct ServiceReceiptInvoiceFacts<'a> {
    pub party: &'a str,
    pub currency: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
    pub source_type: &'a str,
    pub outstanding: i64,
}
pub fn validate_service_receipt_invoice(
    f: &ServiceReceiptInvoiceFacts<'_>,
    party: &str,
    currency: &str,
    amount: i64,
) -> Result<(), BusinessError> {
    if amount <= 0
        || f.party != party
        || f.currency != currency
        || f.kind != "receivable"
        || !matches!(f.status, "posted" | "partially_paid")
        || f.source_type != "field_service"
        || f.outstanding < amount
    {
        return Err(error(
            FailureKind::InvalidInput,
            "PAYMENT_SETTLEMENT_SOURCE_INVALID",
            "Choose an open invoice for the same party, currency and payment amount.",
        ));
    }
    Ok(())
}
pub struct ServiceReceiptReviewFacts<'a> {
    pub status: &'a str,
    pub version: i64,
    pub has_bank_account: bool,
    pub same_creator: bool,
    pub same_approver: bool,
    pub same_releaser: bool,
}
/// Invoke after current command authority and exact receipt replay lookup,
/// before loading the fresh source, matching the retained Finance ordering.
pub fn require_service_receipt_evidence(proof: &str, release: bool) -> Result<(), BusinessError> {
    if js_trim(proof).is_empty() {
        return Err(error(
            FailureKind::InvalidInput,
            "HUMAN_DECISION_REQUIRED",
            if release {
                "Payment release evidence is required."
            } else {
                "Payment approval evidence is required."
            },
        ));
    }
    Ok(())
}
pub fn validate_service_receipt_approval(
    f: &ServiceReceiptReviewFacts<'_>,
    expected: i64,
) -> Result<(), BusinessError> {
    if f.version != expected {
        return Err(error(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "Payment changed. Refresh before approving.",
        ));
    }
    if f.same_creator {
        return Err(error(
            FailureKind::Conflict,
            "SELF_REVIEW_FORBIDDEN",
            "A different person must approve the payment.",
        ));
    }
    if !matches!(f.status, "draft" | "in_review") {
        return Err(error(
            FailureKind::Conflict,
            "PAYMENT_NOT_APPROVABLE",
            "Payment is not awaiting approval.",
        ));
    }
    Ok(())
}
pub fn validate_service_receipt_release(
    f: &ServiceReceiptReviewFacts<'_>,
    expected: i64,
) -> Result<(), BusinessError> {
    // The retained source query admits approved records only, before version.
    if f.status != "approved" {
        return Err(error(
            FailureKind::NotFound,
            "PAYMENT_NOT_APPROVED",
            "Only an approved payment can be released.",
        ));
    }
    if f.version != expected {
        return Err(error(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "Payment changed. Refresh before releasing.",
        ));
    }
    if f.same_creator || f.same_approver {
        return Err(error(
            FailureKind::Forbidden,
            "INDEPENDENT_PAYER_REQUIRED",
            "A third person must release and confirm this approved payment.",
        ));
    }
    if !f.has_bank_account {
        return Err(error(
            FailureKind::InvalidInput,
            "BANK_ACCOUNT_REQUIRED",
            "Assign a bank ledger account before release.",
        ));
    }
    Ok(())
}
pub fn validate_service_receipt_confirmation(
    f: &ServiceReceiptReviewFacts<'_>,
    expected: i64,
) -> Result<(), BusinessError> {
    if f.status != "released" || f.version != expected {
        return Err(error(
            FailureKind::Conflict,
            "PAYMENT_NOT_CONFIRMABLE",
            "Refresh the released payment before confirming it.",
        ));
    }
    if !f.has_bank_account {
        return Err(error(
            FailureKind::InvalidInput,
            "PAYMENT_SETUP_REQUIRED",
            "The payment needs a party and bank ledger account.",
        ));
    }
    if f.same_creator || f.same_approver || !f.same_releaser {
        return Err(error(
            FailureKind::Forbidden,
            "INDEPENDENT_PAYER_REQUIRED",
            "The independent payer who released this payment must confirm its bank result.",
        ));
    }
    Ok(())
}

/// The retained receipt REST date is an exact ISO calendar day.
pub fn valid_service_receipt_date(value: &str) -> bool {
    value.len() == 10
        && time::Date::parse(
            value,
            time::macros::format_description!("[year]-[month]-[day]"),
        )
        .is_ok()
}
