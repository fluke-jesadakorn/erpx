import { tableByIdSql } from './finance-read-tail-r49-sql';
import { columnByIdSql } from './finance-read-tail-r53-sql';
import {materializedClockCteSql} from './finance-read-tail-r19-sql';
import { COMMAND_RECEIPT_EXISTS_SQL } from './finance-read-tail-r15-sql';
import { withFinancePurchasedCommand, withFinancePurchasedRead, requireFinanceCurrentAuthority } from "@erpx/server-finance/adapters/purchase-boundary";
import { financeDueDrafts, financeDueDraftsWorker } from '@erpx/server-collaboration/adapters/native-finance-due-drafts';
import { financePackageEnforcementMode, holdingPurchaseReadLocks, checkPurchasedCapability } from "@erpx/server-commercial/public/purchase-command-boundary";
import { purchasedCapabilityPredicate, packageClockSql } from "@erpx/server-commercial/public/purchased-capability-sql";
import { flushFinanceDeferredEffects } from "@erpx/server-finance/adapters/finance-transactions";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { HoldingId } from "../../../../packages/server-runtime/src/public/runtime";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import { businessBillingSchema, recognitionRunSchema, prepareBusinessBilling, prepareRecognitionRun } from "@erpx/server-finance/public/finance-business-contracts";
import { assetPolicyRunSchema, prepareAssetPolicyRun } from "@erpx/server-finance/public/finance-asset-policies";
import { instrumentRunSchema, prepareInstrumentRun } from "@erpx/server-finance/public/finance-instruments";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createFinanceDueDrafts } from "@erpx/server-finance/application/due-drafts";
import { financeDueDraftSchema, financeDueDecisionSchema, type Kind, type Row } from "@erpx/server-finance/domain/due-drafts";
import { financeDelegatedAssignment, financeDueAssignment, financeDueAuthority } from "@erpx/server-identity_access/public/finance-delegation";
export { financeDueDraftSchema, financeDueDecisionSchema } from "@erpx/server-finance/domain/due-drafts";
const sources: Record<Kind, string> = { billing: "finance_business_contracts", recognition: "finance_business_billing_runs", asset_depreciation: "finance_asset_policy_editions", instrument_payment: "finance_instrument_editions" };
export async function financeDueWorkspace(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>nativeDue(db).financeDueWorkspace(actor)); }
export async function scheduleFinanceDraft(actor: WorkContext, raw: z.input<typeof financeDueDraftSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.due-draft.schedule",purpose:"settle-existing",capability:raw.kind==="instrument_payment"?"finance.advanced":"finance.accounting"},tx=>nativeDue(tx,true).scheduleFinanceDraft(actor,raw,key)); }
export async function decideFinanceDueDraft(actor: WorkContext, id: string, raw: z.input<typeof financeDueDecisionSchema>, action: "retry" | "cancel", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.due-draft.${action}`,purpose:action==="cancel"?"history":"settle-existing",capability:"finance.accounting",companyPurchases:async db=>{const row=await db.prepare(columnByIdSql('kind', 'finance_due_drafts')).bind(id,actor.companyId).first<{kind:string}>();return row?.kind==="instrument_payment"?[{companyId:actor.companyId,capability:"finance.advanced",purpose:action==="cancel"?"history" as const:"settle-existing" as const}]:[];}},tx=>nativeDue(tx,true).decideFinanceDueDraft(actor,id,raw,action,key)); }
export async function drainFinanceDueDrafts(write: SqlDatabase, holdingId: HoldingId, now = new Date()): Promise<number> { return nativeDue(write).drainFinanceDueDrafts(holdingId, now); }
function nativeDue(provided?: SqlDatabase, providedOwned=false) {
    let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined;
    const database = () => owned ? Promise.resolve(owned) : provided ? Promise.resolve(provided) : pending!;
    let completions: {actor:WorkContext;capability:string;purpose:"history"|"settle-existing"}[]=[];
    const purchaseSql = async (purpose:"history"|"settle-existing") => await financePackageEnforcementMode()!=="enforced" ? "TRUE" : `((d.kind='instrument_payment' AND ${purchasedCapabilityPredicate("d.company_id","finance.advanced",purpose,"finance_due_purchase_clock.now_ms")}) OR (d.kind<>'instrument_payment' AND ${purchasedCapabilityPredicate("d.company_id","finance.accounting",purpose,"finance_due_purchase_clock.now_ms")}))`;
    const prepareDue = async <Result>(actor:WorkContext,key:string,operation:string,capability:string,run:(database:SqlDatabase)=>Promise<Result>) => {
      const db=await database();
      if(await financePackageEnforcementMode()!=="disabled"){
        const prior=await db.prepare(COMMAND_RECEIPT_EXISTS_SQL).bind(key,operation,actor.userId,actor.companyId).first();
        completions.push({actor,capability,purpose:prior?"history":"settle-existing"});
      }
      return withFinancePurchasedCommand(actor,{key,operation,purpose:"settle-existing",capability},tx=>run(tx??db),db);
    };
    return createFinanceDueDrafts({ error: (...args) => new ApiError(...args), businessError: error => error instanceof ApiError ? { code: error.code, message: error.message } : null, permission: requirePermission, now: Date.now, hash: sha256, ready: async (actor) => { if (!provided)
            await (pending ??= holdingResources(actor.holdingId).then(r => r.write)); }, transaction: async <Result>(work: () => Promise<Result>) => providedOwned ? work() : financeTransaction(await database(), async (tx) => { owned = tx;completions=[]; try {
            if(await financePackageEnforcementMode()!=="disabled")await tx.batch(holdingPurchaseReadLocks(tx));
            const result=await work();
            if(completions.length){await flushFinanceDeferredEffects(tx);for(const completion of completions){await requireFinanceCurrentAuthority(tx,completion.actor,`finance-due-final:${crypto.randomUUID()}`);await checkPurchasedCapability(tx,await financePackageEnforcementMode(),{companyId:completion.actor.companyId,capability:completion.capability,purpose:completion.purpose});}}
            return result;
        }
        finally {
            owned = undefined;completions=[];
        } }), replay: async (actor, key, op, id, input) => replayCommand(await database(), actor, key, op, id, input), workspace: async (actor) => (await (await database()).prepare(`${materializedClockCteSql('finance_due_purchase_clock', packageClockSql, true, true)} SELECT d.* FROM finance_due_drafts d CROSS JOIN finance_due_purchase_clock WHERE d.company_id=? AND ${await purchaseSql("history")} ORDER BY run_on DESC,created_at DESC LIMIT 500`).bind(actor.companyId).all<Row>()).results, source: async (actor, input) => (await database()).prepare(tableByIdSql(sources[input.kind])).bind(input.sourceId, actor.companyId).first<Row>(), assignment: async (actor, now) => financeDelegatedAssignment(await database(), actor, now), dueAssignment: async (row, now) => financeDueAssignment(await database(), row, now), authority: async (row) => financeDueAuthority(await database(), row),
        commitSchedule: async ({ actor, input, id, op, key, sourceVersion, assignmentVersion, bytes, hash, now }) => { const db = await database(); return executeCommand(db, actor, key, op, input, id, 1, { id, status: "scheduled", version: 1 }, [await financeDueDrafts(db, actor.companyId).schedule({ id, companyId: actor.companyId, holdingId: actor.holdingId, kind: input.kind, sourceId: input.sourceId, runOn: input.runOn, sourceVersion, inputJson: bytes, inputHash: hash, userId: actor.userId, assignmentId: actor.assignmentId, assignmentVersion, role: actor.role, reason: input.reason, now })]); }, row: async (actor, id) => (await database()).prepare(tableByIdSql('finance_due_drafts')).bind(id, actor.companyId).first<Row>(), commitDecision: async ({ actor, id, input, op, key, status, now }) => { const db = await database(); return executeCommand(db, actor, key, op, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [assertion(db, key, "schedule-version", "EXISTS(SELECT 1 FROM finance_due_drafts WHERE id=? AND company_id=? AND version=? AND status IN ('scheduled','blocked'))", [id, actor.companyId, input.expectedVersion]), await financeDueDrafts(db, actor.companyId).decide({ status, now, id })]); },
        candidates: async (holdingId, today, time) => (await (await database()).prepare(`${materializedClockCteSql('finance_due_purchase_clock', packageClockSql, true, true)} SELECT d.id FROM finance_due_drafts d CROSS JOIN finance_due_purchase_clock WHERE d.holding_id=? AND status IN ('scheduled','blocked') AND run_on<=? AND next_attempt_at<=? AND ${await purchaseSql("settle-existing")} ORDER BY run_on,id LIMIT 25`).bind(holdingId, today, time).all<{
            id: string;
        }>()).results, claim: async (id, time) => (await database()).prepare("SELECT * FROM finance_due_drafts WHERE id=? AND status IN ('scheduled','blocked') AND next_attempt_at<=? FOR UPDATE SKIP LOCKED").bind(id, time).first<Row>(), prepareBilling: async (actor, id, input, key) => prepareDue(actor,key,"finance.business-billing.prepare","finance.accounting",db=>prepareBusinessBilling(actor, id, input as z.input<typeof businessBillingSchema>, key, db)), prepareRecognition: async (actor, input, key) => prepareDue(actor,key,"finance.recognition.prepare","finance.accounting",db=>prepareRecognitionRun(actor, input as z.input<typeof recognitionRunSchema>, key, db)), prepareAsset: async (actor, id, input, key) => prepareDue(actor,key,"finance.asset-policy-run.prepare","finance.accounting",db=>prepareAssetPolicyRun(actor, id, input as z.input<typeof assetPolicyRunSchema>, key, db)), prepareInstrument: async (actor, id, input, key) => prepareDue(actor,key,"finance.instrument-run.prepare","finance.advanced",db=>prepareInstrumentRun(actor, id, input as z.input<typeof instrumentRunSchema>, key, db)), complete: async (row, result, time, holdingId) => { await financeDueDraftsWorker(await database(), holdingId).complete({ result, time, id: row.id }); }, block: async (id, code, message, time, holdingId) => { await financeDueDraftsWorker(await database(), holdingId).block({ code, message, time, id }); } });
}
