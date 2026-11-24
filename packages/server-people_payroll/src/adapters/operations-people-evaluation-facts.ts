import {ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleEvaluationOperationsFacts(database: SqlDatabase) { return { readValidateEvaluationRolePeriodHistory(companyId: string, id: string) { return database.prepare(ppRd36TwoPredTail("before_json,after_json,created_at","staff_organization_history","company_id","engagement_id"," ORDER BY created_at,id")).bind(companyId, id).all<{
        before_json: string;
        after_json: string;
        created_at: number;
    }>(); } }; }
