import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function EmployeeRequestsFilesOwnedFacts(database: SqlDatabase) {
    return { readRequestAttachmentsEffect(id: string) {
            return database.prepare("SELECT id,original_name,media_type,bytes,checksum_sha256,purpose,revision FROM attachments WHERE owner_type='employee_request' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at,id")
                .bind(id)
                .all<{
                id: string;
                original_name: string;
                media_type: string;
                bytes: number;
                checksum_sha256: string;
                purpose: string;
                revision: number;
            }>();
        } };
}
