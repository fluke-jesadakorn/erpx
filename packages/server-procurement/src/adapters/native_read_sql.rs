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

/// Native successor of the three identical TypeScript request-allocation
/// literals (`records-employee-requests-owned-facts` act/handoff paths and
/// `purchase-request-progress`). The owner plans this literal; the caller
/// executes it on its open unit with the same `[requestId]` bind and reads all
/// allocation rows.
pub const RD_REQUEST_ALLOCATIONS: &str = "procurement_rd_request_allocations";
const SQL_REQUEST_ALLOCATIONS: &str =
    "SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal";
const READ_REQUEST_ALLOCATIONS: NativeReadStatement = NativeReadStatement {
    kind: RD_REQUEST_ALLOCATIONS,
    sql: SQL_REQUEST_ALLOCATIONS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the two TypeScript request-order link literals in
/// `records-employee-requests-owned-facts` (linked-order check and handoff
/// existing-link check). The owner plans this literal; the caller executes it
/// on its open unit with the same `[requestId, orderId]` binds.
pub const RD_REQUEST_ORDER_LINK: &str = "procurement_rd_request_order_link";
const SQL_REQUEST_ORDER_LINK: &str =
    "SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?";
const READ_REQUEST_ORDER_LINK: NativeReadStatement = NativeReadStatement {
    kind: RD_REQUEST_ORDER_LINK,
    sql: SQL_REQUEST_ORDER_LINK,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `supplier-bills` price-variance read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[matchId]` bind. It reads back matched-bill lines written by the
/// mounted command-plan statements.
pub const RD_BILL_MATCH_VARIANCE: &str = "procurement_rd_bill_match_variance";
const SQL_BILL_MATCH_VARIANCE: &str =
    "SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1";
const READ_BILL_MATCH_VARIANCE: NativeReadStatement = NativeReadStatement {
    kind: RD_BILL_MATCH_VARIANCE,
    sql: SQL_BILL_MATCH_VARIANCE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `supplier-bills` `receiptHistory` read.
/// The owner plans this literal; the caller executes it on its open unit with
/// the same `[fulfillmentId]` bind.
pub const RD_BILL_MATCH_RECEIPT_HISTORY: &str = "procurement_rd_bill_match_receipt_history";
const SQL_BILL_MATCH_RECEIPT_HISTORY: &str =
    "SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1";
const READ_BILL_MATCH_RECEIPT_HISTORY: NativeReadStatement = NativeReadStatement {
    kind: RD_BILL_MATCH_RECEIPT_HISTORY,
    sql: SQL_BILL_MATCH_RECEIPT_HISTORY,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `supplier-bills` `matchingRequired`
/// policy-existence read. The owner plans this literal; the caller executes it
/// on its open unit with the same `[companyId]` bind.
pub const RD_INVOICE_POLICY_MATCHING_REQUIRED: &str =
    "procurement_rd_invoice_policy_matching_required";
const SQL_INVOICE_POLICY_MATCHING_REQUIRED: &str =
    "SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'";
const READ_INVOICE_POLICY_MATCHING_REQUIRED: NativeReadStatement = NativeReadStatement {
    kind: RD_INVOICE_POLICY_MATCHING_REQUIRED,
    sql: SQL_INVOICE_POLICY_MATCHING_REQUIRED,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `record-purchases`
/// `requireProcurementWorkspaceRecordRead` supplier-bill existence literal. The
/// owner plans this exact literal; the caller executes it on its open unit with
/// the same `[id]` bind. The scoped workspace authorization query stays intact
/// in the owner transport.
pub const RD_RECORD_BILL_MATCH_EXISTS: &str = "procurement_rd_record_bill_match_exists";
const SQL_RECORD_BILL_MATCH_EXISTS: &str = "SELECT 1 FROM procurement_bill_matches WHERE id=?";
const READ_RECORD_BILL_MATCH_EXISTS: NativeReadStatement = NativeReadStatement {
    kind: RD_RECORD_BILL_MATCH_EXISTS,
    sql: SQL_RECORD_BILL_MATCH_EXISTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `record-purchases`
/// `requireProcurementWorkspaceRecordRead` RFQ existence literal. Same exact
/// literal, `[id]` bind and scoped-authorization boundary as the bill-match
/// kind above.
pub const RD_RECORD_RFQ_EXISTS: &str = "procurement_rd_record_rfq_exists";
const SQL_RECORD_RFQ_EXISTS: &str = "SELECT 1 FROM procurement_rfqs WHERE id=?";
const READ_RECORD_RFQ_EXISTS: NativeReadStatement = NativeReadStatement {
    kind: RD_RECORD_RFQ_EXISTS,
    sql: SQL_RECORD_RFQ_EXISTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `supplier-bills` `detail` bill-match
/// events read. The owner plans this literal; the caller executes it on its
/// open unit with the same `[matchId]` bind. It reads back event rows written
/// by the mounted command-plan statement `procurement_stmt_1`.
pub const RD_BILL_MATCH_EVENTS: &str = "procurement_rd_bill_match_events";
const SQL_BILL_MATCH_EVENTS: &str =
    "SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC";
const READ_BILL_MATCH_EVENTS: NativeReadStatement = NativeReadStatement {
    kind: RD_BILL_MATCH_EVENTS,
    sql: SQL_BILL_MATCH_EVENTS,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `commerce-facts`
/// `readProcurementBillMatchLines1` read. The owner plans this literal; the
/// caller executes it on its open unit with the same `[invoiceId, lineId]`
/// binds.
pub const RD_BILL_MATCH_LINES_FOR_INVOICE: &str = "procurement_rd_bill_match_lines_for_invoice";
const SQL_BILL_MATCH_LINES_FOR_INVOICE: &str =
    "SELECT l.subtotal_minor,l.tax_minor FROM procurement_bill_match_lines l JOIN procurement_bill_matches m ON m.id=l.match_id WHERE m.invoice_id=? AND m.status='posted' AND l.fulfillment_line_id=?";
const READ_BILL_MATCH_LINES_FOR_INVOICE: NativeReadStatement = NativeReadStatement {
    kind: RD_BILL_MATCH_LINES_FOR_INVOICE,
    sql: SQL_BILL_MATCH_LINES_FOR_INVOICE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `commerce-facts`
/// `readProcurementBillMatches1` read. The owner plans this literal; the caller
/// executes it on its open unit with the same `[invoiceId, companyId]` binds.
pub const RD_POSTED_BILL_MATCH_FOR_INVOICE: &str = "procurement_rd_posted_bill_match_for_invoice";
const SQL_POSTED_BILL_MATCH_FOR_INVOICE: &str =
    "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='posted'";
const READ_POSTED_BILL_MATCH_FOR_INVOICE: NativeReadStatement = NativeReadStatement {
    kind: RD_POSTED_BILL_MATCH_FOR_INVOICE,
    sql: SQL_POSTED_BILL_MATCH_FOR_INVOICE,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `records-employee-requests-owned-facts`
/// `readActOnEmployeeRequestQuotation` literal. The owner plans this exact
/// statement; the caller executes it on its open unit with the same
/// `[requestId, purchaseOrderId]` binds.
pub const RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION: &str =
    "procurement_rd_employee_request_ordered_quotation";
const SQL_EMPLOYEE_REQUEST_ORDERED_QUOTATION: &str =
    "SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'";
const READ_EMPLOYEE_REQUEST_ORDERED_QUOTATION: NativeReadStatement = NativeReadStatement {
    kind: RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION,
    sql: SQL_EMPLOYEE_REQUEST_ORDERED_QUOTATION,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript `records-employee-requests-owned-facts`
/// `readPrepareRequestOrderHandoffQuotation` literal. Same owner literal and
/// `[requestId, rfqId]` binds.
pub const RD_HANDOFF_APPROVED_QUOTATION: &str = "procurement_rd_handoff_approved_quotation";
const SQL_HANDOFF_APPROVED_QUOTATION: &str =
    "SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'";
const READ_HANDOFF_APPROVED_QUOTATION: NativeReadStatement = NativeReadStatement {
    kind: RD_HANDOFF_APPROVED_QUOTATION,
    sql: SQL_HANDOFF_APPROVED_QUOTATION,
    bind_count: 2,
    scope: "holding",
};

const READS: &[NativeReadStatement] = &[
    READ_INVOICE_MATCH_FOR_FINANCE,
    READ_REQUEST_QUOTATION,
    READ_OPERATIONAL_ATTACHMENT_BILL_MATCH,
    READ_DOCUMENT_POSTING_MATCHED_BILL,
    READ_BILL_MATCH_RECEIPTS,
    READ_ACTIVE_QUOTATION_COUNT,
    READ_REQUEST_ALLOCATIONS,
    READ_REQUEST_ORDER_LINK,
    READ_BILL_MATCH_VARIANCE,
    READ_BILL_MATCH_RECEIPT_HISTORY,
    READ_INVOICE_POLICY_MATCHING_REQUIRED,
    READ_RECORD_BILL_MATCH_EXISTS,
    READ_RECORD_RFQ_EXISTS,
    READ_BILL_MATCH_EVENTS,
    READ_BILL_MATCH_LINES_FOR_INVOICE,
    READ_POSTED_BILL_MATCH_FOR_INVOICE,
    READ_EMPLOYEE_REQUEST_ORDERED_QUOTATION,
    READ_HANDOFF_APPROVED_QUOTATION,
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
        assert!(is_read(RD_REQUEST_ALLOCATIONS));
        assert!(is_read(RD_REQUEST_ORDER_LINK));
        assert!(is_read(RD_BILL_MATCH_VARIANCE));
        assert!(is_read(RD_BILL_MATCH_RECEIPT_HISTORY));
        assert!(is_read(RD_INVOICE_POLICY_MATCHING_REQUIRED));
        assert!(is_read(RD_RECORD_BILL_MATCH_EXISTS));
        assert!(is_read(RD_RECORD_RFQ_EXISTS));
        assert!(is_read(RD_BILL_MATCH_EVENTS));
        assert!(is_read(RD_BILL_MATCH_LINES_FOR_INVOICE));
        assert!(is_read(RD_POSTED_BILL_MATCH_FOR_INVOICE));
        assert!(is_read(RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION));
        assert!(is_read(RD_HANDOFF_APPROVED_QUOTATION));
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
            (
                RD_REQUEST_ALLOCATIONS,
                1usize,
                "procurement_rfq_lines",
            ),
            (
                RD_REQUEST_ORDER_LINK,
                2usize,
                "procurement_request_orders",
            ),
            (
                RD_BILL_MATCH_VARIANCE,
                1usize,
                "procurement_bill_match_lines",
            ),
            (
                RD_BILL_MATCH_RECEIPT_HISTORY,
                1usize,
                "procurement_bill_match_receipts",
            ),
            (
                RD_INVOICE_POLICY_MATCHING_REQUIRED,
                1usize,
                "procurement_invoice_policies",
            ),
            (
                RD_RECORD_BILL_MATCH_EXISTS,
                1usize,
                "procurement_bill_matches",
            ),
            (RD_RECORD_RFQ_EXISTS, 1usize, "procurement_rfqs"),
            (
                RD_BILL_MATCH_EVENTS,
                1usize,
                "procurement_bill_match_events",
            ),
            (
                RD_BILL_MATCH_LINES_FOR_INVOICE,
                2usize,
                "procurement_bill_match_lines",
            ),
            (
                RD_POSTED_BILL_MATCH_FOR_INVOICE,
                2usize,
                "procurement_bill_matches",
            ),
            (
                RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION,
                2usize,
                "procurement_rfqs",
            ),
            (RD_HANDOFF_APPROVED_QUOTATION, 2usize, "procurement_rfqs"),
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
    fn request_allocations_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_REQUEST_ALLOCATIONS).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_REQUEST_ALLOCATIONS));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn request_order_link_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_REQUEST_ORDER_LINK).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_REQUEST_ORDER_LINK));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn bill_match_variance_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_BILL_MATCH_VARIANCE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BILL_MATCH_VARIANCE));
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

    #[test]
    fn bill_match_receipt_history_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_BILL_MATCH_RECEIPT_HISTORY).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BILL_MATCH_RECEIPT_HISTORY));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn invoice_policy_matching_required_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_INVOICE_POLICY_MATCHING_REQUIRED).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_INVOICE_POLICY_MATCHING_REQUIRED));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn record_bill_match_exists_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_RECORD_BILL_MATCH_EXISTS).expect("read kind");
        assert_eq!(read.sql, "SELECT 1 FROM procurement_bill_matches WHERE id=?");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_RECORD_BILL_MATCH_EXISTS));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn record_rfq_exists_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_RECORD_RFQ_EXISTS).expect("read kind");
        assert_eq!(read.sql, "SELECT 1 FROM procurement_rfqs WHERE id=?");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_RECORD_RFQ_EXISTS));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn bill_match_events_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_BILL_MATCH_EVENTS).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC"
        );
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BILL_MATCH_EVENTS));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn bill_match_lines_for_invoice_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_BILL_MATCH_LINES_FOR_INVOICE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT l.subtotal_minor,l.tax_minor FROM procurement_bill_match_lines l JOIN procurement_bill_matches m ON m.id=l.match_id WHERE m.invoice_id=? AND m.status='posted' AND l.fulfillment_line_id=?"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_BILL_MATCH_LINES_FOR_INVOICE));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn posted_bill_match_for_invoice_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_POSTED_BILL_MATCH_FOR_INVOICE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='posted'"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_POSTED_BILL_MATCH_FOR_INVOICE));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn employee_request_ordered_quotation_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_EMPLOYEE_REQUEST_ORDERED_QUOTATION));
        assert!(!is_read("procurement_stmt_1"));
    }

    #[test]
    fn handoff_approved_quotation_plan_matches_retired_typescript_literal() {
        let read = read_statement(RD_HANDOFF_APPROVED_QUOTATION).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'"
        );
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_HANDOFF_APPROVED_QUOTATION));
        assert!(!is_read("procurement_stmt_1"));
    }
}
