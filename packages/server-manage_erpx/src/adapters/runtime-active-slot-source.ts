import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { betaSlotPointSql } from "./beta-slot-point-sql";
export async function runtimeBetaSlotActive(db: SqlDatabase, holdingId: string): Promise<boolean> {
  return Boolean(await db.prepare(betaSlotPointSql('1', 'holding_id', " AND state='active'")).bind(holdingId).first());
}
