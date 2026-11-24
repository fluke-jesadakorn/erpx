import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export const betaPolicyStorageBytesSql = "SELECT storage_bytes FROM beta_policy WHERE company_id=?";
export function attachmentServiceManageErpxFacts(db: SqlDatabase) { return { readBetaPolicy20: (facts: {
        companyId: string;
    }) => { return db.prepare(betaPolicyStorageBytesSql).bind(facts.companyId).first<{
        storage_bytes: number;
    }>(); } }; }
