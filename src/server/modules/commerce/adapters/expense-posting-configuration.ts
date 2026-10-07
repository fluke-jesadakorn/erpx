import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function expensePostingConfiguration(db: SqlDatabase, companyId: string) {
    return db.prepare("SELECT * FROM commerce_settings WHERE company_id=?").bind(companyId).first<Record<string, string | number | null>>();
}
