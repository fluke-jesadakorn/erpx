import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function attachmentServiceManageErpxFacts(db: SqlDatabase) { return { readBetaPolicy20: (facts: {
        companyId: string;
    }) => { return db.prepare("SELECT 1").bind().first<{
        storage_bytes: number;
    }>(); } }; }
