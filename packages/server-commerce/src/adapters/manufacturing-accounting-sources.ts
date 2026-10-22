import { productionReadProjection as productionfinanceProjection } from '@erpx/server-finance/public/production-read-projections';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function manufacturingAccountingSources(db: SqlDatabase) {
    return { guardInventoryMapping(key: string, value: "chart", companyId: string, chartId: string, inventoryAccountId: string) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=? AND chart_id=? AND inventory_account_id=?)", [companyId, chartId, inventoryAccountId]); },
        guardFunctionalCurrency(key: string, value: "production-currency", companyId: string, value2: string, value3: string) { return assertion(db, key, value, `EXISTS(SELECT 1 FROM commerce_settings s ${productionfinanceProjection.chart} WHERE s.company_id=? AND s.chart_id=? AND c.base_currency='THB' AND s.inventory_account_id=?)`, [companyId, value2, value3]); } };
}
