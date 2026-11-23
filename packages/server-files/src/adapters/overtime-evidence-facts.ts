import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeFilesReadAll } from './native-files-command';

/**
 * Overtime command evidence attachment-set facts. The read is owned by the
 * native Files read kind `RD_OVERTIME_EVIDENCE_SETS`; this module emits no SQL
 * and fails closed without the native transport. Set IDs are passed as one
 * JSON-array bind so the owner statement keeps static arity, and the caller's
 * `.results` shape is preserved.
 */
export function overtimeEvidenceFacts(database: SqlDatabase) {
    return {
        readCommandOvertimeSets(companyId: string, attachmentSetIds: string[], _placeholders?: string) {
            return nativeFilesReadAll<{
                id: string;
                purpose_key: string;
                component_kind: string;
            }>('RD_OVERTIME_EVIDENCE_SETS', [companyId, JSON.stringify(attachmentSetIds)], database, companyId);
        },
    };
}
