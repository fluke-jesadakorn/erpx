import { z } from 'zod';
import { prepareFxRevaluationSchema, decideFxRevaluationSchema, type FinanceRow, type FxExposure, type JournalInput, type JournalLineInput } from '../domain/finance';
import type { JournalPlan } from './journal';
import { createFinancePolicy } from '../domain/finance-policy';
import { samePerson } from "@/lib/identity_access/iam";
import { minorToDecimal } from "@/lib/finance/finance-money";
import type { FinanceError } from '../domain/invoice-line-revision';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
export type FxPrepareInput = z.infer<typeof prepareFxRevaluationSchema>;
export type FxDecisionInput = z.infer<typeof decideFxRevaluationSchema>;
export type FxCalculation = {
    chart: FinanceRow;
    exposures: FxExposure[];
    sourceHash: string;
};
export interface FxRevaluationPorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    id(): string;
    digest(value: unknown): Promise<string>;
    ready(actor: WorkContext, readOnly?: boolean): Promise<void>;
    prior(actor: WorkContext, key: string, operation: string, input: unknown): Promise<Record<string,unknown>|null>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    prepareJournal(actor: WorkContext, input: JournalInput, key: string, status: 'posted'): Promise<JournalPlan>;
    exposureChart(facts: {
        chartId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    exposureRows(facts: {
        period: FinanceRow;
        actor: WorkContext;
        chartId: string;
        chart: FinanceRow;
    }): Promise<FinanceRow[]>;
    workspacePeriod(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    workspaceCurrencies(facts: {
        actor: WorkContext;
        period: FinanceRow;
    }): Promise<FinanceRow[]>;
    workspaceAccounts(facts: {
        actor: WorkContext;
    }): Promise<FinanceRow[]>;
    workspaceRuns(facts: {
        actor: WorkContext;
        periodId: string;
    }): Promise<FinanceRow[]>;
    openPeriod(facts: {
        input: FxPrepareInput;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    validAdjustmentAccounts(facts: {
        actor: WorkContext;
        input: FxPrepareInput;
        calculated: FxCalculation;
    }): Promise<FinanceRow[]>;
    commitPreparation(facts: {
        actor: WorkContext;
        key: string;
        input: FxPrepareInput;
        output: Record<string,unknown>;
        runId: string;
        calculated: FxCalculation;
        rates: Record<string, string>;
        now: number;
    }): Promise<Record<string,unknown>>;
    decidedRun(facts: {
        runId: string;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    commitReject(facts: {
        actor: WorkContext;
        key: string;
        input: FxDecisionInput;
        runId: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    runPeriod(facts: {
        run: FinanceRow;
        actor: WorkContext;
    }): Promise<FinanceRow | null>;
    commitApproval(facts: {
        actor: WorkContext;
        key: string;
        input: FxDecisionInput;
        runId: string;
        journal: JournalPlan | null;
        gain: number;
        loss: number;
        run: FinanceRow;
        calculated: FxCalculation;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createFxRevaluation(ports: FxRevaluationPorts) {
    const { fxMicros, convertAtFx, requireFinanceRole } = createFinancePolicy(ports.error);
    async function fxExposures(actor: WorkContext, chartId: string, period: FinanceRow, rates: Record<string, string>) {
        const chart = await ports.exposureChart({ chartId, actor });
        if (!chart)
            throw ports.error(404, "CHART_NOT_FOUND", "Active ledger chart not found.");
        const rows = await ports.exposureRows({ period, actor, chartId, chart });
        const exposures: FxExposure[] = [];
        for (const row of rows) {
            const outstanding = Number(row.transaction_outstanding);
            if (outstanding <= 0)
                continue;
            const rate = fxMicros(String(row.currency), String(chart.base_currency), rates[String(row.currency)]), priorBase = Number(row.prior_base), revaluedBase = convertAtFx(outstanding, rate);
            exposures.push({ invoiceId: String(row.id), kind: String(row.kind) as FxExposure["kind"], currency: String(row.currency), controlAccountId: String(row.control_account_id), transactionOutstanding: outstanding, priorBase, revaluedBase, adjustment: revaluedBase - priorBase, rateMicros: rate, version: Number(row.version) });
        }
        return { chart, exposures, sourceHash: await ports.digest({ chartId, periodId: period.id, periodEnd: period.end_date, exposures }) };
    }
    async function fxRevaluationWorkspace(actor: WorkContext, periodId: string) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor, true);
        const period = await ports.workspacePeriod({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        const [currencies, accounts, runs] = await Promise.all([
            ports.workspaceCurrencies({ actor, period }),
            ports.workspaceAccounts({ actor }),
            ports.workspaceRuns({ actor, periodId }),
        ]);
        return { period, currencies: currencies, accounts: accounts, runs: runs };
    }
    async function prepareFxRevaluation(actor: WorkContext, raw: z.input<typeof prepareFxRevaluationSchema>, key: string) {
        actor = ports.authorize(actor, "finance.write");
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company finance assignment to prepare revaluation.");
        const input = prepareFxRevaluationSchema.parse(raw);
        await ports.ready(actor);
        const previous = await ports.prior(actor, key, "finance.fx-revaluation.prepare", input);
        if (previous)
            return previous;
        if (new Set(input.rates.map(row => row.currency)).size !== input.rates.length)
            throw ports.error(422, "DUPLICATE_FX_RATE", "Provide one closing rate per currency.");
        const period = await ports.openPeriod({ input, actor });
        if (!period)
            throw ports.error(409, "PERIOD_NOT_OPEN", "Revaluation requires an open period.");
        if (input.entryDate !== period.end_date)
            throw ports.error(422, "REVALUATION_DATE_INVALID", "Post revaluation on the period end date.");
        const rates = Object.fromEntries(input.rates.map(row => [row.currency, row.rate])), calculated = await fxExposures(actor, input.chartId, period, rates);
        if (!calculated.exposures.length)
            throw ports.error(422, "NO_FOREIGN_EXPOSURE", "No open foreign monetary items require revaluation for this ledger and period.");
        const valid = await ports.validAdjustmentAccounts({ actor, input, calculated });
        if (valid.length !== 2 || valid.find(row => row.id === input.gainAccountId)?.category !== "other_income" || valid.find(row => row.id === input.lossAccountId)?.category !== "other_expense")
            throw ports.error(422, "FX_ACCOUNT_INVALID", "Select functional-currency other-income and other-expense accounts from this ledger.");
        const runId = `fx-revaluation-${ports.id()}`, now = ports.now(), output = { id: runId, status: "prepared", version: 1, sourceHash: calculated.sourceHash, lines: calculated.exposures.length, netAdjustment: minorToDecimal(calculated.exposures.reduce((sum, row) => sum + row.adjustment, 0)) };
        return ports.commitPreparation({ actor, key, input, output, runId, calculated, rates, now });
    }
    async function decideFxRevaluation(actor: WorkContext, runId: string, raw: z.input<typeof decideFxRevaluationSchema>, decision: "approve" | "reject", key: string) {
        actor = ports.authorize(actor, "finance.approve");
        requireFinanceRole(actor, true);
        if (actor.branchId || actor.departmentId)
            throw ports.error(403, "COMPANY_FINANCE_REQUIRED", "Use a Company finance assignment to approve revaluation.");
        const input = decideFxRevaluationSchema.parse(raw);
        await ports.ready(actor);
        const previous = await ports.replay(actor, key, `finance.fx-revaluation.${decision}`, runId, input);
        if (previous)
            return previous;
        const run = await ports.decidedRun({ runId, actor });
        if (!run)
            throw ports.error(404, "FX_REVALUATION_NOT_FOUND", "Revaluation run not found.");
        if (run.status !== "prepared" || Number(run.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "Revaluation changed. Refresh before deciding.");
        if (samePerson(actor, run.created_by))
            throw ports.error(409, "SELF_REVIEW_FORBIDDEN", "A different Finance approver must decide revaluation.");
        const now = ports.now();
        if (decision === "reject")
            return ports.commitReject({ actor, key, input, runId, now });
        if (run.period_state !== "open")
            throw ports.error(409, "PERIOD_NOT_OPEN", "The revaluation period is no longer open.");
        const rates = JSON.parse(String(run.rates_json)) as Record<string, string>, period = await ports.runPeriod({ run, actor }), calculated = await fxExposures(actor, String(run.chart_id), period!, rates);
        if (calculated.sourceHash !== run.source_hash)
            throw ports.error(409, "FX_REVALUATION_STALE", "Foreign balances changed after preparation. Reject this run and prepare it again.");
        const adjustmentLines: JournalLineInput[] = [], gain = calculated.exposures.filter(row => row.kind === "receivable" ? row.adjustment > 0 : row.adjustment < 0).reduce((sum, row) => sum + Math.abs(row.adjustment), 0), loss = calculated.exposures.filter(row => row.kind === "receivable" ? row.adjustment < 0 : row.adjustment > 0).reduce((sum, row) => sum + Math.abs(row.adjustment), 0);
        for (const row of calculated.exposures) {
            if (!row.adjustment)
                continue;
            const debit = row.kind === "receivable" ? row.adjustment > 0 : row.adjustment < 0;
            adjustmentLines.push({ accountId: row.controlAccountId, debit: debit ? minorToDecimal(Math.abs(row.adjustment)) : "0", credit: debit ? "0" : minorToDecimal(Math.abs(row.adjustment)), description: `Period-end FX revaluation ${row.invoiceId}` });
        }
        if (gain)
            adjustmentLines.push({ accountId: String(run.gain_account_id), debit: "0", credit: minorToDecimal(gain), description: "Unrealized foreign exchange gain" });
        if (loss)
            adjustmentLines.push({ accountId: String(run.loss_account_id), debit: minorToDecimal(loss), credit: "0", description: "Unrealized foreign exchange loss" });
        const journal = adjustmentLines.length ? await ports.prepareJournal(actor, { chartId: String(run.chart_id), periodId: String(run.period_id), entryDate: String(run.entry_date), currency: String(run.base_currency), memo: `Period-end FX revaluation ${run.entry_date}`, sourceType: "fx_revaluation", sourceId: runId, isOpening: false, lines: adjustmentLines }, `${key}-journal`, "posted") : null;
        return ports.commitApproval({ actor, key, input, runId, journal, gain, loss, run, calculated, now });
    }
    return { fxExposures, fxRevaluationWorkspace, prepareFxRevaluation, decideFxRevaluation };
}

