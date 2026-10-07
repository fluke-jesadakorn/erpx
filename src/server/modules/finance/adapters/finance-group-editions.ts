import {groupReportingPersonNames} from '../../people_payroll/public/finance-group-person-facts';
import { intercompanyPairSchema } from '../public/finance-intercompany-pairs';
import { fiscalCloseSourceSql, capturedFiscalClose } from "@/server/modules/finance/public/finance-fiscal-close-source";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { financeTransaction } from "@/server/modules/finance/public/finance-transactions";
import { requireFinanceTool } from "@/server/modules/finance/public/finance-company-tools";
import { requireFinanceMoneyScale } from "@/server/modules/finance/public/finance-money-policy";
import { assertion } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { groupOwnershipWorkspace } from "@/server/modules/finance/public/finance-group-ownership";
import { intercompanyPairWorkspace } from "@/server/modules/finance/public/finance-intercompany-pairs";
import { intercompanyPairSource } from "@/server/modules/finance/public/finance-intercompany-pairs";
import { holdingResources as nativeHoldingResources } from '../../runtime/public/runtime';
import { replayCommand as nativeReplayCommand, executeCommand as nativeExecuteCommand } from '../../workflow/public/commands';
import { ApiError as NativeApiError } from '../../runtime/public/http';
import { type Row, groupReportEditionSchema, groupReportDecisionSchema } from '../domain/finance-group-editions';
import { createFinanceGroupEditionsApplication, type FinanceGroupEditionsRepository, type FinanceGroupEditionsPorts } from '../application/finance-group-editions';
import { BusinessRuleError } from '../../shared';
import type { SqlStatement } from '../../runtime/public/runtime/types';
export { groupReportEditionSchema } from '../domain/finance-group-editions';
export { groupReportDecisionSchema } from '../domain/finance-group-editions';
function nativePorts(provided?: SqlDatabase): FinanceGroupEditionsPorts<SqlStatement> {
    let database = provided!;
    const repository: FinanceGroupEditionsRepository<SqlStatement> = { readSourceRegistration(holdingId: string, companyId: string, chartId: string) {
            return database.prepare("SELECT e.id,e.company_id,e.reporting_name,e.functional_currency,e.version,c.name chart_name,c.base_currency,c.status chart_status FROM finance_group_entities e JOIN finance_charts c ON c.company_id=e.company_id WHERE e.holding_id=? AND e.company_id=? AND e.status='active' AND c.id=?")
                .bind(holdingId, companyId, chartId)
                .first<Row>();
        },
        readSourcePeriods(companyId: string, periodIds: string[], entity: {
            companyId: string;
            chartId: string;
            periodIds: string[];
            method: "full" | "equity";
        }) {
            return database.prepare(`SELECT id,start_date,end_date,state,close_version FROM finance_periods WHERE company_id=? AND id IN (${entity.periodIds.map(() => "?").join(",")}) ORDER BY start_date,id`)
                .bind(companyId, ...periodIds)
                .all<Row>();
        },
        readSourceLines(companyId: string, chartId: string, endDate: string) {
            return database.prepare(`SELECT l.id,l.account_id,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json,j.id journal_id,j.version,j.entry_date,j.status,j.source_type,CASE WHEN ${fiscalCloseSourceSql()} THEN 1 ELSE 0 END is_fiscal_close,j.source_id,j.currency,j.memo,a.code,a.name,a.category,a.version account_version FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? ORDER BY j.entry_date,j.id,l.ordinal LIMIT 100001`)
                .bind(companyId, chartId, endDate)
                .all<Row>();
        },
        readSourcePair(id: string, holdingId: string) {
            return database.prepare("SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND status='confirmed'")
                .bind(id, holdingId)
                .first<Row>();
        },
        readSourceAcquisition(acquisitionId: string, holdingId: string, companyId: string, currency: string, endDate: string) {
            return database.prepare("SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND company_id=? AND status='approved' AND currency=? AND acquisition_date<=?")
                .bind(acquisitionId, holdingId, companyId, currency, endDate)
                .first<Row>();
        },
        readSourceOwnership(ownership_id: string | number, holdingId: string) {
            return database.prepare("SELECT o.*,i.company_id investor_company,v.company_id investee_company FROM finance_ownership_interests o JOIN finance_group_entities i ON i.id=o.investor_entity_id JOIN finance_group_entities v ON v.id=o.investee_entity_id WHERE o.id=? AND o.holding_id=? AND o.status IN ('approved','retired') AND o.approved_by IS NOT NULL")
                .bind(ownership_id, holdingId)
                .first<Row>();
        },
        readSourceCalc(calculation_run_id: string | number | null, holdingId: string, companyId: string) {
            return database.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND holding_id=? AND company_id=? AND status='approved' AND kind='consolidation'")
                .bind(calculation_run_id, holdingId, companyId)
                .first<Row>();
        },
        readSourceCalc2(calculationRunId: string, holdingId: string, companyId: string, currency: string) {
            return database.prepare("SELECT * FROM finance_calculation_runs WHERE id=? AND holding_id=? AND company_id=? AND kind='consolidation' AND status='approved' AND currency=? AND currency_scale=2")
                .bind(calculationRunId, holdingId, companyId, currency)
                .first<Row>();
        },
        readSourceOwnership2(ownershipId: string, holdingId: string, startDate: string, endDate: string) {
            return database.prepare("SELECT o.*,v.company_id investee_company,i.company_id investor_company FROM finance_ownership_interests o JOIN finance_group_entities v ON v.id=o.investee_entity_id JOIN finance_group_entities i ON i.id=o.investor_entity_id WHERE o.id=? AND o.holding_id=? AND o.approved_by IS NOT NULL AND o.status IN ('approved','retired') AND o.effective_from<=? AND (o.effective_to IS NULL OR o.effective_to>=?)")
                .bind(ownershipId, holdingId, startDate, endDate)
                .first<Row>();
        },
        readSourceEffect(value: string, holdingId: string, investee_entity_id: string | number | null, endDate: string, currency: string, id: string | number | null) {
            return database.prepare("SELECT * FROM finance_acquisitions WHERE id=? AND holding_id=? AND investee_entity_id=? AND status IN ('approved','impaired') AND acquisition_date<=? AND currency=? AND calculation_run_id=?")
                .bind(value, holdingId, investee_entity_id, endDate, currency, id)
                .first<Row>();
        },
        readGetR(id: string, holdingId: string, companyId: string) {
            return database.prepare("SELECT * FROM finance_group_report_editions WHERE id=? AND holding_id=? AND company_id=?")
                .bind(id, holdingId, companyId)
                .first<Row>();
        },
        stagePrepareGroupReportEditionEffect(id: string, holdingId: string, companyId: string, value: string | number | null, value2: string | number, value3: number, startDate: string, endDate: string, currency: string, value4: string, value5: string, value6: string, hash: string, csv: string, exportHash: string, userId: string, reason: string, now: number, now2: number) {
            return database.prepare("INSERT INTO finance_group_report_editions(id,holding_id,company_id,parent_id,root_id,edition,start_date,end_date,currency,content_json,source_json,result_json,source_hash,export_content,export_sha256,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id, holdingId, companyId, value, value2, value3, startDate, endDate, currency, value4, value5, value6, hash, csv, exportHash, userId, reason, now, now2);
        },
        stageDecideGroupReportEditionEffect(now: number, id: string | number | null) {
            return database.prepare("UPDATE finance_group_report_editions SET status='superseded',version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        },
        guard14(key: string, value: "group-edition-version", id: string, holdingId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM finance_group_report_editions WHERE id=? AND holding_id=? AND status='draft' AND version=?)", [id, holdingId, expectedVersion]);
        },
        stageDecideGroupReportEditionEffect2(value: "approved" | "rejected", userId: string, now: number, id: string) {
            return database.prepare("UPDATE finance_group_report_editions SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
                .bind(value, userId, now, id);
        },
        readGroupReportEditionWorkspaceEffect(holdingId: string, companyId: string) {
            return database.prepare("SELECT * FROM finance_group_report_editions WHERE holding_id=? AND company_id=? ORDER BY created_at DESC,id LIMIT 100")
                .bind(holdingId, companyId)
                .all<Row>();
        },
        readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations(holdingId: string) {
            return database.prepare("SELECT c.*,e.reporting_name FROM finance_charts c JOIN finance_group_entities e ON e.company_id=c.company_id AND e.holding_id=? AND e.status='active' WHERE c.status='active' ORDER BY e.reporting_name,c.name,c.id")
                .bind(holdingId)
                .all<Row>();
        },
        readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations2(holdingId: string) {
            return database.prepare("SELECT p.* FROM finance_periods p JOIN finance_group_entities e ON e.company_id=p.company_id AND e.holding_id=? AND e.status='active' WHERE p.state='closed' ORDER BY p.start_date,p.id")
                .bind(holdingId)
                .all<Row>();
        },
        readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations3(holdingId: string) {
            return database.prepare("SELECT a.*,e.reporting_name FROM finance_accounts a JOIN finance_group_entities e ON e.company_id=a.company_id AND e.holding_id=? AND e.status='active' ORDER BY e.reporting_name,a.code,a.id")
                .bind(holdingId)
                .all<Row>();
        },
        readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations4(holdingId: string) {
            return database.prepare("SELECT j.id journal_id,j.company_id,j.chart_id,j.entry_date,j.currency,j.source_type,j.source_id,j.memo,e.reporting_name,a.id account_id,a.code,a.name,a.category,SUM(l.debit_minor-l.credit_minor) amount_minor FROM finance_journals j JOIN finance_group_entities e ON e.company_id=j.company_id AND e.holding_id=? AND e.status='active' JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.status='posted' GROUP BY j.id,e.reporting_name,a.id HAVING SUM(l.debit_minor-l.credit_minor)<>0 ORDER BY j.entry_date DESC,j.id,a.code LIMIT 1000")
                .bind(holdingId)
                .all<Row>();
        },
        readGroupExpansionWorkspaceEditionsownershippairschartsperiodsaccountsdocumentscalculations5(holdingId: string, companyId: string) {
            return database.prepare("SELECT * FROM finance_calculation_runs WHERE holding_id=? AND company_id=? AND kind='consolidation' AND status='approved' ORDER BY created_at DESC,id LIMIT 200")
                .bind(holdingId, companyId)
                .all<Row>();
        },
        readGroupRecordNamesPeople(companyId:string,ids:string[]){return groupReportingPersonNames(database,companyId,ids);},
        readGroupExpansionRecordRecord(holdingId: string, companyId: string, id: string, table: string) {
            return database.prepare(`SELECT * FROM ${table} WHERE holding_id=? AND company_id=? AND id=?`)
                .bind(holdingId, companyId, id)
                .first<Row>();
        } };
    return { repository, parseIntercompanyPair: value => intercompanyPairSchema.parse(value), clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, commands: { replay: (...args) => nativeReplayCommand(database, ...args), commit: (...args) => nativeExecuteCommand(database, ...args) }, prepare: async (holding) => {
            if (!provided)
                database = (await nativeHoldingResources(holding)).write;
        }, batch: effects => database.batch(effects), transaction: run => financeTransaction(database, tx => run(nativePorts(tx))), digestBytes: bytes => crypto.subtle.digest('SHA-256', bytes), capturedFiscalClose: (...args) => fromNativeSync(() => capturedFiscalClose(...args)), requirePermission: (...args) => fromNativeSync(() => requirePermission(...args)), requireFinanceTool: (...args) => fromNative(() => requireFinanceTool(database, ...args)), requireFinanceMoneyScale: (...args) => fromNative(() => requireFinanceMoneyScale(database, ...args)), sha256: (...args) => fromNative(() => sha256(...args)), groupOwnershipWorkspace: (...args) => fromNative(() => groupOwnershipWorkspace(...args)), intercompanyPairWorkspace: (...args) => fromNative(() => intercompanyPairWorkspace(...args)), intercompanyPairSource: (...args) => fromNative(() => intercompanyPairSource(database, ...args)) };
}
function application(provided?: SqlDatabase) { return createFinanceGroupEditionsApplication(nativePorts(provided)); }
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof NativeApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
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
export async function prepareGroupReportEdition(actor: WorkContext, raw: z.input<typeof groupReportEditionSchema>, key: string) { return invoke(() => application().prepareGroupReportEdition(actor, raw, key)); }
export async function decideGroupReportEdition(actor: WorkContext, id: string, raw: z.input<typeof groupReportDecisionSchema>, action: "approve" | "reject", key: string) { return invoke(() => application().decideGroupReportEdition(actor, id, raw, action, key)); }
export async function groupReportEditionExport(actor: WorkContext, id: string) { return invoke(() => application().groupReportEditionExport(actor, id)); }
export async function groupEditionSourceJournal(actor: WorkContext, id: string, journalId: string) { return invoke(() => application().groupEditionSourceJournal(actor, id, journalId)); }
export async function groupReportEditionWorkspace(actor: WorkContext) { return invoke(() => application().groupReportEditionWorkspace(actor)); }
export async function groupExpansionWorkspace(actor: WorkContext) { return invoke(() => application().groupExpansionWorkspace(actor)); }
export async function groupExpansionRecord(actor: WorkContext, kind: "finance_group_edition" | "finance_group_ownership" | "finance_group_acquisition" | "finance_group_pair", id: string) { return invoke(() => application().groupExpansionRecord(actor, kind, id)); }
export async function groupEditionAccountSources(actor: WorkContext, id: string, accountCode: string, page: number) { return invoke(() => application().groupEditionAccountSources(actor, id, accountCode, page)); }
