//! Owner-owned caller-unit read plans for CRM. Generated from the retired
//! TypeScript SELECT literal with its exact positional payload; the axum
//! `command-plan` route resolves the kind here and returns the statement for
//! the caller to prepare and execute on its own already-open unit.
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

/// Native successor of the TypeScript `collaboration-notification-lead` read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[recordId, companyId]` binds.
pub const RD_COLLAB_NOTIFICATION_LEAD: &str = "crm_rd_collab_notification_lead";
const SQL_COLLAB_NOTIFICATION_LEAD: &str =
    "SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?";
const READ_COLLAB_NOTIFICATION_LEAD: NativeReadStatement = NativeReadStatement {
    kind: RD_COLLAB_NOTIFICATION_LEAD,
    sql: SQL_COLLAB_NOTIFICATION_LEAD,
    bind_count: 2,
};

const READS: &[NativeReadStatement] = &[READ_COLLAB_NOTIFICATION_LEAD];

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
        let read = read_statement(RD_COLLAB_NOTIFICATION_LEAD).expect("read kind");
        assert!(read.sql.to_ascii_uppercase().starts_with("SELECT"));
        assert!(!read.sql.to_ascii_uppercase().contains("INSERT"));
        assert!(!read.sql.to_ascii_uppercase().contains("UPDATE"));
        assert!(!read.sql.to_ascii_uppercase().contains("DELETE"));
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert!(is_read(RD_COLLAB_NOTIFICATION_LEAD));
        assert!(!is_read("crm_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn read_sql_matches_retired_typeScript_literal() {
        let read = read_statement(RD_COLLAB_NOTIFICATION_LEAD).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT lead_id FROM crm_lead_activities WHERE id=? AND company_id=?"
        );
    }
}
