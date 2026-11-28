import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
/** One owner-local fixed projection for the Records search assignment
 * existence read. The native identity assignment reader owns the same
 * authority shape. */
const activeAssignmentSql = "SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1";
export function RecordsIdentityAccessFacts(database: SqlDatabase) {
    return { readSearchReferencesAssignment(user_id: string, now: number, now2: number) { return database.prepare(activeAssignmentSql).bind(user_id, now, now2).first(); } };
}
