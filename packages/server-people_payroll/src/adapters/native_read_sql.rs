//! Owner-owned caller-unit read plans for People Payroll. Generated from the
//! retired TypeScript SELECT literal with its exact positional payload; the
//! axum `command-plan` route resolves the kind here and returns the statement
//! for the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the
/// positional argument count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `records-record-details-owned-facts`
/// posted-payroll-run supplement rows read. The owner plans this literal; the
/// caller executes it on its open unit with the same
/// `[companyId, payMonth, currency, offset]` binds.
pub const RD_RECORD_SUPPLEMENT_ROWS: &str = "people_payroll_rd_record_supplement_rows";
const SQL_RECORD_SUPPLEMENT_ROWS: &str = "SELECT r.id,r.period_start,r.period_end,r.pay_date,r.currency,r.status,COUNT(i.id) headcount,SUM(i.gross_minor) gross_minor,SUM(i.deductions_minor) deductions_minor,SUM(i.net_minor) net_minor FROM hr_payroll_runs r JOIN hr_payroll_items i ON i.run_id=r.id WHERE r.company_id=? AND r.status='posted' AND substring(r.pay_date FROM 1 FOR 7)=? AND r.currency=? GROUP BY r.id ORDER BY r.pay_date DESC,r.id LIMIT 31 OFFSET ?";
const READ_RECORD_SUPPLEMENT_ROWS: NativeReadStatement = NativeReadStatement {
    kind: RD_RECORD_SUPPLEMENT_ROWS,
    sql: SQL_RECORD_SUPPLEMENT_ROWS,
    bind_count: 4,
};

/// Native successor of the TypeScript `organization-roster-facts`
/// active-exit-engagement-ids read: same literal and two positional binds.
pub const RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS: &str = "people_payroll_rd_org_roster_exit_engagement_ids";
const SQL_ORG_ROSTER_EXIT_ENGAGEMENT_IDS: &str = "SELECT id FROM hr_engagements WHERE company_id=? AND position_id=? AND status='active'";
const READ_ORG_ROSTER_EXIT_ENGAGEMENT_IDS: NativeReadStatement = NativeReadStatement {
    kind: RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,
    sql: SQL_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,
    bind_count: 2,
};

/// Native successor of the TypeScript `organization-roster-facts`
/// active-monthly-pay-groups read: same literal and one positional bind.
pub const RD_ORG_MONTHLY_PAY_GROUPS: &str = "people_payroll_rd_org_monthly_pay_groups";
const SQL_ORG_MONTHLY_PAY_GROUPS: &str = "SELECT id,name,currency FROM hr_pay_groups WHERE company_id=? AND status='active' AND frequency='monthly' ORDER BY name";
const READ_ORG_MONTHLY_PAY_GROUPS: NativeReadStatement = NativeReadStatement {
    kind: RD_ORG_MONTHLY_PAY_GROUPS,
    sql: SQL_ORG_MONTHLY_PAY_GROUPS,
    bind_count: 1,
};

/// Native successor of the TypeScript `work-report-facts` people-name read:
/// same literal and one positional bind.
pub const RD_WORK_REPORT_PEOPLE: &str = "people_payroll_rd_work_report_people";
const SQL_WORK_REPORT_PEOPLE: &str = "SELECT user_id,legal_name FROM hr_people WHERE company_id=? AND merged_into_id IS NULL";
const READ_WORK_REPORT_PEOPLE: NativeReadStatement = NativeReadStatement {
    kind: RD_WORK_REPORT_PEOPLE,
    sql: SQL_WORK_REPORT_PEOPLE,
    bind_count: 1,
};

/// Native successor of the TypeScript `final-pay-cancellation-sources`
/// schema-availability probe: same literal and no positional binds.
pub const RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE: &str = "people_payroll_rd_final_pay_cancellation_schema_available";
const SQL_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE: &str = "SELECT to_regprocedure('hr_final_pay_cancellation_candidate(hr_payroll_runs)') IS NOT NULL AND to_regprocedure('hr_final_pay_cancellation_valid(text,text)') IS NOT NULL ready";
const READ_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE: NativeReadStatement = NativeReadStatement {
    kind: RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE,
    sql: SQL_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE,
    bind_count: 0,
};

/// Native successor of the TypeScript `payroll-other-remittance-sources`
/// schema-availability probe: same literal and no positional binds.
pub const RD_PAYROLL_OTHER_SCHEMA_AVAILABLE: &str = "people_payroll_rd_payroll_other_schema_available";
const SQL_PAYROLL_OTHER_SCHEMA_AVAILABLE: &str = "SELECT 1 FROM erpx_migration_receipts() WHERE name='1066_payroll_other_liability_remittances.sql'";
const READ_PAYROLL_OTHER_SCHEMA_AVAILABLE: NativeReadStatement = NativeReadStatement {
    kind: RD_PAYROLL_OTHER_SCHEMA_AVAILABLE,
    sql: SQL_PAYROLL_OTHER_SCHEMA_AVAILABLE,
    bind_count: 0,
};

/// Native successor of the two identical TypeScript `workflow-flow-routing-facts`
/// legal-name reads (prepare-flow-command person and assign-flow-primary
/// administrator). Canonical owner read deduped into one kind: same literal and
/// two positional binds, executed on the caller's open unit.
pub const RD_FLOW_ROUTING_PERSON_LEGAL_NAME: &str = "people_payroll_rd_flow_routing_person_legal_name";
const SQL_FLOW_ROUTING_PERSON_LEGAL_NAME: &str = "SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL";
const READ_FLOW_ROUTING_PERSON_LEGAL_NAME: NativeReadStatement = NativeReadStatement {
    kind: RD_FLOW_ROUTING_PERSON_LEGAL_NAME,
    sql: SQL_FLOW_ROUTING_PERSON_LEGAL_NAME,
    bind_count: 2,
};

/// Native successor of the TypeScript `work-ui-staff-facts` head-placement
/// read: same literal and three positional binds. The owner plans this
/// statement; the caller executes it on its open unit with the same
/// `[company_id, department_id, person_id]` binds.
pub const RD_WORK_UI_HEAD_PLACEMENT: &str = "people_payroll_rd_work_ui_head_placement";
const SQL_WORK_UI_HEAD_PLACEMENT: &str = "SELECT e.position_id FROM hr_engagements e WHERE e.company_id=? AND e.department_id=? AND e.person_id=? AND e.status='active' ORDER BY e.updated_at DESC LIMIT 1";
const READ_WORK_UI_HEAD_PLACEMENT: NativeReadStatement = NativeReadStatement {
    kind: RD_WORK_UI_HEAD_PLACEMENT,
    sql: SQL_WORK_UI_HEAD_PLACEMENT,
    bind_count: 3,
};

/// Native successors of the TypeScript `attachment-migration-facts`
/// attachment-storage reconciliation reads: same literals and zero positional
/// binds. The owner plans each statement; the caller executes it on its open
/// unit with no binds.
pub const RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS: &str = "people_payroll_rd_attachment_migration_payment_exports";
const SQL_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS: &str = "SELECT id,file_key,file_checksum FROM hr_payment_batches WHERE file_key IS NOT NULL ORDER BY id";
const READ_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS: NativeReadStatement = NativeReadStatement {
    kind: RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,
    sql: SQL_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,
    bind_count: 0,
};

/// Native successor of the TypeScript `attachment-migration-facts`
/// payroll-review-export reconciliation read: same literal and zero binds.
pub const RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS: &str = "people_payroll_rd_attachment_migration_payroll_review_exports";
const SQL_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS: &str = "SELECT id,file_key,checksum_sha256 FROM hr_payroll_review_exports ORDER BY id";
const READ_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS: NativeReadStatement = NativeReadStatement {
    kind: RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS,
    sql: SQL_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS,
    bind_count: 0,
};

const READS: &[NativeReadStatement] = &[
    READ_RECORD_SUPPLEMENT_ROWS,
    READ_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,
    READ_ORG_MONTHLY_PAY_GROUPS,
    READ_WORK_REPORT_PEOPLE,
    READ_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE,
    READ_PAYROLL_OTHER_SCHEMA_AVAILABLE,
    READ_FLOW_ROUTING_PERSON_LEGAL_NAME,
    READ_WORK_UI_HEAD_PLACEMENT,
    READ_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,
    READ_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS,
];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_RECORD_SUPPLEMENT_ROWS).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        assert!(is_read(RD_RECORD_SUPPLEMENT_ROWS));
        assert!(!is_read("people_payroll_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn organization_roster_read_plans_are_ledger_free_and_bounded() {
        for (kind, binds) in [
            (RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS, 2usize),
            (RD_ORG_MONTHLY_PAY_GROUPS, 1usize),
        ] {
            let read = read_statement(kind).expect("read kind");
            assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
            assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
            assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
            assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
            assert_eq!(read.bind_count, binds);
            assert_eq!(read.sql.matches('?').count(), binds);
            assert!(is_read(kind));
        }
    }

    #[test]
    fn work_report_people_read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_WORK_REPORT_PEOPLE).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert!(is_read(RD_WORK_REPORT_PEOPLE));
    }

    #[test]
    fn final_pay_cancellation_schema_probe_is_ledger_free_and_zero_bound() {
        let read = read_statement(RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert!(read.sql.contains("to_regprocedure"));
        assert!(is_read(RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE));
    }

    #[test]
    fn payroll_other_schema_probe_is_ledger_free_and_zero_bound() {
        let read = read_statement(RD_PAYROLL_OTHER_SCHEMA_AVAILABLE).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert!(read.sql.contains("erpx_migration_receipts"));
        assert!(read.sql.contains("1066_payroll_other_liability_remittances.sql"));
        assert!(is_read(RD_PAYROLL_OTHER_SCHEMA_AVAILABLE));
    }

    #[test]
    fn flow_routing_person_legal_name_read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_FLOW_ROUTING_PERSON_LEGAL_NAME).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert!(is_read(RD_FLOW_ROUTING_PERSON_LEGAL_NAME));
    }

    #[test]
    fn work_ui_head_placement_read_plan_is_ledger_free_and_bounded() {
        let read = read_statement(RD_WORK_UI_HEAD_PLACEMENT).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE "));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.sql.matches('?').count(), 3);
        assert!(read.sql.contains("hr_engagements"));
        assert!(is_read(RD_WORK_UI_HEAD_PLACEMENT));
    }

    #[test]
    fn attachment_migration_reconciliation_read_plans_are_ledger_free_and_zero_bound() {
        for (kind, table) in [
            (RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS, "hr_payment_batches"),
            (RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS, "hr_payroll_review_exports"),
        ] {
            let read = read_statement(kind).expect("read kind");
            assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
            assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
            assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
            assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
            assert_eq!(read.bind_count, 0);
            assert_eq!(read.sql.matches('?').count(), 0);
            assert!(read.sql.contains(table));
            assert!(is_read(kind));
        }
    }
}
