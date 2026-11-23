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

/// Native successor of the TypeScript `app-upload-task-source` read. The owner
/// plans this literal; the caller executes it on its open unit with the same
/// `[id, company_id]` bind.
pub const RD_APP_UPLOAD_TASK_STATE: &str = "operations_rd_app_upload_task_state";
const SQL_APP_UPLOAD_TASK_STATE: &str =
    "SELECT state FROM staff_tasks WHERE id=? AND company_id=?";
const READ_APP_UPLOAD_TASK_STATE: NativeReadStatement = NativeReadStatement {
    kind: RD_APP_UPLOAD_TASK_STATE,
    sql: SQL_APP_UPLOAD_TASK_STATE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `workflow-flow-routing-facts` read. The
/// owner plans this literal; the caller executes it on its open unit with the
/// same `[company_id, id]` bind. `id` may be a string, number or null exactly as
/// the retired owner literal bound it.
pub const RD_FLOW_DEFINITIONS_DELEGATION: &str = "operations_rd_flow_definitions_delegation";
const SQL_FLOW_DEFINITIONS_DELEGATION: &str = "SELECT delegate_user_id FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY created_at DESC,id DESC LIMIT 1";
const READ_FLOW_DEFINITIONS_DELEGATION: NativeReadStatement = NativeReadStatement {
    kind: RD_FLOW_DEFINITIONS_DELEGATION,
    sql: SQL_FLOW_DEFINITIONS_DELEGATION,
    bind_count: 2,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[
    READ_TASK_PROJECTION_SOURCE,
    READ_APP_UPLOAD_TASK_STATE,
    READ_FLOW_DEFINITIONS_DELEGATION,
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

    #[test]
    fn app_upload_task_state_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_APP_UPLOAD_TASK_STATE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_APP_UPLOAD_TASK_STATE));
    }

    #[test]
    fn app_upload_task_state_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_APP_UPLOAD_TASK_STATE).expect("read kind");
        assert_eq!(read.sql, "SELECT state FROM staff_tasks WHERE id=? AND company_id=?");
    }

    #[test]
    fn flow_definitions_delegation_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_FLOW_DEFINITIONS_DELEGATION).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(upper.contains("STAFF_TASK_DELEGATIONS"));
        assert!(upper.contains("DELEGATE_USER_ID"));
        assert!(is_read(RD_FLOW_DEFINITIONS_DELEGATION));
    }

    #[test]
    fn flow_definitions_delegation_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_FLOW_DEFINITIONS_DELEGATION).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT delegate_user_id FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY created_at DESC,id DESC LIMIT 1"
        );
    }
}
