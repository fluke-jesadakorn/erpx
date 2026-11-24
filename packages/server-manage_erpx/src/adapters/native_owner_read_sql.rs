//! Owner-owned caller-unit read plans for Manage ERPX. Generated from the
//! retired TypeScript `beta_policy` point-read literal with the exact
//! positional payload. The mounted `/manage/command-plan` route resolves the
//! allow-listed kind here and returns the statement for the caller to prepare
//! and execute on its own open unit.
//!
//! The three kinds are the closed projection union admitted by the owner
//! helper: `*`, `member_limit` and `branch_limit`. No other projection or SQL
//! text can cross the wire. Reads carry no receipt/audit/outbox ledger.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound and `bind_count` the number
/// of positional arguments the caller must send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeManageReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `betaPolicyPointSql('*')` point read.
/// The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_ALL: &str = "manage_rd_beta_policy_point_all";
const SQL_BETA_POLICY_POINT_ALL: &str = "SELECT * FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_ALL: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_ALL,
    sql: SQL_BETA_POLICY_POINT_ALL,
    bind_count: 1,
};

/// Native successor of the TypeScript `betaPolicyPointSql('member_limit')`
/// point read. The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_MEMBER: &str = "manage_rd_beta_policy_point_member_limit";
const SQL_BETA_POLICY_POINT_MEMBER: &str =
    "SELECT member_limit FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_MEMBER: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_MEMBER,
    sql: SQL_BETA_POLICY_POINT_MEMBER,
    bind_count: 1,
};

/// Native successor of the TypeScript `betaPolicyPointSql('branch_limit')`
/// point read. The caller binds `[company_id]`.
pub const ME_RD_BETA_POLICY_POINT_BRANCH: &str = "manage_rd_beta_policy_point_branch_limit";
const SQL_BETA_POLICY_POINT_BRANCH: &str =
    "SELECT branch_limit FROM beta_policy WHERE company_id=?";
const READ_BETA_POLICY_POINT_BRANCH: NativeManageReadStatement = NativeManageReadStatement {
    kind: ME_RD_BETA_POLICY_POINT_BRANCH,
    sql: SQL_BETA_POLICY_POINT_BRANCH,
    bind_count: 1,
};

const STATEMENTS: &[NativeManageReadStatement] = &[
    READ_BETA_POLICY_POINT_ALL,
    READ_BETA_POLICY_POINT_MEMBER,
    READ_BETA_POLICY_POINT_BRANCH,
];

/// Resolve an allow-listed owner read kind. Any unregistered kind fails closed.
pub fn read_statement(kind: &str) -> Option<&'static NativeManageReadStatement> {
    STATEMENTS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_point_reads_render_the_retired_literals_byte_for_byte() {
        let expected = [
            (ME_RD_BETA_POLICY_POINT_ALL, SQL_BETA_POLICY_POINT_ALL),
            (ME_RD_BETA_POLICY_POINT_MEMBER, SQL_BETA_POLICY_POINT_MEMBER),
            (ME_RD_BETA_POLICY_POINT_BRANCH, SQL_BETA_POLICY_POINT_BRANCH),
        ];
        for (kind, sql) in expected {
            let statement = read_statement(kind).expect("registered beta_policy point read");
            assert_eq!(statement.sql, sql);
            assert_eq!(statement.bind_count, 1);
            assert_eq!(sql.matches('?').count(), 1);
            assert!(sql.starts_with("SELECT "));
            assert!(sql.ends_with(" FROM beta_policy WHERE company_id=?"));
        }
        assert_eq!(STATEMENTS.len(), 3);
        assert_eq!(
            SQL_BETA_POLICY_POINT_ALL,
            "SELECT * FROM beta_policy WHERE company_id=?"
        );
        assert_eq!(
            SQL_BETA_POLICY_POINT_MEMBER,
            "SELECT member_limit FROM beta_policy WHERE company_id=?"
        );
        assert_eq!(
            SQL_BETA_POLICY_POINT_BRANCH,
            "SELECT branch_limit FROM beta_policy WHERE company_id=?"
        );
    }

    #[test]
    fn unadmitted_projections_fail_closed() {
        for kind in [
            "manage_rd_beta_policy_point_ai_monthly_limit",
            "manage_rd_beta_policy_point",
            "beta_policy_point_select",
        ] {
            assert!(read_statement(kind).is_none(), "{kind} must stay unregistered");
        }
    }

    #[test]
    fn owner_reads_are_select_only_without_locks_or_writes() {
        for statement in STATEMENTS {
            assert!(!statement.sql.contains(';'));
            assert!(!statement.sql.to_ascii_lowercase().contains("lock "));
            assert!(!statement.sql.to_ascii_lowercase().contains("for update"));
            assert!(!statement.sql.to_ascii_lowercase().contains("insert "));
            assert!(!statement.sql.to_ascii_lowercase().contains("update "));
            assert!(!statement.sql.to_ascii_lowercase().contains("delete "));
        }
    }
}
