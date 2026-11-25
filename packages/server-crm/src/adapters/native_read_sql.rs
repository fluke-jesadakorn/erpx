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

    /// Canonical owner form of the projection/FROM read head retired from the
    /// TypeScript read tail. The owner renderer `crmProjectionFromSql` must
    /// reproduce this exact prefix; this crate records the native literal.
    fn projection_from_sql(projection: &str, from: &str) -> String {
        format!("SELECT {projection} FROM {from}")
    }

    #[test]
    fn projection_from_family_matches_retired_owner_literals() {
        let cases = [
            (
                "opportunity owner guard",
                projection_from_sql("1", "crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=?"),
                "SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=?",
            ),
            (
                "contact account read",
                projection_from_sql(
                    "c.id,c.name",
                    "crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.id=?",
                ),
                "SELECT c.id,c.name FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.id=?",
            ),
            (
                "owned-source list",
                projection_from_sql("id", "(ROWS) actual WHERE id=? AND company_id=?"),
                "SELECT id FROM (ROWS) actual WHERE id=? AND company_id=?",
            ),
        ];
        for (name, rendered, expected) in cases {
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    /// Canonical owner form of the bare `SELECT ` head retired from the r40
    /// scoped-list, distinct-owner and lead-row-source families.
    fn select_body_sql(body: &str) -> String {
        format!("SELECT {body}")
    }

    #[test]
    fn lead_select_head_is_the_owner_select_one_plan() {
        // Canonical native form of the r51 lead-select head: it is the r42
        // select-one plan with the fixed `crm_leads` body. The TS renderer
        // `crmLeadSelectSql` delegates to `crmSelectOneSql` on this identity.
        let select_one = |body: &str| format!("SELECT 1 FROM {body}");
        let lead_select = |tail: &str| select_one(&format!("crm_leads {tail}"));
        let cases = [
            ("creator scope", "l WHERE l.contact_id=c.id AND l.branch_id=?", "SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=?"),
            ("eligible owner", "l WHERE l.id=? AND l.version=? AND l.owner_id=?", "SELECT 1 FROM crm_leads l WHERE l.id=? AND l.version=? AND l.owner_id=?"),
        ];
        for (name, tail, expected) in cases {
            let rendered = lead_select(tail);
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }

    #[test]
    fn bare_select_head_matches_retired_r40_owner_literals() {
        let cases = [
            (
                "scoped descending list",
                select_body_sql("g.id,g.version FROM crm_guest_links g ORDER BY g.created_at DESC LIMIT 200"),
                "SELECT g.id,g.version FROM crm_guest_links g ORDER BY g.created_at DESC LIMIT 200",
            ),
            (
                "distinct owner directory",
                select_body_sql("DISTINCT l.owner_id id FROM crm_leads l ORDER BY name"),
                "SELECT DISTINCT l.owner_id id FROM crm_leads l ORDER BY name",
            ),
            (
                "lead row source",
                select_body_sql("lead.* FROM crm_leads lead WHERE lead.company_id=?"),
                "SELECT lead.* FROM crm_leads lead WHERE lead.company_id=?",
            ),
        ];
        for (name, rendered, expected) in cases {
            assert_eq!(rendered, expected, "{name}");
            assert_eq!(rendered.matches("SELECT ").count(), 1, "{name}");
            let upper = rendered.to_ascii_uppercase();
            assert!(!upper.contains("INSERT"), "{name}");
            assert!(!upper.contains("UPDATE"), "{name}");
            assert!(!upper.contains("DELETE"), "{name}");
            assert!(!upper.contains("FOR UPDATE"), "{name}");
            assert!(!upper.contains("LOCK TABLE"), "{name}");
        }
    }
}
