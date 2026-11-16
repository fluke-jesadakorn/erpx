import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function appLegalEnrollmentAttempt(db: SqlDatabase, shareId: string, at: number, previousWindow: number, previousAttempts: number) {
  return db.prepare("SELECT 1").bind().first<{ attempts: number }>();
}
