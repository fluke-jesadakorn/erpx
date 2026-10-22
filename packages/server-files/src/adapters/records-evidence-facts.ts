import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function recordEvidenceFacts(database: SqlDatabase) {
    return { readPrepareBusinessEvidenceRows(companyId: string, ids: string[]) {
            return database.prepare(`SELECT target_type FROM attachment_sets WHERE company_id=? AND id IN (${ids.map(() => '?').join(',')})`).bind(companyId, ...ids).all<{
                target_type: string;
            }>();
        } };
}
