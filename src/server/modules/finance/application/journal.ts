import { createFinancePolicy } from '../domain/finance-policy';
import type { JournalInput, FunctionalFxEvidence } from '../domain/finance';
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export interface JournalPorts {
    error: FinanceError;
    now(): number;
    chart(companyId: string, chartId: string): Promise<{
        id: string;
        base_currency: string;
    } | null>;
    period(actor: WorkContext, periodId: string, date: string): Promise<unknown>;
    moneyScale(companyId: string, currency: string): Promise<number>;
    functionalFx(actor: WorkContext, currency: string, baseCurrency: string, date: string): Promise<FunctionalFxEvidence>;
    accounts(companyId: string, chartId: string, ids: string[]): Promise<{
        id: string;
    }[]>;
    openingExists(companyId: string, chartId: string): Promise<boolean>;
}
export function createJournalPreparation(ports: JournalPorts) {
    const { decimalToMinor, fxMicros, convertAtFx } = createFinancePolicy(ports.error);
    return async (actor: WorkContext, input: JournalInput, key: string, status: 'draft' | 'posted' = 'draft') => {
        const chart = await ports.chart(actor.companyId, input.chartId);
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Active chart not found.");
        await ports.period(actor, input.periodId, input.entryDate);
        await ports.moneyScale(actor.companyId, input.currency);
        await ports.moneyScale(actor.companyId, chart.base_currency);
        const preparedFxEvidence = input.fxRate ? null : await ports.functionalFx(actor, input.currency, chart.base_currency, input.entryDate);
        const fx = input.fxRate ? fxMicros(input.currency, chart.base_currency, input.fxRate) : preparedFxEvidence!.micros;
        const convert = (amount: number) => convertAtFx(amount, fx);
        const values = input.lines.map(l => { const debit = decimalToMinor(l.debit), credit = decimalToMinor(l.credit); if (debit < 0 || credit < 0 || (debit > 0) === (credit > 0))
            throw ports.error(422, "INVALID_JOURNAL_LINE", "Each line requires one positive debit or credit."); return { ...l, debit, credit, baseDebit: convert(debit), baseCredit: convert(credit) }; });
        const total = (field: "debit" | "credit" | "baseDebit" | "baseCredit") => values.reduce((sum, l) => sum + BigInt(l[field]), BigInt(0));
        if (total("debit") !== total("credit"))
            throw ports.error(422, "UNBALANCED_JOURNAL", "Debits and credits must balance in transaction currency.");
        if (total("debit") > BigInt(Number.MAX_SAFE_INTEGER))
            throw ports.error(422, "MONEY_OVERFLOW", "The journal total exceeds supported precision.");
        const baseTarget = convert(Number(total("debit"))), lastDebit = [...values].reverse().find(line => line.debit > 0), lastCredit = [...values].reverse().find(line => line.credit > 0);
        if (!lastDebit || !lastCredit)
            throw ports.error(422, "UNBALANCED_JOURNAL", "A journal requires both debit and credit lines.");
        lastDebit.baseDebit += baseTarget - Number(total("baseDebit"));
        lastCredit.baseCredit += baseTarget - Number(total("baseCredit"));
        if (lastDebit.baseDebit < 0 || lastCredit.baseCredit < 0 || total("baseDebit") !== total("baseCredit"))
            throw ports.error(422, "UNBALANCED_JOURNAL", "Functional-currency rounding could not be balanced.");
        const accountIds = [...new Set(values.map(l => l.accountId))];
        const valid = await ports.accounts(actor.companyId, input.chartId, accountIds);
        if (valid.length !== accountIds.length)
            throw ports.error(422, "ACCOUNT_INVALID", "Every posting account must be active and belong to this chart.");
        const journalId = `journal-${key}`, number = `J-${key}`, now = ports.now();
        if (input.isOpening) {
            const existing = await ports.openingExists(actor.companyId, input.chartId);
            if (existing)
                throw ports.error(409, "OPENING_BALANCE_EXISTS", "Reverse or finish the existing opening balance before importing another one.");
        }
        return { chart, fx, accountIds, journalId, number, now, values, preparedFxEvidence, result: { id: journalId, number, status, version: 1, debit: minorToDecimal(Number(total("debit"))), credit: minorToDecimal(Number(total("credit"))), fxRateMicros: fx, baseCurrency: chart.base_currency } };
    };
}
export type JournalPlan = Awaited<ReturnType<ReturnType<typeof createJournalPreparation>>>;
