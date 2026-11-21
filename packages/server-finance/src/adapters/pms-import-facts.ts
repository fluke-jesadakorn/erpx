import "server-only";
import { financePmsImport } from '@erpx/server-collaboration/adapters/native-finance-pms-import';
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
        }) => financePmsImport(db, facts.companyId).party({ partyId: facts.partyId, companyId: facts.companyId, code: facts.fact2, name: facts.name, now: facts.now, now2: facts.now2 }).then(s => s.run()),
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
        }) => financePmsImport(db, facts.companyId).payment({ paymentId: facts.paymentId, companyId: facts.companyId, partyId: facts.partyId, paymentNumber: facts.fact3, cutoverDate: facts.cutoverDate, total: facts.total, journalId: facts.opening_journal_id, createdBy: facts.created_by, approvedBy: facts.approved_by, now: facts.now, now2: facts.now2 }).then(s => s.run())
    };
}
