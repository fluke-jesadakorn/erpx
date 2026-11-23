import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
/** One owner-local fixed projection for the Records search/mention assignment
 * existence read. The two facts ports previously repeated the same SQL literal;
 * one projection now serves both call sites so they cannot drift apart. The
 * native identity assignment reader owns the same authority shape. */
const activeAssignmentSql = "SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1";
export function RecordsIdentityAccessFacts(database: SqlDatabase) {
    return { readSearchReferencesAssignment(user_id: string, now: number, now2: number) { return database.prepare(activeAssignmentSql).bind(user_id, now, now2).first(); },
        readAddPrincipalMentionEligible(targetId: string, now: number, now2: number) { return database.prepare(activeAssignmentSql).bind(targetId, now, now2).first(); } };
}
