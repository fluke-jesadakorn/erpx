import "server-only";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function runtimeAppSources(db: SqlDatabase) { return {
  jobsHeartbeat: () => db.prepare("SELECT heartbeat_at,details_json FROM runtime_health WHERE service='jobs'").first<{ heartbeat_at: number; details_json: string }>(),
  readyHeartbeat: () => db.prepare("SELECT heartbeat_at FROM runtime_health WHERE service='jobs'").first<{ heartbeat_at: number }>(),
}; }
export async function customerHoldingBindingExists(db: PostgresDatabase, holdingId: string): Promise<boolean> {
  return Boolean((await db.pool.query("SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND holding_id<>'h003'", [holdingId])).rowCount);
}
