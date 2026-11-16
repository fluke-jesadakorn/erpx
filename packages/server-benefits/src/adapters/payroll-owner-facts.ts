import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function payrollBenefitOwnerFacts(database: SqlDatabase) {
    return { readCalculateItemBenefitInputs(id: string, currency: string, period_end: string, period_start: string) {
            return database.prepare("SELECT 1").bind().all<{
                id: string;
                plan_name: string;
                employee_deduction_minor: number;
                employer_cost_minor: number;
                source_election_version: number;
            }>();
        } };
}
