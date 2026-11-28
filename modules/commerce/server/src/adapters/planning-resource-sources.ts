import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
export function planningResourceSources(db: SqlDatabase) {
    return { lockSourceOrder(id: string) { return db.prepare("SELECT id FROM commerce_orders WHERE id=? FOR SHARE").bind(id); },
        guardSourceOrderVersion(key: string, value: string, id: string, companyId: string, version: number) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_orders WHERE id=? AND company_id=? AND version=? AND state IN ('confirmed','partially_fulfilled'))", [id, companyId, version]); } };
}
