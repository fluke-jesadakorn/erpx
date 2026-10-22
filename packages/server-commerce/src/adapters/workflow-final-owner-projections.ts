import 'server-only';
/** Fixed explicit-column owning projections; retains the consumer's supplied statement snapshot. */
export function workflowFinalOwnerProjections(){return {
  "flow_routing_commerce_fulfillments_f_from": "FROM (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f"
} as const;}
