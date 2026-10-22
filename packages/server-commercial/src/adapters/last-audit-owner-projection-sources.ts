import "server-only";
/** Commercial-owned Control usage facts stay in the caller's original rating snapshot and transaction. */
export const lastAuditOwnerProjectionSources = {
  billing_usage_snapshots: "SELECT \"holding_id\",\"month\",\"snapshot_id\",\"refreshed_at\",\"source_count\",\"ai_coverage\",\"storage_coverage\",\"unreconciled_originals\",\"seat_coverage\",\"seat_source_count\",\"active_seat_count\",\"active_seat_observed_at\" FROM billing_usage_snapshots",
  billing_usage_meters: "SELECT \"holding_id\",\"month\",\"source_key\",\"company_id\",\"company_name\",\"branch_id\",\"branch_name\",\"meter_kind\",\"funding_source\",\"pricing_status\",\"quantity\",\"input_tokens\",\"output_tokens\",\"average_daily_bytes\",\"active_at_period_end\",\"snapshot_id\",\"request_status\",\"gateway_status\",\"provider\",\"model\",\"units_json\",\"cost_status\",\"verified_cost_satang\",\"cost_evidence_key\",\"cost_reconciled_at\",\"active_from\",\"active_until\",\"byte_milliseconds\" FROM billing_usage_meters",
  billing_seat_usage: "SELECT \"holding_id\",\"month\",\"user_key\",\"intervals_json\",\"snapshot_id\" FROM billing_seat_usage",
} as const;
