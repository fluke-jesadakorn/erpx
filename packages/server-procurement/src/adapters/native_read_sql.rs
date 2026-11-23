//! Owner-owned caller-unit read plans for Procurement. Generated from the
//! retired TypeScript owner-local SELECT literals with the exact positional
//! payloads; the axum `command-plan` route resolves the kind here and returns
//! the statement for the caller to prepare and run on its own already-open
//! unit.
//!
//! Reads carry no receipt/audit/outbox ledger: the caller executes them on the
//! same unit as the surrounding command, so they observe uncommitted effects.
//! No SQL is ever accepted from the wire and no TypeScript SQL remains for
//! these kinds.

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

/// Native successor of the TypeScript `finance-entrypoint-queries` invoice
/// match read. The owner plans this literal; the caller executes it on its open
/// unit with the same `[invoiceId, companyId]` binds.
pub const RD_INVOICE_MATCH_FOR_FINANCE: &str = "procurement_rd_invoice_match_finance";
const SQL_INVOICE_MATCH_FOR_FINANCE: &str =
    "SELECT id,reference FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?";
const READ_INVOICE_MATCH_FOR_FINANCE: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_MATCH_FOR_FINANCE,
    sql: SQL_INVOICE_MATCH_FOR_FINANCE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `people-app-entrypoint-facts` quotation
/// read. Same owner literal and `[requestId, companyId]` binds.
pub const RD_REQUEST_QUOTATION: &str = "procurement_rd_request_quotation";
const SQL_REQUEST_QUOTATION: &str =
    "SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'";
const READ_REQUEST_QUOTATION: NativeReadStatement = NativeReadStatement {
    kind: RD_REQUEST_QUOTATION,
    sql: SQL_REQUEST_QUOTATION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `attachments-operational-facts` bill
/// match read. Same owner literal and `[targetId, companyId]` binds.
pub const RD_OPERATIONAL_ATTACHMENT_BILL_MATCH: &str =
    "procurement_rd_operational_attachment_bill_match";
const SQL_OPERATIONAL_ATTACHMENT_BILL_MATCH: &str =
    "SELECT company_id,branch_id,department_id,version FROM procurement_bill_matches WHERE id=? AND company_id=?";
const READ_OPERATIONAL_ATTACHMENT_BILL_MATCH: NativeReadStatement = NativeReadStatement {
    kind: RD_OPERATIONAL_ATTACHMENT_BILL_MATCH,
    sql: SQL_OPERATIONAL_ATTACHMENT_BILL_MATCH,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `document-posting-preview-facts`
/// matched-invoice read. Same owner literal and `[invoiceId, companyId]` binds.
pub const RD_DOCUMENT_POSTING_MATCHED_BILL: &str = "procurement_rd_document_posting_matched_bill";
const SQL_DOCUMENT_POSTING_MATCHED_BILL: &str =
    "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='invoiced'";
const READ_DOCUMENT_POSTING_MATCHED_BILL: NativeReadStatement = NativeReadStatement {
    kind: RD_DOCUMENT_POSTING_MATCHED_BILL,
    sql: SQL_DOCUMENT_POSTING_MATCHED_BILL,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `supplier-bills` `billReceipts` read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[matchId]` bind. It reads back receipts written by the mounted
/// command-plan statements `procurement_stmt_5`/`procurement_stmt_9`.
pub const RD_BILL_MATCH_RECEIPTS: &str = "procurement_rd_bill_match_receipts";
const SQL_BILL_MATCH_RECEIPTS: &str =
    "SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id";
const READ_BILL_MATCH_RECEIPTS: NativeReadStatement = NativeReadStatement {
    kind: RD_BILL_MATCH_RECEIPTS,
    sql: SQL_BILL_MATCH_RECEIPTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the two identical TypeScript
/// `records-employee-requests-owned-facts` active-quotation-count literals.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[requestId]` bind.
pub const RD_ACTIVE_QUOTATION_COUNT: &str = "procurement_rd_active_quotation_count";
const SQL_ACTIVE_QUOTATION_COUNT: &str =
    "SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'";
const READ_ACTIVE_QUOTATION_COUNT: NativeReadStatement = NativeReadStatement {
    kind: RD_ACTIVE_QUOTATION_COUNT,
    sql: SQL_ACTIVE_QUOTATION_COUNT,
    bind_count: 1,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[
    READ_INVOICE_MATCH_FOR_FINANCE,
    READ_REQUEST_QUOTATION,
    READ_OPERATIONAL_ATTACHMENT_BILL_MATCH,
    READ_DOCUMENT_POSTING_MATCHED_BILL,
    READ_BILL_MATCH_RECEIPTS,
    READ_ACTIVE_QUOTATION_COUNT,
];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    READS.iter().any(|read| read.kind == kind)
}

/// Resolve an allow-listed owner read by kind.
pub fn read_statement(kind: &str) -> Option<&'static NativeReadStatement> {
    READS.iter().find(|read| read.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_kinds_are_distinct_from_write_kinds() {
        assert!(is_read(RD_INVOICE_MATCH_FOR_FINANCE));
        assert!(is_read(RD_REQUEST_QUOTATION));
        assert!(is_read(RD_OPERATIONAL_ATTACHMENT_BILL_MATCH));
        assert!(is_read(RD_DOCUMENT_POSTING_MATCHED_BILL));
        assert!(is_read(RD_BILL_MATCH_RECEIPTS));
        assert!(is_read(RD_ACTIVE_QUOTATION_COUNT));
        assert!(!is_read("procurement_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn entrypoint_read_plans_are_ledger_free_and_bounded() {
        for (kind, binds, table) in [
            (
                RD_INVOICE_MATCH_FOR_FINANCE,
                2usize,
                "procurement_bill_matches",
            ),
            (RD_REQUEST_QUOTATION, 2usize, "procurement_rfqs"),
            (
                RD_OPERATIONAL_ATTACHMENT_BILL_MATCH,
                2usize,
                "procurement_bill_matches",
            ),
            (
                RD_DOCUMENT_POSTING_MATCHED_BILL,
                2usize,
                "procurement_bill_matches",
            ),
            (
                RD_BILL_MATCH_RECEIPTS,
                1usize,
                "procurement_bill_match_receipts",
            ),
            (
                RD_ACTIVE_QUOTATION_COUNT,
                1usize,
                "procurement_rfqs",
            ),
        ] {
            let read = read_statement(kind).expect("read kind");
            let upper = read.sql.to_ascii_uppercase();
            assert!(upper.starts_with("SELECT"));
            assert!(!upper.contains("INSERT"));
            assert!(!upper.contains("UPDATE"));
            assert!(!upper.contains("DELETE"));
            assert_eq!(read.bind_count, binds);
            assert_eq!(read.sql.matches('?').count(), binds);
            assert!(read.sql.contains(table));
            assert!(is_read(kind));
        }
    }

    #[test]
    fn operational_attachment_bill_match_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_OPERATIONAL_ATTACHMENT_BILL_MATCH).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT company_id,branch_id,department_id,version FROM procurement_bill_matches WHERE id=? AND company_id=?"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_OPERATIONAL_ATTACHMENT_BILL_MATCH));
    }

    #[test]
    fn document_posting_matched_bill_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_DOCUMENT_POSTING_MATCHED_BILL).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='invoiced'"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_DOCUMENT_POSTING_MATCHED_BILL));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn active_quotation_count_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_ACTIVE_QUOTATION_COUNT).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_ACTIVE_QUOTATION_COUNT));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn bill_match_receipts_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_BILL_MATCH_RECEIPTS).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BILL_MATCH_RECEIPTS));
        assert!(!is_read("procurement_stmt_1"));
    }
}
