import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function RequestPageCommerceOwnedFacts(database: SqlDatabase) {
    return { readRequestPageOptionsNodesResultcompanySettings2(companyId: string) {
            return database.prepare("SELECT currency FROM commerce_settings WHERE company_id=?")
                .bind(companyId)
                .first<{
                currency: string;
            }>();
        } };
}
