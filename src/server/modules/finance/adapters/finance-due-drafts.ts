import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { HoldingId } from "@/server/modules/runtime/public/runtime";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import { businessBillingSchema, recognitionRunSchema, prepareBusinessBilling, prepareRecognitionRun } from "@/server/modules/finance/public/finance-business-contracts";
import { assetPolicyRunSchema, prepareAssetPolicyRun } from "@/server/modules/finance/public/finance-asset-policies";
import { instrumentRunSchema, prepareInstrumentRun } from "@/server/modules/finance/public/finance-instruments";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import { createFinanceDueDrafts } from "../application/due-drafts";
import { financeDueDraftSchema, financeDueDecisionSchema, type Kind, type Row } from "../domain/due-drafts";
import { financeDelegatedAssignment, financeDueAssignment, financeDueAuthority } from "@/server/modules/identity_access/public/finance-delegation";
export { financeDueDraftSchema, financeDueDecisionSchema } from "../domain/due-drafts";
const sources: Record<Kind, string> = { billing: "finance_business_contracts", recognition: "finance_business_billing_runs", asset_depreciation: "finance_asset_policy_editions", instrument_payment: "finance_instrument_editions" };
export async function financeDueWorkspace(actor: WorkContext) { return nativeDue().financeDueWorkspace(actor); }
export async function scheduleFinanceDraft(actor: WorkContext, raw: z.input<typeof financeDueDraftSchema>, key: string) { return nativeDue().scheduleFinanceDraft(actor, raw, key); }
export async function decideFinanceDueDraft(actor: WorkContext, id: string, raw: z.input<typeof financeDueDecisionSchema>, action: "retry" | "cancel", key: string) { return nativeDue().decideFinanceDueDraft(actor, id, raw, action, key); }
export async function drainFinanceDueDrafts(write: SqlDatabase, holdingId: HoldingId, now = new Date()): Promise<number> { return nativeDue(write).drainFinanceDueDrafts(holdingId, now); }
function nativeDue(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined;
    const database = () => owned ? Promise.resolve(owned) : provided ? Promise.resolve(provided) : pending!;
    return createFinanceDueDrafts({ error: (...args) => new ApiError(...args), businessError: error => error instanceof ApiError ? { code: error.code, message: error.message } : null, permission: requirePermission, now: Date.now, hash: sha256, ready: async (actor) => { if (!provided)
            await (pending ??= holdingResources(actor.holdingId).then(r => r.write)); }, transaction: async <Result>(work: () => Promise<Result>) => financeTransaction(await database(), async (tx) => { owned = tx; try {
            return await work();
        }
        finally {
            owned = undefined;
        } }), replay: async (actor, key, op, id, input) => replayCommand(await database(), actor, key, op, id, input), workspace: async (actor) => (await (await database()).prepare("SELECT * FROM finance_due_drafts WHERE company_id=? ORDER BY run_on DESC,created_at DESC LIMIT 500").bind(actor.companyId).all<Row>()).results, source: async (actor, input) => (await database()).prepare(`SELECT * FROM ${sources[input.kind]} WHERE id=? AND company_id=?`).bind(input.sourceId, actor.companyId).first<Row>(), assignment: async (actor, now) => financeDelegatedAssignment(await database(), actor, now), dueAssignment: async (row, now) => financeDueAssignment(await database(), row, now), authority: async (row) => financeDueAuthority(await database(), row),
        commitSchedule: async ({ actor, input, id, op, key, sourceVersion, assignmentVersion, bytes, hash, now }) => { const db = await database(); return executeCommand(db, actor, key, op, input, id, 1, { id, status: "scheduled", version: 1 }, [db.prepare("INSERT INTO finance_due_drafts(id,company_id,holding_id,kind,source_id,run_on,source_version,input_json,input_hash,delegated_by,assignment_id,assignment_version,role,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, actor.holdingId, input.kind, input.sourceId, input.runOn, sourceVersion, bytes, hash, actor.userId, actor.assignmentId, assignmentVersion, actor.role, input.reason, now, now)]); }, row: async (actor, id) => (await database()).prepare("SELECT * FROM finance_due_drafts WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Row>(), commitDecision: async ({ actor, id, input, op, key, status, now }) => { const db = await database(); return executeCommand(db, actor, key, op, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [assertion(db, key, "schedule-version", "EXISTS(SELECT 1 FROM finance_due_drafts WHERE id=? AND company_id=? AND version=? AND status IN ('scheduled','blocked'))", [id, actor.companyId, input.expectedVersion]), db.prepare("UPDATE finance_due_drafts SET status=?,version=version+1,next_attempt_at=0,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?").bind(status, now, id)]); },
        candidates: async (holdingId, today, time) => (await (await database()).prepare("SELECT id FROM finance_due_drafts WHERE holding_id=? AND status IN ('scheduled','blocked') AND run_on<=? AND next_attempt_at<=? ORDER BY run_on,id LIMIT 25").bind(holdingId, today, time).all<{
            id: string;
        }>()).results, claim: async (id, time) => (await database()).prepare("SELECT * FROM finance_due_drafts WHERE id=? AND status IN ('scheduled','blocked') AND next_attempt_at<=? FOR UPDATE SKIP LOCKED").bind(id, time).first<Row>(), prepareBilling: async (actor, id, input, key) => prepareBusinessBilling(actor, id, input as z.input<typeof businessBillingSchema>, key, await database()), prepareRecognition: async (actor, input, key) => prepareRecognitionRun(actor, input as z.input<typeof recognitionRunSchema>, key, await database()), prepareAsset: async (actor, id, input, key) => prepareAssetPolicyRun(actor, id, input as z.input<typeof assetPolicyRunSchema>, key, await database()), prepareInstrument: async (actor, id, input, key) => prepareInstrumentRun(actor, id, input as z.input<typeof instrumentRunSchema>, key, await database()), complete: async (row, result, time) => { await (await database()).prepare("UPDATE finance_due_drafts SET status='prepared',version=version+1,result_json=?,attempts=attempts+1,last_code=NULL,last_message=NULL,updated_at=? WHERE id=?").bind(JSON.stringify(result), time, row.id).run(); }, block: async (id, code, message, time) => { await (await database()).prepare("UPDATE finance_due_drafts SET status='blocked',version=version+1,attempts=attempts+1,last_code=?,last_message=?,next_attempt_at=?,updated_at=? WHERE id=? AND status IN ('scheduled','blocked')").bind(code, message, time + 3600000, time, id).run(); } });
}
