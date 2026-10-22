import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { FinanceError } from '@erpx/server-finance/domain/invoice-line-revision';
import { financeDueDraftSchema, financeDueDecisionSchema, parsedInput, type DueInput, type DueDecision, type Row } from '@erpx/server-finance/domain/due-drafts';
import { recognitionRunSchema } from '@erpx/server-finance/domain/due-inputs';
export interface FinanceDuePorts {
    error: FinanceError;
    businessError(error: unknown): {
        code: string;
        message: string;
    } | null;
    permission(actor: WorkContext, permission: string): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    hash(value: string): Promise<string>;
    transaction<Result>(work: () => Promise<Result>): Promise<Result>;
    replay(actor: WorkContext, key: string, op: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    workspace(actor: WorkContext): Promise<Row[]>;
    source(actor: WorkContext, input: DueInput): Promise<Row | null>;
    assignment(actor: WorkContext, now: number): Promise<{
        version: number;
    } | null>;
    dueAssignment(row: Row, now: number): Promise<{
        version: number;
    } | null>;
    authority(row: Row): Promise<NonNullable<WorkContext['iam']>>;
    row(actor: WorkContext, id: string): Promise<Row | null>;
    commitSchedule(facts: {
        actor: WorkContext;
        input: DueInput;
        id: string;
        op: string;
        key: string;
        sourceVersion: number;
        assignmentVersion: number;
        bytes: string;
        hash: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    commitDecision(facts: {
        actor: WorkContext;
        id: string;
        input: DueDecision;
        op: string;
        key: string;
        status: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    candidates(holdingId: WorkContext['holdingId'], today: string, time: number): Promise<{
        id: string;
    }[]>;
    claim(id: string, time: number): Promise<Row | null>;
    prepareBilling(actor: WorkContext, id: string, input: Record<string, unknown>, key: string): Promise<Record<string, unknown>>;
    prepareRecognition(actor: WorkContext, input: Record<string, unknown>, key: string): Promise<Record<string, unknown>>;
    prepareAsset(actor: WorkContext, id: string, input: Record<string, unknown>, key: string): Promise<Record<string, unknown>>;
    prepareInstrument(actor: WorkContext, id: string, input: Record<string, unknown>, key: string): Promise<Record<string, unknown>>;
    complete(row: Row, result: Record<string, unknown>, time: number): Promise<void>;
    block(id: string, code: string, message: string, time: number): Promise<void>;
}
export function createFinanceDueDrafts(ports: FinanceDuePorts) {
    function holding(actor: WorkContext, permission: string) { actor = ports.permission(actor, permission); if (actor.scope.kind !== 'holding' || actor.assignmentScope !== 'holding')
        throw ports.error(403, 'FINANCE_SCHEDULE_HOLDING_REQUIRED', 'Schedule Company-wide Finance drafts with a Holding Finance assignment.'); return actor; }
    async function financeDueWorkspace(actor: WorkContext) { actor = holding(actor, 'finance.read'); await ports.ready(actor); return ports.workspace(actor); }
    async function scheduleFinanceDraft(actor: WorkContext, raw: z.input<typeof financeDueDraftSchema>, key: string) { actor = holding(actor, 'finance.write'); const input = financeDueDraftSchema.parse(raw), payload = parsedInput(input.kind, input.input), id = `finance-due-${key}`, op = 'finance.due-draft.schedule'; await ports.ready(actor); if (input.kind === 'recognition' && recognitionRunSchema.parse(payload).billingRunId !== input.sourceId)
        throw ports.error(422, 'FINANCE_SCHEDULE_SOURCE', 'Recognition must use the selected billing run.'); return ports.transaction(async () => { const prior = await ports.replay(actor, key, op, id, input); if (prior)
        return prior; const source = await ports.source(actor, input), assignment = await ports.assignment(actor, ports.now()), sourceVersion = Number(source?.version), expected = 'expectedVersion' in payload ? payload.expectedVersion : sourceVersion; if (!source || !assignment || !Number.isSafeInteger(sourceVersion) || sourceVersion !== expected || input.kind !== 'recognition' && source.status !== 'active')
        throw ports.error(409, 'FINANCE_SCHEDULE_SOURCE_CHANGED', 'Choose the current active source edition and a current Company Finance assignment.'); const bytes = JSON.stringify(payload); return ports.commitSchedule({ actor, input, id, op, key, sourceVersion, assignmentVersion: assignment.version, bytes, hash: await ports.hash(bytes), now: ports.now() }); }); }
    async function decideFinanceDueDraft(actor: WorkContext, id: string, raw: z.input<typeof financeDueDecisionSchema>, action: 'retry' | 'cancel', key: string) { actor = holding(actor, 'finance.write'); const input = financeDueDecisionSchema.parse(raw), op = `finance.due-draft.${action}`; await ports.ready(actor); return ports.transaction(async () => { const prior = await ports.replay(actor, key, op, id, input); if (prior)
        return prior; const row = await ports.row(actor, id); if (!row || row.version !== input.expectedVersion || !['scheduled', 'blocked'].includes(String(row.status)))
        throw ports.error(409, 'FINANCE_SCHEDULE_CHANGED', 'Refresh this pending Finance schedule before changing it.'); return ports.commitDecision({ actor, id, input, op, key, status: action === 'retry' ? 'scheduled' : 'cancelled', now: ports.now() }); }); }
    async function drainFinanceDueDrafts(holdingId: WorkContext['holdingId'], now: Date): Promise<number> { const time = now.getTime(), today = new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Bangkok', year: 'numeric', month: '2-digit', day: '2-digit' }).format(now), candidates = await ports.candidates(holdingId, today, time); let completed = 0; for (const candidate of candidates) {
        try {
            const done = await ports.transaction(async () => { const r = await ports.claim(candidate.id, time); if (!r)
                return false; if (await ports.hash(String(r.input_json)) !== r.input_hash)
                throw ports.error(409, 'FINANCE_SCHEDULE_EVIDENCE_CHANGED', 'The retained scheduled draft differs from its original checksum.'); const a = await ports.dueAssignment(r, ports.now()); if (!a || a.version !== r.assignment_version)
                throw ports.error(403, 'FINANCE_SCHEDULE_AUTHORITY_CHANGED', 'The delegated Finance assignment changed. Cancel and schedule with a current assignment.'); const iam = await ports.authority(r); iam.queuedAssignmentVersion = a.version; const actor = holding({ userId: String(r.delegated_by), name: 'Scheduled Finance preparer', email: '', assignmentId: String(r.assignment_id), role: String(r.role), companyId: String(r.company_id), branchId: null, departmentId: null, holdingId, holdingSlug: '', holdingName: '', scope: { kind: 'holding', companyId: String(r.company_id), holdingId, holdingSlug: '', holdingName: '' }, assignmentScope: 'holding', inherited: false, modules: ['finance'], assignments: [], workspaces: [], iam }, 'finance.write'), p = JSON.parse(String(r.input_json)) as Record<string, unknown>, key = `${r.id}:draft`; let result: Record<string, unknown>; switch (r.kind) {
                case 'billing':
                    result = await ports.prepareBilling(actor, String(r.source_id), p, key);
                    break;
                case 'recognition':
                    result = await ports.prepareRecognition(actor, p, key);
                    break;
                case 'asset_depreciation':
                    result = await ports.prepareAsset(actor, String(r.source_id), p, key);
                    break;
                case 'instrument_payment':
                    result = await ports.prepareInstrument(actor, String(r.source_id), p, key);
                    break;
                default: throw ports.error(422, 'FINANCE_SCHEDULE_KIND', 'Choose a supported draft workflow.');
            } await ports.complete(r, result, time); return true; });
            if (done)
                completed++;
        }
        catch (error) {
            const retained = ports.businessError(error);
            await ports.block(candidate.id, retained?.code ?? 'FINANCE_SCHEDULE_RETRY', retained?.message ?? 'The draft could not be prepared. Check the source configuration and retry.', time);
        }
    } return completed; }
    return { financeDueWorkspace, scheduleFinanceDraft, decideFinanceDueDraft, drainFinanceDueDrafts };
}

