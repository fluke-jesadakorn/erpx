import { withFinancePurchasedCommand, withFinancePurchasedRead } from "./purchase-boundary";
import {financeCloseManufacturingSource} from "../../manufacturing/public/finance-close-source";
import { createFinanceEngines } from '../application/engines';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import "server-only";
import { z } from "zod";
import { executeCommand, assertion, replayCommand } from "@/server/modules/workflow/public/commands";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { ApiError } from "@/server/modules/runtime/public/http";
type Row = Record<string, string | number | null>;
export { calculationRunSchema, calculationDecisionSchema } from '../domain/engines';
import { calculationRunSchema, calculationDecisionSchema } from '../domain/engines';
async function digest(value: unknown) { return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(value))))).map(byte => byte.toString(16).padStart(2, "0")).join(""); }
export async function financeEngineWorkspace(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>engineService(db).financeEngineWorkspace(actor),"finance.advanced"); }
export async function createCalculationRun(actor: WorkContext, raw: z.input<typeof calculationRunSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.calculation.prepare",purpose:"new-work",capability:"finance.advanced"},tx=>engineService(tx).createCalculationRun(actor,raw,key)); }
export async function decideCalculationRun(actor: WorkContext, runId: string, input: z.infer<typeof calculationDecisionSchema>, action: "review" | "approve" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.calculation.${action}`,purpose:"settle-existing",capability:"finance.advanced"},tx=>engineService(tx).decideCalculationRun(actor,runId,input,action,key)); }
export async function calculationRunDetail(actor: WorkContext, runId: string) { return withFinancePurchasedRead(actor,db=>engineService(db).calculationRunDetail(actor,runId),"finance.advanced"); }
export async function snapshotCloseChecks(actor: WorkContext, periodId: string, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.close.check",purpose:"settle-existing",capability:"finance.close"},tx=>engineService(tx).snapshotCloseChecks(actor,periodId,key)); }
function engineService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext) => pending ??= holdingResources(actor.holdingId).then(r => r.write);
    return createFinanceEngines({ error: (...args) => new ApiError(...args), authorize: requirePermission, now: Date.now, ready: async (actor) => { await db(actor); }, digest, replay: async (actor, key, operation, id, input) => replayCommand(await db(actor), actor, key, operation, id, input), closeCounts: async (actor, period, periodId) => {
            const write = await db(actor);
            const checks = [
                { key: "draft-journals", query: "SELECT count(*) n FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review')", args: [actor.companyId, periodId] },
                { key: "draft-invoices", query: "SELECT count(*) n FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?", args: [actor.companyId, period.start_date, period.end_date] },
                { key: "unmatched-bank", query: "SELECT count(*) n FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?", args: [actor.companyId, period.start_date, period.end_date] },
                { key: "open-manufacturing", query: financeCloseManufacturingSource(write).engineQuery, args: [actor.companyId] },
                { key: "unapproved-calculations", query: "SELECT count(*) n FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale')", args: [actor.companyId, periodId] },
            ];
            const values = [] as Array<{
                key: string;
                count: number;
                status: "pass" | "block";
            }>;
            for (const check of checks) {
                const count = Number(await write.prepare(check.query).bind(...check.args).first("n") ?? 0);
                values.push({ key: check.key, count, status: count ? "block" : "pass" });
            }
            return values.map(({ key, count }) => ({ key, count }));
        }, runs: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_calculation_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 50").bind(actor.companyId).all<Row>()).results; },
        counts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT kind,status,count(*) count FROM finance_calculation_runs WHERE company_id=? GROUP BY kind,status ORDER BY kind,status").bind(actor.companyId).all<Row>()).results; },
        periods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY start_date DESC LIMIT 24").bind(actor.companyId).all<Row>()).results; },
        periodExists: async (facts) => { const { parsed, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=?").bind(parsed.periodId, actor.companyId).first(); },
        policyApproved: async (facts) => { const { parsed, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_calculation_policies WHERE id=? AND company_id=? AND version=? AND status='approved'").bind(parsed.policyId, actor.companyId, parsed.policyVersion).first(); },
        commitPreparedCalculation: async (facts) => {
            const { actor, key, parsed, runId, output, sourceHash, result, explanation, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.calculation.prepare", parsed, runId, 1, output, [
                write.prepare("INSERT INTO finance_calculation_runs(id,company_id,holding_id,kind,policy_id,policy_version,period_id,currency,currency_scale,source_hash,input_json,result_json,explanation_json,status,version,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'prepared',1,?,?,?)").bind(runId, actor.companyId, actor.holdingId, parsed.kind, parsed.policyId ?? null, parsed.policyVersion, parsed.periodId ?? null, parsed.currency, parsed.currencyScale, sourceHash, JSON.stringify(parsed.input), JSON.stringify(result), JSON.stringify(explanation), actor.userId, now, now),
                write.prepare("INSERT INTO finance_calculation_events VALUES(?,?,1,'prepare',?,'Prepared deterministic calculation',?,?)").bind(crypto.randomUUID(), runId, actor.userId, await digest(result), now),
            ]);
        },
        run: async (facts) => { const { runId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?").bind(runId, actor.companyId).first<Row>(); },
        commitCalculationDecision: async (facts) => {
            const { actor, key, operation, input, runId, version, output, run, action, next, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, runId, version, output, [
                assertion(write, key, "run-version", "EXISTS(SELECT 1 FROM finance_calculation_runs WHERE id=? AND company_id=? AND status=? AND version=?)", [runId, actor.companyId, run.status, run.version]),
                write.prepare(`UPDATE finance_calculation_runs SET status=?,version=version+1,updated_at=?,${action === "review" ? "reviewed_by" : "approved_by"}=? WHERE id=?`).bind(next, now, actor.userId, runId),
                write.prepare("INSERT INTO finance_calculation_events VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(), runId, version, action, actor.userId, input.reason, await digest(JSON.parse(String(run.result_json))), now),
            ]);
        },
        runDetail: async (facts) => { const { runId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND company_id=?").bind(runId, actor.companyId).first<Row>(); },
        events: async (facts) => { const { runId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_calculation_events WHERE run_id=? ORDER BY version").bind(runId).all<Row>()).results; },
        period: async (facts) => { const { periodId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<Row>(); },
        commitCloseSnapshot: async (facts) => { const { actor, key, periodId, period, output, values, sourceVersion, now } = facts; const write = await db(actor); return executeCommand(write, actor, key, "finance.close.check", { periodId }, periodId, Number(period.close_version), output, values.map(check => write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(), actor.companyId, periodId, check.key, sourceVersion, check.status, check.count, JSON.stringify(check), now, actor.userId))); } });
}
