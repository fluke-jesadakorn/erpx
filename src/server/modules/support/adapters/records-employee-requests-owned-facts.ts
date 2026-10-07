import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function EmployeeRequestsSupportOwnedFacts(database: SqlDatabase) {
    return { readEmployeeRequestDetailTeam(team_id: string, companyId: string) {
            return database.prepare("SELECT name FROM service_teams WHERE id=? AND company_id=?")
                .bind(team_id, companyId)
                .first<{
                name: string;
            }>();
        } };
}
