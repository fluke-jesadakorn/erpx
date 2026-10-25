import {workflowReferenceRows} from "./workflow-document-sources";
import {financeJournalPeopleFacts} from '@/server/modules/people_payroll/public/finance-journal-people-facts';
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { accountingProfiles } from "@/server/modules/finance/public/accounting-profiles";
import { ApiError } from "@/server/modules/runtime/public/http";
import { listCharts, statements, type ReportDates } from "@/server/modules/finance/public/finance";
import { createLedgerService } from '../application/ledger';
import type { LedgerRow } from '../domain/ledger';
export type { LedgerRow } from '../domain/ledger';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
export async function ledgerWorkspace(actor: WorkContext, chartId?: string, query?: {
    status?: string;
    cursor?: string;
    print?: string;
    reportDates?: ReportDates;
    periodId?: string;
}) { return ledgerService(actor).ledgerWorkspace(chartId, query); }
export async function workJournalRows(actor: WorkContext, provided?:SqlDatabase) { return ledgerService(actor,provided).workJournalRows(); }
export async function journalDetail(actor: WorkContext, id: string, provided?:SqlDatabase) { return ledgerService(actor,provided).journalDetail(id); }
function ledgerService(actor: WorkContext,provided?:SqlDatabase) {
    let store: Promise<SqlDatabase> | undefined;
    const db = () => store ??= provided ? Promise.resolve(provided) : holdingResources(actor.holdingId).then(resources => resources.write);
    return createLedgerService({ error: (...args) => new ApiError(...args), charts: async () => await listCharts(actor,provided) as LedgerRow[], profiles: () => accountingProfiles(actor), accounts: async (chartId) => (await (await db()).prepare("SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY code").bind(chartId, actor.companyId).all<LedgerRow>()).results, periods: async () => (await (await db()).prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(actor.companyId).all<LedgerRow>()).results, journals: async (chartId, query, paginated) => {
            const write = await db();
            return paginated ? readListPage<LedgerRow & {
                id: string;
                page_key: string;
            }>(write, { sql: `SELECT j.*,j.created_at::text page_key,COALESCE((SELECT SUM(debit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) debit,
        COALESCE((SELECT SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=j.id),0) credit,(SELECT COUNT(*) FROM finance_journal_lines WHERE journal_id=j.id) line_count FROM finance_journals j
        WHERE j.chart_id=? AND j.company_id=? AND (? IN ('','all') OR j.status=?)`, args: [chartId, actor.companyId, query?.status ?? '', query?.status ?? ''], cursor: query?.cursor, descending: true, scope: [actor.companyId, chartId, query?.status ?? ''] }).then(page => ({ results: page.items, page })) : write.prepare("SELECT j.*,COALESCE(SUM(l.debit_minor),0) debit,COALESCE(SUM(l.credit_minor),0) credit,COUNT(l.id) line_count FROM finance_journals j LEFT JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.chart_id=? AND j.company_id=? GROUP BY j.id ORDER BY j.created_at DESC,j.id").bind(chartId, actor.companyId).all<LedgerRow>().then(result => ({ ...result, page: null }));
        }, statements: (periodId, chartId, dates) => statements(actor, periodId, chartId, dates), dimensions: async () => (await (await db()).prepare("SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>()).results, dimensionValues: async () => (await (await db()).prepare("SELECT id,dimension_id,code,name FROM finance_dimension_values WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<LedgerRow>()).results, workJournals: async () => (await workflowReferenceRows(await db(),"journal",actor.companyId)).results, journal: async (id) => (await db()).prepare(`SELECT j.*,${financeJournalPeopleFacts().createdBy},${financeJournalPeopleFacts().postedBy} FROM finance_journals j WHERE j.id=? AND j.company_id=?`).bind(id, actor.companyId).first<LedgerRow>(), journalLines: async (id) => (await (await db()).prepare("SELECT l.*,a.code,a.name FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE l.journal_id=? ORDER BY l.ordinal").bind(id).all<LedgerRow>()).results, draftHistory: async (id) => (await (await db()).prepare("SELECT * FROM finance_manual_draft_editions WHERE journal_id=? AND company_id=? ORDER BY journal_version").bind(id, actor.companyId).all<LedgerRow>()).results });
}
