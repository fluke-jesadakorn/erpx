import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function RoutinesProjectsFacts(database: SqlDatabase) {
    return { readOccurrenceStatementsEffect(project_id: string, company_id: string) {
            return database.prepare("SELECT 1 FROM staff_projects WHERE id=? AND company_id=?")
                .bind(project_id, company_id)
                .first();
        } };
}
