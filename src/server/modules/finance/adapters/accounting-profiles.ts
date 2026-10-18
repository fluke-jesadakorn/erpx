import { withFinancePurchasedRead, withFinanceConfigurationCommand } from "./purchase-boundary";
import { financeOwnerDecisionEffects } from "../../workflow/public/finance-owner-decision-effects";
import { createAccountingProfiles } from '../application/accounting-profiles';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { createHash } from "node:crypto";
import { z } from "zod";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
export { accountingProfileSchema, accountingProfileDecisionSchema, type AccountingProfile } from '../domain/accounting-profiles';
import { accountingProfileSchema, accountingProfileDecisionSchema, type AccountingProfile } from '../domain/accounting-profiles';
export async function accountingProfiles(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>profileService(db).accountingProfiles(actor),"finance.accounting",true); }
export async function prepareAccountingProfile(actor: WorkContext, input: z.input<typeof accountingProfileSchema>, key: string) { return withFinanceConfigurationCommand(actor,{key,operation:"finance.profile.prepare",purpose:"new-work"},tx=>profileService(tx).prepareAccountingProfile(actor,input,key)); }
export async function decideAccountingProfile(actor: WorkContext, id: string, decision: "approve" | "reject", input: z.input<typeof accountingProfileDecisionSchema>, key: string) { return withFinanceConfigurationCommand(actor,{key,operation:`finance.profile.${decision}`,purpose:"settle-existing"},tx=>profileService(tx).decideAccountingProfile(actor,id,decision,input,key)); }
export async function financialEvents(actor: WorkContext, query: {
    chartId: string;
    startDate?: string;
    endDate?: string;
}) { return withFinancePurchasedRead(actor,db=>profileService(db).financialEvents(actor,query)); }
function profileService(provided?: SqlDatabase) {
    let pending: Promise<SqlDatabase> | undefined = provided ? Promise.resolve(provided) : undefined;
    const db = (actor: WorkContext) => pending ??= holdingResources(actor.holdingId).then(r => r.write);
    return createAccountingProfiles({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, now: Date.now, replay: async <Result>(actor:WorkContext,key:string,operation:string,id:string,input:unknown) => replayCommand<Result>(await db(actor), actor, key, operation, id, input), profiles: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT p.*,c.name chart_name,c.base_currency FROM finance_accounting_profiles p JOIN finance_charts c ON c.id=p.statutory_chart_id WHERE p.company_id=? ORDER BY p.created_at DESC,p.id").bind(actor.companyId).all<AccountingProfile>()).results; }, activeChart: async (facts) => {
            const { input, actor } = facts;
            const write = await db(actor);
            return write.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.statutoryChartId, actor.companyId).first<{
                base_currency: string;
            }>();
        }, approvedExists: async (facts) => { const { actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved'").bind(actor.companyId).first(); }, commitPrepareProfile: async (facts) => {
            const { actor, key, input, id } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.profile.prepare", input, id, 1, { id, status: "draft", version: 1 }, [
                assertion(write, key, "ledger", "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')", [input.statutoryChartId, actor.companyId]),
                assertion(write, key, "profile", "NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')", [actor.companyId]),
                write.prepare("INSERT INTO finance_accounting_profiles(id,company_id,statutory_chart_id,standard,fiscal_start_month,effective_date,eligibility_evidence,policy_evidence,tax_basis,inventory_policy,depreciation_policy,prepared_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(id, actor.companyId, input.statutoryChartId, input.standard, input.fiscalStartMonth, input.effectiveDate, input.eligibilityEvidence, input.policyEvidence, input.taxBasis, input.inventoryPolicy, input.depreciationPolicy, actor.userId, Date.now()),
            ]);
        }, profile: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_accounting_profiles WHERE id=? AND company_id=?").bind(id, actor.companyId).first<AccountingProfile>(); }, commitProfileDecision: async (facts) => {
            const { actor, key, operation, input, id, status, decision, profile, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [
                assertion(write, key, "profile", "EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status='draft')", [id, actor.companyId, input.expectedVersion]),
                ...(decision === "approve" ? [
                    assertion(write, key, "ledger", "EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency='THB')", [profile.statutory_chart_id, actor.companyId]),
                    assertion(write, key, "single-statutory-ledger", "NOT EXISTS(SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved')", [actor.companyId]),
                ] : []),
                write.prepare("UPDATE finance_accounting_profiles SET status=?,reviewed_by=?,review_reason=?,reviewed_at=?,version=version+1 WHERE id=? AND company_id=?")
                    .bind(status, actor.userId, input.reason, now, id, actor.companyId),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:key,recordId:id,action:operation,actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:createHash("sha256").update(JSON.stringify(input)).digest("hex"),createdAt:now}),
            ]);
        }, chartExists: async (facts) => { const { query, actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_charts WHERE id=? AND company_id=?").bind(query.chartId, actor.companyId).first(); }, events: async (facts) => {
            const { actor, query } = facts;
            const write = await db(actor);
            return (await write.prepare("SELECT * FROM finance_financial_events WHERE company_id=? AND chart_id=? AND (?::text IS NULL OR effective_date>=?) AND (?::text IS NULL OR effective_date<=?) ORDER BY effective_date,id LIMIT 1000")
                .bind(actor.companyId, query.chartId, query.startDate ?? null, query.startDate ?? null, query.endDate ?? null, query.endDate ?? null).all<AccountingProfile>()).results;
        } });
}
