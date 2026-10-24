

import { requireFinanceDocumentHistory } from "@erpx/server-finance/adapters/document-purchases";
import { salesInvoiceLines } from "@erpx/server-finance/adapters/sales-lines";

import { createFinanceResourcePolicy } from '@erpx/server-finance/application/access';

import { createFinanceReads } from '@erpx/server-finance/application/finance-reads';

import { type FinanceRow, type HistoricalAgingRow } from '@erpx/server-finance/domain/finance';
import { fiscalCloseSourceSql } from "@erpx/server-finance/public/finance-fiscal-close-source";
import { readListPage } from "@erpx/server-work_projection/public/list-page";

import { invoiceHistoryCte } from "@erpx/server-finance/public/finance-history";

import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";

export async function resources(actor:WorkContext,readOnly=false){createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,readOnly);return holdingResources(actor.holdingId);}
export async function getInvoice(actor: WorkContext, invoiceId: string, provided?:SqlDatabase) { const db=provided??(await holdingResources(actor.holdingId)).write;await requireFinanceDocumentHistory(db,actor,"invoice",invoiceId);return financeReadService(db).getInvoice(actor,invoiceId); }
export function financeReadService(provided?:SqlDatabase,historyOnly=false) {
    let pending: Promise<SqlDatabase> | undefined;
    const db = (actor: WorkContext) => pending ??= provided ? (createFinanceResourcePolicy({error:(...args)=>new ApiError(...args),hasPermission})(actor,true),Promise.resolve(provided)) : resources(actor, true).then(r => r.write);
    return createFinanceReads({ error: (...args) => new ApiError(...args), authorize: requirePermission, ready: async (actor) => { await db(actor); }, currentLines: async (companyId, id, revision) => salesInvoiceLines(await pending!, companyId, id, revision), reportCharts: async (actor) => (await (await db(actor)).prepare('SELECT id FROM finance_charts WHERE company_id=?').bind(actor.companyId).all<{
            id: string;
        }>()).results, reportChart: async (actor, id) => !!await (await db(actor)).prepare('SELECT 1 FROM finance_charts WHERE id=? AND company_id=?').bind(id, actor.companyId).first(), reportPeriod: async (actor, id) => !!await (await db(actor)).prepare('SELECT 1 FROM finance_periods WHERE id=? AND company_id=?').bind(id, actor.companyId).first(), reportPeriodDates: async (actor, id) => (await db(actor)).prepare('SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?').bind(id, actor.companyId).first<{
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
        }, charts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_charts WHERE company_id=? ORDER BY name").bind(actor.companyId).all<FinanceRow>()).results; },
        accounts: async (facts) => { const { actor, chartId } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code").bind(actor.companyId, chartId).all<FinanceRow>()).results; },
        invoices: async (facts) => {
            const { actor, limit } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?`).bind(actor.companyId, Math.max(1, Math.min(limit, 100))).all<FinanceRow>()).results;
        },
        workInvoices: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT i.* FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? ORDER BY i.updated_at DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        invoice: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return write.prepare(`SELECT i.*,p.code AS party_code,p.name AS party_name,
    s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId, actor.companyId).first<FinanceRow>();
        },
        invoiceAllocations: async (facts) => {
            const { invoiceId, actor } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT a.id,a.payment_id,a.amount_minor,a.created_at,p.payment_number,p.status,p.payment_slip_id
    FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id
    WHERE a.invoice_id=? AND a.company_id=? ORDER BY a.created_at`).bind(invoiceId, actor.companyId).all<FinanceRow>()).results;
        },
        lineHistory: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT source_version,origin_kind,COUNT(*) line_count,SUM(subtotal_minor) subtotal_minor,SUM(tax_minor) tax_minor,MIN(created_at) created_at FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? GROUP BY source_version,origin_kind ORDER BY source_version").bind(invoiceId, actor.companyId).all<FinanceRow>()).results; },
        thaiDocuments: async (facts) => { const { invoiceId, actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,document_number,document_kind,edition,issue_date FROM finance_issued_thai_documents WHERE invoice_id=? AND company_id=? ORDER BY edition").bind(invoiceId, actor.companyId).all<FinanceRow>()).results; },
        payments: async (facts) => {
            const { actor, limit } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.company_id=? ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT ?`).bind(actor.companyId, Math.max(1, Math.min(limit, 100))).all<FinanceRow>()).results;
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
            return (await write.prepare(`SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status
    FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id
    WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at`).bind(paymentId, actor.companyId).all<FinanceRow>()).results;
        },
        paymentReversal: async (facts) => { const { paymentId, actor } = facts; const write = await db(actor); return write.prepare("SELECT * FROM finance_payment_reversals WHERE payment_id=?").bind(paymentId).first<FinanceRow>(); },
        openPeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        confirmInvoices: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT id,invoice_number,kind,outstanding_minor,status,version
      FROM finance_invoices WHERE company_id=? AND party_id=? AND currency=? AND kind=? AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim' AND (?::text IS NULL OR id=?)
      ORDER BY due_date,invoice_number`).bind(actor.companyId, payment.party_id, payment.currency, payment.kind === "receipt" ? "receivable" : "payable", payment.settlement_invoice_id ?? null, payment.settlement_invoice_id ?? null).all<FinanceRow>()).results;
        },
        matchingBankRows: async (facts) => { if(historyOnly)return [];
            const { actor, payment } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number
      FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
      WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.transaction_date>=?
      ORDER BY t.transaction_date DESC,t.external_id`).bind(actor.companyId, payment.bank_account, payment.currency, payment.kind === "receipt" ? payment.amount_minor : -(Number(payment.amount_minor) - Number(payment.withheld_minor ?? 0)), payment.payment_date).all<FinanceRow>()).results;
        },
        fxAccounts: async (facts) => { if(historyOnly)return [];
            const { payment, actor } = facts;
            const write = await db(actor);
            return (await write.prepare(`SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency
      FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id JOIN finance_accounts a ON a.chart_id=c.id
      WHERE bank.id=? AND bank.company_id=? AND a.company_id=bank.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')
      ORDER BY a.code`).bind(payment.bank_account, actor.companyId).all<FinanceRow>()).results;
        },
        workspaceAccounts: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<FinanceRow>()).results; },
        workspaceParties: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_parties WHERE company_id=? AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name").bind(actor.companyId).all<FinanceRow>()).results; },
        workspacePeriods: async (facts) => { const { actor } = facts; const write = await db(actor); return (await write.prepare("SELECT * FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>()).results; },
        statement: async (facts) => { const { id, actor } = facts; const write = await db(actor); return write.prepare("SELECT s.*,a.currency,a.code,a.name,c.base_currency FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.id=? AND s.company_id=?").bind(id, actor.companyId).first<FinanceRow>(); },
        statementRows: async (facts) => { const { id, actor, page } = facts; const write = await db(actor); return (await write.prepare('SELECT * FROM finance_bank_transactions WHERE statement_id=? AND company_id=? ORDER BY transaction_date,id LIMIT 31 OFFSET ?').bind(id, actor.companyId, (page - 1) * 30).all<FinanceRow>()).results; },
        agingBalances: async (facts) => {
            const { actor, asOf, kind, chartId } = facts;
            const write = await db(actor);
            return (await write.prepare(`${invoiceHistoryCte}
    SELECT i.*,p.code party_code,p.name party_name FROM historical_invoice_balances i
    JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id
    WHERE i.kind=? AND i.historical_outstanding_minor>0 AND (?::text IS NULL OR i.chart_id=?) ORDER BY i.due_date,i.id`)
                .bind(actor.companyId, asOf, kind, chartId ?? null, chartId ?? null).all<HistoricalAgingRow>()).results;
        },
        listJournals: async (actor, query) => {
            const write = await db(actor);
            const clauses = ["company_id=?"];
            const args: (string | number)[] = [actor.companyId];
            if (query.periodId) {
                clauses.push("period_id=?");
                args.push(query.periodId);
            }
            if (query.status) {
                clauses.push("status=?");
                args.push(query.status);
            }
            return (await write.prepare(`SELECT * FROM finance_journals WHERE ${clauses.join(" AND ")} ORDER BY entry_date DESC,journal_number DESC LIMIT 500`).bind(...args).all<FinanceRow>()).results;
        },
        financeOverviewCounts: async (actor) => {
            const write = await db(actor);
            return (await write.prepare(`SELECT
    (SELECT count(*) FROM finance_journals WHERE company_id=? ) AS journals,
    (SELECT count(*) FROM finance_journals WHERE company_id=? AND status='draft') AS posting,
    (SELECT count(*) FROM finance_invoices WHERE company_id=?) AS invoices,
    (SELECT count(*) FROM finance_payments WHERE company_id=?) AS payments,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS receivable,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS payable,
    (SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched') AS reconcile`)
                .bind(...Array(7).fill(actor.companyId)).first<{
                journals: number;
                posting: number;
                invoices: number;
                payments: number;
                receivable: number;
                payable: number;
                reconcile: number;
            }>())!;
        },
        invoicesPage: async (actor, query) => {
            const write = await db(actor);
            return readListPage<FinanceRow & {
                id: string;
                page_key: string;
            }>(write, {
                sql: `SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name,
      s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id,
      i.invoice_date||':'||i.invoice_number AS page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
      LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
      WHERE i.company_id=?
      AND (?='' OR EXISTS (SELECT 1 FROM finance_accounts a WHERE a.id=i.debit_account_id AND a.company_id=i.company_id AND a.chart_id=?))
      AND (?='' OR i.kind=?) AND (?='' OR i.status=?) AND (?<>'open' OR (i.status IN ('posted','partially_paid') AND i.outstanding_minor>0))
      AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)`,
                args: [actor.companyId, query.chartId ?? '', query.chartId ?? '', query.kind ?? '', query.kind ?? '', query.status ?? '', query.status ?? '', query.queue ?? '', query.q ?? '', query.q ?? ''],
                descending: true, cursor: query.cursor, scope: [actor.companyId, 'invoices', query.chartId ?? '', query.q ?? '', query.kind ?? '', query.status ?? '', query.queue ?? ''],
            });
        },
        bankWorkspace: async (actor, query) => {
            const write = await db(actor);
            const statementSql = `SELECT s.*,a.currency,c.base_currency,s.statement_date AS page_key FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.company_id=? AND (?='' OR a.chart_id=?)`;
            const transactionSql = `SELECT t.*,s.bank_account,s.statement_number,t.transaction_date AS page_key FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id WHERE t.company_id=? AND (?='' OR a.chart_id=?) AND (?<>'unmatched' OR t.match_status<>'matched') AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
            const statementArgs = [actor.companyId, query?.chart ?? '', query?.chart ?? ''];
            const transactionArgs = [...statementArgs, query?.view ?? '', query?.q ?? '', query?.q ?? ''];
            const paged = !!query && query.print !== '1';
            const [accounts, statements, transactions, payments, selectedTransaction] = await Promise.all([
                !query || query.new === '1' ? write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=a.company_id AND r.secondary_chart_id=a.chart_id AND r.status='approved') ORDER BY CASE WHEN a.control_kind='bank' THEN 0 ELSE 1 END,a.code,a.id").bind(actor.companyId).all<FinanceRow>() : { results: [] as FinanceRow[] },
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
                query?.match ? write.prepare("SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?").bind(query.match, actor.companyId).first<FinanceRow>() : null,
            ]);
            return { accounts: accounts.results, statements: statements.results, statementPage: statements.page, transactions: transactions.results, transactionPage: transactions.page, payments: payments.results, selectedTransaction };
        } });
}
