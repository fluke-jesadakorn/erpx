import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  customer_portal_actions: "SELECT \"id\",\"company_id\",\"binding_id\",\"order_id\",\"customer_id\",\"user_id\",\"action\",\"note\",\"source_version\",\"request_key\",\"request_hash\",\"status\",\"created_at\" FROM customer_portal_actions",
} as const;
