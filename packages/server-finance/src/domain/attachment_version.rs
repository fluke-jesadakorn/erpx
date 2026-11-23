//! Portable Finance attachment target-version rules.
//!
//! This is the owner-side authority for the two retired dynamic `UPDATE`
//! writers in `attachments-finance-facts.ts` and
//! `attachments-service-facts.ts`. Every table, version column, extra
//! predicate and `updated_at` flag below is a trusted owner constant selected
//! by an allow-listed target type; no caller-supplied text ever becomes SQL.
//! The adapter rechecks the caller's authoritative Company scope and executes
//! the guard and the single version step on the caller's serializable unit.
use erpx_server_shared::public::BusinessError;
use sha2::{Digest, Sha256};

/// Which retained Files chain a target belongs to. Both chains are owned by
/// Finance; the family only makes the future mount patch explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentVersionFamily {
    /// `FinanceAttachmentFinanceFacts` via the finance-operations policy.
    FinanceOperations,
    /// `attachmentServiceFinanceFacts` via invoice-payment evidence and the
    /// maintenance/planning record barrels.
    Service,
}

/// Trusted SQL fragments for one allow-listed attachment target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachmentVersionTarget {
    pub family: AttachmentVersionFamily,
    pub table: &'static str,
    pub column: &'static str,
    pub extra: Option<&'static str>,
    pub updated_at: bool,
    pub capability: &'static str,
}

impl AttachmentVersionTarget {
    /// Assert fragment mirroring the TS `versionGuard` assertion. The
    /// company_id predicate is owner-enforced here, never transported from a
    /// caller label.
    pub fn guard_exists(&self) -> String {
        format!(
            "EXISTS(SELECT 1 FROM {} WHERE id=$1 AND company_id=$2 AND {}=$3{})",
            self.table,
            self.column,
            self.extra
                .map(|extra| format!(" AND {extra}"))
                .unwrap_or_default()
        )
    }

    /// The single-version step, assembled only from allow-listed fragments.
    /// `version = version + 1` happens at most once per expected version. The
    /// company predicate is always owner-enforced, even though the retired TS
    /// service writer relied on its separate guard for scope.
    pub fn step_update(&self) -> String {
        let set = if self.updated_at {
            format!("{}={}+1,updated_at=$1", self.column, self.column)
        } else {
            format!("{}={}+1", self.column, self.column)
        };
        let (id, version, company) = if self.updated_at {
            ("$2", "$3", "$4")
        } else {
            ("$1", "$2", "$3")
        };
        let mut sql = format!(
            "UPDATE {} SET {} WHERE id={} AND {}={} AND company_id={}",
            self.table, set, id, self.column, version, company
        );
        if let Some(extra) = self.extra {
            sql.push_str(" AND ");
            sql.push_str(extra);
        }
        sql
    }

    /// Generic owner-write variant of the single version step. Same
    /// allow-listed fragments, `?` placeholders for the Finance owner-write
    /// transport, and Company scope enforced by the separate version guard
    /// assertion. Returns the exact positional value count for this
    /// descriptor so the owner-write ingress can arity-fence the command.
    pub fn step_update_unscoped_q(&self) -> (String, usize) {
        let set = if self.updated_at {
            format!("{}={}+1,updated_at=?", self.column, self.column)
        } else {
            format!("{}={}+1", self.column, self.column)
        };
        let mut sql = format!(
            "UPDATE {} SET {} WHERE id=? AND {}=?",
            self.table, set, self.column
        );
        if let Some(extra) = self.extra {
            sql.push_str(" AND ");
            sql.push_str(extra);
        }
        (sql, if self.updated_at { 3 } else { 2 })
    }
}

/// Resolve the exact owner descriptor for a target type. Unknown or
/// injection-shaped inputs resolve to `None` so the adapter fails closed
/// before any SQL is prepared.
pub fn attachment_version_target(target_type: &str) -> Option<AttachmentVersionTarget> {
    use AttachmentVersionFamily::{FinanceOperations, Service};
    let (table, column, extra, updated_at, capability) = match target_type {
        "finance_billing_correction" => (
            "finance_billing_corrections",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_business_bill" => (
            "finance_business_billing_runs",
            "version",
            Some(
                "no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=finance_business_billing_runs.id)",
            ),
            false,
            "finance.write",
        ),
        "finance_plan_edition" => (
            "finance_plan_editions",
            "version",
            Some("status='draft'"),
            false,
            "finance.write",
        ),
        "finance_book_rule" => (
            "finance_book_rules",
            "version",
            Some("status='draft'"),
            false,
            "finance.write",
        ),
        "finance_book_adjustment" => (
            "finance_book_adjustments",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_bank_match_group" => (
            "finance_bank_match_groups",
            "version",
            Some("status='proposed'"),
            true,
            "finance.write",
        ),
        "finance_collection_case" => (
            "finance_collection_cases",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_business_contract" => (
            "finance_business_contracts",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_recognition_run" => (
            "finance_recognition_runs",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_instrument" => (
            "finance_instrument_editions",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_instrument_run" => (
            "finance_instrument_runs",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_asset_policy" => (
            "finance_asset_policy_editions",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_asset_policy_run" => (
            "finance_asset_policy_runs",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_group_report" => (
            "finance_group_report_editions",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_ownership_interest" => (
            "finance_ownership_interests",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_acquisition" => (
            "finance_acquisitions",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_intercompany_pair" => (
            "finance_intercompany_document_pairs",
            "version",
            Some("status='draft'"),
            true,
            "finance.write",
        ),
        "finance_journal" => ("finance_journals", "version", None, true, "finance.write"),
        "finance_bank_statement" => (
            "finance_bank_statements",
            "version",
            None,
            false,
            "finance.write",
        ),
        "finance_bank_transaction" => (
            "finance_bank_transactions",
            "version",
            None,
            true,
            "finance.write",
        ),
        "finance_period" => (
            "finance_periods",
            "close_version",
            None,
            false,
            "finance.write",
        ),
        "finance_accrual_schedule" => (
            "finance_accrual_schedules",
            "version",
            None,
            true,
            "finance.write",
        ),
        "finance_asset" => ("finance_assets", "version", None, false, "finance.write"),
        "finance_tax_identity" => (
            "finance_tax_identities",
            "version",
            Some("party_id IS NULL AND status='draft'"),
            true,
            "finance.write",
        ),
        "finance_lease" => ("finance_leases", "version", None, true, "finance.write"),
        "finance_loan" => ("finance_loans", "version", None, true, "finance.write"),
        "finance_revenue_contract" => (
            "finance_revenue_contracts",
            "version",
            None,
            true,
            "finance.write",
        ),
        "finance_payment" => ("finance_payments", "version", None, true, "finance.pay"),
        "finance_refund_obligation" => (
            "finance_refund_obligations",
            "version",
            None,
            true,
            "finance.pay",
        ),
        "finance_supplier_batch" => (
            "finance_supplier_batches",
            "version",
            None,
            true,
            "finance.pay",
        ),
        "invoice_payment" => ("finance_invoices", "version", None, true, "finance.write"),
        "supplier_bill" => ("finance_invoices", "version", None, true, "finance.write"),
        _ => return None,
    };
    let family = if matches!(target_type, "invoice_payment" | "supplier_bill") {
        Service
    } else {
        FinanceOperations
    };
    Some(AttachmentVersionTarget {
        family,
        table,
        column,
        extra,
        updated_at,
        capability,
    })
}

fn refusal(status: u16, code: &'static str, message: &'static str) -> BusinessError {
    let failure = match status {
        403 => erpx_server_shared::public::FailureKind::Forbidden,
        404 => erpx_server_shared::public::FailureKind::NotFound,
        409 => erpx_server_shared::public::FailureKind::Conflict,
        422 => erpx_server_shared::public::FailureKind::InvalidInput,
        _ => erpx_server_shared::public::FailureKind::Unavailable,
    };
    BusinessError::new(failure, code, message)
}

pub fn unsupported_target() -> BusinessError {
    refusal(
        422,
        "ATTACHMENT_TARGET_UNSUPPORTED",
        "This file target has no atomic version guard.",
    )
}

pub fn unsupported_record() -> BusinessError {
    refusal(
        422,
        "RECORD_ATTACHMENT_BIND_UNSUPPORTED",
        "This record does not support standalone document binding.",
    )
}

pub fn scope_forbidden() -> BusinessError {
    refusal(
        403,
        "ATTACHMENT_SCOPE_FORBIDDEN",
        "This attachment target belongs to another Company scope.",
    )
}

pub fn target_not_found() -> BusinessError {
    refusal(
        404,
        "ATTACHMENT_TARGET_NOT_FOUND",
        "The Finance attachment target was not found.",
    )
}

pub fn target_changed() -> BusinessError {
    refusal(
        409,
        "ATTACHMENT_TARGET_CHANGED",
        "The Finance attachment target changed.",
    )
}

pub fn invalid_expected_version() -> BusinessError {
    refusal(
        422,
        "FINANCE_ATTACHMENT_VERSION_INVALID",
        "Use the current attachment target version.",
    )
}

pub fn unavailable() -> BusinessError {
    refusal(
        503,
        "FINANCE_ATTACHMENT_VERSION_UNAVAILABLE",
        "The Finance attachment version guard is unavailable.",
    )
}

/// The composition supplies the authorized actor; this owner-side check must
/// still reject empty or foreign scope rather than trusting a transport label.
pub fn require_actor_scope(
    company_id: &str,
    assignment_id: &str,
    user_id: &str,
    scope_kind: &str,
) -> Result<(), BusinessError> {
    if company_id.is_empty()
        || company_id.len() > 200
        || assignment_id.is_empty()
        || user_id.is_empty()
        || (scope_kind != "company"
            && scope_kind != "holding"
            && scope_kind != "branch"
            && scope_kind != "department")
    {
        return Err(scope_forbidden());
    }
    Ok(())
}

/// Deterministic request fingerprint over the retained authority fields. The
/// caller's command key is the idempotency identity; these fields bind it to
/// one actor, one target and one expected version.
pub fn version_fingerprint(
    user_id: &str,
    assignment_id: &str,
    company_id: &str,
    operation: &str,
    record: &str,
    expected_version: i64,
    target_type: &str,
) -> String {
    let canonical = format!(
        "{{\"actor\":{},\"assignment\":{},\"company\":{},\"operation\":{},\"recordId\":{},\"expectedVersion\":{},\"targetType\":{}}}",
        string(user_id),
        string(assignment_id),
        string(company_id),
        string(operation),
        string(record),
        expected_version,
        string(target_type)
    );
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_retained_finance_target_resolves_to_its_exact_owner_table() {
        let expected: &[(&str, &str, &str, bool)] = &[
            (
                "finance_billing_correction",
                "finance_billing_corrections",
                "version",
                true,
            ),
            (
                "finance_business_bill",
                "finance_business_billing_runs",
                "version",
                false,
            ),
            (
                "finance_plan_edition",
                "finance_plan_editions",
                "version",
                false,
            ),
            ("finance_book_rule", "finance_book_rules", "version", false),
            (
                "finance_book_adjustment",
                "finance_book_adjustments",
                "version",
                true,
            ),
            (
                "finance_bank_match_group",
                "finance_bank_match_groups",
                "version",
                true,
            ),
            (
                "finance_collection_case",
                "finance_collection_cases",
                "version",
                true,
            ),
            (
                "finance_business_contract",
                "finance_business_contracts",
                "version",
                true,
            ),
            (
                "finance_recognition_run",
                "finance_recognition_runs",
                "version",
                true,
            ),
            (
                "finance_instrument",
                "finance_instrument_editions",
                "version",
                true,
            ),
            (
                "finance_instrument_run",
                "finance_instrument_runs",
                "version",
                true,
            ),
            (
                "finance_asset_policy",
                "finance_asset_policy_editions",
                "version",
                true,
            ),
            (
                "finance_asset_policy_run",
                "finance_asset_policy_runs",
                "version",
                true,
            ),
            (
                "finance_group_report",
                "finance_group_report_editions",
                "version",
                true,
            ),
            (
                "finance_ownership_interest",
                "finance_ownership_interests",
                "version",
                true,
            ),
            (
                "finance_acquisition",
                "finance_acquisitions",
                "version",
                true,
            ),
            (
                "finance_intercompany_pair",
                "finance_intercompany_document_pairs",
                "version",
                true,
            ),
            ("finance_journal", "finance_journals", "version", true),
            (
                "finance_bank_statement",
                "finance_bank_statements",
                "version",
                false,
            ),
            (
                "finance_bank_transaction",
                "finance_bank_transactions",
                "version",
                true,
            ),
            ("finance_period", "finance_periods", "close_version", false),
            (
                "finance_accrual_schedule",
                "finance_accrual_schedules",
                "version",
                true,
            ),
            ("finance_asset", "finance_assets", "version", false),
            (
                "finance_tax_identity",
                "finance_tax_identities",
                "version",
                true,
            ),
            ("finance_lease", "finance_leases", "version", true),
            ("finance_loan", "finance_loans", "version", true),
            (
                "finance_revenue_contract",
                "finance_revenue_contracts",
                "version",
                true,
            ),
            ("finance_payment", "finance_payments", "version", true),
            (
                "finance_refund_obligation",
                "finance_refund_obligations",
                "version",
                true,
            ),
            (
                "finance_supplier_batch",
                "finance_supplier_batches",
                "version",
                true,
            ),
        ];
        assert_eq!(expected.len(), 30);
        for (target, table, column, updated_at) in expected {
            let resolved =
                attachment_version_target(target).unwrap_or_else(|| panic!("missing {target}"));
            assert_eq!(resolved.family, AttachmentVersionFamily::FinanceOperations);
            assert_eq!(resolved.table, *table);
            assert_eq!(resolved.column, *column);
            assert_eq!(resolved.updated_at, *updated_at);
        }
    }

    #[test]
    fn service_targets_share_the_finance_invoice_guard() {
        for target in ["invoice_payment", "supplier_bill"] {
            let resolved = attachment_version_target(target).unwrap();
            assert_eq!(resolved.family, AttachmentVersionFamily::Service);
            assert_eq!(resolved.table, "finance_invoices");
            assert_eq!(resolved.column, "version");
            assert!(resolved.updated_at);
            assert_eq!(resolved.capability, "finance.write");
        }
    }

    #[test]
    fn allow_list_fails_closed_for_unknown_or_injected_targets() {
        for target in [
            "finance_unknown",
            "",
            "invoice_payment; DROP TABLE finance_invoices",
            "finance_journal; DROP TABLE finance_journals",
            "FINANCE_JOURNAL",
            "finance_journal\u{0000}",
        ] {
            assert!(
                attachment_version_target(target).is_none(),
                "{target} must fail closed"
            );
        }
        assert_eq!(
            attachment_version_target("finance_journal")
                .unwrap()
                .capability,
            "finance.write"
        );
        assert_eq!(
            attachment_version_target("finance_payment")
                .unwrap()
                .capability,
            "finance.pay"
        );
    }

    #[test]
    fn guard_and_step_are_built_from_owner_fragments_only() {
        let journal = attachment_version_target("finance_journal").unwrap();
        assert_eq!(
            journal.guard_exists(),
            "EXISTS(SELECT 1 FROM finance_journals WHERE id=$1 AND company_id=$2 AND version=$3)"
        );
        assert_eq!(
            journal.step_update(),
            "UPDATE finance_journals SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3 AND company_id=$4"
        );
        let period = attachment_version_target("finance_period").unwrap();
        assert_eq!(
            period.step_update(),
            "UPDATE finance_periods SET close_version=close_version+1 WHERE id=$1 AND close_version=$2 AND company_id=$3"
        );
        let bill = attachment_version_target("finance_business_bill").unwrap();
        assert!(bill.step_update().contains("no_charge_status='draft'"));
        assert!(!bill.step_update().contains("updated_at"));
    }

    #[test]
    fn scope_validation_rejects_foreign_or_empty_authority() {
        assert!(require_actor_scope("co", "grant-1", "user-1", "company").is_ok());
        assert!(require_actor_scope("co", "grant-1", "user-1", "holding").is_ok());
        assert_eq!(
            require_actor_scope("", "grant-1", "user-1", "company")
                .unwrap_err()
                .code,
            "ATTACHMENT_SCOPE_FORBIDDEN"
        );
        assert!(require_actor_scope("co", "", "user-1", "company").is_err());
        assert!(require_actor_scope("co", "grant-1", "", "company").is_err());
        assert!(require_actor_scope("co", "grant-1", "user-1", "system").is_err());
    }

    #[test]
    fn fingerprint_is_stable_and_binds_the_guard_facts() {
        let first = version_fingerprint(
            "user-1",
            "grant-1",
            "co",
            "finance.attachment.version-step",
            "journal-1",
            2,
            "finance_journal",
        );
        assert_eq!(first.len(), 64);
        assert_eq!(
            first,
            version_fingerprint(
                "user-1",
                "grant-1",
                "co",
                "finance.attachment.version-step",
                "journal-1",
                2,
                "finance_journal"
            )
        );
        assert_ne!(
            first,
            version_fingerprint(
                "user-2",
                "grant-1",
                "co",
                "finance.attachment.version-step",
                "journal-1",
                2,
                "finance_journal"
            )
        );
        assert_ne!(
            first,
            version_fingerprint(
                "user-1",
                "grant-1",
                "co",
                "finance.attachment.version-step",
                "journal-1",
                3,
                "finance_journal"
            )
        );
        assert_ne!(
            first,
            version_fingerprint(
                "user-1",
                "grant-1",
                "other",
                "finance.attachment.version-step",
                "journal-1",
                2,
                "finance_journal"
            )
        );
    }

    #[test]
    fn owner_write_step_uses_only_allow_listed_fragments() {
        let target = attachment_version_target("finance_journal").unwrap();
        let (sql, arity) = target.step_update_unscoped_q();
        assert_eq!(arity, 3);
        assert_eq!(
            sql,
            "UPDATE finance_journals SET version=version+1,updated_at=? WHERE id=? AND version=?"
        );
        let target = attachment_version_target("finance_period").unwrap();
        let (sql, arity) = target.step_update_unscoped_q();
        assert_eq!(arity, 2);
        assert_eq!(
            sql,
            "UPDATE finance_periods SET close_version=close_version+1 WHERE id=? AND close_version=?"
        );
        let target = attachment_version_target("finance_business_bill").unwrap();
        let (sql, arity) = target.step_update_unscoped_q();
        assert_eq!(arity, 2);
        assert_eq!(
            sql,
            "UPDATE finance_business_billing_runs SET version=version+1 WHERE id=? AND version=? \
             AND no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=finance_business_billing_runs.id)"
        );
        assert!(attachment_version_target("finance_journal; DROP TABLE finance_journals").is_none());
    }

    #[test]
    fn refusals_keep_the_retained_oracle_codes() {
        assert_eq!(unsupported_target().code, "ATTACHMENT_TARGET_UNSUPPORTED");
        assert_eq!(
            unsupported_record().code,
            "RECORD_ATTACHMENT_BIND_UNSUPPORTED"
        );
        assert_eq!(target_not_found().code, "ATTACHMENT_TARGET_NOT_FOUND");
        assert_eq!(target_changed().code, "ATTACHMENT_TARGET_CHANGED");
        assert_eq!(
            target_changed().kind,
            erpx_server_shared::public::FailureKind::Conflict
        );
    }
}
