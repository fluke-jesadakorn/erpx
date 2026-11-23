import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller.
 *
 * Customer 360 now consumes the fenced native sources through the final-root
 * witness in composition; the agreement-event, order-event and order fragments
 * were retired from this TS authority. The remaining two statements are kept
 * for their native projection parity pins and the shared-projection follow-up. */
export const finalRootOwnerProjectionSources = {
  field_service_agreements: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"customer_id\",\"asset_id\",\"title\",\"start_date\",\"end_date\",\"currency\",\"visit_rate_minor\",\"status\",\"created_by\",\"reviewed_by\",\"reviewed_at\",\"version\",\"created_at\",\"updated_at\",\"warranty_discount_bps\" FROM field_service_agreements",
  field_service_billing_releases: "SELECT \"id\",\"order_id\",\"agreement_id\",\"customer_id\",\"currency\",\"amount_minor\",\"source_order_version\",\"status\",\"released_by\",\"reason\",\"created_at\",\"warranty_applied\",\"warranty_discount_bps\",\"warranty_end_snapshot\",\"pricing_date\" FROM field_service_billing_releases",
} as const;
