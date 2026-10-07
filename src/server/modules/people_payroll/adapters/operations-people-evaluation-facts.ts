import "server-only";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleEvaluationOperationsFacts(database: SqlDatabase) { return { readValidateEvaluationRolePeriodHistory(companyId: string, id: string) { return database.prepare("SELECT before_json,after_json,created_at FROM staff_organization_history WHERE company_id=? AND engagement_id=? ORDER BY created_at,id").bind(companyId, id).all<{
        before_json: string;
        after_json: string;
        created_at: number;
    }>(); } }; }
