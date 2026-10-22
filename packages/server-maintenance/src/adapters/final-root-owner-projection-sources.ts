import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  maintenance_work_orders: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"asset_id\",\"title\",\"priority\",\"status\",\"owner_id\",\"scheduled_at\",\"downtime_started_at\",\"downtime_ended_at\",\"blocked_reason\",\"completion_evidence\",\"resulting_condition\",\"version\",\"created_by\",\"created_at\",\"updated_at\" FROM maintenance_work_orders",
} as const;
