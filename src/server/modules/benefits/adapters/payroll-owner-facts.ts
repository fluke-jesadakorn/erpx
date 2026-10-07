import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function payrollBenefitOwnerFacts(database: SqlDatabase) {
    return { readCalculateItemBenefitInputs(id: string, currency: string, period_end: string, period_start: string) {
            return database.prepare("SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id").bind(id, currency, period_end, period_start).all<{
                id: string;
                plan_name: string;
                employee_deduction_minor: number;
                employer_cost_minor: number;
                source_election_version: number;
            }>();
        } };
}
