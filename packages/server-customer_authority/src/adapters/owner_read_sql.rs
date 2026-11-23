//! Owner-owned caller-unit read plans for Customer Authority. The SQL is the
//! native authority: it was copied from the retired TypeScript read literal
//! with the exact positional payloads. The mounted customer-access
//! `command-plan` route resolves the allow-listed kind here and returns the
//! statement; the TypeScript caller prepares and executes it on its own
//! already-open unit. No SQL is accepted from the wire and no TypeScript
//! mirror remains for these reads.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller executes them on the
//! same unit as the surrounding command, so they observe uncommitted effects.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, and `bind_count` the
/// positional argument count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

/// Native successor of the TypeScript `sales-review-counts` read. The owner
/// plans the literal; the caller binds `[companyId, companyId, companyId,
/// branchId, branchId, departmentId, departmentId]` on its open unit.
pub const RD_SALES_REVIEW_COUNTS: &str = "customer_authority_rd_sales_review_counts";

/// The closed commerce order projection is inlined verbatim from the owner
/// projection contract so the planned statement is byte-equal to the retired
/// TypeScript statement.
const SQL_RD_SALES_REVIEW_COUNTS: &str = concat!(
    "SELECT 'identity' kind,COUNT(*) count FROM customer_identity_changes WHERE company_id=? AND status='pending' ",
    "UNION ALL SELECT 'duplicates',COUNT(*) FROM customer_duplicate_reviews WHERE company_id=? AND status='pending' ",
    "UNION ALL SELECT 'access',COUNT(*) FROM customer_access_bindings b JOIN (",
    "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",",
    "\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",",
    "\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",",
    "\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders",
    ") o ON o.id=b.order_id AND o.company_id=b.company_id ",
    "WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND b.status='pending'",
);

const READ_SALES_REVIEW_COUNTS: NativeReadStatement = NativeReadStatement {
    kind: RD_SALES_REVIEW_COUNTS,
    sql: SQL_RD_SALES_REVIEW_COUNTS,
    bind_count: 7,
};

/// Native successor of the TypeScript `identity.ts` change-history read. The
/// owner plans the literal; the caller binds `[changeId]` on its open unit.
pub const RD_IDENTITY_CHANGE_HISTORY: &str = "customer_authority_rd_identity_change_history";

const SQL_RD_IDENTITY_CHANGE_HISTORY: &str =
    "SELECT * FROM customer_identity_change_events WHERE change_id=? ORDER BY version";

const READ_IDENTITY_CHANGE_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_IDENTITY_CHANGE_HISTORY,
    sql: SQL_RD_IDENTITY_CHANGE_HISTORY,
    bind_count: 1,
};

/// Native successor of the TypeScript `duplicates.ts` review-history read.
pub const RD_DUPLICATE_REVIEW_HISTORY: &str = "customer_authority_rd_duplicate_review_history";

const SQL_RD_DUPLICATE_REVIEW_HISTORY: &str =
    "SELECT * FROM customer_duplicate_events WHERE review_id=? ORDER BY version";

const READ_DUPLICATE_REVIEW_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_DUPLICATE_REVIEW_HISTORY,
    sql: SQL_RD_DUPLICATE_REVIEW_HISTORY,
    bind_count: 1,
};

/// Native successors of the TypeScript customer-access history reads. The
/// owner plans the literal; the caller binds `[bindingId]` on its open unit.
pub const RD_ACCESS_EVENTS: &str = "customer_authority_rd_access_events";
pub const RD_ACCESS_READS: &str = "customer_authority_rd_access_reads";

const SQL_RD_ACCESS_EVENTS: &str =
    "SELECT version,action,actor_id,reason,created_at FROM customer_access_events WHERE binding_id=? ORDER BY version";

const SQL_RD_ACCESS_READS: &str =
    "SELECT binding_version,user_id,read_at,projection_version FROM customer_access_reads WHERE binding_id=? ORDER BY read_at DESC LIMIT 100";

const READ_ACCESS_EVENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_ACCESS_EVENTS,
    sql: SQL_RD_ACCESS_EVENTS,
    bind_count: 1,
};

const READ_ACCESS_READS: NativeReadStatement = NativeReadStatement {
    kind: RD_ACCESS_READS,
    sql: SQL_RD_ACCESS_READS,
    bind_count: 1,
};

/// Native successor of the TypeScript `access.ts` bound-identity existence
/// read. The owner plans the literal; the caller binds `[bindingId]` on its
/// open unit and treats any row as "an identity is already bound".
pub const RD_ACCESS_IDENTITIES: &str = "customer_authority_rd_access_identities";

const SQL_RD_ACCESS_IDENTITIES: &str =
    "SELECT 1 FROM customer_access_identities WHERE binding_id=?";

const READ_ACCESS_IDENTITIES: NativeReadStatement = NativeReadStatement {
    kind: RD_ACCESS_IDENTITIES,
    sql: SQL_RD_ACCESS_IDENTITIES,
    bind_count: 1,
};

/// Native successor of the TypeScript `executive-metric-projections.ts`
/// pending duplicate-review metric count. The owner plans the literal; the
/// caller binds `[companyId]` on its own unit and reuses the exact planned
/// statement for executive source drill-down. It is a distinct owner-local
/// count read, not the duplicate review-history event read.
pub const RD_DUPLICATE_REVIEWS_PENDING: &str = "customer_authority_rd_duplicate_reviews_pending";

const SQL_RD_DUPLICATE_REVIEWS_PENDING: &str =
    "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";

const READ_DUPLICATE_REVIEWS_PENDING: NativeReadStatement = NativeReadStatement {
    kind: RD_DUPLICATE_REVIEWS_PENDING,
    sql: SQL_RD_DUPLICATE_REVIEWS_PENDING,
    bind_count: 1,
};

/// Native successors of the TypeScript `executive-metric-projections.ts`
/// identity-change-events breakdown. The owner plans the literal; the caller
/// binds `[companyId, from, to]` on its own unit and reuses the exact planned
/// statement for executive source drill-down. The month/default and action
/// breakdowns are distinct owner-local fixed statements; no TypeScript mirror
/// remains for either.
pub const RD_IDENTITY_CHANGE_EVENTS_MONTH: &str = "customer_authority_rd_identity_change_events_month";
pub const RD_IDENTITY_CHANGE_EVENTS_ACTION: &str = "customer_authority_rd_identity_change_events_action";

const SQL_RD_IDENTITY_CHANGE_EVENTS_MONTH: &str = concat!(
    "SELECT to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value ",
    "FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id ",
    "WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1",
);

const SQL_RD_IDENTITY_CHANGE_EVENTS_ACTION: &str = concat!(
    "SELECT e.action label,count(*)::text value ",
    "FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id ",
    "WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1",
);

const READ_IDENTITY_CHANGE_EVENTS_MONTH: NativeReadStatement = NativeReadStatement {
    kind: RD_IDENTITY_CHANGE_EVENTS_MONTH,
    sql: SQL_RD_IDENTITY_CHANGE_EVENTS_MONTH,
    bind_count: 3,
};

const READ_IDENTITY_CHANGE_EVENTS_ACTION: NativeReadStatement = NativeReadStatement {
    kind: RD_IDENTITY_CHANGE_EVENTS_ACTION,
    sql: SQL_RD_IDENTITY_CHANGE_EVENTS_ACTION,
    bind_count: 3,
};

const READS: &[NativeReadStatement] = &[
    READ_SALES_REVIEW_COUNTS,
    READ_IDENTITY_CHANGE_HISTORY,
    READ_DUPLICATE_REVIEW_HISTORY,
    READ_ACCESS_EVENTS,
    READ_ACCESS_READS,
    READ_ACCESS_IDENTITIES,
    READ_DUPLICATE_REVIEWS_PENDING,
    READ_IDENTITY_CHANGE_EVENTS_MONTH,
    READ_IDENTITY_CHANGE_EVENTS_ACTION,
];

/// Resolve an owner read kind. `None` for every unregistered discriminator.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|read| read.kind == kind)
}

/// Whether a discriminator is an owner read kind (never a write kind).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sales_review_counts_is_a_fixed_select_with_exact_binds() {
        let read = read_statement(RD_SALES_REVIEW_COUNTS).expect("read kind");
        assert!(is_read(RD_SALES_REVIEW_COUNTS));
        assert_eq!(read.bind_count, 7);
        assert_eq!(read.sql.matches('?').count(), 7);
        assert!(read.sql.starts_with("SELECT 'identity' kind,COUNT(*) count"));
        assert!(read.sql.contains("customer_duplicate_reviews"));
        assert!(read.sql.contains("customer_access_bindings"));
        assert!(read.sql.contains("commerce_orders"));
        assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
        assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
        assert!(!read.sql.to_uppercase().contains("UPDATE "));
    }

    #[test]
    fn history_reads_are_fixed_selects_with_exact_binds() {
        for (kind, table) in [
            (RD_IDENTITY_CHANGE_HISTORY, "customer_identity_change_events"),
            (RD_DUPLICATE_REVIEW_HISTORY, "customer_duplicate_events"),
        ] {
            let read = read_statement(kind).expect("read kind");
            assert!(is_read(kind));
            assert_eq!(read.bind_count, 1);
            assert_eq!(read.sql.matches('?').count(), 1);
            assert!(read.sql.starts_with("SELECT *"));
            assert!(read.sql.contains(table));
            assert!(read.sql.contains("ORDER BY version"));
            assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
            assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
            assert!(!read.sql.to_uppercase().contains("UPDATE "));
        }
    }

    #[test]
    fn access_history_reads_are_fixed_selects_with_exact_binds() {
        for (kind, table) in [
            (RD_ACCESS_EVENTS, "customer_access_events"),
            (RD_ACCESS_READS, "customer_access_reads"),
        ] {
            let read = read_statement(kind).expect("read kind");
            assert!(is_read(kind));
            assert_eq!(read.bind_count, 1);
            assert_eq!(read.sql.matches('?').count(), 1);
            assert!(read.sql.starts_with("SELECT"));
            assert!(read.sql.contains(table));
            assert!(read.sql.contains("binding_id=?"));
            assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
            assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
            assert!(!read.sql.to_uppercase().contains("UPDATE "));
        }
    }

    #[test]
    fn access_identity_read_is_a_fixed_select_with_exact_binds() {
        let read = read_statement(RD_ACCESS_IDENTITIES).expect("read kind");
        assert!(is_read(RD_ACCESS_IDENTITIES));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert!(read.sql.starts_with("SELECT 1 FROM customer_access_identities"));
        assert!(read.sql.contains("binding_id=?"));
        assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
        assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
        assert!(!read.sql.to_uppercase().contains("UPDATE "));
    }

    #[test]
    fn duplicate_reviews_pending_is_a_fixed_select_with_exact_binds() {
        let read = read_statement(RD_DUPLICATE_REVIEWS_PENDING).expect("read kind");
        assert!(is_read(RD_DUPLICATE_REVIEWS_PENDING));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(
            read.sql,
            "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'"
        );
        assert!(read.sql.contains("count(*)::text value"));
        assert!(read.sql.contains("company_id=?"));
        assert!(read.sql.contains("status='pending'"));
        assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
        assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
        assert!(!read.sql.to_uppercase().contains("UPDATE "));
    }

    #[test]
    fn identity_change_events_breakdowns_are_fixed_selects_with_exact_binds() {
        for (kind, label) in [
            (RD_IDENTITY_CHANGE_EVENTS_MONTH, "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label"),
            (RD_IDENTITY_CHANGE_EVENTS_ACTION, "e.action label"),
        ] {
            let read = read_statement(kind).expect("read kind");
            assert!(is_read(kind));
            assert_eq!(read.bind_count, 3);
            assert_eq!(read.sql.matches('?').count(), 3);
            assert!(read.sql.starts_with(&format!("SELECT {label},count(*)::text value")));
            assert!(read.sql.contains("FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id"));
            assert!(read.sql.contains("WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?"));
            assert!(read.sql.ends_with("GROUP BY 1 ORDER BY 1"));
            assert!(!read.sql.to_uppercase().contains("INSERT INTO"));
            assert!(!read.sql.to_uppercase().contains("DELETE FROM"));
            assert!(!read.sql.to_uppercase().contains("UPDATE "));
        }
    }

    #[test]
    fn unregistered_kinds_are_rejected() {
        assert!(read_statement("unknown_read_kind").is_none());
        assert!(!is_read("portal_access_audit"));
    }
}
