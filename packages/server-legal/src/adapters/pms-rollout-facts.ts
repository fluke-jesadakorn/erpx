import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsRolloutLegalFacts(db: SqlDatabase) {
    return {
        legal: (companyId: string) => db.prepare('SELECT 1 FROM legal_settings WHERE company_id=? AND enabled').bind(companyId).first()
    };
}
