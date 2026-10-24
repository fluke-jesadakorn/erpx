import { withFinancePurchasedCommand, withFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import { registeredGroupPurchases, requireGroupCompanyHistory, groupHistoryPredicate, groupSourceSql, groupRecordHistorySql, consolidationHistorySql } from "@erpx/server-finance/adapters/group-purchase-sources";
import { financeTransaction } from '@erpx/server-finance/public/finance-transactions';
import "server-only";
import { z } from "zod";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {decimalToMinor,minorToDecimal} from '@erpx/server-finance/public/finance';
import { holdingResources as nativeHoldingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '@erpx/server-workflow/public/commands';
import { ApiError as NativeApiError } from '@erpx/server-runtime/public/http';
import { type ConsolidationRun, type ConsolidationElimination, type ConsolidationAccount, consolidationRunSchema, consolidationDecisionSchema, consolidationEliminationSchema, consolidationEliminationDecisionSchema, type EliminationAction, transition } from '@erpx/server-finance/domain/finance-consolidation';
import { createFinanceConsolidationApplication, type FinanceConsolidationRepository, type FinanceConsolidationPorts } from '@erpx/server-finance/application/finance-consolidation';
import { BusinessRuleError } from '@erpx/server-shared/index';
export { consolidationRunSchema } from '@erpx/server-finance/domain/finance-consolidation';
export { consolidationDecisionSchema } from '@erpx/server-finance/domain/finance-consolidation';
export { consolidationEliminationSchema } from '@erpx/server-finance/domain/finance-consolidation';
export { consolidationEliminationDecisionSchema } from '@erpx/server-finance/domain/finance-consolidation';
function nativePorts(provided?: SqlDatabase, owned = false, inheritedHistory?: (company:string)=>string): FinanceConsolidationPorts<SqlStatement> {
    let database = provided!;
    let historySql: (company:string)=>string = inheritedHistory ?? (()=>"TRUE");
    const repository: FinanceConsolidationRepository<SqlStatement> = { readConsolidationRunWorkspaceRunseliminationsaccounts(holdingId: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_runs r WHERE holding_id=? ORDER BY as_of_date DESC,created_at DESC LIMIT 50", [consolidationHistorySql("r",historySql)])).bind(holdingId).all<ConsolidationRun>(); },
        readConsolidationRunWorkspaceRunseliminationsaccounts2(holdingId: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_eliminations WHERE run_id IN (SELECT id FROM finance_consolidation_runs WHERE holding_id=?) ORDER BY created_at,id", [`EXISTS(SELECT 1 FROM finance_consolidation_runs purchased_run WHERE purchased_run.id=finance_consolidation_eliminations.run_id AND (${consolidationHistorySql("purchased_run",historySql)}))`])).bind(holdingId).all<ConsolidationElimination>(); },
        readConsolidationRunWorkspaceRunseliminationsaccounts3(holdingId: string) {
            return database.prepare(groupSourceSql(`SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency
      FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id
      JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'
      WHERE a.status='active' AND c.status='active' ORDER BY a.company_id,a.code`,[historySql('a.company_id')])).bind(holdingId).all<ConsolidationAccount>();
        },
        readEntityPeriodsEntities(holdingId: string) {
            return database.prepare("SELECT company_id,reporting_name,functional_currency FROM finance_group_entities WHERE holding_id=? AND status='active' ORDER BY reporting_name,company_id").bind(holdingId).all<{
                company_id: string;
                reporting_name: string;
                functional_currency: string;
            }>();
        },
        async readEntityPeriodsPeriod(company_id: string, asOfDate: string) {
            await requireGroupCompanyHistory(database,company_id);
            return database.prepare("SELECT id,name,end_date,close_version FROM finance_periods WHERE company_id=? AND state='closed' AND end_date<=? ORDER BY end_date DESC,id DESC LIMIT 1").bind(company_id, asOfDate).first<{
                id: string;
                name: string;
                end_date: string;
                close_version: number;
            }>();
        },
        async readSnapshotAccounts(companyId: string, periodId: string) {
            await requireGroupCompanyHistory(database,companyId);
            return database.prepare(`SELECT a.code,a.name,a.category,a.currency,COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) AS balance_minor
      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id
      WHERE j.company_id=? AND j.period_id=? AND j.status='posted'
      GROUP BY a.code,a.name,a.category,a.currency ORDER BY a.code`).bind(companyId, periodId).all<Record<string, string | number | null>>();
        },
        readSnapshotIntercompany(holdingId: string) { return database.prepare(groupSourceSql("SELECT id,source_company_id,target_company_id,reference,currency,amount_minor,status,version FROM finance_intercompany_transactions WHERE holding_id=? AND status IN ('settled','eliminated') ORDER BY id", [groupRecordHistorySql("intercompany","finance_intercompany_transactions",historySql)])).bind(holdingId).all<Record<string, string | number | null>>(); },
        readConsolidationAccountsRows(holdingId: string, unique: string[]) {
            return database.prepare(groupSourceSql(`SELECT a.id,a.company_id,a.chart_id,a.code,a.name,a.currency,c.base_currency
    FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id
    JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active'
    WHERE a.id IN (${unique.map(() => "?").join(",")}) AND a.status='active' AND c.status='active'`,[historySql('a.company_id')])).bind(holdingId, ...unique).all<ConsolidationAccount>();
        },
        readConsolidationRunRun(runId: string, holdingId: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_runs r WHERE id=? AND holding_id=?", [consolidationHistorySql("r",historySql)])).bind(runId, holdingId).first<ConsolidationRun>(); },
        readCreateConsolidationEliminationTransaction(sourceTransactionId: string, holdingId: string) {
            return database.prepare(groupSourceSql("SELECT id,status,currency,amount_minor,source_company_id,target_company_id FROM finance_intercompany_transactions WHERE id=? AND holding_id=? AND status IN ('settled','eliminated')", [groupRecordHistorySql("intercompany","finance_intercompany_transactions",historySql)])).bind(sourceTransactionId, holdingId).first<{
                id: string;
                status: string;
                currency: string;
                amount_minor: number;
                source_company_id: string;
                target_company_id: string;
            }>();
        },
        guard11(key: string, value: "elimination-unique-source", runId: string, sourceTransactionId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND source_transaction_id=?)", [runId, sourceTransactionId]); },
        stageCreateConsolidationEliminationEffect(id: string, runId: string, description: string, debitAccountId: string, creditAccountId: string, amount: number, sourceTransactionId: string, userId: string, now: number, now2: number, value: string) {
            return database.prepare(`INSERT INTO finance_consolidation_eliminations(id,run_id,description,debit_account_id,credit_account_id,amount_minor,status,journal_id,version,source_transaction_id,created_by,created_at,updated_at,evidence_json)
      VALUES(?,?,?,?,?,?, 'proposed',NULL,1,?,?,?,?,?)`).bind(id, runId, description, debitAccountId, creditAccountId, amount, sourceTransactionId, userId, now, now2, value);
        },
        stageCreateConsolidationEliminationEffect2(value: string, id: string, runId: string, holdingId: string, value2: "proposed", value3: null, value4: "proposed", userId: string, value5: 1, value6: "Elimination account pair prepared from a settled intercompany source.", value7: string, now: number) { return database.prepare("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(value, id, runId, holdingId, value2, value3, value4, userId, value5, value6, value7, now); },
        readDecideConsolidationEliminationRow(eliminationId: string, holdingId: string) {
            return database.prepare(`SELECT e.*,r.status run_status,r.as_of_date,r.presentation_currency,r.source_hash,r.source_snapshot_json
    FROM finance_consolidation_eliminations e JOIN finance_consolidation_runs r ON r.id=e.run_id
    WHERE e.id=? AND r.holding_id=?`).bind(eliminationId, holdingId).first<ConsolidationElimination & {
                run_status: string;
                as_of_date: string;
                presentation_currency: string;
                source_hash: string;
                source_snapshot_json: string;
            }>();
        },
        guard15(key: string, value: "elimination-version", eliminationId: string, expectedVersion: number, from: "proposed" | "approved") { return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_consolidation_eliminations WHERE id=? AND version=? AND status=?)", [eliminationId, expectedVersion, from]); },
        stageDecideConsolidationEliminationEffect(userId: string, now: number, now2: number, eliminationId: string, expectedVersion: number) { return database.prepare("UPDATE finance_consolidation_eliminations SET status='approved',approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='proposed'").bind(userId, now, now2, eliminationId, expectedVersion); },
        readDecideConsolidationEliminationLineCount(journalId: string) {
            return database.prepare("SELECT COUNT(*) count FROM finance_consolidation_journal_lines WHERE journal_id=?").bind(journalId).first<{
                count: number;
            }>();
        },
        guard18(key: string, value: "elimination-journal-not-reversed", journalId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM finance_consolidation_journals WHERE id=? AND status='reversed')", [journalId]); },
        guard19(key: string, value: "elimination-lines-unique", value2: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM finance_consolidation_journal_lines WHERE id=?)", [value2]); },
        stageDecideConsolidationEliminationEffect2(journalId: string, run_id: string, holdingId: string, journalNumber: string, as_of_date: string, presentation_currency: string, value: "Controlled Group Finance elimination journal", source_hash: string, created_by: string | number | null, userId: string, now: number, now2: number, now3: number) {
            return database.prepare(`INSERT INTO finance_consolidation_journals(id,run_id,holding_id,journal_number,entry_date,currency,memo,status,source_hash,version,created_by,posted_by,posted_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,'posted',?,1,?,?,?,?,?) ON CONFLICT(run_id) DO NOTHING`).bind(journalId, run_id, holdingId, journalNumber, as_of_date, presentation_currency, value, source_hash, created_by, userId, now, now2, now3);
        },
        stageDecideConsolidationEliminationEffect3(value: string, journalId: string, debit_account_id: string, description: string, amount_minor: number, ordinal: number) { return database.prepare("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,?,0,?)").bind(value, journalId, debit_account_id, description, amount_minor, ordinal); },
        stageDecideConsolidationEliminationEffect4(value: string, journalId: string, credit_account_id: string, description: string, amount_minor: number, value2: number) { return database.prepare("INSERT INTO finance_consolidation_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,ordinal) VALUES(?,?,?, ?,0,?,?)").bind(value, journalId, credit_account_id, description, amount_minor, value2); },
        stageDecideConsolidationEliminationEffect5(userId: string, now: number, now2: number, journalId: string, run_id: string) { return database.prepare("UPDATE finance_consolidation_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=? AND run_id=?").bind(userId, now, now2, journalId, run_id); },
        stageDecideConsolidationEliminationEffect6(journalId: string, userId: string, now: number, now2: number, eliminationId: string, expectedVersion: number) { return database.prepare("UPDATE finance_consolidation_eliminations SET status='posted',consolidation_journal_id=?,posted_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=? AND version=? AND status='approved'").bind(journalId, userId, now, now2, eliminationId, expectedVersion); },
        stageDecideConsolidationEliminationEffect7(value: string, eliminationId: string, run_id: string, holdingId: string, event: "approved" | "posted", status: "proposed" | "approved", to: "approved" | "posted", userId: string, version: number, reason: string, value2: string, now: number) { return database.prepare("INSERT INTO finance_consolidation_elimination_events(id,elimination_id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(value, eliminationId, run_id, holdingId, event, status, to, userId, version, reason, value2, now); },
        readCreateConsolidationRunDuplicate(holdingId: string, asOfDate: string, presentationCurrency: string) {
            return database.prepare("SELECT id FROM finance_consolidation_runs WHERE holding_id=? AND as_of_date=? AND presentation_currency=?").bind(holdingId, asOfDate, presentationCurrency).first<{
                id: string;
            }>();
        },
        guard27(key: string, value: "consolidation-unique", holdingId: string, asOfDate: string, presentationCurrency: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM finance_consolidation_runs WHERE holding_id=? AND as_of_date=? AND presentation_currency=?)", [holdingId, asOfDate, presentationCurrency]); },
        stageCreateConsolidationRunEffect(id: string, holdingId: string, presentationCurrency: string, asOfDate: string, value: string, sourceHash: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO finance_consolidation_runs(id,holding_id,presentation_currency,as_of_date,status,source_snapshot_json,source_hash,version,created_by,created_at,updated_at) VALUES(?,?,?,?, 'draft',?,?,1,?,?,?)").bind(id, holdingId, presentationCurrency, asOfDate, value, sourceHash, userId, now, now2); },
        stageCreateConsolidationRunEffect2(value: string, id: string, holdingId: string, value2: "prepared", value3: null, value4: "draft", userId: string, value5: 1, value6: "Consolidation snapshot prepared from closed company periods.", value7: string, now: number) { return database.prepare("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(value, id, holdingId, value2, value3, value4, userId, value5, value6, value7, now); },
        readDecideConsolidationRunRun(runId: string, holdingId: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_runs r WHERE id=? AND holding_id=?", [consolidationHistorySql("r",historySql)])).bind(runId, holdingId).first<ConsolidationRun>(); },
        readDecideConsolidationRunPending(runId: string) { return database.prepare("SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND status='proposed' LIMIT 1").bind(runId).first(); },
        readDecideConsolidationRunPending2(runId: string) { return database.prepare("SELECT 1 FROM finance_consolidation_eliminations WHERE run_id=? AND status<>'posted' LIMIT 1").bind(runId).first(); },
        stageDecideConsolidationRunUpdate(action: keyof typeof transition, userId: string, now: number, runId: string, holdingId: string, expectedVersion: number) {
            const rule = transition[action];
            const updates = action === "review" ? "status=?,reviewed_by=?,reviewed_at=?,version=version+1,updated_at=?" : action === "approve" ? "status=?,approved_by=?,version=version+1,updated_at=?" : "status=?,published_by=?,published_at=?,version=version+1,updated_at=?";
            const values = action === "review" ? [rule.to, userId, now, now] : action === "approve" ? [rule.to, userId, now] : [rule.to, userId, now, now];
            return database.prepare(`UPDATE finance_consolidation_runs SET ${updates} WHERE id=? AND holding_id=? AND status=? AND version=?`).bind(...values, runId, holdingId, rule.from, expectedVersion);
        },
        stageDecideConsolidationRunEvent(value: string, runId: string, holdingId: string, event: "approved" | "reviewed" | "published", status: "approved" | "draft" | "review", to: "approved" | "review" | "published", userId: string, version: number, reason: string, value2: string, now: number) { return database.prepare("INSERT INTO finance_consolidation_events(id,run_id,holding_id,action,from_status,to_status,actor_id,version,reason,evidence_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(value, runId, holdingId, event, status, to, userId, version, reason, value2, now); },
        guard35(key: string, value: "consolidation-version", runId: string, holdingId: string, from: "approved" | "draft" | "review", expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_consolidation_runs WHERE id=? AND holding_id=? AND status=? AND version=?)", [runId, holdingId, from, expectedVersion]); },
        readConsolidationDetailRow(id: string, holdingId: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_runs r WHERE id=? AND holding_id=?", [consolidationHistorySql("r",historySql)])).bind(id, holdingId).first<ConsolidationRun>(); },
        readConsolidationDetailEffect(id: string) { return database.prepare(groupSourceSql("SELECT * FROM finance_consolidation_eliminations WHERE run_id=? ORDER BY created_at,id", [`EXISTS(SELECT 1 FROM finance_consolidation_runs purchased_run WHERE purchased_run.id=finance_consolidation_eliminations.run_id AND (${consolidationHistorySql("purchased_run",historySql)}))`])).bind(id).all<ConsolidationElimination>(); },
        readConsolidationEliminationDetailElimination(id: string, holdingId: string) { return database.prepare('SELECT e.* FROM finance_consolidation_eliminations e JOIN finance_consolidation_runs r ON r.id=e.run_id WHERE e.id=? AND r.holding_id=?').bind(id, holdingId).first<ConsolidationElimination>(); } };
    return { repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            historySql=await groupHistoryPredicate();
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, batch: effects => database.batch(effects), transaction: run => owned && provided ? run(nativePorts(provided,true,historySql)) : financeTransaction(database, tx => run(nativePorts(tx,true,historySql))), digestBytes: bytes => crypto.subtle.digest('SHA-256', bytes), requirePermission: (...args) => fromNativeSync(() => requirePermission(...args)), decimalToMinor: (...args) => fromNativeSync(() => decimalToMinor(...args)), minorToDecimal: (...args) => fromNativeSync(() => minorToDecimal(...args)) };
}
function application(provided?: SqlDatabase, owned = false) { return createFinanceConsolidationApplication(nativePorts(provided,owned)); }

function fromNativeSync<Result>(run: () => Result): Result {
    try {
        return run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new NativeApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function consolidationRunWorkspace(actor: WorkContext) { return withFinancePurchasedRead(actor,db=>invoke(()=>application(db).consolidationRunWorkspace(actor)),"finance.advanced"); }
export async function createConsolidationElimination(actor: WorkContext, runId: string, input: z.infer<typeof consolidationEliminationSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.consolidation.elimination.propose",purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>registeredGroupPurchases(db,actor)},tx=>invoke(()=>application(tx,true).createConsolidationElimination(actor,runId,input,key))); }
export async function decideConsolidationElimination(actor: WorkContext, eliminationId: string, action: EliminationAction, input: z.infer<typeof consolidationEliminationDecisionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.consolidation.elimination.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>registeredGroupPurchases(db,actor)},tx=>invoke(()=>application(tx,true).decideConsolidationElimination(actor,eliminationId,action,input,key))); }
export async function createConsolidationRun(actor: WorkContext, input: z.infer<typeof consolidationRunSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:"finance.consolidation.prepare",purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>registeredGroupPurchases(db,actor)},tx=>invoke(()=>application(tx,true).createConsolidationRun(actor,input,key))); }
export async function decideConsolidationRun(actor: WorkContext, runId: string, action: keyof typeof transition, input: z.infer<typeof consolidationDecisionSchema>, key: string) { return withFinancePurchasedCommand(actor,{key,operation:`finance.consolidation.${action}`,purpose:"settle-existing",capability:"finance.advanced",companyPurchases:db=>registeredGroupPurchases(db,actor)},tx=>invoke(()=>application(tx,true).decideConsolidationRun(actor,runId,action,input,key))); }
export async function consolidationDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>invoke(()=>application(db).consolidationDetail(actor, id)),"finance.advanced"); }
export async function consolidationEliminationDetail(actor: WorkContext, id: string) { return withFinancePurchasedRead(actor,db=>invoke(()=>application(db).consolidationEliminationDetail(actor, id)),"finance.advanced"); }
