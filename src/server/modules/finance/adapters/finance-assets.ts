import { financeOwnerDecisionEffects } from "../../workflow/public/finance-owner-decision-effects";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { createAssets } from "../application/assets";
import { assetSchema, assetEditSchema, assetActivationSchema, assetDepreciationSchema, assetDecisionSchema, assetDisposalSchema, type Asset, type Disposal } from "../domain/assets";
export { assetSchema, assetEditSchema, assetActivationSchema, assetDepreciationSchema, assetDecisionSchema, assetDisposalSchema } from "../domain/assets";
function assetGuards(write: SqlDatabase, actor: WorkContext, id: string, asset: Asset) {
    return [
        write.prepare("SELECT id FROM finance_assets WHERE id=? AND company_id=? FOR UPDATE").bind(asset.id, actor.companyId),
        assertion(write, id, "asset-version", "EXISTS(SELECT 1 FROM finance_assets WHERE id=? AND company_id=? AND version=? AND status=?)", [asset.id, actor.companyId, asset.version, asset.status]),
    ];
}
function event(write: SqlDatabase, actor: WorkContext, key: string, assetId: string, assetVersion: number, type: string, payload: unknown, createdAt: number) {
    return write.prepare("INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`asset-event-${key}`, assetId, actor.companyId, type, assetVersion, key, JSON.stringify(payload), actor.userId, createdAt);
}
export async function assetDetail(actor: WorkContext, id: string) { return nativeAssets().assetDetail(actor, id); }
export async function assetPage(actor: WorkContext) { return nativeAssets().assetPage(actor); }
export async function assetOptions(actor: WorkContext) { return nativeAssets().assetOptions(actor); }
export async function createAsset(actor: WorkContext, input: z.infer<typeof assetSchema>, key: string) { return nativeAssets().createAsset(actor, input, key); }
export async function editAsset(actor: WorkContext, id: string, input: z.infer<typeof assetEditSchema>, key: string) { return nativeAssets().editAsset(actor, id, input, key); }
export async function cancelDraftAsset(actor: WorkContext, id: string, input: z.infer<typeof assetDecisionSchema>, key: string) { return nativeAssets().cancelDraftAsset(actor, id, input, key); }
export async function activateAsset(actor: WorkContext, id: string, input: z.infer<typeof assetActivationSchema>, key: string) { return nativeAssets().activateAsset(actor, id, input, key); }
export async function depreciateAsset(actor: WorkContext, id: string, input: z.infer<typeof assetDepreciationSchema>, key: string) { return nativeAssets().depreciateAsset(actor, id, input, key); }
export async function prepareAssetDisposal(actor: WorkContext, id: string, input: z.infer<typeof assetDisposalSchema>, key: string) { return nativeAssets().prepareAssetDisposal(actor, id, input, key); }
export async function decideAssetDisposal(actor: WorkContext, id: string, action: "post" | "cancel", input: z.infer<typeof assetDecisionSchema>, key: string) { return nativeAssets().decideAssetDisposal(actor, id, action, input, key); }
function nativeAssets() {
    let pending: Promise<SqlDatabase> | undefined, owned: SqlDatabase | undefined;
    const database = (actor: WorkContext) => owned ? Promise.resolve(owned) : pending ??= holdingResources(actor.holdingId).then(r => r.write);
    const journals = new Map<string, Awaited<ReturnType<typeof prepareJournal>>>(), moneyPlans = new Map<string, Awaited<ReturnType<typeof snapshotMoney>>>(), policies = new Map<string, Awaited<ReturnType<typeof import("@/server/modules/finance/public/finance-asset-policies")["prepareAssetPolicyDisposal"]>>>();
    let decisionSteps: SqlStatement[] = [];
    return createAssets({ error: (...args) => new ApiError(...args), permission: requirePermission, now: Date.now, id: () => crypto.randomUUID(), hash: sha256, ready: async (actor) => { await database(actor); }, transaction: async <Result>(actor: WorkContext, work: () => Promise<Result>) => financeTransaction(await database(actor), async (tx) => { owned = tx; try {
            return await work();
        }
        finally {
            owned = undefined;
        } }), replay: async (actor, key, op, id, input) => replayCommand(await database(actor), actor, key, op, id, input),
        asset: async (actor, id) => (await database(actor)).prepare("SELECT * FROM finance_assets WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Asset>(),
        accountRows: async (actor, ids) => (await (await database(actor)).prepare("SELECT id,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND id IN (?,?,?)").bind(actor.companyId, ids.cost, ids.accumulated ?? null, ids.expense ?? null).all<{
            id: string;
            chart_id: string;
            category: string;
        }>()).results,
        secondaryChart: async (actor, chart) => !!await (await database(actor)).prepare("SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'").bind(actor.companyId, chart).first(),
        events: async (actor, id) => (await (await database(actor)).prepare("SELECT * FROM finance_asset_events WHERE asset_id=? ORDER BY created_at,id").bind(id).all()).results,
        depreciations: async (actor, id) => (await (await database(actor)).prepare("SELECT * FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal").bind(id).all()).results,
        disposals: async (actor, id) => (await (await database(actor)).prepare("SELECT * FROM finance_asset_disposals WHERE asset_id=? ORDER BY created_at,id").bind(id).all()).results,
        page: async (actor) => (await (await database(actor)).prepare("SELECT * FROM finance_assets WHERE company_id=? ORDER BY asset_number,id").bind(actor.companyId).all()).results,
        options: async (actor) => { const write = await database(actor), [accounts, periods, dimensions, dimensionValues] = await Promise.all([write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.category,a.status,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ORDER BY a.code,a.id").bind(actor.companyId).all(), write.prepare("SELECT id,name,start_date,end_date,state FROM finance_periods WHERE company_id=? ORDER BY end_date DESC,id").bind(actor.companyId).all(), write.prepare("SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all(), write.prepare("SELECT dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all()]); return { accounts: accounts.results, periods: periods.results, dimensions: dimensions.results, dimensionValues: dimensionValues.results }; },
        reviewedPolicy: async (actor, id) => !!await (await database(actor)).prepare("SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status IN ('active','superseded')").bind(id).first(),
        pendingDisposal: async (actor, id) => !!await (await database(actor)).prepare("SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft'").bind(id).first(),
        depreciationHistory: async (actor, id) => (await (await database(actor)).prepare("SELECT ordinal,amount_minor FROM finance_asset_depreciations WHERE asset_id=? ORDER BY ordinal").bind(id).all<{
            ordinal: number;
            amount_minor: number;
        }>()).results,
        latestDepreciation: async (actor, id) => (await (await database(actor)).prepare("SELECT MAX(entry_date) date FROM finance_asset_depreciations WHERE asset_id=?").bind(id).first<{
            date: string | null;
        }>())?.date ?? null,
        disposalAccounts: async (actor, chart, input) => (await (await database(actor)).prepare("SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (?,?,?)").bind(actor.companyId, chart, input.proceedsAccountId ?? null, input.gainAccountId, input.lossAccountId).all<{
            id: string;
            category: string;
        }>()).results,
        disposal: async (actor, id) => (await database(actor)).prepare("SELECT * FROM finance_asset_disposals WHERE id=? AND company_id=?").bind(id, actor.companyId).first<Disposal>(),
        retainedPolicyHash: async (actor, id) => (await (await database(actor)).prepare("SELECT policy_source_hash FROM finance_asset_disposals WHERE id=?").bind(id).first<{
            policy_source_hash: string | null;
        }>())?.policy_source_hash ?? null,
        preparePolicyDisposal: async (actor, assetId, input, key, status) => { const plan = await (await import("@/server/modules/finance/public/finance-asset-policies")).prepareAssetPolicyDisposal(await database(actor), actor, assetId, input, key, status); policies.set(input.id, plan); return { hash: plan.hash, dimensions: plan.dimensions, journalIds: plan.journalIds }; },
        prepareJournal: async (actor, input, key) => { const plan = await prepareJournal(await database(actor), actor, input, key, "posted"); journals.set(plan.result.id, plan); return { result: { id: plan.result.id } }; },
        prepareMoney: async (actor, input) => { moneyPlans.set(input.amountKind, await snapshotMoney(await database(actor), actor, input)); },
        prepareDecision: async (actor, id, key, asset, disposal) => { const write = await database(actor); decisionSteps = [...assetGuards(write, actor, key, asset), write.prepare("SELECT id FROM finance_asset_disposals WHERE id=? FOR UPDATE").bind(id), assertion(write, key, "disposal-version", "EXISTS(SELECT 1 FROM finance_asset_disposals WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, actor.companyId, disposal.version])]; },
        commitCreate: async ({ actor, input, key, id, amount, now, result }) => {
            const write = await database(actor);
            const money = moneyPlans.get("cost")!;
            return executeCommand(write, actor, key, "finance.asset.create", input, id, 1, result, [
                write.prepare("INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,depreciation_account_id,depreciation_expense_account_id,currency,cost_minor,residual_minor,useful_life_months,in_service_date,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(id, actor.companyId, input.assetNumber, input.name, input.accountId, input.depreciationAccountId ?? null, input.depreciationExpenseAccountId ?? null, input.currency, amount.cost, amount.residual, input.usefulLifeMonths, input.inServiceDate, actor.userId, now, now),
                money.statement, event(write, actor, key, id, 1, "created", input, now),
            ]);
        },
        commitEdit: async ({ actor, input, key, id, asset, amount, next, now, result }) => {
            const write = await database(actor);
            const money = moneyPlans.get("cost")!;
            return executeCommand(write, actor, key, "finance.asset.edit", input, id, next, result, [...assetGuards(write, actor, key, asset),
                write.prepare("UPDATE finance_assets SET asset_number=?,name=?,account_id=?,depreciation_account_id=?,depreciation_expense_account_id=?,currency=?,cost_minor=?,residual_minor=?,useful_life_months=?,in_service_date=?,version=?,updated_at=? WHERE id=?").bind(input.assetNumber, input.name, input.accountId, input.depreciationAccountId ?? null, input.depreciationExpenseAccountId ?? null, input.currency, amount.cost, amount.residual, input.usefulLifeMonths, input.inServiceDate, next, now, id),
                money.statement, event(write, actor, key, id, next, "edited", input, now),
            ]);
        },
        commitCancel: async ({ actor, input, key, id, asset, now, result }) => {
            const write = await database(actor);
            return executeCommand(write, actor, key, "finance.asset.cancel", input, id, result.version, result, [...assetGuards(write, actor, key, asset), write.prepare("UPDATE finance_assets SET status='retired',version=?,updated_at=? WHERE id=?").bind(result.version, now, id), event(write, actor, key, id, result.version, "cancelled", input, now)]);
        },
        commitActivate: async ({ actor, input, key, id, asset, next, now, result, journalId, decisionId, decisionHash }) => {
            const write = await database(actor);
            const journal = journals.get(journalId)!;
            return executeCommand(write, actor, key, "finance.asset.activate", input, id, next, result, [...assetGuards(write, actor, key, asset), ...journal.steps, write.prepare("UPDATE finance_assets SET status='in_service',activation_journal_id=?,activated_by=?,version=?,updated_at=? WHERE id=?").bind(journal.result.id, actor.userId, next, now, id), event(write, actor, key, id, next, "activated", { ...input, journalId: journal.result.id }, now), financeOwnerDecisionEffects(write).appendReviewedDecision({id:decisionId,commandId:key,recordId:id,action:"activate",actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:asset.version,inputDigest:decisionHash,createdAt:now})]);
        },
        commitDepreciation: async ({ actor, input, key, id, asset, next, index, amount, target, now, result, journalId }) => {
            const write = await database(actor);
            const journal = journals.get(journalId)!, money = moneyPlans.get("depreciation")!;
            return executeCommand(write, actor, key, "finance.asset.depreciate", input, id, next, result, [...assetGuards(write, actor, key, asset), assertion(write, key, "asset-policy-owner", "NOT EXISTS(SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status IN ('active','superseded'))", [id]), assertion(write, key, "asset-disposal", "NOT EXISTS(SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft')", [id]), ...journal.steps, money.statement, write.prepare("INSERT INTO finance_asset_depreciations(id,asset_id,company_id,depreciation_month,ordinal,period_id,entry_date,amount_minor,journal_id,asset_version,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(`asset-depreciation-${key}`, id, actor.companyId, input.entryDate.slice(0, 7), index + 1, input.periodId, input.entryDate, amount, journal.result.id, next, key, now), write.prepare("UPDATE finance_assets SET accumulated_depreciation_minor=?,version=?,updated_at=? WHERE id=?").bind(Number(target), next, now, id), event(write, actor, key, id, next, "depreciated", { ...input, amountMinor: amount, journalId: journal.result.id }, now)]);
        },
        commitPrepareDisposal: async ({ actor, input, key, id, asset, disposalId, amount, now, result, policy }) => {
            const write = await database(actor);
            return executeCommand(write, actor, key, "finance.asset.disposal.prepare", input, disposalId, 1, result, [...assetGuards(write, actor, key, asset), write.prepare("INSERT INTO finance_asset_disposals(id,asset_id,company_id,asset_version,period_id,entry_date,proceeds_minor,proceeds_account_id,gain_account_id,loss_account_id,evidence_ref,created_by,created_at,updated_at,policy_source_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(disposalId, id, actor.companyId, asset.version, input.periodId, input.entryDate, amount, input.proceedsAccountId ?? null, input.gainAccountId, input.lossAccountId, input.evidenceRef, actor.userId, now, now, policy.hash), event(write, actor, key, id, asset.version, "disposal_prepared", { ...input, disposalId }, now)]);
        },
        commitDisposal: async ({ actor, id, action, input, key, operation, disposal, asset, journalId, secondaryJournalIds, now, result, decisionId, decisionHash }) => {
            const write = await database(actor), steps = decisionSteps;
            if (action === "post") {
                const journal = journals.get(journalId!)!, policy = policies.get(id)!, money = moneyPlans.get("proceeds")!;
                steps.push(...journal.steps, ...policy.steps, money.statement, write.prepare("UPDATE finance_assets SET status='retired',version=version+1,updated_at=? WHERE id=?").bind(now, asset.id));
            }
            steps.push(write.prepare("UPDATE finance_asset_disposals SET status=?,journal_id=?,decided_by=?,version=?,updated_at=? WHERE id=?").bind(result.status, journalId, actor.userId, result.version, now, id), event(write, actor, key, asset.id, result.assetVersion, action === "post" ? "disposed" : "disposal_cancelled", { ...input, disposalId: id, journalId, secondaryJournalIds }, now), financeOwnerDecisionEffects(write).appendReviewedDecision({id:decisionId,commandId:key,recordId:id,action:action,actorId:actor.userId,assignmentId:actor.assignmentId,reviewedVersion:disposal.version,inputDigest:decisionHash,createdAt:now}));
            return executeCommand(write, actor, key, operation, input, id, result.version, result, steps);
        }
    });
}
