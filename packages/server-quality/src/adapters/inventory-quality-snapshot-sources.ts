import 'server-only';
/** Closed explicit-column owner projections for the supplied IAM statement snapshot.
 * No caller-supplied table, SQL, predicate or alias is accepted. */
export function inventoryQualitySnapshotSources(){return {
  "inventory_counts_inventory_quality_holds_from": "FROM (SELECT \"id\", \"company_id\", \"branch_id\", \"warehouse_id\", \"product_id\", \"reason\", \"due_date\", \"status\", \"created_by\", \"last_inspector\", \"last_result\", \"released_by\", \"release_reason\", \"released_at\", \"version\", \"created_at\", \"updated_at\", \"cancel_requested_by\", \"cancel_reason\", \"cancelled_by\", \"cancelled_at\", \"lot_id\", \"held_quantity_milli\", \"source_type\", \"source_id\", \"disposition_kind\", \"disposition_reason\", \"disposition_requested_by\", \"disposition_count_id\", \"disposed_by\", \"disposed_at\", \"spec_id\", \"spec_version\", \"quantity_scope\", \"disposition_quantity_milli\" FROM inventory_quality_holds) inventory_quality_holds"
} as const;}
