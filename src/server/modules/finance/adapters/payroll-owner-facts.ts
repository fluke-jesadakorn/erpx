import {financeSnapshotSources} from "../../people_payroll/public/finance-snapshot-sources";
import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
export function payrollFinanceFacts(database: SqlDatabase) {
    return { readPayrollPostingOptionsChartsperiodsmappingsgroups(companyId: string) { return database.prepare("SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active' ORDER BY name").bind(companyId).all(); },
        readPayrollPostingOptionsChartsperiodsmappingsgroups2(companyId: string) { return database.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(companyId).all(); },
        readPayrollPostingOptionsAccounts(companyId: string, chartIds: string[]) { return database.prepare(`SELECT id,chart_id,code,name,category,currency FROM finance_accounts WHERE company_id=? AND status='active' AND chart_id IN (${chartIds.map(() => "?").join(",")}) ORDER BY code`).bind(companyId, ...chartIds).all(); },
        readResolvePayrollPostPeriods(companyId: string, dateHint: string, dateHint2: string) {
            return database.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date").bind(companyId, dateHint, dateHint2).all<{
                id: string;
                start_date: string;
            }>();
        },
        readResolvePayrollPostNext(companyId: string, period_end: string) {
            return database.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date>? ORDER BY start_date LIMIT 1").bind(companyId, period_end).all<{
                id: string;
                start_date: string;
            }>();
        },
        readResolvePayrollPostEffect(companyId: string, entryDate: string, entryDate2: string) {
            return database.prepare("SELECT id,start_date FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(companyId, entryDate, entryDate2).all<{
                id: string;
                start_date: string;
            }>();
        },
        readSavePayrollAccountMappingChart(chartId: string, companyId: string) {
            return database.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId, companyId).first<{
                id: string;
                base_currency: string;
            }>();
        },
        readSavePayrollAccountMappingAccounts(companyId: string, chartId: string, ids: string[]) {
            return database.prepare(`SELECT id,currency,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")})`).bind(companyId, chartId, ...ids).all<{
                id: string;
                currency: string;
                category: string;
            }>();
        },
        readPreparePayrollAccrualValid(companyId: string, chartId: string, ids: string[]) {
            return database.prepare(`SELECT id,category,currency FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")})`).bind(companyId, chartId, ...ids).all<{
                id: string;
                category: string;
                currency: string;
            }>();
        },
        readCreatePaymentBatchInvoice(sourceId: string, companyId: string) {
            return database.prepare(`SELECT i.id,i.status,i.currency,i.total_minor,p.name AS legal_name
      FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
      WHERE i.id=? AND i.company_id=? AND i.kind='payable' AND p.kind='supplier' AND p.person_id IS NULL AND i.source_type!='expense_claim'`).bind(sourceId, companyId).first<{
                id: string;
                status: string;
                currency: string;
                total_minor: number;
                legal_name: string;
            }>();
        },
        readCreatePaymentBatchBank(bankAccountId: string, companyId: string, currency: string) { return database.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND currency=? AND status='active'").bind(bankAccountId, companyId, currency).first(); },
        guard185(commandId: string, value: "supplier-payment-reservation", sourceId: string) { return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown','confirmed'))", [sourceId]); },
        stageCreatePaymentBatchEffect(now: number, sourceId: string, companyId: string) { return database.prepare('UPDATE finance_invoices SET version=version+1,updated_at=? WHERE id=? AND company_id=?').bind(now, sourceId, companyId); },
        readConfirmPaymentBatchPosting(source_id: string, companyId: string) {
            return database.prepare(`SELECT p.journal_id,j.chart_id,r.pay_date ${financeSnapshotSources().payrollPostingSource} JOIN finance_journals j ON j.id=p.journal_id WHERE p.run_id=? AND r.company_id=? AND j.status='posted'`).bind(source_id, companyId).first<{
                journal_id: string;
                chart_id: string;
                pay_date: string;
            }>();
        },
        readConfirmPaymentBatchPayables(journal_id: string) {
            return database.prepare("SELECT DISTINCT account_id FROM finance_journal_lines WHERE journal_id=? AND description IN ('Net payroll payable','Net pay payable to employees')").bind(journal_id).all<{
                account_id: string;
            }>();
        },
        readConfirmPaymentBatchBankAccount(bank_account_id: string | null, companyId: string, chart_id: string, currency: string) { return database.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category='asset' AND status='active'").bind(bank_account_id, companyId, chart_id, currency).first(); },
        readConfirmPaymentBatchTransaction(bankTransactionId: string, companyId: string) {
            return database.prepare("SELECT t.id,t.version,t.amount_minor,t.currency,t.match_status,t.transaction_date,t.external_id,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?").bind(bankTransactionId, companyId).first<{
                id: string;
                version: number;
                amount_minor: number;
                currency: string;
                match_status: string;
                transaction_date: string;
                external_id: string;
                bank_account: string;
            }>();
        },
        readConfirmPaymentBatchPeriods(companyId: string, transaction_date: string, transaction_date2: string) {
            return database.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(companyId, transaction_date, transaction_date2).all<{
                id: string;
            }>();
        },
        guard216(commandId: string, value: string, id: string, companyId: string, bankVersion: number, bank_account_id: string, currency: string, value2: number, confirmationRef: string) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.external_id=?)", [id, companyId, bankVersion, bank_account_id, currency, value2, confirmationRef]); },
        stageConfirmPaymentBatchEffect2(id: string, id2: string, bankVersion: number) { return database.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_payment',matched_id=?,version=version+1 WHERE id=? AND version=?").bind(id, id2, bankVersion); } };
}
