import "server-only";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { holdingBindingsSql, runtimeHealthSql } from './read-tail-sql';
export function runtimeAppSources(db: SqlDatabase) { return {
  jobsHeartbeat: () => db.prepare(runtimeHealthSql('heartbeat_at,details_json', " WHERE service='jobs'")).first<{ heartbeat_at: number; details_json: string }>(),
  readyHeartbeat: () => db.prepare(runtimeHealthSql('heartbeat_at', " WHERE service='jobs'")).first<{ heartbeat_at: number }>(),
}; }
export async function customerHoldingBindingExists(db: PostgresDatabase, holdingId: string): Promise<boolean> {
  return Boolean((await db.pool.query(holdingBindingsSql('1', " WHERE holding_id=$1 AND holding_id<>'h003'"), [holdingId])).rowCount);
}
