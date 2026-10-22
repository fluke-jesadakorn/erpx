import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function pmsImportFinanceFacts(db: SqlDatabase) {
    return {
        customer: (facts: {
            partyId: string | undefined;
            companyId: string;
        }) => db.prepare("SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND currency='THB' AND status='active' FOR SHARE").bind(facts.partyId, facts.companyId).first(),
        openingJournal: (facts: {
            journalId: string;
            companyId: string;
            chartId: string;
            businessDate: string;
        }) => db.prepare(`SELECT id FROM finance_journals WHERE id=? AND company_id=? AND chart_id=? AND status='posted' AND is_opening=1 AND entry_date=? AND posted_by IS NOT NULL AND NOT iam_same_person(created_by,posted_by,company_id) FOR SHARE`).bind(facts.journalId, facts.companyId, facts.chartId, facts.businessDate).first(),
        openingInvoice: (facts: {
            openingInvoiceId: string;
            companyId: string;
            journalId: string;
            due: number;
            due2: number;
            receivable: string;
            partyId: string;
        }) => db.prepare(`SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND journal_id=? AND source_type='opening_import'
      AND status='posted' AND outstanding_minor=? AND total_minor=? AND currency='THB' AND debit_account_id=? AND party_id=? FOR SHARE`).bind(facts.openingInvoiceId, facts.companyId, facts.journalId, facts.due, facts.due2, facts.receivable, facts.partyId).first(),
        receivableSchedules: (facts: {
            companyId: string;
            journalId: string;
        }) => db.prepare("SELECT count(*) count FROM finance_invoices WHERE company_id=? AND journal_id=? AND kind='receivable' AND outstanding_minor>0").bind(facts.companyId, facts.journalId).first<number>("count"),
        accountBalance: (facts: {
            journalId: string;
            account: string;
        }) => db.prepare("SELECT COALESCE(sum(debit_minor-credit_minor),0) value FROM finance_journal_lines WHERE journal_id=? AND account_id=?").bind(facts.journalId, facts.account).first<number>("value"),
        party: (facts: {
            partyId: string;
            companyId: string;
            fact2: string;
            name: string;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,'THB','active',?,?) ON CONFLICT(id) DO NOTHING`).bind(facts.partyId, facts.companyId, facts.fact2, facts.name, facts.now, facts.now2).run(),
        payment: (facts: {
            paymentId: string;
            companyId: string;
            partyId: string;
            fact3: string;
            cutoverDate: string;
            total: number;
            opening_journal_id: string | null;
            created_by: string;
            approved_by: string;
            now: number;
            now2: number;
        }) => db.prepare(`INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'receipt',?,?,'THB',?,0,'settled',?,?,?,?,?)`).bind(facts.paymentId, facts.companyId, facts.partyId, facts.fact3, facts.cutoverDate, facts.total, facts.opening_journal_id, facts.created_by, facts.approved_by, facts.now, facts.now2).run()
    };
}
