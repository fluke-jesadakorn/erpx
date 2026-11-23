import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativeFilesReadAll } from './native-files-command';

export type RefundProofSource = {
    id: string;
    purpose_key: string;
    component_kind: string;
    component_ref: string | null;
};

/**
 * Finance refund-proof attachment-set facts. The read is owned by the native
 * Files read kind `RD_FINANCE_REFUND_PROOF_SETS`; this module emits no SQL and
 * fails closed without the native transport. Set IDs are passed as one
 * JSON-array bind so the owner statement keeps static arity, and the caller's
 * `.results` shape is preserved.
 */
export function financeRefundProofFacts(database: SqlDatabase) {
    return {
        readRefundProofSets(companyId: string, attachmentSetIds: string[]) {
            return nativeFilesReadAll<RefundProofSource>(
                'RD_FINANCE_REFUND_PROOF_SETS',
                [companyId, JSON.stringify(attachmentSetIds)],
                database,
                companyId,
            );
        },
    };
}
