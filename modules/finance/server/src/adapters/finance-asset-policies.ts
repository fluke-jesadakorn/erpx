import { assetAccountJoinByIdSql } from './finance-read-tail-r59-sql';
import { versionStatusGuardSql } from './finance-read-tail-r52-sql';
import { financeReferenceListSql } from './finance-read-tail-r42-sql';
import { tableByIdSql } from './finance-read-tail-r49-sql';
import { ASSET_DRAFT_DISPOSAL_SQL } from './finance-read-tail-r39-sql';
import { idCompanyStatusVersionGuard } from './finance-read-tail-r20-sql';
import { ASSET_DEPRECIATION_COUNT_SQL, ASSET_POLICY_EDITION_PENDING_SQL, ASSET_POLICY_RUN_PENDING_SQL } from './finance-read-tail-r24-sql';
import { OPEN_PERIOD_COVERED_SQL, OPEN_PERIOD_LIST_ASC_SQL } from './finance-read-tail-r34-sql';
import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/finance-server/adapters/purchase-boundary";
import { nativeFinanceOwnerWriteSystem } from './native-finance-owner-write';
import { nativeFinanceReadAll, nativeFinanceReadFirst, FINANCE_RD_ASSET_POLICY_RULE, FINANCE_RD_ASSET_POLICY_MAPPINGS, FINANCE_RD_ASSET_POLICY_POSTED_RUNS, FINANCE_RD_ASSET_POLICY_LEGACY, FINANCE_RD_ASSET_POLICY_WORKSPACE_ASSETS, FINANCE_RD_ASSET_POLICY_WORKSPACE_BOOKS, FINANCE_RD_ASSET_POLICY_DISPOSAL, FINANCE_RD_ASSET_POLICY_GAIN_LOSS } from './native-finance-read';
import { assetPolicyRunSchema } from '@erpx/finance-server/domain/due-inputs';
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import { financeTransaction, financeOwnedTransaction } from "@erpx/finance-server/public/finance-transactions";
import { requireFinanceTool } from "@erpx/finance-server/public/finance-company-tools";
import {prepareJournal} from '@erpx/finance-server/public/journal-planning';
import { prepareOwnedJournalReversal } from "@erpx/finance-server/public/finance-owned-reversal";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { assetCarryingEvidence } from "@erpx/finance-server/public/finance-asset-carrying";
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { createAssetPolicyApplication, type AssetPolicyPorts, type AssetPolicyRepository } from '@erpx/finance-server/application/asset-policies';
import { assetPolicySchema, assetPolicyDecisionSchema, assetPolicyReversalSchema, type Row } from '@erpx/finance-server/domain/asset-policies';
export { assetPolicySchema, assetPolicyDecisionSchema, assetPolicyReversalSchema, assetPolicyRunSchema };
async function inward<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<T>(action: () => Promise<T>) { try {
    return await action();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
function bindPorts(read: () => SqlDatabase, base: () => SqlDatabase, owned: SqlDatabase | undefined, prepare: (holdingId: string) => Promise<void>): AssetPolicyPorts<SqlStatement> {
    const repository: AssetPolicyRepository<SqlStatement> = { async asset(id: string, companyId: string) {
            const db = read();
            return db
                .prepare(assetAccountJoinByIdSql(" AND a.company_id=?"))
                .bind(id, companyId)
                .first<Row>();
        }, async pendingDisposal(id: string) {
            const db = read();
            return db
                .prepare(ASSET_DRAFT_DISPOSAL_SQL)
                .bind(id)
                .first();
        }, async bookRules(companyId: string, chart_id: string | number | null, date: string) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_RULE, [companyId, chart_id, date], db);
        }, async bookMappings(id: string | number | null, account_id: string | number | null, depreciation_account_id: string | number | null, depreciation_expense_account_id: string | number | null) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_MAPPINGS, [id, account_id, depreciation_account_id, depreciation_expense_account_id], db);
        }, async bookAccounts(companyId: string, accountIds: string[]) {
            const db = read();
            return db
                .prepare(`SELECT a.id,a.version,a.category,a.control_kind,a.currency,c.status chart_status,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.id IN (${accountIds.map(() => "?").join(",")}) ORDER BY a.id`)
                .bind(companyId, ...accountIds)
                .all<Row>();
        }, async edition(id: string, companyId: string) {
            const db = read();
            return db
                .prepare(tableByIdSql('finance_asset_policy_editions'))
                .bind(id, companyId)
                .first<Row>();
        }, async legacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare(ASSET_DEPRECIATION_COUNT_SQL)
                .bind(id)
                .first<number>("n");
        }, async postedRuns(id: string | number | null) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_POSTED_RUNS, [id], db);
        }, async policyLegacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare(ASSET_DEPRECIATION_COUNT_SQL)
                .bind(id)
                .first<number>("n");
        }, async pendingRun(id: string | number | null) {
            const db = read();
            return db
                .prepare(ASSET_POLICY_RUN_PENDING_SQL)
                .bind(id)
                .first();
        }, async preparePolicy(id: string, companyId: string, assetId: string, value: string | number | null, value2: string, hash: string, userId: string, reason: string, now: number, now2: number, value3: string) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, companyId, 'prepare_policy_edition', [id, companyId, assetId, value, value2, hash, userId, reason, now, now2, value3]);
        }, async supersedePolicy(now: number, id: string | number | null) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'supersede_policy_edition', [now, id]);
        }, guardPolicyVersion(key: string, value: "asset-policy-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, versionStatusGuardSql('finance_asset_policy_editions', "'draft'", ''), [id, companyId, expectedVersion]);
        }, async decidePolicy(value: "active" | "rejected", userId: string, now: number, id: string) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'decide_policy_edition', [value, userId, now, id]);
        }, async openPeriod(periodId: string, companyId: string, entryDate: string, entryDate2: string) {
            const db = read();
            return db
                .prepare(OPEN_PERIOD_COVERED_SQL)
                .bind(periodId, companyId, entryDate, entryDate2)
                .first();
        }, async runLegacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare(ASSET_DEPRECIATION_COUNT_SQL)
                .bind(id)
                .first<number>("n");
        }, async prepareRun(runId: string, companyId: string, id: string | number | null, id2: string, ordinal: number, entryDate: string, value: string, value2: string, hash: string, userId: string, reason: string, now: number, now2: number) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, companyId, 'prepare_policy_run', [runId, companyId, id, id2, ordinal, entryDate, value, value2, hash, userId, reason, now, now2]);
        }, async run(id: string, companyId: string) {
            const db = read();
            return db
                .prepare(tableByIdSql('finance_asset_policy_runs'))
                .bind(id, companyId)
                .first<Row>();
        }, async updateAssetCarrying(value: string, now: number, id: string | number | null) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'update_asset_carrying', [value, now, id]);
        }, async updatePolicyVersion(now: number, id: string | number | null) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'bump_policy_edition', [now, id]);
        }, async assetEvent(value: string, id: string | number | null, companyId: string, value2: number, key: string, value3: string, userId: string, now: number) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, companyId, 'asset_depreciation_event', [value, id, companyId, value2, key, value3, userId, now]);
        }, guardRunVersion(key: string, value: "asset-policy-run-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, idCompanyStatusVersionGuard("finance_asset_policy_runs", "draft"), [id, companyId, expectedVersion]);
        }, async decideRun(value: "posted" | "rejected", userId: string, value2: string, now: number, id: string) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'decide_policy_run', [value, userId, value2, now, id]);
        }, async laterRuns(id: string | number | null, ordinal: string | number | null) {
            const db = read();
            return nativeFinanceReadFirst(FINANCE_RD_ASSET_POLICY_LEGACY, [id, ordinal], db);
        }, async pendingPolicy(id: string | number | null) {
            const db = read();
            return db
                .prepare(ASSET_POLICY_EDITION_PENDING_SQL)
                .bind(id)
                .first();
        }, async reverseAssetCarrying(value: string, now: number, id: string | number | null) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'reverse_asset_carrying', [value, now, id]);
        }, async reversePolicyVersion(now: number, id: string | number | null) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'reverse_policy_version', [now, id]);
        }, async reverseRun(value: string, entryDate: string, now: number, id: string) {
            const db = read();
            return await nativeFinanceOwnerWriteSystem(db, 'system', 'reverse_policy_run', [value, entryDate, now, id]);
        }, async workspacePolicies(companyId: string) {
            const db = read();
            return db
                .prepare(financeReferenceListSql('finance_asset_policy_editions','company_id=?','created_at DESC LIMIT 500'))
                .bind(companyId)
                .all<Row>();
        }, async workspaceRuns(companyId: string) {
            const db = read();
            return db
                .prepare(financeReferenceListSql('finance_asset_policy_runs','company_id=?','created_at DESC LIMIT 500'))
                .bind(companyId)
                .all<Row>();
        }, async workspaceAssets(companyId: string) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_WORKSPACE_ASSETS, [companyId], db);
        }, async workspaceBooks(companyId: string) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_WORKSPACE_BOOKS, [companyId], db);
        }, async workspacePeriods(companyId: string) {
            const db = read();
            return db
                .prepare(OPEN_PERIOD_LIST_ASC_SQL)
                .bind(companyId)
                .all<Row>();
        }, async activeDisposalPolicy(assetId: string, companyId: string) {
            const db = read();
            return nativeFinanceReadFirst<Row>(FINANCE_RD_ASSET_POLICY_DISPOSAL, [assetId, companyId], db);
        }, async disposalAsset(assetId: string, companyId: string) {
            const db = read();
            return db
                .prepare(assetAccountJoinByIdSql(" AND a.company_id=? AND a.status='in_service'"))
                .bind(assetId, companyId)
                .first<Row>();
        }, async disposalPendingRun(assetId: string) {
            const db = read();
            return db
                .prepare(ASSET_POLICY_RUN_PENDING_SQL)
                .bind(assetId)
                .first();
        }, async disposalPendingPolicy(assetId: string) {
            const db = read();
            return db
                .prepare(ASSET_POLICY_EDITION_PENDING_SQL)
                .bind(assetId)
                .first();
        }, async disposalBookMappings(ruleId: string | null, gainAccountId: string, lossAccountId: string) {
            const db = read();
            return nativeFinanceReadAll<Row>(FINANCE_RD_ASSET_POLICY_GAIN_LOSS, [ruleId, gainAccountId, lossAccountId], db);
        } };
    return { repository, prepare, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, requirePermission: (actor, permission) => { try {
            return requirePermission(actor, permission);
        }
        catch (error) {
            if (error instanceof ApiError)
                throw new BusinessRuleError(error.status, error.code, error.message);
            throw error;
        } }, requireTool: (...args) => inward(() => requireFinanceTool(read(), ...args)), transaction: work => inward(() => owned ? work(bindPorts(read,base,owned,prepare)) : financeTransaction(base(), db => work(bindPorts(() => db, base, db, async (holdingId) => { await holdingResources(holdingId); })))), ownedTransaction: (available, work) => inward(() => financeOwnedTransaction(base(), available ? owned : undefined, db => work(bindPorts(() => db, base, db, async (holdingId) => { await holdingResources(holdingId); })))), carrying: (...args) => inward(() => assetCarryingEvidence(read(), ...args)), prepareJournal: (...args) => inward(() => prepareJournal(read(), ...args)), prepareReversal: (...args) => inward(() => prepareOwnedJournalReversal(read(), ...args)), commands: { replay: (...args) => inward(() => replayCommand(read(), ...args)), commit: (...args) => inward(() => executeCommand(read(), ...args)) } };
}
function application(owned?: SqlDatabase) { let db: SqlDatabase | undefined; const read = () => { if (!db)
    throw Error('ASSET_POLICY_CONTEXT_REQUIRED'); return owned ?? db; }; const base=()=>{if(!db)throw Error('ASSET_POLICY_CONTEXT_REQUIRED');return db;}; return createAssetPolicyApplication<SqlStatement>(bindPorts(read, base, owned, async (holdingId) => { db = (await holdingResources(holdingId)).write; })); }
export function prepareAssetPolicy(actor: WorkContext, raw: z.input<typeof assetPolicySchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.asset-policy.prepare",purpose:"new-work"},tx=>exposed(() => application(tx).prepareAssetPolicy(actor, raw, key))); }
export function decideAssetPolicy(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyDecisionSchema>, action: "approve" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.asset-policy.${action}`,purpose:"settle-existing"},tx=>exposed(() => application(tx).decideAssetPolicy(actor, id, raw, action, key))); }
export function prepareAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyRunSchema>, key: string, ownedTransaction?: SqlDatabase) { return withFinancePurchasedCommand(actor,{key,operation:"finance.asset-policy-run.prepare",purpose:"settle-existing"},tx=>exposed(() => application(tx).prepareAssetPolicyRun(actor,id,raw,key,Boolean(tx))),ownedTransaction); }
export function decideAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyDecisionSchema>, action: "post" | "reject", key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.asset-policy-run.${action}`,purpose:"settle-existing"},tx=>exposed(() => application(tx).decideAssetPolicyRun(actor, id, raw, action, key))); }
export function reverseAssetPolicyRun(actor: WorkContext, id: string, raw: z.input<typeof assetPolicyReversalSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.asset-policy-run.reverse",purpose:"settle-existing"},tx=>exposed(() => application(tx).reverseAssetPolicyRun(actor, id, raw, key))); }
export function assetPolicyWorkspace(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>exposed(() => application(db).assetPolicyWorkspace(actor))); }
export function prepareAssetPolicyDisposal(db: SqlDatabase, actor: WorkContext, assetId: string, input: {
    id: string;
    periodId: string;
    entryDate: string;
    proceeds: number;
    gainAccountId: string;
    lossAccountId: string;
}, key: string, status: "draft" | "posted") { return exposed(() => createAssetPolicyApplication<SqlStatement>(bindPorts(() => db, () => db, db, async () => { })).prepareAssetPolicyDisposal(actor, assetId, input, key, status)); }
