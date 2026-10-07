import {financeOwnedSnapshotSources as pmsSnapshotSources} from "../../pms/public/finance-owned-snapshot-sources";
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) => (await db
    .prepare(sql)
    .bind(...values)
    .all<T>()).results;
const run = async (db: SqlDatabase, sql: string, ...values: unknown[]) => db
    .prepare(sql)
    .bind(...values)
    .run();
export function pmsFinanceFacts(db: SqlDatabase) {
    return {
        readFinanceAccounts: (facts: {
            companyId: string;
        }) => query<{
            id: string;
            name: string;
            code: string;
            chart_id: string;
            category: string;
        }>(db, "SELECT id,name,code,chart_id,category FROM finance_accounts WHERE company_id=? AND status='active' AND currency='THB' ORDER BY code", facts.companyId),
        readFinanceCharts: (facts: {
            companyId: string;
        }) => query<{
            id: string;
            name: string;
        }>(db, "SELECT id,name FROM finance_charts WHERE company_id=? AND base_currency='THB' AND status='active'", facts.companyId),
        readFinanceParties: (facts: {
            companyId: string;
        }) => query<{
            id: string;
            name: string;
        }>(db, "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL", facts.companyId),
        readFinanceJournalLines: (facts: {
            id: string;
        }) => query<{
            account_id: string;
            debit: number;
            credit: number;
        }>(db, `SELECT l.account_id,SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN (SELECT property_id,journal_id ${pmsSnapshotSources().pms_command_facts_pms_postings_from} UNION SELECT property_id,opening_journal_id ${pmsSnapshotSources().pms_command_facts_pms_import_batches_from} WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=l.journal_id JOIN finance_journals j ON j.id=l.journal_id WHERE p.property_id=? AND j.status='posted' GROUP BY l.account_id`, facts.id),
        readFinanceBankTransactions: (facts: {
            companyId: string;
            bank: string;
            currency: string;
        }) => query<{
            id: string;
            transaction_date: string;
            external_id: string;
            amount_minor: number;
            currency: string;
            version: number;
        }>(db, `SELECT t.id,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.version
        FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
        WHERE t.company_id=? AND b.bank_account=? AND t.currency=? AND t.amount_minor>0 AND t.match_status IN ('unmatched','proposed')
        ORDER BY t.transaction_date DESC,t.id LIMIT 100`, facts.companyId, facts.bank, facts.currency),
        readFinanceCharts2: (facts: {
            chartId: string;
            companyId: string;
            currency: string;
        }) => db.prepare("SELECT id FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency=?").bind(facts.chartId, facts.companyId, facts.currency).first(),
        readFinanceAccounts2: (facts: {
            id: string;
            companyId: string;
            chartId: string;
            currency: string;
        }) => db.prepare("SELECT category FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND status='active' FOR SHARE").bind(facts.id, facts.companyId, facts.chartId, facts.currency).first<{
            category: string;
        }>(),
        readFinanceAccounts3: (facts: {
            account: string;
            companyId: string;
            chartId: string;
            currency: string;
        }) => db.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='revenue' AND status='active'").bind(facts.account, facts.companyId, facts.chartId, facts.currency).first(),
        readFinanceAccounts4: (facts: {
            id: string;
            companyId: string;
            chartId: string;
            currency: string;
        }) => db.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='liability' AND status='active'").bind(facts.id, facts.companyId, facts.chartId, facts.currency).first(),
        readFinancePeriods: (facts: {
            companyId: string;
            date: string;
            date2: string;
        }) => query<{
            id: string;
        }>(db, "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? FOR SHARE", facts.companyId, facts.date, facts.date2),
        insertFinanceParties: (facts: {
            party: string;
            companyId: string;
            fact2: string;
            name: string;
            currency: string;
            now: number;
            now2: number;
        }) => run(db, "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,?,'active',?,?) ON CONFLICT(id) DO NOTHING", facts.party, facts.companyId, facts.fact2, facts.name, facts.currency, facts.now, facts.now2),
        insertFinanceInvoices: (facts: {
            id: string;
            companyId: string;
            party: string;
            fact3: string;
            businessDate: string;
            fact5: string;
            currency: string;
            fact7: number;
            tax: number;
            total: number;
            currentDue: number;
            receivable: string;
            revenue: string;
            tax2: string;
            id2: string;
            fact15: "paid" | "posted";
            fact16: string | null;
            userId: string;
            approvedBy: string | null;
            now: number;
            now2: number;
            now3: number;
        }) => run(db, "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,?,?,?,?,'pms',?,?,?,?,?,?,?,?)", facts.id, facts.companyId, facts.party, facts.fact3, facts.businessDate, facts.fact5, facts.currency, facts.fact7, facts.tax, facts.total, facts.currentDue, facts.receivable, facts.revenue, facts.tax2, facts.id2, facts.fact15, facts.fact16, facts.userId, facts.approvedBy, facts.now, facts.now2, facts.now3),
        insertFinancePayments: (facts: {
            id: string;
            companyId: string;
            party: string;
            fact3: "refund" | "receipt";
            fact4: string;
            businessDate: string;
            currency: string;
            amount: number;
            fact8: string;
            fact9: string | null;
            userId: string;
            userId2: string;
            now: number;
            now2: number;
        }) => run(db, "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?)", facts.id, facts.companyId, facts.party, facts.fact3, facts.fact4, facts.businessDate, facts.currency, facts.amount, facts.fact8, facts.fact9, facts.userId, facts.userId2, facts.now, facts.now2),
        deleteFinanceAllocations: (facts: {
            id: string;
            companyId: string;
        }) => run(db, "DELETE FROM finance_allocations WHERE invoice_id=? AND company_id=?", facts.id, facts.companyId),
        updateFinanceInvoices: (facts: {
            outstanding: number;
            outstanding2: number;
            outstanding3: number;
            outstanding4: number;
            fact4: number;
            id: string;
            companyId: string;
        }) => run(db, "UPDATE finance_invoices SET outstanding_minor=?,base_outstanding_minor=?,status=CASE WHEN ?=0 THEN 'paid' WHEN ?<total_minor THEN 'partially_paid' ELSE 'posted' END,version=version+1,updated_at=? WHERE id=? AND company_id=?", facts.outstanding, facts.outstanding2, facts.outstanding3, facts.outstanding4, facts.fact4, facts.id, facts.companyId),
        insertFinanceAllocations: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            id: string;
            id2: string;
            n: number;
            userId: string;
            fact6: number;
        }) => run(db, "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)", facts.fact0, facts.companyId, facts.id, facts.id2, facts.n, facts.userId, facts.fact6),
        readFinanceParties2: (facts: {
            partyId: string;
            companyId: string;
            currency: string;
        }) => db.prepare("SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL").bind(facts.partyId, facts.companyId, facts.currency).first(),
        updateFinanceBankTransactions: (facts: {
            bankTransactionId: string;
            companyId: string;
        }) => db.prepare(`SELECT t.*,b.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
          WHERE t.id=? AND t.company_id=? FOR UPDATE OF t`).bind(facts.bankTransactionId, facts.companyId).first<{
            id: string;
            version: number;
            match_status: string;
            bank_account: string;
            currency: string;
            amount_minor: number;
            external_id: string;
            transaction_date: string;
        }>(),
        updateFinanceBankTransactions2: (facts: {
            id: string;
            matchedBankId: string;
        }) => run(db, "UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=? AND match_status IN ('unmatched','proposed')", facts.id, facts.matchedBankId)
    };
}
