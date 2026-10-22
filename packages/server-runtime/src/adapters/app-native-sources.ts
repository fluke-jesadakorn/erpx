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
export function platformHoldingBindingEffects(db: SqlDatabase) { return {
  activate: (holdingId: string, slug: string, displayName: string, writeBinding: string, readBinding: string, filesBinding: string) => db.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?, 'active') ON CONFLICT(holding_id) DO UPDATE SET slug=excluded.slug,display_name=excluded.display_name,write_binding=excluded.write_binding,read_binding=excluded.read_binding,files_binding=excluded.files_binding,status='active'").bind(holdingId, slug, displayName, writeBinding, readBinding, filesBinding),
}; }
