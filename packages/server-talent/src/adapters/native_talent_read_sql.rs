//! Owner-owned caller-unit read plans for Talent. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `talent/command-plan` route resolves the kind here and returns the statement
//! for the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller executes them on the
//! same unit as the surrounding command. No SQL is ever accepted from the wire
//! and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTalentReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `nativeTalentDetail` application-event
/// history read. The caller binds `[application_id]` on its open unit.
pub const RD_APPLICATION_EVENTS: &str = "talent_rd_application_events";
const SQL_RD_APPLICATION_EVENTS: &str =
    "SELECT * FROM talent_application_events WHERE application_id=? ORDER BY version";
const READ_APPLICATION_EVENTS: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_APPLICATION_EVENTS,
    sql: SQL_RD_APPLICATION_EVENTS,
    bind_count: 1,
};

/// Native successor of the TypeScript `nativeTalentDetail` proposal read.
pub const RD_PROPOSALS: &str = "talent_rd_proposals";
const SQL_RD_PROPOSALS: &str = "SELECT * FROM talent_proposals WHERE application_id=?";
const READ_PROPOSALS: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_PROPOSALS,
    sql: SQL_RD_PROPOSALS,
    bind_count: 1,
};

/// Native successor of the TypeScript `nativeTalentDetail` offer read.
pub const RD_OFFERS: &str = "talent_rd_offers";
const SQL_RD_OFFERS: &str = "SELECT * FROM talent_offers WHERE application_id=?";
const READ_OFFERS: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_OFFERS,
    sql: SQL_RD_OFFERS,
    bind_count: 1,
};

/// Native successor of the TypeScript `nativeTalentDetail` accepted-hire
/// handoff read.
pub const RD_HIRE_HANDOFFS: &str = "talent_rd_hire_handoffs";
const SQL_RD_HIRE_HANDOFFS: &str = "SELECT * FROM talent_hire_handoffs WHERE offer_id=?";
const READ_HIRE_HANDOFFS: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_HIRE_HANDOFFS,
    sql: SQL_RD_HIRE_HANDOFFS,
    bind_count: 1,
};

/// Native successor of the TypeScript `nativeTalentExecutiveEvidence` open
/// requisition row read. The caller binds `[id, company_id, branch_id,
/// branch_id, department_id, department_id]` on its open unit.
pub const RD_EXEC_REQUISITION: &str = "talent_rd_exec_requisition";
const SQL_RD_EXEC_REQUISITION: &str =
    "SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id \
     FROM talent_requisitions r WHERE r.id=? AND r.company_id=? \
     AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?) \
     AND r.status='open' LIMIT 1";
const READ_EXEC_REQUISITION: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_EXEC_REQUISITION,
    sql: SQL_RD_EXEC_REQUISITION,
    bind_count: 6,
};

/// Native successor of the TypeScript `nativeTalentExecutiveEvidence`
/// application row read. The caller binds `[id, company_id, branch_id,
/// branch_id, department_id, department_id, from, to_exclusive]`.
pub const RD_EXEC_APPLICATION: &str = "talent_rd_exec_application";
const SQL_RD_EXEC_APPLICATION: &str =
    "SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id \
     FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id \
     WHERE a.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) \
     AND (?::text IS NULL OR r.department_id=?) AND a.updated_at>=? AND a.updated_at<? LIMIT 1";
const READ_EXEC_APPLICATION: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_EXEC_APPLICATION,
    sql: SQL_RD_EXEC_APPLICATION,
    bind_count: 8,
};

/// Native successor of the TypeScript `candidatePrivacyWorkspace` candidate
/// privacy-event history read. The caller binds `[candidate_id, company_id]`
/// on its open unit.
pub const RD_PRIVACY_HISTORY: &str = "talent_rd_privacy_history";
const SQL_RD_PRIVACY_HISTORY: &str =
    "SELECT id,action,candidate_version,created_at FROM talent_candidate_privacy_events WHERE candidate_id=? AND company_id=? ORDER BY candidate_version,id";
const READ_PRIVACY_HISTORY: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_PRIVACY_HISTORY,
    sql: SQL_RD_PRIVACY_HISTORY,
    bind_count: 2,
};

/// Native successor of the TypeScript `peopleHireHandoffFacts`
/// hire-consumption receipt read. The caller binds `[handoff_id, company_id]`
/// on its open unit.
pub const RD_CONSUME_HIRE_HANDOFF_RECEIPT: &str = "talent_rd_consume_hire_handoff_receipt";
const SQL_RD_CONSUME_HIRE_HANDOFF_RECEIPT: &str =
    "SELECT id FROM talent_hire_consumption_receipts WHERE handoff_id=? AND company_id=?";
const READ_CONSUME_HIRE_HANDOFF_RECEIPT: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_CONSUME_HIRE_HANDOFF_RECEIPT,
    sql: SQL_RD_CONSUME_HIRE_HANDOFF_RECEIPT,
    bind_count: 2,
};

/// Native successor of the TypeScript `peopleHireHandoffFacts` candidate
/// privacy row read. The caller binds `[candidate_id, company_id]` on its unit.
pub const RD_CHANGE_CANDIDATE_PRIVACY_ROW: &str = "talent_rd_change_candidate_privacy_row";
const SQL_RD_CHANGE_CANDIDATE_PRIVACY_ROW: &str =
    "SELECT id,status,privacy_hold,privacy_requested_by,version,retention_until FROM talent_candidates WHERE id=? AND company_id=?";
const READ_CHANGE_CANDIDATE_PRIVACY_ROW: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_CHANGE_CANDIDATE_PRIVACY_ROW,
    sql: SQL_RD_CHANGE_CANDIDATE_PRIVACY_ROW,
    bind_count: 2,
};

/// Native successors of the TypeScript `nativeTalentWorkspace` list reads. The
/// caller binds `[company_id, branch_id, branch_id, department_id,
/// department_id]` (requisitions) or `[company_id]` (candidates) on its unit.
pub const RD_WORKSPACE_REQUISITIONS: &str = "talent_rd_workspace_requisitions";
const SQL_RD_WORKSPACE_REQUISITIONS: &str =
    "SELECT * FROM talent_requisitions WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC";
const READ_WORKSPACE_REQUISITIONS: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_WORKSPACE_REQUISITIONS,
    sql: SQL_RD_WORKSPACE_REQUISITIONS,
    bind_count: 5,
};

pub const RD_WORKSPACE_CANDIDATES: &str = "talent_rd_workspace_candidates";
const SQL_RD_WORKSPACE_CANDIDATES: &str =
    "SELECT id,legal_name,email,phone,retention_until,status,version,updated_at FROM talent_candidates WHERE company_id=? ORDER BY updated_at DESC";
const READ_WORKSPACE_CANDIDATES: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_WORKSPACE_CANDIDATES,
    sql: SQL_RD_WORKSPACE_CANDIDATES,
    bind_count: 1,
};

/// Native successor of the TypeScript `SensitiveAttachmentTalentFacts`
/// offer row read. The caller binds `[offer_id, company_id]` on its unit.
pub const RD_SENSITIVE_OFFER: &str = "talent_rd_sensitive_offer";
const SQL_RD_SENSITIVE_OFFER: &str =
    "SELECT o.company_id,o.version,r.branch_id,r.department_id,a.owner_id,CASE WHEN (o.status<>'accepted' OR EXISTS(SELECT 1 FROM talent_hire_handoffs handoff JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=handoff.id AND receipt.company_id=handoff.company_id AND receipt.source_offer_version=handoff.source_offer_version WHERE handoff.offer_id=o.id AND handoff.company_id=o.company_id)) THEN 0 ELSE 1 END pending_hire_delivery FROM talent_offers o JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id WHERE o.id=? AND o.company_id=?";
const READ_SENSITIVE_OFFER: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_SENSITIVE_OFFER,
    sql: SQL_RD_SENSITIVE_OFFER,
    bind_count: 2,
};

/// Native successor of the TypeScript `peopleHireHandoffFacts` accepted hire
/// source row read. The caller binds `[handoff_id, company_id]` on its unit.
pub const RD_HIRE_SOURCE_ROW: &str = "talent_rd_hire_source_row";
const SQL_RD_HIRE_SOURCE_ROW: &str =
    "SELECT h.*,o.version offer_version,o.status offer_status,o.start_date,o.currency,o.base_amount_minor,o.accepted_evidence,r.job_title,c.version candidate_version,c.status candidate_status,c.retention_until,c.legal_name,c.email,c.phone,c.privacy_hold,c.consent_evidence FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id AND o.company_id=h.company_id JOIN talent_applications a ON a.id=o.application_id AND a.company_id=h.company_id JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=h.company_id AND r.branch_id=h.branch_id AND r.department_id IS NOT DISTINCT FROM h.department_id JOIN talent_candidates c ON c.id=h.candidate_id AND c.id=a.candidate_id AND c.company_id=h.company_id WHERE h.id=? AND h.company_id=?";
const READ_HIRE_SOURCE_ROW: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_HIRE_SOURCE_ROW,
    sql: SQL_RD_HIRE_SOURCE_ROW,
    bind_count: 2,
};

/// Native successor of the TypeScript `peopleHireHandoffFacts` hire queue read.
/// The caller binds `[company_id]` on its unit.
pub const RD_HIRE_HANDOFF_QUEUE: &str = "talent_rd_hire_handoff_queue";
const SQL_RD_HIRE_HANDOFF_QUEUE: &str =
    "SELECT h.*,o.application_id,o.currency,o.base_amount_minor,o.start_date,o.version offer_version,c.legal_name,c.version candidate_version,c.status candidate_status,r.job_title,x.id receipt_id,x.person_id,x.engagement_id FROM talent_hire_handoffs h JOIN talent_offers o ON o.id=h.offer_id JOIN talent_candidates c ON c.id=h.candidate_id JOIN talent_applications a ON a.id=o.application_id JOIN talent_requisitions r ON r.id=a.requisition_id LEFT JOIN talent_hire_consumption_receipts x ON x.handoff_id=h.id WHERE h.company_id=? ORDER BY h.created_at,h.id";
const READ_HIRE_HANDOFF_QUEUE: NativeTalentReadStatement = NativeTalentReadStatement {
    kind: RD_HIRE_HANDOFF_QUEUE,
    sql: SQL_RD_HIRE_HANDOFF_QUEUE,
    bind_count: 1,
};

const STATEMENTS: &[NativeTalentReadStatement] = &[
    READ_APPLICATION_EVENTS,
    READ_PROPOSALS,
    READ_OFFERS,
    READ_HIRE_HANDOFFS,
    READ_EXEC_REQUISITION,
    READ_EXEC_APPLICATION,
    READ_PRIVACY_HISTORY,
    READ_CONSUME_HIRE_HANDOFF_RECEIPT,
    READ_CHANGE_CANDIDATE_PRIVACY_ROW,
    READ_WORKSPACE_REQUISITIONS,
    READ_WORKSPACE_CANDIDATES,
    READ_SENSITIVE_OFFER,
    READ_HIRE_SOURCE_ROW,
    READ_HIRE_HANDOFF_QUEUE,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeTalentReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_reads_are_owner_authored_select_only_and_single_bound() {
        let expected = [
            (RD_APPLICATION_EVENTS, SQL_RD_APPLICATION_EVENTS),
            (RD_PROPOSALS, SQL_RD_PROPOSALS),
            (RD_OFFERS, SQL_RD_OFFERS),
            (RD_HIRE_HANDOFFS, SQL_RD_HIRE_HANDOFFS),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 1);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("talent_"),
                "owner read must stay on owner-local Talent tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), 1);
        }
        assert_eq!(STATEMENTS.len(), 14);
    }

    #[test]
    fn workspace_list_reads_are_owner_authored_select_only() {
        let expected = [
            (RD_WORKSPACE_REQUISITIONS, SQL_RD_WORKSPACE_REQUISITIONS, 5usize),
            (RD_WORKSPACE_CANDIDATES, SQL_RD_WORKSPACE_CANDIDATES, 1usize),
        ];
        for (kind, sql, binds) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, binds);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("talent_"),
                "owner read must stay on owner-local Talent tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
    }

    #[test]
    fn sensitive_offer_read_is_owner_authored_select_only() {
        let statement = read_statement(RD_SENSITIVE_OFFER).expect("registered owner read kind");
        assert_eq!(statement.kind, RD_SENSITIVE_OFFER);
        assert_eq!(statement.sql, SQL_RD_SENSITIVE_OFFER);
        assert_eq!(statement.bind_count, 2);
        assert!(statement.sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}", sql = statement.sql);
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!statement.sql.contains(forbidden), "owner read must not mutate: {forbidden}");
        }
        for table in [
            "talent_offers",
            "talent_applications",
            "talent_requisitions",
            "talent_hire_handoffs",
            "talent_hire_consumption_receipts",
        ] {
            assert!(
                statement.sql.contains(table),
                "sensitive offer read must stay on owner-local Talent tables: {table}"
            );
        }
        assert_eq!(statement.sql.matches('?').count(), 2);
    }

    #[test]
    fn hire_handoff_reads_are_owner_authored_select_only() {
        let expected = [
            (
                RD_CONSUME_HIRE_HANDOFF_RECEIPT,
                SQL_RD_CONSUME_HIRE_HANDOFF_RECEIPT,
            ),
            (
                RD_CHANGE_CANDIDATE_PRIVACY_ROW,
                SQL_RD_CHANGE_CANDIDATE_PRIVACY_ROW,
            ),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 2);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("talent_"),
                "owner read must stay on owner-local Talent tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), 2);
        }
        assert!(read_statement("talent_rd_consume_hire_handoff_receipt").is_some());
        assert!(read_statement("talent_rd_change_candidate_privacy_row").is_some());
    }

    #[test]
    fn privacy_history_read_is_owner_authored_select_only() {
        let statement = read_statement(RD_PRIVACY_HISTORY).expect("registered owner read kind");
        assert_eq!(statement.kind, RD_PRIVACY_HISTORY);
        assert_eq!(statement.sql, SQL_RD_PRIVACY_HISTORY);
        assert_eq!(statement.bind_count, 2);
        assert!(statement.sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}", sql = statement.sql);
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
            assert!(!statement.sql.contains(forbidden), "owner read must not mutate: {forbidden}");
        }
        assert!(
            statement.sql.contains("talent_candidate_privacy_events"),
            "privacy history must stay on the owner-local Talent table: {sql}",
            sql = statement.sql
        );
        assert_eq!(statement.sql.matches('?').count(), 2);
    }

    #[test]
    fn executive_evidence_reads_are_owner_authored_select_only() {
        let expected = [
            (RD_EXEC_REQUISITION, SQL_RD_EXEC_REQUISITION, 6usize),
            (RD_EXEC_APPLICATION, SQL_RD_EXEC_APPLICATION, 8usize),
        ];
        for (kind, sql, binds) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, binds);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            assert!(
                sql.contains("talent_"),
                "owner read must stay on owner-local Talent tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
    }

    #[test]
    fn hire_handoff_fact_reads_are_owner_authored_select_only() {
        let expected = [
            (RD_HIRE_SOURCE_ROW, SQL_RD_HIRE_SOURCE_ROW, 2usize),
            (RD_HIRE_HANDOFF_QUEUE, SQL_RD_HIRE_HANDOFF_QUEUE, 1usize),
        ];
        for (kind, sql, binds) in expected {
            let statement = read_statement(kind).expect("registered owner read kind");
            assert_eq!(statement.kind, kind);
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, binds);
            assert!(sql.starts_with("SELECT "), "owner read must be SELECT-only: {sql}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE"] {
                assert!(!sql.contains(forbidden), "owner read must not mutate: {forbidden}");
            }
            for table in [
                "talent_hire_handoffs",
                "talent_offers",
                "talent_applications",
                "talent_requisitions",
                "talent_candidates",
            ] {
                assert!(sql.contains(table), "hire fact read must stay on owner-local Talent tables: {table}");
            }
            assert_eq!(sql.matches('?').count(), binds);
        }
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT * FROM talent_offers").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("talent_rd_unknown").is_none());
        let statement = read_statement(RD_PROPOSALS).expect("proposal read");
        assert!(statement.bind_count != 2);
    }
}
