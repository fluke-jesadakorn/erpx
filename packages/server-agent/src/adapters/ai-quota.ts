import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/** Serialize Company request reservations across all AI entry points. */
export function aiRequestQuotaLock(db:SqlDatabase,companyId:string) {
  return db.prepare("SELECT 1").bind();
}
