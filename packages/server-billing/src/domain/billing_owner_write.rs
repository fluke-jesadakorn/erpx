//! Native Billing owner-write command kinds.
//!
//! Every remaining TypeScript write literal in the Billing owner is a
//! typed, allow-listed kind here. The SQL for each kind lives only in the
//! owner adapter; transport callers send a kind plus positional values and
//! never send or hold statement text. Any unknown kind fails closed.

use erpx_server_shared::public::{BusinessError, FailureKind};

pub const BETA_RETAIN_CUSTOMER: &str = "beta_retain_customer";
pub const BETA_ENQUEUE_EVENT: &str = "beta_enqueue_event";
pub const BETA_IGNORE_EVENT: &str = "beta_ignore_event";
pub const BETA_APPLY_SUBSCRIPTION: &str = "beta_apply_subscription";
pub const BETA_APPLY_EVENT: &str = "beta_apply_event";
pub const BETA_PREPARE_BILLING: &str = "beta_prepare_billing";
pub const EVIDENCE_RETAIN: &str = "evidence_retain";
pub const EVIDENCE_STORAGE_LEDGER: &str = "evidence_storage_ledger";
pub const EVIDENCE_SCAN_ATTEMPT: &str = "evidence_scan_attempt";
pub const EVIDENCE_EVENT_QUARANTINED: &str = "evidence_event_quarantined";
pub const SCAN_SOURCE: &str = "scan_source";
pub const RECONCILIATION_RUN: &str = "reconciliation_run";
pub const SCAN_CLAIM: &str = "scan_claim";
pub const SCAN_MARK_SCANNING: &str = "scan_mark_scanning";
pub const SCAN_CLEAN_ATTEMPT: &str = "scan_clean_attempt";
pub const SCAN_CLEAN_EVIDENCE: &str = "scan_clean_evidence";
pub const SCAN_CLEAN_EVENT: &str = "scan_clean_event";
pub const SCAN_INFECTED_ATTEMPT: &str = "scan_infected_attempt";
pub const SCAN_INFECTED_EVIDENCE: &str = "scan_infected_evidence";
pub const SCAN_INFECTED_EVENT: &str = "scan_infected_event";
pub const SCAN_ERROR_ATTEMPT: &str = "scan_error_attempt";
pub const SCAN_ERROR_EVIDENCE: &str = "scan_error_evidence";
pub const SCAN_ERROR_EVENT: &str = "scan_error_event";
pub const SCAN_RETRY_ATTEMPT: &str = "scan_retry_attempt";
pub const MIRROR_INVOICE_STATE: &str = "mirror_invoice_state";
pub const RELEASE_VOIDED_COSTS: &str = "release_voided_costs";
pub const MARK_VOIDED_PERIOD_INCOMPLETE: &str = "mark_voided_period_incomplete";
pub const RETAIN_TEST_FINANCE_PROJECTION: &str = "retain_test_finance_projection";
pub const RETAIN_JOB_PROGRESS: &str = "retain_job_progress";
pub const RETAIN_STRIPE_INVOICE: &str = "retain_stripe_invoice";
pub const RETAIN_INVOICE_MODE: &str = "retain_invoice_mode";
pub const COMPLETE_JOB: &str = "complete_job";
pub const RETAIN_CURRENT_CARD: &str = "retain_current_card";
pub const RESCHEDULE_JOB: &str = "reschedule_job";
pub const RETAIN_SETUP_STATE: &str = "retain_setup_state";
pub const RETAIN_SETUP_CARD: &str = "retain_setup_card";
pub const APPLY_STRIPE_EVENT: &str = "apply_stripe_event";
pub const RESCHEDULE_STRIPE_EVENT: &str = "reschedule_stripe_event";
pub const JOBS_RETRY_LEGACY_EVENT: &str = "jobs_retry_legacy_event";
pub const RECORD_RATING_RUN: &str = "record_rating_run";
pub const RETAIN_RATED_PERIOD: &str = "retain_rated_period";
pub const RECORD_SCHEDULED_CHANGE: &str = "record_scheduled_change";
pub const END_ENROLLMENT: &str = "end_enrollment";
pub const ACTIVATE_PENDING_PLAN: &str = "activate_pending_plan";
pub const CREATE_BILLING_PERIOD: &str = "create_billing_period";
pub const CLOSE_ELAPSED_PERIODS: &str = "close_elapsed_periods";
pub const ENSURE_ACCOUNT: &str = "ensure_account";
pub const RETAIN_CUSTOMER: &str = "retain_customer";
pub const ENROLL: &str = "enroll";
pub const SCHEDULE_ENROLLMENT_CHANGE: &str = "schedule_enrollment_change";
pub const RETAIN_CONSENTED_CARD: &str = "retain_consented_card";
pub const QUEUE_MANUAL_INVOICE: &str = "queue_manual_invoice";
pub const WITHDRAW_CONSENT: &str = "withdraw_consent";
pub const SET_PAYMENT_MODE: &str = "set_payment_mode";
pub const INSERT_PLAN_REVISION: &str = "insert_plan_revision";
pub const ASSIGN_TARGET_PRICE: &str = "assign_target_price";
pub const RECORD_PROVIDER_COST: &str = "record_provider_cost";
pub const RECORD_PROVIDER_STATEMENT: &str = "record_provider_statement";
pub const RECORD_OPERATING_COST: &str = "record_operating_cost";
pub const RECORD_CAPACITY_REVIEW: &str = "record_capacity_review";
pub const RECORD_ROLLOUT_APPROVAL: &str = "record_rollout_approval";
pub const RECORD_TARGET_EXTRA: &str = "record_target_extra";
pub const ENROLL_TARGET: &str = "enroll_target";
pub const INSERT_TARGET_INVOICE: &str = "insert_target_invoice";
pub const APPROVE_TARGET_RATING: &str = "approve_target_rating";
pub const QUEUE_TARGET_ISSUE: &str = "queue_target_issue";
pub const RETRY_LEGACY_EVENT: &str = "retry_legacy_event";
pub const RETRY_JOB: &str = "retry_job";
pub const INCREMENT_ACCOUNT_VERSION: &str = "increment_account_version";
pub const RECORD_COMMAND_AUDIT: &str = "record_command_audit";
pub const RECORD_COMMAND_RECEIPT: &str = "record_command_receipt";

/// Every owner write kind. Anything else is rejected before an owner unit opens.
pub const OWNER_WRITE_KINDS: &[&str] = &[
    BETA_RETAIN_CUSTOMER,
    BETA_ENQUEUE_EVENT,
    BETA_IGNORE_EVENT,
    BETA_APPLY_SUBSCRIPTION,
    BETA_APPLY_EVENT,
    BETA_PREPARE_BILLING,
    EVIDENCE_RETAIN,
    EVIDENCE_STORAGE_LEDGER,
    EVIDENCE_SCAN_ATTEMPT,
    EVIDENCE_EVENT_QUARANTINED,
    SCAN_SOURCE,
    RECONCILIATION_RUN,
    SCAN_CLAIM,
    SCAN_MARK_SCANNING,
    SCAN_CLEAN_ATTEMPT,
    SCAN_CLEAN_EVIDENCE,
    SCAN_CLEAN_EVENT,
    SCAN_INFECTED_ATTEMPT,
    SCAN_INFECTED_EVIDENCE,
    SCAN_INFECTED_EVENT,
    SCAN_ERROR_ATTEMPT,
    SCAN_ERROR_EVIDENCE,
    SCAN_ERROR_EVENT,
    SCAN_RETRY_ATTEMPT,
    MIRROR_INVOICE_STATE,
    RELEASE_VOIDED_COSTS,
    MARK_VOIDED_PERIOD_INCOMPLETE,
    RETAIN_TEST_FINANCE_PROJECTION,
    RETAIN_JOB_PROGRESS,
    RETAIN_STRIPE_INVOICE,
    RETAIN_INVOICE_MODE,
    COMPLETE_JOB,
    RETAIN_CURRENT_CARD,
    RESCHEDULE_JOB,
    RETAIN_SETUP_STATE,
    RETAIN_SETUP_CARD,
    APPLY_STRIPE_EVENT,
    RESCHEDULE_STRIPE_EVENT,
    JOBS_RETRY_LEGACY_EVENT,
    RECORD_RATING_RUN,
    RETAIN_RATED_PERIOD,
    RECORD_SCHEDULED_CHANGE,
    END_ENROLLMENT,
    ACTIVATE_PENDING_PLAN,
    CREATE_BILLING_PERIOD,
    CLOSE_ELAPSED_PERIODS,
    ENSURE_ACCOUNT,
    RETAIN_CUSTOMER,
    ENROLL,
    SCHEDULE_ENROLLMENT_CHANGE,
    RETAIN_CONSENTED_CARD,
    QUEUE_MANUAL_INVOICE,
    WITHDRAW_CONSENT,
    SET_PAYMENT_MODE,
    INSERT_PLAN_REVISION,
    ASSIGN_TARGET_PRICE,
    RECORD_PROVIDER_COST,
    RECORD_PROVIDER_STATEMENT,
    RECORD_OPERATING_COST,
    RECORD_CAPACITY_REVIEW,
    RECORD_ROLLOUT_APPROVAL,
    RECORD_TARGET_EXTRA,
    ENROLL_TARGET,
    INSERT_TARGET_INVOICE,
    APPROVE_TARGET_RATING,
    QUEUE_TARGET_ISSUE,
    RETRY_LEGACY_EVENT,
    RETRY_JOB,
    INCREMENT_ACCOUNT_VERSION,
    RECORD_COMMAND_AUDIT,
    RECORD_COMMAND_RECEIPT,
];

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::InvalidInput,
        "BILLING_OWNER_WRITE_INVALID",
        "Review the Billing owner write request.",
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnerWriteKind(&'static str);

impl OwnerWriteKind {
    /// Resolve a transport kind against the owner allow-list.
    pub fn parse(kind: &str) -> Result<Self, BusinessError> {
        OWNER_WRITE_KINDS
            .iter()
            .find(|candidate| **candidate == kind)
            .map(|candidate| Self(*candidate))
            .ok_or_else(invalid)
    }
    pub fn as_str(self) -> &'static str { self.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_parses_and_unknown_is_rejected() {
        for kind in OWNER_WRITE_KINDS {
            assert_eq!(OwnerWriteKind::parse(kind).unwrap().as_str(), *kind);
        }
        assert!(OwnerWriteKind::parse("not_a_billing_write").is_err());
    }
}
