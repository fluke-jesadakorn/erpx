//! Finance owner-write command model.
//!
//! A finance owner write is a typed `kind` discriminator plus an ordered array
//! of positional bind values. The kind allow-list is owned here, in the Finance
//! domain, and the matching SQL lives in the owner adapter crate. Values are
//! validated as bounded JSON scalars; the transport never accepts SQL text.
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MAX_VALUES: usize = 8192;
pub const MAX_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_KIND_BYTES: usize = 96;

/// Kind prefix for the native attachment version step. The suffix is an exact
/// allow-listed attachment target type resolved in the owner adapter; no
/// caller-supplied text ever becomes SQL.
pub const ATTACHMENT_VERSION_STEP_PREFIX: &str = "finance_attachment_version_step_";

#[derive(Clone, Debug, serde::Serialize)]
pub struct OwnerStatement {
    pub sql: String,
    pub values: Vec<Value>,
}

pub fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FINANCE_OWNER_WRITE_INVALID",
        "Review the finance owner write request.",
    )
}

pub fn unsupported_kind() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "FINANCE_OWNER_WRITE_KIND_UNSUPPORTED",
        "The finance owner write kind is not supported.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_OWNER_WRITE_UNAVAILABLE",
        "The native Finance owner write is unavailable.",
    )
}

/// Every statement kind this owner will serve. The HTTP ingress rejects any
/// other kind before it reaches a backend, so no caller-supplied SQL is ever
/// interpreted.
pub const SUPPORTED_KINDS: &[&str] = &[
    "acquisition_decide",
    "acquisition_insert",
    "acquisition_supersede",
    "ai_stmt_1",
    "ai_stmt_10",
    "ai_stmt_11",
    "ai_stmt_12",
    "ai_stmt_13",
    "ai_stmt_14",
    "ai_stmt_15",
    "ai_stmt_16",
    "ai_stmt_17",
    "ai_stmt_18",
    "ai_stmt_2",
    "ai_stmt_3",
    "ai_stmt_4",
    "ai_stmt_5",
    "ai_stmt_6",
    "ai_stmt_7",
    "ai_stmt_8",
    "ai_stmt_9",
    "approve_no_charge",
    "asset_depreciation_event",
    "bump_policy_edition",
    "commerce_allocation_insert",
    "commerce_invoice_adjust",
    "commerce_invoice_insert",
    "commerce_invoice_outstanding",
    "commerce_journal_insert",
    "commerce_journal_lines_insert",
    "commerce_party_insert",
    "commerce_party_update",
    "commerce_payment_insert",
    "commerce_refund_obligation_insert",
    "consolidation_run_update_approve",
    "consolidation_run_update_publish",
    "consolidation_run_update_review",
    "currency_preference_insert",
    "currency_scale_insert",
    "decide_contract",
    "decide_policy_edition",
    "decide_policy_run",
    "decide_recognition",
    "elimination_approve",
    "elimination_event_decide",
    "elimination_event_prepare",
    "elimination_insert",
    "elimination_post",
    "expense_employee_party",
    "expense_invoice_voided",
    "expense_journal_reversed",
    "expense_payable_invoice",
    "expense_reversal_journal",
    "expense_reversal_lines",
    "finance_stmt_1",
    "finance_stmt_10",
    "finance_stmt_11",
    "finance_stmt_12",
    "finance_stmt_13",
    "finance_stmt_14",
    "finance_stmt_15",
    "finance_stmt_16",
    "finance_stmt_17",
    "finance_stmt_18",
    "finance_stmt_19",
    "finance_stmt_2",
    "finance_stmt_20",
    "finance_stmt_21",
    "finance_stmt_22",
    "finance_stmt_23",
    "finance_stmt_24",
    "finance_stmt_25",
    "finance_stmt_26",
    "finance_stmt_27",
    "finance_stmt_28",
    "finance_stmt_29",
    "finance_stmt_3",
    "finance_stmt_30",
    "finance_stmt_31",
    "finance_stmt_32",
    "finance_stmt_33",
    "finance_stmt_34",
    "finance_stmt_35",
    "finance_stmt_36",
    "finance_stmt_37",
    "finance_stmt_38",
    "finance_stmt_39",
    "finance_stmt_4",
    "finance_stmt_40",
    "finance_stmt_41",
    "finance_stmt_42",
    "finance_stmt_43",
    "finance_stmt_44",
    "finance_stmt_45",
    "finance_stmt_46",
    "finance_stmt_47",
    "finance_stmt_48",
    "finance_stmt_49",
    "finance_stmt_5",
    "finance_stmt_6",
    "finance_stmt_7",
    "finance_stmt_8",
    "finance_stmt_9",
    "fx_rate_insert",
    "group_calc_event_decide",
    "group_calc_event_prepare",
    "group_calc_run_decide",
    "group_calc_run_insert",
    "journal_insert",
    "journal_lines_credit",
    "journal_lines_debit",
    "journal_post",
    "money_snapshot_batch_insert",
    "money_snapshot_insert",
    "money_snapshot_insert2",
    "money_snapshot_insert3",
    "ownership_interest_decide",
    "ownership_interest_insert",
    "ownership_interest_retire",
    "pos_account_insert",
    "pos_allocation_insert",
    "pos_bank_match",
    "pos_bank_match_payout",
    "pos_invoice_insert",
    "pos_journal_insert",
    "pos_journal_lines_insert",
    "pos_party_insert",
    "pos_payment_insert",
    "pos_payment_party_update",
    "pos_refund_payment_insert",
    "prepare_bill",
    "prepare_contract",
    "prepare_policy_edition",
    "prepare_policy_run",
    "prepare_recognition",
    "reverse_asset_carrying",
    "reverse_policy_run",
    "reverse_policy_version",
    "reverse_recognition",
    "run_event_decide",
    "run_event_prepare",
    "run_insert",
    "supersede_contract",
    "supersede_policy_edition",
    "update_asset_carrying",
    "void_contract_invoice",
];

pub fn supported(kind: &str) -> bool {
    if SUPPORTED_KINDS.binary_search(&kind).is_ok() {
        return true;
    }
    match kind.strip_prefix(ATTACHMENT_VERSION_STEP_PREFIX) {
        Some(target_type) => {
            crate::domain::attachment_version::attachment_version_target(target_type).is_some()
        }
        None => false,
    }
}

/// Validate transport facts independently of storage. Values must be a bounded
/// array of JSON scalars (never SQL text, objects or arrays).
pub fn validate(kind: &str, values: &[Value]) -> Result<(), BusinessError> {
    if kind.is_empty()
        || kind.len() > MAX_KIND_BYTES
        || !kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(invalid());
    }
    if values.len() > MAX_VALUES {
        return Err(invalid());
    }
    for value in values {
        let scalar = match value {
            Value::Null | Value::Bool(_) | Value::Number(_) => true,
            Value::String(text) => text.len() <= MAX_VALUE_BYTES,
            _ => false,
        };
        if !scalar {
            return Err(invalid());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::json;

    #[test]
    fn allow_list_is_sorted_and_unique() {
        assert!(SUPPORTED_KINDS.windows(2).all(|w| w[0] < w[1]));
        assert!(SUPPORTED_KINDS.len() > 100);
    }

    #[test]
    fn attachment_version_steps_are_admitted_by_allow_listed_suffix() {
        assert!(supported(&format!(
            "{}finance_journal",
            ATTACHMENT_VERSION_STEP_PREFIX
        )));
        assert!(supported(&format!(
            "{}invoice_payment",
            ATTACHMENT_VERSION_STEP_PREFIX
        )));
        assert!(!supported(&format!(
            "{}finance_unknown",
            ATTACHMENT_VERSION_STEP_PREFIX
        )));
        assert!(!supported(ATTACHMENT_VERSION_STEP_PREFIX));
        assert!(!supported("finance_attachment_version_step_finance_journal;DROP"));
    }

    #[test]
    fn rejects_objects_and_arrays() {
        assert!(validate("finance_stmt_1", &[json!(1)]).is_ok());
        assert!(validate("finance_stmt_1", &[json!({"sql": "DROP TABLE x"})]).is_err());
        assert!(validate("bad kind", &[]).is_err());
    }
}
