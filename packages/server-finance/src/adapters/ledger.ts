import {withManufacturingInitialStockJournalRead} from '@erpx/server-finance/adapters/manufacturing-initial-stock-journal-read';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {initialPhysicalStockFinanceJournalFilter} from '@erpx/server-finance/adapters/document-purchases';
import {packageClockSql} from '@erpx/server-commercial/public/purchased-capability-sql';
import {requireFinanceDocumentHistory} from "@erpx/server-finance/adapters/document-purchases";
import { requireFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";
import {workflowReferenceRows} from "@erpx/server-finance/adapters/workflow-document-sources";
import {financeJournalPeopleFacts} from '@erpx/server-people_payroll/public/finance-journal-people-facts';
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { accountingProfiles } from "@erpx/server-finance/public/accounting-profiles";
import { ApiError } from "@erpx/server-runtime/public/http";
import { listCharts, statements, type ReportDates } from "@erpx/server-finance/public/finance";
import { createLedgerService } from '@erpx/server-finance/application/ledger';
import type { LedgerRow } from '@erpx/server-finance/domain/ledger';
export type { LedgerRow } from '@erpx/server-finance/domain/ledger';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export async function ledgerWorkspace(actor: WorkContext, chartId?: string, query?: {
    status?: string;
    cursor?: string;
    print?: string;
    reportDates?: ReportDates;
    periodId?: string;
}) { return ledgerService(actor).ledgerWorkspace(chartId, query); }
export async function workJournalRows(actor: WorkContext, provided?:SqlDatabase) { try { return await ledgerService(actor,provided).workJournalRows(); } catch (error) { if (error instanceof ApiError && error.code === "PACKAGE_REQUIRED") return []; throw error; } }
export async function journalDetail(actor: WorkContext, id: string, provided?:SqlDatabase) { return withManufacturingInitialStockJournalRead(actor,id,database=>ledgerService(actor,database,id).journalDetail(id),provided); }
function ledgerService(actor: WorkContext,provided?:SqlDatabase,documentId?:string) {
    let store: Promise<SqlDatabase> | undefined;
    const db = () => store ??= (provided ? Promise.resolve(provided) : holdingResources(actor.holdingId).then(resources => resources.write)).then(async write => { if(documentId)await requireFinanceDocumentHistory(write,actor,"journal",documentId);else await requireFinancePurchasedRead(write,actor); return write; });
    return createLedgerService({ error: (...args) => new ApiError(...args), charts: async () => await listCharts(actor,await db()) as LedgerRow[], profiles: () => accountingProfiles(actor), accounts: async (chartId) => (await (await db()).prepare("SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY code").bind(chartId, actor.companyId).all<LedgerRow>()).results, periods: async () => (await (await db()).prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(actor.companyId).all<LedgerRow>()).results, journals: async (chartId, query, paginated) => {
            const write = await db();
            let initialReader=true;try{await requireQueuedCommandAuthority(write,actor,'finance.read');}catch(error){if(error instanceof ApiError&&(error.status===401||error.status===403))initialReader=false;else throw error;}
            const clock=`WITH finance_ledger_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms) `;
            const initialFilter=`(SELECT ${initialPhysicalStockFinanceJournalFilter('j','finance_ledger_clock.now_ms',initialReader)} FROM finance_ledger_clock)`;
            return paginated ? readListPage<LedgerRow & {
                id: string;
                page_key: string;
            }>(write, { sql: `${clock}SELECT j.*,j.created_at::text page_key,COALESCE((SELECT SUM(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) debit,
        COALESCE((SELECT SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) credit,(SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=j.id) line_count FROM finance_journals j
        WHERE j.chart_id=? AND j.company_id=? AND (? IN ('','all') OR j.status=?) AND ${initialFilter}`, args: [chartId, actor.companyId, query?.status ?? '', query?.status ?? ''], cursor: query?.cursor, descending: true, scope: [actor.companyId, chartId, query?.status ?? ''] }).then(page => ({ results: page.items, page })) : write.prepare(`${clock}SELECT j.*,COALESCE(SUM(l.debit_minor),0) debit,COALESCE(SUM(l.credit_minor),0) credit,COUNT(l.id) line_count FROM finance_journals j LEFT JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.chart_id=? AND j.company_id=? AND ${initialFilter} GROUP BY j.id ORDER BY j.created_at DESC,j.id`).bind(chartId, actor.companyId).all<LedgerRow>().then(result => ({ ...result, page: null }));
        }, statements: (periodId, chartId, dates) => statements(actor, periodId, chartId, dates), dimensions: async () => (await (await db()).prepare("SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>()).results, dimensionValues: async () => (await (await db()).prepare("SELECT id,dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>()).results, workJournals: async () => (await workflowReferenceRows(await db(),"journal",actor.companyId)).results, journal: async (id) => (await db()).prepare(`SELECT j.*,${financeJournalPeopleFacts().createdBy},${financeJournalPeopleFacts().postedBy} FROM finance_journals j WHERE j.id=? AND j.company_id=?`).bind(id, actor.companyId).first<LedgerRow>(), journalLines: async (id) => (await (await db()).prepare("SELECT l.*,a.code,a.name FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE l.journal_id=? ORDER BY l.ordinal").bind(id).all<LedgerRow>()).results, draftHistory: async (id) => (await (await db()).prepare("SELECT * FROM finance_manual_draft_editions WHERE journal_id=? AND company_id=? ORDER BY journal_version").bind(id, actor.companyId).all<LedgerRow>()).results });
}
