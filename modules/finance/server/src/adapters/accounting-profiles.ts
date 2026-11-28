import { versionStatusGuardSql } from './finance-read-tail-r52-sql';
import { tableByIdSql } from './finance-read-tail-r49-sql';
import { financeAccountingProfiles } from '@erpx/collaboration-server/adapters/native-finance-accounting-profiles';
import { withFinancePurchasedRead, withFinanceConfigurationCommand } from "@erpx/finance-server/adapters/purchase-boundary";
import { ACTIVE_CHART_BASE_CURRENCY_SQL } from "./chart-base-currency-dedupe-sql";
import { CHART_EXISTS_BY_COMPANY_SQL } from './finance-read-tail-r36-sql';
import { ACTIVE_THB_CHART_GUARD_SQL, NO_APPROVED_PROFILE_GUARD_SQL } from './finance-read-tail-r45-sql';
import { financeOwnerDecisionEffects } from "@erpx/workflow-server/public/finance-owner-decision-effects";
import { createAccountingProfiles } from '@erpx/finance-server/application/accounting-profiles';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { createHash } from "node:crypto";
import { z } from "zod";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
export { accountingProfileSchema, accountingProfileDecisionSchema, type AccountingProfile } from '@erpx/finance-server/domain/accounting-profiles';
import { accountingProfileSchema, accountingProfileDecisionSchema, type AccountingProfile } from '@erpx/finance-server/domain/accounting-profiles';
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
            return write.prepare(ACTIVE_CHART_BASE_CURRENCY_SQL).bind(input.statutoryChartId, actor.companyId).first<{
                base_currency: string;
            }>();
        }, approvedExists: async (facts) => { const { actor } = facts; const write = await db(actor); return !!await write.prepare("SELECT 1 FROM finance_accounting_profiles WHERE company_id=? AND status='approved'").bind(actor.companyId).first(); }, commitPrepareProfile: async (facts) => {
            const { actor, key, input, id } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, "finance.profile.prepare", input, id, 1, { id, status: "draft", version: 1 }, [
                assertion(write, key, "ledger", ACTIVE_THB_CHART_GUARD_SQL, [input.statutoryChartId, actor.companyId]),
                assertion(write, key, "profile", NO_APPROVED_PROFILE_GUARD_SQL, [actor.companyId]),
                await financeAccountingProfiles(write, actor.companyId).create({ id, companyId: actor.companyId, statutoryChartId: input.statutoryChartId, standard: input.standard, fiscalStartMonth: input.fiscalStartMonth, effectiveDate: input.effectiveDate, eligibilityEvidence: input.eligibilityEvidence, policyEvidence: input.policyEvidence, taxBasis: input.taxBasis, inventoryPolicy: input.inventoryPolicy, depreciationPolicy: input.depreciationPolicy, userId: actor.userId, now: Date.now() }),
            ]);
        }, profile: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare(tableByIdSql('finance_accounting_profiles')).bind(id, actor.companyId).first<AccountingProfile>(); }, commitProfileDecision: async (facts) => {
            const { actor, key, operation, input, id, status, decision, profile, now } = facts;
            const write = await db(actor);
            return executeCommand(write, actor, key, operation, input, id, input.expectedVersion + 1, { id, status, version: input.expectedVersion + 1 }, [
                assertion(write, key, "profile", versionStatusGuardSql('finance_accounting_profiles', "'draft'", ''), [id, actor.companyId, input.expectedVersion]),
                ...(decision === "approve" ? [
                    assertion(write, key, "ledger", ACTIVE_THB_CHART_GUARD_SQL, [profile.statutory_chart_id, actor.companyId]),
                    assertion(write, key, "single-statutory-ledger", NO_APPROVED_PROFILE_GUARD_SQL, [actor.companyId]),
                ] : []),
                await financeAccountingProfiles(write, actor.companyId).decide({ status, userId: actor.userId, reason: input.reason, now, id, companyId: actor.companyId }),
                financeOwnerDecisionEffects(write).appendRetainedLayoutDecision({id:crypto.randomUUID(),commandId:key,recordId:id,action:operation,actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:input.expectedVersion,inputDigest:createHash("sha256").update(JSON.stringify(input)).digest("hex"),createdAt:now}),
            ]);
        }, chartExists: async (facts) => { const { query, actor } = facts; const write = await db(actor); return !!await write.prepare(CHART_EXISTS_BY_COMPANY_SQL).bind(query.chartId, actor.companyId).first(); }, events: async (facts) => {
            const { actor, query } = facts;
            const write = await db(actor);
            return (await write.prepare("SELECT * FROM finance_financial_events WHERE company_id=? AND chart_id=? AND (?::text IS NULL OR effective_date>=?) AND (?::text IS NULL OR effective_date<=?) ORDER BY effective_date,id LIMIT 1000")
                .bind(actor.companyId, query.chartId, query.startDate ?? null, query.startDate ?? null, query.endDate ?? null, query.endDate ?? null).all<AccountingProfile>()).results;
        } });
}
