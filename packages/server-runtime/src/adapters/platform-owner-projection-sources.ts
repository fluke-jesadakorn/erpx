import "server-only";
import { executiveGenerationJobsSql, holdingBindingsSql } from './read-tail-sql';
/** Closed owner projections for Platform review and operational snapshots in the caller transaction. */
export const platformOwnerProjectionSources = Object.freeze({
  runtime_health: "SELECT \"service\",\"heartbeat_at\",\"details_json\" FROM runtime_health",
  executive_generation_jobs: executiveGenerationJobsSql('"company_id","created_at"'),
  holding_bindings: holdingBindingsSql('"holding_id","slug","display_name","write_binding","read_binding","files_binding","status"'),
} as const);
