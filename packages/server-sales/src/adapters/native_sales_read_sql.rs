//! Owner-owned caller-unit read plans for Sales. Generated from the retired
//! TypeScript SELECT literals with their exact positional payloads; the mounted
//! axum `sales/command-plan` route resolves the kind here and returns the
//! statement for the caller to prepare and execute on its own already-open unit.
//!
//! This family covers the small owner-local active price-list reads on the
//! Sales-owned `sales_price_lists` and `sales_price_list_items` relations.
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, `bind_count` the positional
/// argument count and `scope` the routing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSalesReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Native successor of the TypeScript `CommerceSalesFacts.readSalesPriceLists1`
/// active price-list read. The caller binds `[company_id]`.
pub const RD_PRICE_LIST_ACTIVE: &str = "sales_rd_price_list_active";
const SQL_PRICE_LIST_ACTIVE: &str =
    "SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name";
const READ_PRICE_LIST_ACTIVE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICE_LIST_ACTIVE,
    sql: SQL_PRICE_LIST_ACTIVE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `CommerceSalesFacts.readSalesPriceListItems1`
/// active price-list item read. The caller binds `[company_id]`.
pub const RD_PRICE_LIST_ITEMS_ACTIVE: &str = "sales_rd_price_list_items_active";
const SQL_PRICE_LIST_ITEMS_ACTIVE: &str =
    "SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal";
const READ_PRICE_LIST_ITEMS_ACTIVE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICE_LIST_ITEMS_ACTIVE,
    sql: SQL_PRICE_LIST_ITEMS_ACTIVE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript notification-purchases
/// `customerNotificationReconciliationControls` unknown-attempt read. The caller
/// binds `[notification_id, company_id, attempt]`; the SQL is byte-identical to
/// the retired owner literal.
pub const RD_NOTIFICATION_ATTEMPT: &str = "sales_rd_notification_attempt";
const SQL_NOTIFICATION_ATTEMPT: &str =
    "SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'";
const READ_NOTIFICATION_ATTEMPT: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_NOTIFICATION_ATTEMPT,
    sql: SQL_NOTIFICATION_ATTEMPT,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript notification-purchases
/// `customerNotificationReconciliationControls` reconciliation-installed check.
/// The caller binds no values; the SQL is byte-identical to the retired owner
/// literal.
pub const RD_NOTIFICATION_RECONCILIATION_INSTALLED: &str =
    "sales_rd_notification_reconciliation_installed";
const SQL_NOTIFICATION_RECONCILIATION_INSTALLED: &str =
    "SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed";
const READ_NOTIFICATION_RECONCILIATION_INSTALLED: NativeSalesReadStatement =
    NativeSalesReadStatement {
        kind: RD_NOTIFICATION_RECONCILIATION_INSTALLED,
        sql: SQL_NOTIFICATION_RECONCILIATION_INSTALLED,
        bind_count: 0,
        scope: "holding",
    };

/// Native successor of the TypeScript followups `recipients` read. The
/// caller binds [company, company, order, company, branch, branch, department,
/// department]; every projection and the recipient eligibility predicate is
/// fixed owner SQL, byte-identical to the retired TypeScript statement.
pub const RD_FOLLOWUP_RECIPIENTS: &str = "sales_rd_followup_recipients";
const SQL_FOLLOWUP_RECIPIENTS: &str =
    "SELECT DISTINCT c.owner_id id,p.legal_name name FROM (SELECT DISTINCT user_id owner_id FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) assignments WHERE company_id=?) c JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=c.owner_id AND p.company_id=? JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=? WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=o.company_id AND a.user_id=c.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ORDER BY p.legal_name";
const READ_FOLLOWUP_RECIPIENTS: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_RECIPIENTS,
    sql: SQL_FOLLOWUP_RECIPIENTS,
    bind_count: 8,
    scope: "holding",
};

const READS: &[NativeSalesReadStatement] = &[
    READ_PRICE_LIST_ACTIVE,
    READ_PRICE_LIST_ITEMS_ACTIVE,
    READ_NOTIFICATION_ATTEMPT,
    READ_NOTIFICATION_RECONCILIATION_INSTALLED,
    READ_FOLLOWUP_RECIPIENTS,
];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeSalesReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followup_recipients_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_RECIPIENTS).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_recipients");
        assert_eq!(read.bind_count, 8);
        assert_eq!(read.sql.matches('?').count(), 8);
        assert_eq!(read.sql.len(), 2774);
        assert!(read.sql.starts_with("SELECT DISTINCT c.owner_id id,p.legal_name name"));
        assert!(read.sql.ends_with("ORDER BY p.legal_name"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_RECIPIENTS));
    }

    #[test]
    fn price_list_read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_PRICE_LIST_ACTIVE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICE_LIST_ACTIVE));
        assert!(!is_read("sales_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn price_list_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_PRICE_LIST_ACTIVE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name"
        );
    }

    #[test]
    fn price_list_items_read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_PRICE_LIST_ITEMS_ACTIVE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICE_LIST_ITEMS_ACTIVE));
    }

    #[test]
    fn price_list_items_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_PRICE_LIST_ITEMS_ACTIVE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal"
        );
    }

    #[test]
    fn notification_attempt_read_plan_is_holding_scoped_with_three_binds() {
        let read = read_statement(RD_NOTIFICATION_ATTEMPT).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.sql.matches('?').count(), 3);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_ATTEMPT));
    }

    #[test]
    fn notification_attempt_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_NOTIFICATION_ATTEMPT).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'"
        );
        assert_eq!(read.sql.len(), 456);
    }

    #[test]
    fn reconciliation_installed_read_plan_is_holding_scoped_with_no_binds() {
        let read = read_statement(RD_NOTIFICATION_RECONCILIATION_INSTALLED).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_RECONCILIATION_INSTALLED));
    }

    #[test]
    fn reconciliation_installed_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_NOTIFICATION_RECONCILIATION_INSTALLED).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed"
        );
        assert_eq!(read.sql.len(), 78);
    }

    #[test]
    fn only_owner_read_kinds_are_registered() {
        assert_eq!(READS.len(), 5);
        assert!(READS.iter().all(|read| is_read(read.kind)));
        assert!(!is_read("sales_price_lists"));
        assert!(!is_read("SELECT 1"));
    }
}
