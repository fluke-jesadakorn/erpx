import { activeAccountListSql } from './finance-read-tail-r51-sql';
import { financeReferenceListSql } from './finance-read-tail-r42-sql';
import { finPartiesAllByCompanySql } from './finance-read-tail-r57-sql';
import {
  financeAgingBalancesNative,
  financeInvoicesPageNative,
  financeOverviewCountsNative,
} from './native-lane-2/client';
import { nativeFinanceReadAll, nativeFinanceReadFirst, FINANCE_RD_INVOICE_REPORT_CHARTS, FINANCE_RD_INVOICE_ACCOUNTS, FINANCE_RD_INVOICE_LINE_HISTORY, FINANCE_RD_INVOICE_THAI_DOCUMENTS, FINANCE_RD_INVOICE_PAYMENT_REVERSAL, FINANCE_RD_INVOICE_OPEN_PERIODS, FINANCE_RD_INVOICE_STATEMENT_ROWS, FINANCE_RD_INVOICE_OPEN_INVOICES } from './native-finance-read';
import { FINANCE_RD_INVOICE2_INVOICE_SNAPSHOT, FINANCE_RD_INVOICE2_INVOICE_ALLOCATIONS, FINANCE_RD_INVOICE2_PAYMENTS, FINANCE_RD_INVOICE2_PAYMENT_ALLOCATIONS, FINANCE_RD_INVOICE2_BANK_CANDIDATES, FINANCE_RD_INVOICE2_BANK_ACCOUNTS, FINANCE_RD_INVOICE2_BANK_STATEMENT } from './native-finance-read';
import { financeInvoiceListNative } from './native-lane-7/client';


import { requireFinanceDocumentHistory } from "@erpx/finance-server/adapters/document-purchases";
import { OPEN_PERIOD_LIST_SQL } from './finance-read-tail-r15-sql';
import { BANK_TRANSACTION_STATEMENT_SQL } from './finance-read-tail-r34-sql';
import { CHART_EXISTS_BY_COMPANY_SQL } from './finance-read-tail-r36-sql';
import { APPROVED_SECONDARY_CHART_CORRELATED_GUARD } from './finance-read-tail-r41-sql';
import { PERIOD_EXISTS_BY_ID_SQL } from './finance-read-tail-r37-sql';
import { PERIOD_DATES_SQL } from './finance-read-tail-r18-sql';
import { salesInvoiceLines } from "@erpx/finance-server/adapters/sales-lines";

import { createFinanceResourcePolicy } from '@erpx/finance-server/application/access';

import { createFinanceReads } from '@erpx/finance-server/application/finance-reads';

import { type FinanceRow } from '@erpx/finance-server/domain/finance';
import { fiscalCloseSourceSql } from "@erpx/finance-server/public/finance-fiscal-close-source";
import { readListPage } from "@erpx/work-projection-server/public/list-page";

import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";

import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";

export async function resources(actor:WorkContext,readOnly=false){createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,readOnly);return holdingResources(actor.holdingId);}
export async function getInvoice(actor: WorkContext, invoiceId: string, provided?:SqlDatabase) { const db=provided??(await holdingResources(actor.holdingId)).write;await requireFinanceDocumentHistory(db,actor,"invoice",invoiceId);return financeReadService(db).getInvoice(actor,invoiceId); }
export function financeReadService(provided?:SqlDatabase,historyOnly=false) {
    let pending: Promise<SqlDatabase> | undefined;
    let currentActor: WorkContext | undefined;
    const db = (actor: WorkContext) => pending ??= provided ? (createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,true),Promise.resolve(provided)) : resources(actor, true).then(r => r.write);
    return createFinanceReads({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { currentActor = actor; await db(actor); }, currentLines: async (companyId, id, revision) => salesInvoiceLines(await pending!, currentActor!, companyId, id, revision), reportCharts: async (actor) => (await nativeFinanceReadAll<{
            id: string;
        }>(FINANCE_RD_INVOICE_REPORT_CHARTS, [actor.companyId], await db(actor))).results, reportChart: async (actor, id) => !!await (await db(actor)).prepare(CHART_EXISTS_BY_COMPANY_SQL).bind(id, actor.companyId).first(), reportPeriod: async (actor, id) => !!await (await db(actor)).prepare(PERIOD_EXISTS_BY_ID_SQL).bind(id, actor.companyId).first(), reportPeriodDates: async (actor, id) => (await db(actor)).prepare(PERIOD_DATES_SQL).bind(id, actor.companyId).first<{
            start_date: string;
            end_date: string;
        }>(), trialRows: async ({ actor, periodId, chartId, dates, options }) => {
            const write = await db(actor);
            const clauses = ["j.company_id=?", "j.status IN ('posted','reversed')"];
            const args: (string | number)[] = [actor.companyId];
            if (periodId) {
                clauses.push("j.period_id=?");
                args.push(periodId);
            }
            if (options.excludeYearClose)
                clauses.push(`NOT ${fiscalCloseSourceSql()}`);
            if (chartId) {
                clauses.push("j.chart_id=?");
                args.push(chartId);
            }
            if (dates.startDate) {
                clauses.push("j.entry_date>=?");
                args.push(dates.startDate);
            }
            if (dates.endDate) {
                clauses.push("j.entry_date<=?");
                args.push(dates.endDate);
            }
            const rows = await write.prepare(`SELECT a.id,a.code,a.name,a.category,COALESCE(SUM(l.base_debit_minor),0) AS debit,COALESCE(SUM(l.base_credit_minor),0) AS credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE ${clauses.join(" AND ")} GROUP BY a.id,a.code,a.name,a.category ORDER BY a.code`).bind(...args).all<FinanceRow>();
            return rows.results;
        },
        accounts: async (facts) => { const { actor, chartId } = facts; const write = await db(actor); return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_ACCOUNTS, [actor.companyId, chartId], await db(actor))).results; },
        invoices: async (facts) => financeInvoiceListNative(facts.actor, facts.limit),
        workInvoices: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT i.* FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND NOT erpx_finance_field_invoice_owned(i.id,i.company_id) ORDER BY i.updated_at DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        invoice: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return nativeFinanceReadFirst<FinanceRow>(FINANCE_RD_INVOICE2_INVOICE_SNAPSHOT, [invoiceId, actor.companyId], await db(actor));
        },
        invoiceAllocations: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE2_INVOICE_ALLOCATIONS, [invoiceId, actor.companyId], await db(actor))).results;
        },
        lineHistory: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_LINE_HISTORY, [invoiceId, actor.companyId], await db(actor))).results; },
        thaiDocuments: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_THAI_DOCUMENTS, [invoiceId, actor.companyId], await db(actor))).results; },
        payments: async (facts) => {
            const { actor, limit } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE2_PAYMENTS, [actor.companyId, Math.max(1, Math.min(limit, 100))], await db(actor))).results;
        },
        payment: async (facts) => {
            const { paymentId, actor } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT p.*,f.code AS party_code,f.name AS party_name,
      (SELECT r.result_json::jsonb->>'settlementInvoiceId' FROM command_receipts r
        WHERE r.record_id=p.id AND r.operation='finance.payment.create' AND r.authority_json::jsonb->>'companyId'=p.company_id LIMIT 1) settlement_invoice_id
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.id=? AND p.company_id=?`).bind(paymentId, actor.companyId).first<FinanceRow>();
        },
        paymentAllocations: async (facts) => {
            const { paymentId, actor } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE2_PAYMENT_ALLOCATIONS, [paymentId, actor.companyId], await db(actor))).results;
        },
        paymentReversal: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return nativeFinanceReadFirst<FinanceRow>(FINANCE_RD_INVOICE_PAYMENT_REVERSAL, [paymentId], await db(actor)); },
        openPeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(OPEN_PERIOD_LIST_SQL).bind(actor.companyId).all<FinanceRow>()).results; },
        confirmInvoices: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_OPEN_INVOICES, [actor.companyId, payment.party_id, payment.currency, payment.kind === "receipt" ? "receivable" : "payable", payment.settlement_invoice_id ?? null, payment.settlement_invoice_id ?? null], await db(actor))).results;
        },
        matchingBankRows: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE2_BANK_CANDIDATES, [actor.companyId, payment.bank_account, payment.currency, payment.kind === "receipt" ? payment.amount_minor : -(Number(payment.amount_minor) - Number(payment.withheld_minor ?? 0)), payment.payment_date], await db(actor))).results;
        },
        fxAccounts: async (facts) => { if(historyOnly)return [];
            const { payment, actor } = facts;
            const write = await db(actor);
            return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE2_BANK_ACCOUNTS, [payment.bank_account, actor.companyId], await db(actor))).results;
        },
        workspaceAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(financeReferenceListSql('finance_accounts',"company_id=? AND status='active'",'code')).bind(actor.companyId).all<FinanceRow>()).results; },
        workspaceParties: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare(finPartiesAllByCompanySql(" AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name")).bind(actor.companyId).all<FinanceRow>()).results; },
        workspacePeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_OPEN_PERIODS, [actor.companyId], await db(actor))).results; },
        statement: async (facts) => { const { id, actor } = facts; const write = await db(actor); return nativeFinanceReadFirst<FinanceRow>(FINANCE_RD_INVOICE2_BANK_STATEMENT, [id, actor.companyId], await db(actor)); },
        statementRows: async (facts) => { const { id, actor, page } = facts; const write = await db(actor); return (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_INVOICE_STATEMENT_ROWS, [id, actor.companyId, (page - 1) * 30], write)).results; },
        agingBalances: async (facts) => financeAgingBalancesNative(facts.actor, facts.asOf, facts.kind, facts.chartId),
        financeOverviewCounts: async (actor) => financeOverviewCountsNative(actor),
        invoicesPage: async (actor, query) => financeInvoicesPageNative(actor, query),
        bankWorkspace: async (actor, query) => {
            const write = await db(actor);
            const statementSql = `SELECT s.*,a.currency,c.base_currency,s.statement_date AS page_key FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.company_id=? AND (?='' OR a.chart_id=?)`;
            const transactionSql = `SELECT t.*,s.bank_account,s.statement_number,t.transaction_date AS page_key FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id WHERE t.company_id=? AND (?='' OR a.chart_id=?) AND (?<>'unmatched' OR t.match_status<>'matched') AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
            const statementArgs = [actor.companyId, query?.chart ?? '', query?.chart ?? ''];
            const transactionArgs = [...statementArgs, query?.view ?? '', query?.q ?? '', query?.q ?? ''];
            const paged = !!query && query.print !== '1';
            const [accounts, statements, transactions, payments, selectedTransaction] = await Promise.all([
                !query || query.new === '1' ? write.prepare(`SELECT a.id,a.chart_id,a.code,a.name,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' AND ${APPROVED_SECONDARY_CHART_CORRELATED_GUARD} ORDER BY CASE WHEN a.control_kind='bank' THEN 0 ELSE 1 END,a.code,a.id`).bind(actor.companyId).all<FinanceRow>() : { results: [] as FinanceRow[] },
                paged ? readListPage<FinanceRow & {
                    id: string;
                    page_key: string;
                }>(write, { sql: statementSql, args: statementArgs, cursor: query.statementCursor, descending: true, scope: [actor.companyId, 'bank-statements', query?.chart ?? ''] }).then(page => ({ results: page.items, page })) :
                    write.prepare(`${statementSql} ORDER BY s.statement_date DESC,s.id`).bind(...statementArgs).all<FinanceRow>().then(result => ({ ...result, page: null })),
                paged ? readListPage<FinanceRow & {
                    id: string;
                    page_key: string;
                }>(write, { sql: transactionSql, args: transactionArgs, cursor: query.transactionCursor, descending: true, scope: [actor.companyId, 'bank-transactions', query?.chart ?? '', query?.view ?? '', query?.q ?? ''] }).then(page => ({ results: page.items, page })) :
                    write.prepare(`${transactionSql} ORDER BY t.transaction_date DESC,t.id`).bind(...transactionArgs).all<FinanceRow>().then(result => ({ ...result, page: null })),
                !query || query.match ? write.prepare("SELECT p.* FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.status IN ('released','settled') AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date DESC").bind(actor.companyId).all<FinanceRow>() : { results: [] as FinanceRow[] },
                query?.match ? write.prepare(BANK_TRANSACTION_STATEMENT_SQL).bind(query.match, actor.companyId).first<FinanceRow>() : null,
            ]);
            return { accounts: accounts.results, statements: statements.results, statementPage: statements.page, transactions: transactions.results, transactionPage: transactions.page, payments: payments.results, selectedTransaction };
        } });
}
