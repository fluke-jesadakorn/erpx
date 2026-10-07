import { z } from 'zod';
import { calculationRunSchema, calculationDecisionSchema, labels, type CalculationInput, type CalculationDecision, type EngineRow, type CloseCount, type CloseCheck } from '../domain/engines';
import { calculateFinance } from '../domain/finance-calculations';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { FinanceError } from '../domain/invoice-line-revision';
export interface FinanceEnginePorts {
    error: FinanceError;
    authorize(actor: WorkContext, permission: string): WorkContext;
    now(): number;
    ready(actor: WorkContext): Promise<void>;
    digest(value: unknown): Promise<string>;
    replay(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Record<string,unknown>|null>;
    closeCounts(actor: WorkContext, period: EngineRow, periodId: string): Promise<CloseCount[]>;
    runs(facts: {
        actor: WorkContext;
    }): Promise<EngineRow[]>;
    counts(facts: {
        actor: WorkContext;
    }): Promise<EngineRow[]>;
    periods(facts: {
        actor: WorkContext;
    }): Promise<EngineRow[]>;
    periodExists(facts: {
        parsed: CalculationInput;
        actor: WorkContext;
    }): Promise<boolean>;
    policyApproved(facts: {
        parsed: CalculationInput;
        actor: WorkContext;
    }): Promise<boolean>;
    commitPreparedCalculation(facts: {
        actor: WorkContext;
        key: string;
        parsed: CalculationInput;
        runId: string;
        output: Record<string,unknown>;
        sourceHash: string;
        result: Record<string,unknown>;
        explanation: Record<string, unknown>;
        now: number;
    }): Promise<Record<string,unknown>>;
    run(facts: {
        runId: string;
        actor: WorkContext;
    }): Promise<EngineRow | null>;
    commitCalculationDecision(facts: {
        actor: WorkContext;
        key: string;
        operation: string;
        input: CalculationDecision;
        runId: string;
        version: number;
        output: Record<string,unknown>;
        run: EngineRow;
        action: "review" | "approve" | "reject";
        next: string;
        now: number;
    }): Promise<Record<string,unknown>>;
    runDetail(facts: {
        runId: string;
        actor: WorkContext;
    }): Promise<EngineRow | null>;
    events(facts: {
        runId: string;
        actor: WorkContext;
    }): Promise<EngineRow[]>;
    period(facts: {
        periodId: string;
        actor: WorkContext;
    }): Promise<EngineRow | null>;
    commitCloseSnapshot(facts: {
        actor: WorkContext;
        key: string;
        periodId: string;
        period: EngineRow;
        output: Record<string,unknown>;
        values: CloseCheck[];
        sourceVersion: string;
        now: number;
    }): Promise<Record<string,unknown>>;
}
export function createFinanceEngines(ports: FinanceEnginePorts) {
    async function financeEngineWorkspace(actor: WorkContext) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        const [runs, counts, periods] = await Promise.all([
            ports.runs({ actor }),
            ports.counts({ actor }),
            ports.periods({ actor }),
        ]);
        return { runs: runs, counts: counts, periods: periods, labels };
    }
    async function createCalculationRun(actor: WorkContext, raw: z.input<typeof calculationRunSchema>, key: string) {
        actor = ports.authorize(actor, "finance.write");
        const parsed = calculationRunSchema.parse(raw);
        await ports.ready(actor);
        const runId = `calc-${key}`;
        const existing = await ports.replay(actor, key, "finance.calculation.prepare", runId, parsed);
        if (existing)
            return existing;
        if (parsed.periodId && !await ports.periodExists({ parsed, actor }))
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        if (parsed.policyId && !await ports.policyApproved({ parsed, actor }))
            throw ports.error(409, "POLICY_NOT_APPROVED", "The selected calculation policy and version must be approved.");
        let result: Record<string, unknown>;
        try {
            result = calculateFinance(parsed.kind, parsed.input, parsed.currencyScale);
        }
        catch (error) {
            throw ports.error(422, "CALCULATION_INVALID", error instanceof Error ? error.message : "Calculation input is invalid.");
        }
        const sourceHash = await ports.digest({ kind: parsed.kind, input: parsed.input, policyId: parsed.policyId ?? null, policyVersion: parsed.policyVersion, currency: parsed.currency, scale: parsed.currencyScale }), now = ports.now();
        const explanation = { engine: "erpx-deterministic-v1", label: labels[parsed.kind], sourceHash, inputs: Object.keys(parsed.input), humanApprovalRequired: true };
        const output = { id: runId, kind: parsed.kind, status: "prepared", version: 1, sourceHash, result };
        return ports.commitPreparedCalculation({ actor, key, parsed, runId, output, sourceHash, result, explanation, now });
    }
    async function decideCalculationRun(actor: WorkContext, runId: string, input: z.infer<typeof calculationDecisionSchema>, action: "review" | "approve" | "reject", key: string) {
        actor = ports.authorize(actor, action === "review" ? "finance.write" : "finance.approve");
        await ports.ready(actor);
        const run = await ports.run({ runId, actor });
        if (!run)
            throw ports.error(404, "CALCULATION_NOT_FOUND", "Calculation run not found.");
        if (JSON.parse(String(run.explanation_json)).engine === "source-vat-register-v1")
            throw ports.error(409, "VAT_PACKAGE_WORKFLOW_REQUIRED", "Review this source-derived calculation through its VAT filing package.");
        if (JSON.parse(String(run.explanation_json)).engine === "source-acquisition-v1")
            throw ports.error(409, "ACQUISITION_WORKFLOW_REQUIRED", "Review this source-derived calculation through its acquisition evidence.");
        const expected = action === "review" ? "prepared" : "reviewed", next = action === "reject" ? "rejected" : action === "review" ? "reviewed" : "approved";
        if (run.status !== expected && !(action === "reject" && ["prepared", "reviewed"].includes(String(run.status))))
            throw ports.error(409, "INVALID_CALCULATION_TRANSITION", `Calculation must be ${expected} before ${action}.`);
        if (Number(run.version) !== input.expectedVersion)
            throw ports.error(409, "VERSION_CONFLICT", "The calculation changed. Refresh and review again.");
        if (String(run.prepared_by) === actor.userId)
            throw ports.error(409, "SELF_REVIEW_FORBIDDEN", "A different person must review or approve this calculation.");
        const operation = `finance.calculation.${action}`, version = Number(run.version) + 1, now = ports.now(), output = { id: runId, status: next, version };
        return ports.commitCalculationDecision({ actor, key, operation, input, runId, version, output, run, action, next, now });
    }
    async function calculationRunDetail(actor: WorkContext, runId: string) {
        actor = ports.authorize(actor, "finance.read");
        await ports.ready(actor);
        const run = await ports.runDetail({ runId, actor });
        if (!run)
            throw ports.error(404, "CALCULATION_NOT_FOUND", "Calculation run not found.");
        const events = await ports.events({ runId, actor });
        return { ...run, input: JSON.parse(String(run.input_json)), result: JSON.parse(String(run.result_json)), explanation: JSON.parse(String(run.explanation_json)), events: events };
    }
    async function snapshotCloseChecks(actor: WorkContext, periodId: string, key: string) {
        actor = ports.authorize(actor, "finance.write");
        await ports.ready(actor);
        const period = await ports.period({ periodId, actor });
        if (!period)
            throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
        const values: CloseCheck[] = (await ports.closeCounts(actor, period, periodId)).map(check => ({ ...check, status: check.count ? 'block' : 'pass' }));
        const sourceVersion = await ports.digest({ periodId, version: period.close_version, values }), now = ports.now(), output = { periodId, sourceVersion, checks: values, ready: values.every(item => item.status === "pass") };
        return ports.commitCloseSnapshot({ actor, key, periodId, period, output, values, sourceVersion, now });
    }
    return { financeEngineWorkspace, createCalculationRun, decideCalculationRun, calculationRunDetail, snapshotCloseChecks };
}

