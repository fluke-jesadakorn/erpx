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

const STATEMENTS: &[NativeTalentReadStatement] = &[
    READ_APPLICATION_EVENTS,
    READ_PROPOSALS,
    READ_OFFERS,
    READ_HIRE_HANDOFFS,
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
        assert_eq!(STATEMENTS.len(), 4);
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
