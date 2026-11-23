//! Owner-owned caller-unit read plans for Support. Generated from the retired
//! TypeScript SELECT literals with the exact positional payloads; the mounted
//! `support/command-plan` route resolves the allow-listed kind here and returns
//! the statement for the caller to prepare and execute on its own open unit.
//!
//! This family covers the small owner-local Support service-team coverage and
//! settings-history reads on `service_teams`, `service_team_coverage` and
//! `service_team_revisions`. Reads carry no receipt/audit/outbox ledger: the
//! caller executes them on the same unit as the surrounding command. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSupportReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `serviceCoverageSettingsHistory` read.
/// The caller binds `[company_id]`.
pub const RD_SERVICE_COVERAGE_HISTORY: &str = "support_rd_service_coverage_history";
const SQL_SERVICE_COVERAGE_HISTORY: &str =
    "SELECT t.name title,r.version,r.actor_id actor,r.created_at at,'Service coverage' kind FROM service_team_revisions r JOIN service_teams t ON t.id=r.team_id AND t.company_id=r.company_id WHERE r.company_id=? ORDER BY r.created_at DESC";
const READ_SERVICE_COVERAGE_HISTORY: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_COVERAGE_HISTORY,
    sql: SQL_SERVICE_COVERAGE_HISTORY,
    bind_count: 1,
};

/// Native successor of the TypeScript `resolveServiceTeam` coverage read.
/// The caller binds `[company_id, category, company_id, branch_id, department_id]`.
pub const RD_SERVICE_TEAM_COVERAGE: &str = "support_rd_service_team_coverage";
const SQL_SERVICE_TEAM_COVERAGE: &str = "SELECT t.*,c.scope_kind,c.scope_node_id FROM service_team_coverage c JOIN service_teams t ON t.id=c.team_id AND t.company_id=c.company_id WHERE c.company_id=? AND c.category=? AND t.active=1 AND ((c.scope_kind='holding' AND c.scope_node_id=?) OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?)) ORDER BY CASE c.scope_kind WHEN 'department' THEN 3 WHEN 'branch' THEN 2 ELSE 1 END DESC,t.id";
const READ_SERVICE_TEAM_COVERAGE: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_COVERAGE,
    sql: SQL_SERVICE_TEAM_COVERAGE,
    bind_count: 5,
};

/// Native successor of the TypeScript service-team name lookups used by the
/// records employee-request and workflow flow-routing facts. The caller binds
/// `[team_id, company_id]` and receives the single team name row.
pub const RD_SERVICE_TEAM_NAME: &str = "support_rd_service_team_name";
const SQL_SERVICE_TEAM_NAME: &str =
    "SELECT name FROM service_teams WHERE id=? AND company_id=?";
const READ_SERVICE_TEAM_NAME: NativeSupportReadStatement = NativeSupportReadStatement {
    kind: RD_SERVICE_TEAM_NAME,
    sql: SQL_SERVICE_TEAM_NAME,
    bind_count: 2,
};

const STATEMENTS: &[NativeSupportReadStatement] = &[
    READ_SERVICE_COVERAGE_HISTORY,
    READ_SERVICE_TEAM_COVERAGE,
    READ_SERVICE_TEAM_NAME,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeSupportReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_team_reads_are_owner_authored_select_only() {
        let expected = [
            (
                RD_SERVICE_COVERAGE_HISTORY,
                SQL_SERVICE_COVERAGE_HISTORY,
                1usize,
            ),
            (
                RD_SERVICE_TEAM_COVERAGE,
                SQL_SERVICE_TEAM_COVERAGE,
                5usize,
            ),
            (
                RD_SERVICE_TEAM_NAME,
                SQL_SERVICE_TEAM_NAME,
                2usize,
            ),
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
                sql.contains("service_team"),
                "owner read must stay on owner-local Support team tables: {sql}"
            );
            assert_eq!(sql.matches('?').count(), binds);
        }
        assert_eq!(STATEMENTS.len(), 3);
    }

    #[test]
    fn unknown_and_mismatched_read_kinds_fail_closed() {
        assert!(read_statement("SELECT 1 FROM service_teams").is_none());
        assert!(read_statement("").is_none());
        assert!(read_statement("support_rd_unknown").is_none());
        let statement = read_statement(RD_SERVICE_TEAM_COVERAGE).expect("coverage read");
        assert_eq!(statement.bind_count, 5);
        assert!(statement.bind_count != 1);
        let name = read_statement(RD_SERVICE_TEAM_NAME).expect("team name read");
        assert_eq!(name.bind_count, 2);
    }
}
