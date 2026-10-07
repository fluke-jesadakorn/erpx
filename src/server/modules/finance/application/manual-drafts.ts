import { z } from 'zod';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { JournalInput } from '../domain/finance';
import { manualDraftEditSchema, type ManualDraftInput, type DraftRow, type PreparedDraft, type DraftLine } from '../domain/manual-drafts';
export interface ManualDraftPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.write'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    transaction<Value>(actor: WorkContext, work: () => Promise<Value>): Promise<Value>;
    now(): number;
    hash(value: string): Promise<string>;
    replay<Result=Record<string,unknown>>(actor: WorkContext, key: string, op: string, id: string, input: unknown): Promise<Result|null>;
    journal(actor: WorkContext, id: string): Promise<DraftRow | null>;
    lines(actor: WorkContext, id: string): Promise<DraftRow[]>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string): Promise<PreparedDraft>;
    commit(facts: {
        actor: WorkContext;
        input: ManualDraftInput;
        key: string;
        op: string;
        journalId: string;
        old: DraftRow;
        now: number;
        version: number;
        nextLines: DraftLine[];
        originalSnapshot: string;
        snapshot: string;
        originalHash: string;
        snapshotHash: string;
    }): Promise<Record<string,unknown>>;
}
export function createManualDrafts(ports: ManualDraftPorts) {
    async function editManualDraft(actor: WorkContext, journalId: string, raw: z.input<typeof manualDraftEditSchema>, key: string) {
        actor = ports.permission(actor, 'finance.write');
        const input = manualDraftEditSchema.parse(raw), op = 'finance.manual-draft.edit';
        await ports.ready(actor);
        return ports.transaction(actor, async () => {
            const prior = await ports.replay<Record<string,unknown>>(actor, key, op, journalId, input);
            if (prior)
                return prior;
            const old = await ports.journal(actor, journalId);
            if (!old)
                throw ports.error(404, 'JOURNAL_NOT_FOUND', 'Journal not found in this Company.');
            if (old.status !== 'draft' || old.source_type !== 'manual' || old.is_opening || old.source_id || old.version !== input.expectedVersion)
                throw ports.error(409, 'MANUAL_DRAFT_CHANGED', 'Edit only the current unposted manual draft. Source-owned and posted documents retain their own correction workflow.');
            if (input.chartId !== old.chart_id || input.currency !== old.currency)
                throw ports.error(422, 'MANUAL_DRAFT_BOOK_CHANGED', "Retain this manual draft's book and currency.");
            const lines = await ports.lines(actor, journalId), prepared = await ports.prepareJournal(actor, { ...input, sourceType: 'manual', isOpening: false }, key), now = ports.now(), version = input.expectedVersion + 1;
            const next = { ...old, period_id: input.periodId, entry_date: input.entryDate, memo: input.memo, fx_rate_micros: prepared.result.fxRateMicros, created_by: actor.userId, version, updated_at: now }, nextLines = prepared.preparedLines.map((l, i) => ({ id: `${journalId}:v${version}:${i}`, journal_id: journalId, account_id: l.accountId, description: l.description ?? '', debit_minor: l.debit, credit_minor: l.credit, base_debit_minor: l.baseDebit, base_credit_minor: l.baseCredit, dimension_json: JSON.stringify(l.dimensions ?? {}), ordinal: i })), originalSnapshot = JSON.stringify({ journal: old, lines }), snapshot = JSON.stringify({ journal: next, lines: nextLines });
            return ports.commit({ actor, input, key, op, journalId, old, now, version, nextLines, originalSnapshot, snapshot, originalHash: await ports.hash(originalSnapshot), snapshotHash: await ports.hash(snapshot) });
        });
    }
    return { editManualDraft };
}

