import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function pmsRolloutFinanceFacts(db: SqlDatabase) {
    return {
        finance: (companyId: string) => db.prepare(`SELECT 1 FROM finance_charts c WHERE c.company_id=? AND c.base_currency='THB'
      AND EXISTS(SELECT 1 FROM finance_periods p WHERE p.company_id=c.company_id AND p.state='open') LIMIT 1`).bind(companyId).first()
    };
}
