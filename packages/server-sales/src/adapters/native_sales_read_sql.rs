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

const READS: &[NativeSalesReadStatement] = &[READ_PRICE_LIST_ACTIVE, READ_PRICE_LIST_ITEMS_ACTIVE];

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
    fn only_owner_read_kinds_are_registered() {
        assert_eq!(READS.len(), 2);
        assert!(READS.iter().all(|read| is_read(read.kind)));
        assert!(!is_read("sales_price_lists"));
        assert!(!is_read("SELECT 1"));
    }
}
