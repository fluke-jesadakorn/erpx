import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "finance_procurement_bill_matches_from": "FROM (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"order_id\",\"fulfillment_id\",\"supplier_id\",\"currency\",\"chart_id\",\"clearing_account_id\",\"payable_account_id\",\"tax_account_id\",\"reference\",\"invoice_date\",\"due_date\",\"evidence\",\"expected_subtotal_minor\",\"subtotal_minor\",\"tax_minor\",\"quantity_matches\",\"variance_account_id\",\"reviewed_by\",\"review_reason\",\"invoice_id\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM procurement_bill_matches) procurement_bill_matches",
  "supplier_bill_draft_void_procurement_bill_match_receipts_br_from": "FROM (SELECT \"id\",\"match_id\",\"fulfillment_id\",\"captured_version\",\"active\" FROM procurement_bill_match_receipts) br"
} as const;}
