import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { nativeFilesReadAll } from "./native-files-command";

/**
 * Workflow expense-decision evidence attachment fact. The read is owned by
 * the native Files read kind `RD_WORKFLOW_APPROVAL_EVIDENCE`; this module emits
 * no SQL and fails closed without the native transport. The caller's `.all()`
 * results shape is preserved.
 */
export function createApprovalsSourceRepository(database: SqlDatabase) {
    return {
        readExpenseDecisionEvidenceAssignmentsreceiptsperson2(record_id: string) {
            return nativeFilesReadAll<{
                id: string;
                revision: number;
                checksum_sha256: string;
                original_name: string;
            }>('RD_WORKFLOW_APPROVAL_EVIDENCE', [record_id], database);
        },
    };
}
