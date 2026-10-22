import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
export function RecordsIdentityAccessFacts(database: SqlDatabase) {
    return { readSearchReferencesAssignment(user_id: string, now: number, now2: number) { return database.prepare(`SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1`).bind(user_id, now, now2).first(); },
        readAddPrincipalMentionEligible(targetId: string, now: number, now2: number) { return database.prepare(`SELECT 1 AS ok FROM assignments WHERE user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? LIMIT 1`).bind(targetId, now, now2).first(); } };
}
