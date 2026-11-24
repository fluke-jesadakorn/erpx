import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { COMMERCE_SETTINGS_BY_COMPANY_SQL } from './settings-dedupe-sql';
export function expensePostingConfiguration(db: SqlDatabase, companyId: string) {
    return db.prepare(COMMERCE_SETTINGS_BY_COMPANY_SQL).bind(companyId).first<Record<string, string | number | null>>();
}
