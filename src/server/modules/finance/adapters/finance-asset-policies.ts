import { withFinancePurchasedCommand, withFinancePurchasedRead } from "./purchase-boundary";
import { assetPolicyRunSchema } from '../domain/due-inputs';
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand, assertion } from "@/server/modules/workflow/public/commands";
import { financeTransaction, financeOwnedTransaction } from "@/server/modules/finance/public/finance-transactions";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { prepareOwnedJournalReversal } from "@/server/modules/finance/public/finance-owned-reversal";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import { assetCarryingEvidence } from "@/server/modules/finance/public/finance-asset-carrying";
import { BusinessRuleError } from '../../shared';
import { createAssetPolicyApplication, type AssetPolicyPorts, type AssetPolicyRepository } from '../application/asset-policies';
import { assetPolicySchema, assetPolicyDecisionSchema, assetPolicyReversalSchema, type Row } from '../domain/asset-policies';
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
                .prepare("SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=?")
                .bind(id, companyId)
                .first<Row>();
        }, async pendingDisposal(id: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_disposals WHERE asset_id=? AND status='draft'")
                .bind(id)
                .first();
        }, async bookRules(companyId: string, chart_id: string | number | null, date: string) {
            const db = read();
            return db
                .prepare("SELECT DISTINCT ON (secondary_chart_id) id,secondary_chart_id,effective_date FROM finance_book_rules WHERE company_id=? AND primary_chart_id=? AND status='approved' AND effective_date<=? ORDER BY secondary_chart_id,effective_date DESC")
                .bind(companyId, chart_id, date)
                .all<Row>();
        }, async bookMappings(id: string | number | null, account_id: string | number | null, depreciation_account_id: string | number | null, depreciation_expense_account_id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT primary_account_id,secondary_account_id FROM finance_book_account_mappings WHERE rule_id=? AND primary_account_id IN (?,?,?) ORDER BY primary_account_id")
                .bind(id, account_id, depreciation_account_id, depreciation_expense_account_id)
                .all<Row>();
        }, async bookAccounts(companyId: string, accountIds: string[]) {
            const db = read();
            return db
                .prepare(`SELECT a.id,a.version,a.category,a.control_kind,a.currency,c.status chart_status,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.id IN (${accountIds.map(() => "?").join(",")}) ORDER BY a.id`)
                .bind(companyId, ...accountIds)
                .all<Row>();
        }, async edition(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_editions WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<Row>();
        }, async legacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?")
                .bind(id)
                .first<number>("n");
        }, async postedRuns(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_runs WHERE asset_id=? AND status='posted' ORDER BY ordinal")
                .bind(id)
                .all<Row>();
        }, async policyLegacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?")
                .bind(id)
                .first<number>("n");
        }, async pendingRun(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'")
                .bind(id)
                .first();
        }, preparePolicy(id: string, companyId: string, assetId: string, value: string | number | null, value2: string, hash: string, userId: string, reason: string, now: number, now2: number, value3: string) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_asset_policy_editions(id,company_id,asset_id,parent_id,content_json,source_hash,prepared_by,reason,created_at,updated_at,basis_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, companyId, assetId, value, value2, hash, userId, reason, now, now2, value3);
        }, supersedePolicy(now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        }, guardPolicyVersion(key: string, value: "asset-policy-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_asset_policy_editions WHERE id=? AND company_id=? AND version=? AND status='draft')", [id, companyId, expectedVersion]);
        }, decidePolicy(value: "active" | "rejected", userId: string, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, userId, now, id);
        }, async openPeriod(periodId: string, companyId: string, entryDate: string, entryDate2: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?")
                .bind(periodId, companyId, entryDate, entryDate2)
                .first();
        }, async runLegacyCount(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?")
                .bind(id)
                .first<number>("n");
        }, prepareRun(runId: string, companyId: string, id: string | number | null, id2: string, ordinal: number, entryDate: string, value: string, value2: string, hash: string, userId: string, reason: string, now: number, now2: number) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_asset_policy_runs(id,company_id,asset_id,policy_id,ordinal,entry_date,input_json,result_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(runId, companyId, id, id2, ordinal, entryDate, value, value2, hash, userId, reason, now, now2);
        }, async run(id: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_runs WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<Row>();
        }, updateAssetCarrying(value: string, now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_assets SET accumulated_depreciation_minor=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, now, id);
        }, updatePolicyVersion(now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        }, assetEvent(value: string, id: string | number | null, companyId: string, value2: number, key: string, value3: string, userId: string, now: number) {
            const db = read();
            return db
                .prepare("INSERT INTO finance_asset_events(id,asset_id,company_id,event_type,asset_version,command_id,payload_json,actor_id,created_at) VALUES(?,?,?,'depreciated',?,?,?,?,?)")
                .bind(value, id, companyId, value2, key, value3, userId, now);
        }, guardRunVersion(key: string, value: "asset-policy-run-version", id: string, companyId: string, expectedVersion: number) {
            const db = read();
            return assertion(db, key, value, "EXISTS(SELECT 1 FROM finance_asset_policy_runs WHERE id=? AND company_id=? AND status='draft' AND version=?)", [id, companyId, expectedVersion]);
        }, decideRun(value: "posted" | "rejected", userId: string, value2: string, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_runs SET status=?,reviewed_by=?,journal_ids_json=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, userId, value2, now, id);
        }, async laterRuns(id: string | number | null, ordinal: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND ((status='posted' AND ordinal>?) OR status='draft')")
                .bind(id, ordinal)
                .first();
        }, async pendingPolicy(id: string | number | null) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'")
                .bind(id)
                .first();
        }, reverseAssetCarrying(value: string, now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_assets SET accumulated_depreciation_minor=accumulated_depreciation_minor-?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, now, id);
        }, reversePolicyVersion(now: number, id: string | number | null) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_editions SET version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        }, reverseRun(value: string, entryDate: string, now: number, id: string) {
            const db = read();
            return db
                .prepare("UPDATE finance_asset_policy_runs SET status='reversed',reversal_ids_json=?,reversal_entry_date=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, entryDate, now, id);
        }, async workspacePolicies(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_editions WHERE company_id=? ORDER BY created_at DESC LIMIT 500")
                .bind(companyId)
                .all<Row>();
        }, async workspaceRuns(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500")
                .bind(companyId)
                .all<Row>();
        }, async workspaceAssets(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT a.*,c.chart_id,(SELECT count(*) FROM finance_asset_depreciations old WHERE old.asset_id=a.id)+(SELECT count(*) FROM finance_asset_policy_runs run WHERE run.asset_id=a.id AND run.status='posted') completed_depreciation_months FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.company_id=? AND a.status='in_service' ORDER BY asset_number")
                .bind(companyId)
                .all<Row>();
        }, async workspaceBooks(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT c.id,c.name,r.primary_chart_id FROM finance_charts c LEFT JOIN (SELECT DISTINCT secondary_chart_id,primary_chart_id FROM finance_book_rules WHERE status='approved') r ON r.secondary_chart_id=c.id WHERE c.company_id=? AND c.status='active' ORDER BY c.id")
                .bind(companyId)
                .all<Row>();
        }, async workspacePeriods(companyId: string) {
            const db = read();
            return db
                .prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date")
                .bind(companyId)
                .all<Row>();
        }, async activeDisposalPolicy(assetId: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT * FROM finance_asset_policy_editions WHERE asset_id=? AND company_id=? AND status='active'")
                .bind(assetId, companyId)
                .first<Row>();
        }, async disposalAsset(assetId: string, companyId: string) {
            const db = read();
            return db
                .prepare("SELECT a.*,c.chart_id FROM finance_assets a JOIN finance_accounts c ON c.id=a.account_id WHERE a.id=? AND a.company_id=? AND a.status='in_service'")
                .bind(assetId, companyId)
                .first<Row>();
        }, async disposalPendingRun(assetId: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'")
                .bind(assetId)
                .first();
        }, async disposalPendingPolicy(assetId: string) {
            const db = read();
            return db
                .prepare("SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'")
                .bind(assetId)
                .first();
        }, async disposalBookMappings(ruleId: string | null, gainAccountId: string, lossAccountId: string) {
            const db = read();
            return db
                .prepare("SELECT m.primary_account_id,m.secondary_account_id,a.version,a.status FROM finance_book_account_mappings m JOIN finance_accounts a ON a.id=m.secondary_account_id WHERE m.rule_id=? AND m.primary_account_id IN (?,?) ORDER BY m.primary_account_id")
                .bind(ruleId, gainAccountId, lossAccountId)
                .all<Row>();
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
