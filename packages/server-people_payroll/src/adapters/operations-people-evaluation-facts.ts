import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationOperationsFacts(database: SqlDatabase) { return { readValidateEvaluationRolePeriodHistory(companyId: string, id: string) { return database.prepare("SELECT 1").bind().all<{
        before_json: string;
        after_json: string;
        created_at: number;
    }>(); } }; }
