/**
 * Retained manual journal creation. Opening preview/import moved to the native
 * Finance owner command; only the manual create command still uses this
 * TypeScript application service until its own native cutover.
 */
import { z } from 'zod';
import { createJournalSchema, type JournalInput } from '@erpx/server-finance/domain/finance';
import type { JournalPlan } from '@erpx/server-finance/application/journal';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';

export type JournalResult = {
    id: string;
    number: string;
    status: string;
    version: number;
    debit: string;
    credit: string;
};

export interface JournalCreatePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<JournalResult | null>;
    prepare(actor: WorkContext, input: JournalInput, key: string): Promise<JournalPlan>;
    commitJournal(facts: {
        actor: WorkContext;
        input: JournalInput;
        id: string;
        journalId: string;
        plan: JournalPlan;
    }): Promise<JournalResult>;
}

export function createJournalService(ports: JournalCreatePorts) {
    async function createJournal(actor: WorkContext, input: z.infer<typeof createJournalSchema>, id: string) {
        actor = ports.authorize(actor, "finance.write");
        if (input.sourceType === "fiscal_close")
            throw ports.error(422, "FISCAL_CLOSING_OWNER_REQUIRED", "Prepare and independently review a fiscal closing through the Finance year-closing workflow.");
        if (input.sourceType.startsWith("book_"))
            throw ports.error(422, "BOOK_OWNER_REQUIRED", "Book openings, projections and adjustments require their owning review workflow.");
        if (["business_recognition", "business_billing_credit", "instrument_activation", "instrument_payment", "asset_policy_depreciation"].includes(input.sourceType))
            throw ports.error(422, "FINANCE_SOURCE_OWNER_REQUIRED", "Prepare this journal through its originating Finance workflow.");
        await ports.ready(actor);
        const journalId = `journal-${id}`;
        const previous = await ports.replay(actor, id, "finance.journal.create", journalId, input);
        if (previous)
            return previous;
        const plan = await ports.prepare(actor, input, id);
        return ports.commitJournal({ actor, input, id, journalId, plan });
    }
    return { createJournal };
}
