import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { benefitPayrollInputsSql } from '@erpx/server-benefits/adapters/benefits-read-sql';
export function payrollBenefitOwnerFacts(database: SqlDatabase) {
    return { readCalculateItemBenefitInputs(id: string, currency: string, period_end: string, period_start: string) {
            return database.prepare(benefitPayrollInputsSql).bind(id, currency, period_end, period_start).all<{
                id: string;
                plan_name: string;
                employee_deduction_minor: number;
                employer_cost_minor: number;
                source_election_version: number;
            }>();
        } };
}
