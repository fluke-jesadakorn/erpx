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

const STATEMENTS: &[NativeTalentReadStatement] = &[
    READ_APPLICATION_EVENTS,
    READ_PROPOSALS,
    READ_OFFERS,
    READ_HIRE_HANDOFFS,
    READ_EXEC_REQUISITION,
    READ_EXEC_APPLICATION,
    READ_PRIVACY_HISTORY,
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
        assert_eq!(STATEMENTS.len(), 7);
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
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT * FROM talent_offers").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("talent_rd_unknown").is_none());
        let statement = read_statement(RD_PROPOSALS).expect("proposal read");
        assert!(statement.bind_count != 2);
    }
}
