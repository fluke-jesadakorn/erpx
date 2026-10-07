import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export async function runtimeBetaSlotActive(db: SqlDatabase, holdingId: string): Promise<boolean> {
  return Boolean(await db.prepare("SELECT 1 FROM beta_slots WHERE holding_id=? AND state='active'").bind(holdingId).first());
}
