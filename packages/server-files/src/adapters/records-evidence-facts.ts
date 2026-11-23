import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeFilesReadAll } from "./native-files-command";

/**
 * Records business-evidence attachment-set facts. The read is owned by the
 * native Files read kind `RD_RECORDS_EVIDENCE_SETS`; this module emits no SQL
 * and fails closed without the native transport. Set ids are passed as one
 * JSON-array bind so the owner statement keeps static arity, and the caller's
 * `.results` shape is preserved.
 */
export function recordEvidenceFacts(database: SqlDatabase) {
    return {
        readPrepareBusinessEvidenceRows(companyId: string, ids: string[]) {
            return nativeFilesReadAll<{ target_type: string }>(
                'RD_RECORDS_EVIDENCE_SETS',
                [companyId, JSON.stringify(ids)],
                database,
                companyId,
            );
        },
    };
}
