import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function pmsRolloutCommerceFacts(db: SqlDatabase) {
    return {
        inventory: (companyId: string) => db.prepare("SELECT 1 FROM commerce_warehouses WHERE company_id=? AND status='active' LIMIT 1").bind(companyId).first(),
        commerce: (companyId: string) => db.prepare('SELECT 1 FROM commerce_settings WHERE company_id=?').bind(companyId).first()
    };
}
