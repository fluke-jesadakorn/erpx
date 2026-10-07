import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";

/** Serialize Company request reservations across all AI entry points. */
export function aiRequestQuotaLock(db:SqlDatabase,companyId:string) {
  return db.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(`ai-request-budget:${companyId}`);
}
