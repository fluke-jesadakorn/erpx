import { z } from 'zod';
import { decimalToMinor, minorToDecimal } from '@/lib/finance-money';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
import { accrualScheduleSchema, accrualScheduleDecisionSchema, accrualRunPostSchema, createAccrualPolicy, type AccrualRow, type AccrualAccount, type AccrualInput, type AccrualDecision, type AccrualRunPost } from '../domain/accruals';
export interface AccrualPorts {
    error: FinanceError;
    permission(actor: WorkContext, permission: 'finance.read' | 'finance.write' | 'finance.approve'): WorkContext;
    ready(actor: WorkContext): Promise<void>;
    now(): number;
    hash(value: unknown): Promise<string>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    schedules(facts: {
        actor: WorkContext;
    }): Promise<AccrualRow[]>;
    runs(facts: {
        actor: WorkContext;
    }): Promise<AccrualRow[]>;
    activeAccounts(facts: {
        actor: WorkContext;
    }): Promise<AccrualRow[]>;
    accounts(facts: {
        actor: WorkContext;
        input: AccrualInput;
    }): Promise<AccrualAccount[]>;
    commitCreateAccrualSchedule(facts: {
        actor: WorkContext;
        key: string;
        input: AccrualInput;
        id: string;
        result: Record<string,unknown>;
        total: number;
        now: number;
        count: number;
    }): Promise<Record<string,unknown>>;
    scheduleDecisionRow(facts: {
        scheduleId: string;
        actor: WorkContext;
    }): Promise<AccrualRow | null>;
    commitDecideAccrualSchedule(facts: {
        actor: WorkContext;
        key: string;
        action: 'approve' | 'cancel';
        input: AccrualDecision;
        scheduleId: string;
        version: number;
        status: string;
        row: AccrualRow;
        now: number;
    }): Promise<Record<string,unknown>>;
    schedule(facts: {
        scheduleId: string;
        actor: WorkContext;
    }): Promise<AccrualRow | null>;
    openPeriod(facts: {
        actor: WorkContext;
        periodStart: string;
        periodEndValue: string;
    }): Promise<{
        id: string;
    } | null>;
    existingRun(facts: {
        scheduleId: string;
        periodStart: string;
        actor: WorkContext;
    }): Promise<unknown | null>;
    chart(facts: {
        schedule: AccrualRow;
        actor: WorkContext;
    }): Promise<{
        chart_id: string;
    } | null>;
    commitPrepareAccrualRun(facts: {
        actor: WorkContext;
        key: string;
        scheduleId: string;
        runId: string;
        result: Record<string,unknown>;
        schedule: AccrualRow;
        journalId: string;
        chart: {
            chart_id: string;
        };
        period: {
            id: string;
        };
        number: string;
        periodEndValue: string;
        periodStart: string;
        now: number;
        amount: number;
        sourceHash: string;
    }): Promise<Record<string,unknown>>;
    run(facts: {
        runId: string;
        actor: WorkContext;
    }): Promise<AccrualRow | null>;
    commitPostAccrualRun(facts: {
        actor: WorkContext;
        key: string;
        input: AccrualRunPost;
        runId: string;
        version: number;
        row: AccrualRow;
        now: number;
        next: string;
        complete: boolean;
    }): Promise<Record<string,unknown>>;
    scheduleDetail(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<AccrualRow | null>;
    runDetail(facts: {
        id: string;
        actor: WorkContext;
    }): Promise<AccrualRow | null>;
}
export function createAccruals(ports: AccrualPorts) {
    const { addMonths, periodEnd, frequencyMonths, periodCount, periodIndex, allocate } = createAccrualPolicy(ports.error);
    async function accrualSchedulePage(actor: WorkContext) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const [schedules, runs, accounts] = await Promise.all([
            ports.schedules({ actor }),
            ports.runs({ actor }),
            ports.activeAccounts({ actor }),
        ]);
        return { schedules: schedules, runs: runs, accounts: accounts };
    }
    async function createAccrualSchedule(actor: WorkContext, input: z.infer<typeof accrualScheduleSchema>, key: string) {
        actor = ports.permission(actor, "finance.write");
        await ports.ready(actor);
        const id = `accrual-${key}`, previous = await ports.replay(actor, key, "finance.accrual.create", id, input);
        if (previous)
            return previous;
        if (input.endDate < input.startDate)
            throw ports.error(422, "INVALID_ACCRUAL_DATES", "Accrual end date must follow the start date.");
        const total = decimalToMinor(input.total);
        if (total <= 0)
            throw ports.error(422, "INVALID_ACCRUAL_TOTAL", "Accrual total must be greater than zero.");
        const count = periodCount(input.startDate, input.endDate, input.frequency);
        const accounts = await ports.accounts({ actor, input });
        if (accounts.length !== 2 || accounts.some(account => account.currency !== input.currency || account.base_currency !== input.currency))
            throw ports.error(422, "ACCRUAL_ACCOUNT_INVALID", "Choose two active accounts in the chart's functional currency.");
        if (input.debitAccountId === input.creditAccountId)
            throw ports.error(422, "ACCRUAL_ACCOUNT_INVALID", "Debit and credit accounts must be different.");
        const now = ports.now(), result = { id, description: input.description, status: "draft", version: 1, periodCount: count, total: minorToDecimal(total) };
        return ports.commitCreateAccrualSchedule({ actor, key, input, id, result, total, now, count });
    }
    async function decideAccrualSchedule(actor: WorkContext, scheduleId: string, input: z.infer<typeof accrualScheduleDecisionSchema>, action: "approve" | "cancel", key: string) {
        actor = action === "approve" ? ports.permission(actor, "finance.approve") : ports.permission(actor, "finance.write");
        await ports.ready(actor);
        const previous = await ports.replay(actor, key, `finance.accrual.${action}`, scheduleId, input);
        if (previous)
            return previous;
        const row = await ports.scheduleDecisionRow({ scheduleId, actor });
        if (!row)
            throw ports.error(404, "ACCRUAL_NOT_FOUND", "Accrual schedule not found.");
        if (Number(row.version) !== input.expectedVersion)
            throw ports.error(409, "ACCRUAL_CHANGED", "The accrual schedule changed. Reload before deciding.");
        if (action === "approve" && (row.status !== "draft" || row.created_by === actor.userId))
            throw ports.error(409, row.created_by === actor.userId ? "SELF_REVIEW_FORBIDDEN" : "INVALID_ACCRUAL_TRANSITION", row.created_by === actor.userId ? "A different Finance approver must approve this schedule." : "Only a draft schedule can be approved.");
        if (action === "cancel" && !['draft', 'active'].includes(String(row.status)))
            throw ports.error(409, "INVALID_ACCRUAL_TRANSITION", "Only a draft or active schedule can be cancelled.");
        const status = action === "approve" ? "active" : "cancelled", version = input.expectedVersion + 1, now = ports.now();
        return ports.commitDecideAccrualSchedule({ actor, key, action, input, scheduleId, version, status, row, now });
    }
    async function prepareAccrualRun(actor: WorkContext, scheduleId: string, key: string) {
        actor = ports.permission(actor, "finance.write");
        await ports.ready(actor);
        const runId = `accrual-run-${key}`, previous = await ports.replay(actor, key, "finance.accrual.prepare", runId, { scheduleId });
        if (previous)
            return previous;
        const schedule = await ports.schedule({ scheduleId, actor });
        if (!schedule)
            throw ports.error(404, "ACCRUAL_NOT_FOUND", "Accrual schedule not found.");
        if (schedule.status !== "active")
            throw ports.error(409, "ACCRUAL_NOT_ACTIVE", "Only an active accrual schedule can create a period.");
        const periodStart = String(schedule.next_run_date), frequency = String(schedule.frequency) as "monthly" | "quarterly" | "annual", periodEndValue = periodEnd(periodStart, frequency) > String(schedule.end_date) ? String(schedule.end_date) : periodEnd(periodStart, frequency);
        if (periodStart > String(schedule.end_date))
            throw ports.error(409, "ACCRUAL_COMPLETE", "This schedule has no remaining periods.");
        const period = await ports.openPeriod({ actor, periodStart, periodEndValue });
        if (!period)
            throw ports.error(409, "ACCRUAL_PERIOD_UNAVAILABLE", "Open an accounting period covering the accrual period before preparing it.");
        const count = periodCount(String(schedule.start_date), String(schedule.end_date), frequency), index = periodIndex(String(schedule.start_date), periodStart, frequency);
        const amount = allocate(Number(schedule.total_minor), count, Math.max(0, Math.min(count - 1, index)));
        const sourceHash = await ports.hash({ scheduleId, periodStart, periodEnd: periodEndValue, amount, scheduleVersion: schedule.version });
        const existing = await ports.existingRun({ scheduleId, periodStart, actor });
        if (existing)
            throw ports.error(409, "ACCRUAL_PERIOD_ALREADY_PREPARED", "This accrual period already has a prepared run.");
        const chart = await ports.chart({ schedule, actor });
        if (!chart)
            throw ports.error(422, "ACCRUAL_ACCOUNT_INVALID", "The schedule account is no longer active.");
        const journalId = `journal-${runId}`, number = `ACR-${key}`, now = ports.now();
        const result = { id: runId, scheduleId, journalId, periodStart, periodEnd: periodEndValue, amountMinor: amount, status: "prepared", version: 1 };
        return ports.commitPrepareAccrualRun({ actor, key, scheduleId, runId, result, schedule, journalId, chart, period, number, periodEndValue, periodStart, now, amount, sourceHash });
    }
    async function postAccrualRun(actor: WorkContext, runId: string, input: z.infer<typeof accrualRunPostSchema>, key: string) {
        actor = ports.permission(actor, "finance.approve");
        await ports.ready(actor);
        const previous = await ports.replay(actor, key, "finance.accrual.post", runId, input);
        if (previous)
            return previous;
        const row = await ports.run({ runId, actor });
        if (!row)
            throw ports.error(404, "ACCRUAL_RUN_NOT_FOUND", "Accrual period run not found.");
        if (row.status !== "prepared" || Number(row.version) !== input.expectedVersion)
            throw ports.error(409, "ACCRUAL_RUN_CHANGED", "This accrual run changed. Reload before posting.");
        if (row.prepared_by === actor.userId)
            throw ports.error(409, "SELF_REVIEW_FORBIDDEN", "A different Finance approver must post this accrual.");
        if (row.schedule_status !== "active" || Number(row.schedule_current_version) !== Number(row.schedule_version) || row.journal_status !== "draft")
            throw ports.error(409, "ACCRUAL_SOURCE_CHANGED", "The schedule or journal changed. Prepare the period again.");
        const frequency = String(row.frequency) as "monthly" | "quarterly" | "annual", next = addMonths(String(row.period_start), frequencyMonths(frequency)), complete = next > String(row.end_date), version = input.expectedVersion + 1, now = ports.now();
        return ports.commitPostAccrualRun({ actor, key, input, runId, version, row, now, next, complete });
    }
    async function accrualRecordDetail(actor: WorkContext, kind: "schedule" | "run", id: string) {
        actor = ports.permission(actor, "finance.read");
        await ports.ready(actor);
        const row = kind === "schedule"
            ? await ports.scheduleDetail({ id, actor })
            : await ports.runDetail({ id, actor });
        if (!row)
            throw ports.error(404, "ACCRUAL_NOT_FOUND", "Accrual record not found.");
        const data = await accrualSchedulePage(actor);
        return { row, data: { ...data, schedules: kind === "schedule" ? [row] : [], runs: kind === "run" ? [row] : data.runs.filter(run => run.schedule_id === id) } };
    }
    return { accrualSchedulePage, createAccrualSchedule, decideAccrualSchedule, prepareAccrualRun, postAccrualRun, accrualRecordDetail };
}

