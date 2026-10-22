import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { createFinancePolicy } from '@erpx/server-finance/domain/finance-policy';
import type { FinanceRow } from '@erpx/server-finance/domain/finance';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
export type JournalPostPlan = {
    journalId: string;
    debit: number;
    now: number;
    opening: boolean;
};
export interface JournalReviewPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    ready(actor: WorkContext): Promise<void>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    journal(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    postedJournal(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    debit(journalId: string): Promise<{
        amount: number;
    } | null>;
    openingAllocated(actor: WorkContext, id: string): Promise<boolean>;
    sourceOpening(id: string | number | null): Promise<boolean>;
    reversalRoot(actor: WorkContext, id: string): Promise<{
        source_type: string;
    } | null>;
    commitPost(facts: {
        actor: WorkContext;
        journalId: string;
        id: string;
        input: {
            expectedVersion: number;
        };
        journal: FinanceRow;
        plan: JournalPostPlan;
    }): Promise<Record<string,unknown>>;
    commitReverse(facts: {
        actor: WorkContext;
        journalId: string;
        id: string;
        input: {
            periodId: string;
            entryDate: string;
            memo?: string;
        };
        journal: FinanceRow;
        now: number;
        opening: boolean;
    }): Promise<Record<string,unknown>>;
}
export function createJournalReview(ports: JournalReviewPorts) {
    const { requireFinanceRole } = createFinancePolicy(ports.error);
    async function prepareJournalPost(actor: WorkContext, journal: FinanceRow) {
        const journalId = String(journal.id);
        if (samePerson(actor, journal.created_by) || !hasPermission(actor, "finance.approve"))
            throw ports.error(403, "INDEPENDENT_APPROVER_REQUIRED", "A different finance approver must post a manual journal.");
        const debit = await ports.debit(journalId);
        return { journalId, debit: Number(debit?.amount ?? 0), now: ports.now(), opening: journal.source_type === 'opening_import' };
    }
    async function postJournal(actor: WorkContext, journalId: string, id: string, input: {
        expectedVersion: number;
    }) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.journal.post", journalId, input);
        if (previous)
            return previous;
        const journal = await ports.journal(actor, journalId);
        if (!journal)
            throw ports.error(404, "JOURNAL_NOT_FOUND", "Journal not found.");
        const plan = await prepareJournalPost(actor, journal);
        return ports.commitPost({ actor, journalId, id, input, journal, plan });
    }
    async function reverseJournal(actor: WorkContext, journalId: string, id: string, input: {
        periodId: string;
        entryDate: string;
        memo?: string;
    }) {
        actor = ports.authorize(actor, "finance.write");
        requireFinanceRole(actor, true);
        await ports.ready(actor);
        const previous = await ports.replay(actor, id, "finance.journal.reverse", journalId, input);
        if (previous)
            return previous;
        const journal = await ports.postedJournal(actor, journalId);
        if (!journal)
            throw ports.error(404, "POSTED_JOURNAL_NOT_FOUND", "Posted journal not found.");
        if (!["manual", "opening_import", "reversal"].includes(String(journal.source_type)))
            throw ports.error(409, "REVERSE_AT_SOURCE", "Reverse the originating business document so its stock and settlement records remain consistent.");
        const opening = journal.source_type === "opening_import";
        if (opening && await ports.openingAllocated(actor, journalId))
            throw ports.error(409, "OPENING_SUBLEDGER_SETTLED", "This opening journal has payment or receipt allocations. Reverse those settlements and correct the affected documents instead.");
        if (input.entryDate < String(journal.entry_date))
            throw ports.error(422, "REVERSAL_BEFORE_SOURCE", "The reversal cannot precede the original accounting date.");
        if (journal.source_type === "reversal" && await ports.sourceOpening(journal.source_id))
            throw ports.error(409, "REIMPORT_OPENING_BALANCE", "Prepare a corrected opening import instead of reversing an opening reversal.");
        if (journal.source_type === "reversal") {
            const root = await ports.reversalRoot(actor, journalId);
            if (root?.source_type !== "manual")
                throw ports.error(409, "REVERSE_AT_SOURCE", "Correct the originating business workflow so its retained subledger and reversal evidence stay consistent.");
        }
        return ports.commitReverse({ actor, journalId, id, input, journal, now: ports.now(), opening });
    }
    return { prepareJournalPost, postJournal, reverseJournal };
}

