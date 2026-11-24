import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {nativeCommerceReadFirst,RD_SETTINGS_BY_COMPANY} from './native-commerce-read';
export function expensePostingConfiguration(db: SqlDatabase, companyId: string) {
    return nativeCommerceReadFirst<Record<string, string | number | null>>(RD_SETTINGS_BY_COMPANY, [companyId], db);
}
