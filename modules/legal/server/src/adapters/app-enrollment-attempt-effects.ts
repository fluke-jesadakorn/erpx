import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { legalWave3Statement } from "./native-legal-wave3";

/** Rate-limit a Legal Portal enrollment attempt through the native owner write
 * route. The owner Rust crate owns the INSERT/upsert SQL; this client carries a
 * typed kind and bounded values only and fails closed (no TS fallback). */
export function appLegalEnrollmentAttempt(db: SqlDatabase, shareId: string, at: number, previousWindow: number, previousAttempts: number) {
  return legalWave3Statement(
    db,
    { kind: 'system' },
    'legal_enrollment_attempt',
    [shareId, at, previousWindow, previousAttempts],
    `legal-enrollment-attempt:${shareId}:${previousWindow}`,
    at,
  ).then(statement => statement.first<{ attempts: number }>());
}
