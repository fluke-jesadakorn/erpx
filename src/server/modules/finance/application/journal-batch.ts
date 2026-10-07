import { z } from 'zod';
import { journalBatchSchema, type FinanceRow } from '../domain/finance';
import type { JournalPostPlan } from './journal-review';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export type JournalBatchInput = z.infer<typeof journalBatchSchema>;
export type BatchItemPlan = {
    item: JournalBatchInput['journals'][number];
    childKey: string;
    journal: FinanceRow;
    lines: FinanceRow[];
    fingerprint: string;
    posting: JournalPostPlan;
    evidenceDigest: string;
    now: number;
};
export interface JournalBatchPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    ready(actor: WorkContext): Promise<void>;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result|null>;
    journal(actor: WorkContext, id: string): Promise<FinanceRow | null>;
    lines(id: string): Promise<FinanceRow[]>;
    preparePosting(actor: WorkContext, journal: FinanceRow): Promise<JournalPostPlan>;
    digest(value: unknown): Promise<string>;
    commit(facts: {
        actor: WorkContext;
        input: JournalBatchInput;
        id: string;
        batchId: string;
        plans: BatchItemPlan[];
        lineCount: number;
    }): Promise<Record<string,unknown>>;
}
export function createJournalBatch(ports: JournalBatchPorts) {
    async function postJournalBatch(actor: WorkContext, raw: z.input<typeof journalBatchSchema>, id: string) {
        actor = ports.authorize(ports.authorize(actor, "finance.write"), "finance.approve");
        const input = journalBatchSchema.parse(raw);
        await ports.ready(actor);
        const batchId = `journal-batch-${id}`;
        const previous = await ports.replay<Record<string,unknown>>(actor, id, "finance.batch.post", batchId, input);
        if (previous)
            return previous;
        if (new Set(input.journals.map(j => j.id)).size !== input.journals.length)
            throw ports.error(422, "DUPLICATE_JOURNAL", "Select each draft once.");
        const journals = [] as FinanceRow[];
        for (const item of input.journals) {
            const journal = await ports.journal(actor, item.id);
            if (!journal)
                throw ports.error(404, "JOURNAL_NOT_FOUND", "A selected journal is not in this Company.");
            if (!["manual", "opening_import"].includes(String(journal.source_type)))
                throw ports.error(409, "BATCH_SOURCE_ADAPTER_REQUIRED", "Review source-owned drafts through their module until its batch adapter validates source versions and approved rules.");
            if (journal.version !== item.expectedVersion || !["draft", "in_review"].includes(String(journal.status)))
                throw ports.error(409, "VERSION_CONFLICT", "A selected draft changed. Review the entire batch again.");
            if (journals.length && ["chart_id", "period_id", "currency"].some(field => journal[field] !== journals[0][field]))
                throw ports.error(422, "BATCH_SCOPE_MISMATCH", "A batch requires one Company, ledger, period and transaction currency.");
            journals.push(journal);
        }
        const plans: BatchItemPlan[] = [];
        let lineCount = 0;
        for (const [index, journal] of journals.entries()) {
            const item = input.journals[index], childKey = `${id}:${index}`, lines = await ports.lines(item.id);
            lineCount += lines.length;
            if (lineCount > 1000)
                throw ports.error(422, 'BATCH_LINE_LIMIT', 'A batch may contain at most 1,000 lines.');
            const fingerprint = JSON.stringify(lines.map(l => [l.id, l.account_id, l.debit_minor, l.credit_minor, l.base_debit_minor, l.base_credit_minor, l.dimension_json, l.ordinal]));
            const posting = await ports.preparePosting(actor, journal), evidenceDigest = await ports.digest({ batchId, reason: input.reason, lines }), now = ports.now();
            plans.push({ item, childKey, journal, lines, fingerprint, posting, evidenceDigest, now });
        }
        return ports.commit({ actor, input, id, batchId, plans, lineCount });
    }
    return postJournalBatch;
}

