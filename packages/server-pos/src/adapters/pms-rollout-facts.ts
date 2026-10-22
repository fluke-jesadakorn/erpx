import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsRolloutPosFacts(db: SqlDatabase) {
    return {
        pos: (companyId: string) => db.prepare(`SELECT 1 FROM pos_settings s WHERE s.company_id=? AND s.enabled=true
      AND EXISTS(SELECT 1 FROM pos_registers r WHERE r.company_id=s.company_id AND r.active=true)
      AND EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) p WHERE p.company_id=s.company_id AND p.status='active')`).bind(companyId).first()
    };
}
