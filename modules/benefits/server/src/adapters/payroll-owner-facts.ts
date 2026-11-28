import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { benefitPayrollInputs } from './native-lane-32/client';
/** Native owner read. The axum/sqlx owner executes the released payroll-input
 * projection; this adapter carries only bounded identities, no SQL. */
export function payrollBenefitOwnerFacts(database: SqlDatabase) {
    return { readCalculateItemBenefitInputs(id: string, currency: string, period_end: string, period_start: string, companyId?: string) {
            return benefitPayrollInputs(database, {
                engagementId: id,
                currency,
                periodEnd: period_end,
                periodStart: period_start,
            }, companyId);
        } };
}
