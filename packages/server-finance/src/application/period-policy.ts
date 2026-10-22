import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceRow } from '@erpx/server-finance/domain/finance';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export function createEnsurePeriod(ports: {
    error: FinanceError;
    period(companyId: string, periodId: string): Promise<FinanceRow | null>;
    coveredDate(companyId: string, date: string): Promise<boolean>;
}) {
    async function ensurePeriod(actor: WorkContext, periodId: string, entryDate: string, allowClosing = false) {
        const period = await ports.period(actor.companyId, periodId);
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        if (entryDate < String(period.start_date) || entryDate > String(period.end_date))
            throw ports.error(422, "DATE_OUTSIDE_PERIOD", "Entry date is outside the accounting period.");
        if (period.state === "closed" || (!allowClosing && period.state === "closing"))
            throw ports.error(409, "PERIOD_CLOSED", "The accounting period does not accept postings.");
        if (await ports.coveredDate(actor.companyId, entryDate))
            throw ports.error(409, "PERIOD_CLOSED", "This date is covered by a closed or closing Company period. Use the next open date or independently reopen the period.");
        return period;
    }
    return ensurePeriod;
}
