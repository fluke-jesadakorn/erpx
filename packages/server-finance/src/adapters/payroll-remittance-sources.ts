import {financeOwnedSnapshotSources as peoplepayrollSnapshotSources} from "@erpx/server-people_payroll/public/finance-owned-snapshot-sources";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import {assertion} from "@erpx/server-workflow/public/command-assertions";import type {PostedRun} from '@erpx/server-people_payroll/public/payroll-remittance-contract';export function payrollRemittanceSources(database:SqlDatabase){return {postedRun(runId:string,companyId:string){return database.prepare(`SELECT r.id,r.version,r.currency,r.period_end,j.chart_id,p.journal_id
    ${peoplepayrollSnapshotSources().payroll_remittance_sources_hr_payroll_runs_r_from} ${peoplepayrollSnapshotSources().payroll_remittance_sources_hr_payroll_postings_p_join} ON p.run_id=r.id
    JOIN finance_journals j ON j.id=p.journal_id AND j.status='posted'
    WHERE r.id=? AND r.company_id=? AND r.status='posted'`).bind(runId,companyId).first<PostedRun>();},
credits(journal_id:string,value:string){return database.prepare(`SELECT account_id,SUM(credit_minor-debit_minor) AS amount_minor
    FROM finance_journal_lines WHERE journal_id=? AND description=?
    GROUP BY account_id`).bind(journal_id,value).all<{account_id:string;amount_minor:number}>();},
bankTransaction(bankTransactionId:string,companyId:string){return database.prepare(`SELECT t.id,t.version,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.match_status,
      s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
    WHERE t.id=? AND t.company_id=?`).bind(bankTransactionId,companyId).first<{
    id:string;version:number;transaction_date:string;external_id:string;amount_minor:number;currency:string;match_status:string;bank_account:string;
  }>();},
bankAccount(bankAccountId:string,companyId:string,chart_id:string,currency:string){return database.prepare(`SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=?
    AND category='asset' AND currency=? AND status='active'`).bind(bankAccountId,companyId,chart_id,currency).first();},
periods(companyId:string,transaction_date:string,transaction_date2:string){return database.prepare(`SELECT id FROM finance_periods WHERE company_id=? AND state='open'
    AND start_date<=? AND end_date>=?`).bind(companyId,transaction_date,transaction_date2).all<{id:string}>();},
guard9(key:string,value:"liability-balance",journal_id:string,value2:string,amountMinor:number,runId:string,kind:"withholding_tax" | "social_security"){return assertion(database,key,value,`(SELECT COALESCE(SUM(credit_minor-debit_minor),0) FROM finance_journal_lines WHERE journal_id=? AND description=?)
      >= ? + (SELECT COALESCE(SUM(amount_minor),0) ${peoplepayrollSnapshotSources().payroll_remittance_sources_hr_payroll_statutory_remittances_from} WHERE run_id=? AND kind=?)`,
      [journal_id,value2,amountMinor,runId,kind]);},
guard10(key:string,value:"bank-match",bankTransactionId:string,companyId:string,bankVersion:number,value2:number,currency:string,bankReference:string,period_end:string,bankAccountId:string){return assertion(database,key,value,`EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
      WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND
        t.amount_minor=? AND t.currency=? AND t.external_id=? AND t.transaction_date>=? AND s.bank_account=?)`,
      [bankTransactionId,companyId,bankVersion,value2,currency,bankReference,period_end,bankAccountId]);},
matchBank(remittanceId:string,bankTransactionId:string,bankVersion:number){return database.prepare(`UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_remittance',matched_id=?,
      version=version+1 WHERE id=? AND version=?`).bind(remittanceId,bankTransactionId,bankVersion);}};}
