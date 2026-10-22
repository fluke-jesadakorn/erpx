import "server-only";
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  customer_access_bindings: "SELECT \"id\",\"company_id\",\"order_id\",\"customer_id\",\"email\",\"expires_at\",\"status\",\"reason\",\"created_by\",\"approved_by\",\"reviewed_at\",\"version\",\"created_at\",\"updated_at\" FROM customer_access_bindings",
  customer_access_identities: "SELECT \"binding_id\",\"user_id\",\"email\",\"bound_at\" FROM customer_access_identities",
} as const;
