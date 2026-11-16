import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function expensePostingConfiguration(db: SqlDatabase, companyId: string) {
    return db.prepare("SELECT 1").bind().first<Record<string, string | number | null>>();
}
