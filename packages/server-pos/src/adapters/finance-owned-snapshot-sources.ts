import "server-only";
/** Closed native projections with explicit owned columns for named Finance reads.
 * The consumer retains its supplied transaction and one-statement snapshot;
 * this contract accepts no table name, predicate, SQL, or query from a caller. */
export function financeOwnedSnapshotSources(){return {
  "hospitality_tax_documents_pos_sales_s_join": "JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"register_id\",\"shift_id\",\"number\",\"currency\",\"business_date\",\"party_id\",\"table_id\",\"guests\",\"service_mode\",\"state\",\"subtotal_minor\",\"tax_minor\",\"discount_minor\",\"total_minor\",\"invoice_id\",\"created_by\",\"created_at\",\"updated_at\",\"version\",\"discount_approved_by\",\"discount_reason\",\"accounts_json\",\"discount_requested_by\",\"issuer_json\" FROM pos_sales) s"
} as const;}
