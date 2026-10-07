import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function attachmentServiceManageErpxFacts(db: SqlDatabase) { return { readBetaPolicy20: (facts: {
        companyId: string;
    }) => { return db.prepare("SELECT storage_bytes FROM beta_policy WHERE company_id=?").bind(facts.companyId).first<{
        storage_bytes: number;
    }>(); } }; }
