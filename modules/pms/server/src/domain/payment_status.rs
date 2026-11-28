//! Native PMS PromptPay payment-status read facts.
//!
//! This is a read-only projection of the `GET .../pms/stripe-payments?id=`
//! capability. It ports the exact row shape the retained TypeScript
//! `pmsStripePaymentStatus` returns through its `visible()` helper after the
//! owner reconcile: nine fields, camelCase on the wire, `qrData` only while a
//! payment is `pending`, and `payerEmailProvided` as a boolean witness. No SQL,
//! HTTP, provider call or command lives in this module.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

/// Bounded Company/Payment selection for one status read. The id is the
/// payment request key minted by the retained `createPmsStripePayment`; the
/// Company selector is composition-supplied and never trusted as authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmsPaymentStatusFacts {
    pub company_id: String,
    pub id: String,
}

/// The PMS-owned `pms_stripe_payments` row projection used by the status read.
/// Only the columns needed by `visible()` are selected by the adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentStatusRow {
    pub id: String,
    pub company_id: String,
    pub property_id: String,
    pub state: String,
    pub kind: String,
    pub amount_minor: i64,
    pub payer_email: Option<String>,
    pub provider_id: Option<String>,
    pub qr_data: Option<String>,
    pub expires_at: i64,
    pub entry_id: Option<String>,
    pub refund_state: Option<String>,
}

/// The complete response object. `#[serde(rename_all = "camelCase")]` reproduces
/// the retained `visible()` keys exactly; no field is appended or reshaped by
/// the caller.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmsPaymentStatusView {
    pub id: String,
    pub state: String,
    pub kind: String,
    pub amount_minor: i64,
    pub qr_data: Option<String>,
    pub expires_at: i64,
    pub entry_id: Option<String>,
    pub refund_state: Option<String>,
    pub payer_email_provided: bool,
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PMS_PAYMENT_STATUS_REQUEST_INVALID",
        "Use one bounded payment status read.",
    )
}

/// The retained status read reports `PMS_PAYMENT_NOT_FOUND` (404) for a missing
/// or foreign Company payment; keep the exact code for payload parity.
pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "PMS_PAYMENT_NOT_FOUND",
        "Payment request not found.",
    )
}

pub fn read_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_PAYMENT_STATUS_READ_INVALID",
        "The payment status could not be read.",
    )
}

/// The retained route validated `id` with `z.string()` and the Company selector
/// is composition-supplied; both are bounded tokens here, never authority.
pub fn validate(facts: &PmsPaymentStatusFacts) -> Result<(), BusinessError> {
    let bounded = |value: &str, max: usize| {
        !value.is_empty()
            && value.len() <= max
            && value.chars().count() <= max
            && !value.chars().any(char::is_control)
    };
    if !bounded(&facts.company_id, 200) || !bounded(&facts.id, 200) {
        return Err(request_invalid());
    }
    Ok(())
}

/// Parse the row into the exact `visible()` shape: `qrData` is present only for
/// a `pending` payment and `payerEmailProvided` witnesses a non-empty email.
pub fn project(row: PaymentStatusRow) -> PmsPaymentStatusView {
    let pending = row.state == "pending";
    PmsPaymentStatusView {
        id: row.id,
        state: row.state,
        kind: row.kind,
        amount_minor: row.amount_minor,
        qr_data: if pending { row.qr_data } else { None },
        expires_at: row.expires_at,
        entry_id: row.entry_id,
        refund_state: row.refund_state,
        payer_email_provided: row
            .payer_email
            .as_deref()
            .is_some_and(|email| !email.is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> PaymentStatusRow {
        PaymentStatusRow {
            id: "payment-1".into(),
            company_id: "company".into(),
            property_id: "property-1".into(),
            state: "pending".into(),
            kind: "receipt".into(),
            amount_minor: 1500,
            payer_email: Some("guest@example.test".into()),
            provider_id: Some("pi_1".into()),
            qr_data: Some("000010...".into()),
            expires_at: 1_800_000_000_000,
            entry_id: None,
            refund_state: None,
        }
    }

    #[test]
    fn visible_projection_matches_retained_payload() {
        let value = serde_json::to_value(project(row())).unwrap();
        assert_eq!(value["id"], "payment-1");
        assert_eq!(value["state"], "pending");
        assert_eq!(value["kind"], "receipt");
        assert_eq!(value["amountMinor"], 1500);
        assert_eq!(value["qrData"], "000010...");
        assert_eq!(value["expiresAt"], 1_800_000_000_000_i64);
        assert_eq!(value["entryId"], serde_json::Value::Null);
        assert_eq!(value["refundState"], serde_json::Value::Null);
        assert_eq!(value["payerEmailProvided"], true);
        assert!(value.get("payer_email_provided").is_none());
        assert!(value.get("amount_minor").is_none());
    }

    #[test]
    fn qr_data_is_omitted_outside_pending_and_email_witness_is_boolean() {
        let mut settled = row();
        settled.state = "succeeded".into();
        settled.payer_email = None;
        let value = serde_json::to_value(project(settled)).unwrap();
        assert_eq!(value["qrData"], serde_json::Value::Null);
        assert_eq!(value["payerEmailProvided"], false);
    }

    #[test]
    fn bounds_are_enforced() {
        let good = PmsPaymentStatusFacts {
            company_id: "company".into(),
            id: "payment-1".into(),
        };
        assert!(validate(&good).is_ok());
        let bad = PmsPaymentStatusFacts {
            company_id: "company".into(),
            id: "x".repeat(201),
        };
        assert_eq!(validate(&bad).unwrap_err().code, "PMS_PAYMENT_STATUS_REQUEST_INVALID");
    }
}
