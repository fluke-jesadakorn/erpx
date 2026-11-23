//! Owner-owned caller-unit read plans for Operations. Generated from the
//! retired TypeScript SELECT literal with its exact positional payload; the
//! axum `command-plan` route resolves the kind here and returns the statement
//! for the caller to prepare and execute on its own already-open unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, `bind_count` the positional
/// argument count and `scope` the routing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Native successor of the TypeScript `workflow-projection-source` read. The
/// owner plans this literal; the caller executes it on its open unit with the
/// same `[id]` bind. The typed native port `workflow_projection_source` is
/// mounted on the caller's connection and carries the same owner columns.
pub const RD_TASK_PROJECTION_SOURCE: &str = "operations_rd_task_projection_source";
const SQL_TASK_PROJECTION_SOURCE: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?";
const READ_TASK_PROJECTION_SOURCE: NativeReadStatement = NativeReadStatement {
    kind: RD_TASK_PROJECTION_SOURCE,
    sql: SQL_TASK_PROJECTION_SOURCE,
    bind_count: 1,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[READ_TASK_PROJECTION_SOURCE];

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
    fn read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_TASK_PROJECTION_SOURCE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_TASK_PROJECTION_SOURCE));
        assert!(!is_read("operations_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_TASK_PROJECTION_SOURCE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=?"
        );
    }
}
