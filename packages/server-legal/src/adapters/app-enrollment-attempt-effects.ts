import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function appLegalEnrollmentAttempt(db: SqlDatabase, shareId: string, at: number, previousWindow: number, previousAttempts: number) {
  return db.prepare("INSERT INTO legal_enrollment_attempts(share_id,window_start,attempts) VALUES(?,?,1) ON CONFLICT(share_id) DO UPDATE SET window_start=CASE WHEN legal_enrollment_attempts.window_start<? THEN excluded.window_start ELSE legal_enrollment_attempts.window_start END,attempts=CASE WHEN legal_enrollment_attempts.window_start<? THEN 1 ELSE legal_enrollment_attempts.attempts+1 END RETURNING attempts").bind(shareId, at, previousWindow, previousAttempts).first<{ attempts: number }>();
}
