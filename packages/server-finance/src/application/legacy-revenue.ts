import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceRow, JournalInput } from '@erpx/server-finance/domain/finance';
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export interface LegacyRevenuePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    prior<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, input: unknown): Promise<Result|null>;
    obligation(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    account(actor: WorkContext, id: string | number | null): Promise<{
        chart_id: string;
    } | null>;
    createJournal(actor: WorkContext, input: JournalInput, key: string): Promise<{
        id: string;
    }>;
    postJournal(actor: WorkContext, id: string, key: string, input: {
        expectedVersion: number;
    }): Promise<unknown>;
    commit(facts: {
        actor: WorkContext;
        obligationId: string;
        input: {
            amount: string;
            periodId: string;
            entryDate: string;
        };
        id: string;
        amount: number;
        output: Record<string,unknown>;
    }): Promise<Record<string,unknown>>;
}
export function createLegacyRevenue(ports: LegacyRevenuePorts) {
    const { decimalToMinor } = createFinancePolicy(ports.error);
    async function recognizeRevenue(actor: WorkContext, obligationId: string, input: {
        amount: string;
        periodId: string;
        entryDate: string;
    }, id: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.prior<Record<string,unknown>>(actor, id, "finance.revenue.recognize", input);
        if (previous)
            return previous;
        const obligation = await ports.obligation(actor, obligationId);
        if (!obligation)
            throw ports.error(404, "OBLIGATION_NOT_FOUND", "Revenue obligation not found.");
        const amount = decimalToMinor(input.amount);
        const remaining = Number(obligation.allocated_minor) - Number(obligation.recognized_minor);
        if (amount <= 0 || amount > remaining)
            throw ports.error(422, "REVENUE_EXCEEDS_ALLOCATION", "Recognition exceeds the remaining obligation.");
        const account = await ports.account(actor, obligation.revenue_account_id);
        if (!account)
            throw ports.error(422, "ACCOUNT_INVALID", "Revenue account is unavailable.");
        const journal = await ports.createJournal(actor, { chartId: account.chart_id, periodId: input.periodId, entryDate: input.entryDate, currency: String(obligation.currency), memo: `Revenue recognition ${obligation.description}`, sourceType: "revenue_recognition", sourceId: obligationId, isOpening: false, lines: [{ accountId: String(obligation.deferred_account_id), debit: minorToDecimal(amount), credit: "0" }, { accountId: String(obligation.revenue_account_id), debit: "0", credit: minorToDecimal(amount) }] }, `${id}:journal`);
        await ports.postJournal(actor, journal.id, `${id}:post`, { expectedVersion: 1 });
        const output = { id: obligationId, journalId: journal.id, recognized: minorToDecimal(amount) };
        return ports.commit({ actor, obligationId, input, id, amount, output });
    }
    return recognizeRevenue;
}

