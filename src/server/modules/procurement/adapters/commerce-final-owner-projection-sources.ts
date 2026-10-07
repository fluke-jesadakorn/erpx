import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commerceFinalOwnerProjectionSources = {
  procurement_bill_match_receipts: "SELECT \"id\",\"match_id\",\"fulfillment_id\",\"captured_version\",\"active\" FROM procurement_bill_match_receipts",
  procurement_bill_matches: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"order_id\",\"fulfillment_id\",\"supplier_id\",\"currency\",\"chart_id\",\"clearing_account_id\",\"payable_account_id\",\"tax_account_id\",\"reference\",\"invoice_date\",\"due_date\",\"evidence\",\"expected_subtotal_minor\",\"subtotal_minor\",\"tax_minor\",\"quantity_matches\",\"variance_account_id\",\"reviewed_by\",\"review_reason\",\"invoice_id\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM procurement_bill_matches",
  procurement_request_orders: "SELECT \"request_id\",\"order_id\",\"rfq_id\",\"request_version\",\"request_evidence_version\",\"linked_by\",\"created_at\" FROM procurement_request_orders",
} as const;
