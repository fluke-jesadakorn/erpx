import {financeOwnedSnapshotSources as commerceSnapshotSources} from "@erpx/server-commerce/public/finance-owned-snapshot-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
export function CommerceReturnsFinanceFacts(db:SqlDatabase){return {
readFinanceJournalLines1:(facts:{invoiceId:string;companyId:string;})=>db.prepare(`SELECT i.id,i.company_id,i.kind,i.currency,i.total_minor,i.fx_rate_micros,i.debit_account_id,i.credit_account_id,
  j.id journal_id,j.chart_id,j.status,c.base_currency,
  (SELECT SUM(CASE WHEN i.kind='receivable' THEN l.base_debit_minor-l.base_credit_minor ELSE l.base_credit_minor-l.base_debit_minor END)
   FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END) control_base
  FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id JOIN finance_charts c ON c.id=j.chart_id
  WHERE i.id=? AND i.company_id=?`).bind(facts.invoiceId,facts.companyId).first<{id:string;company_id:string;kind:string;currency:string;total_minor:number;fx_rate_micros:number;debit_account_id:string;credit_account_id:string;journal_id:string;chart_id:string;status:string;base_currency:string;control_base:number}>(),
readFinanceJournalLines2:(facts:{incoming:boolean;control:string;id:string;returnId:string;})=>db.prepare(`SELECT COALESCE(SUM(r.total_minor),0) transaction_total,
  COALESCE(SUM((SELECT SUM(CASE WHEN ? THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END)
   FROM finance_journal_lines l WHERE l.journal_id=r.credit_journal_id AND l.account_id=?)),0) base_total
  ${commerceSnapshotSources().commerce_return_accounting_facts_commerce_returns_r_from} WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted')`).bind(facts.incoming,facts.control,facts.id,facts.returnId).first<{transaction_total:number;base_total:number}>(),
readFinanceJournalLines3:(facts:{journal_id:string;})=>db.prepare(`SELECT account_id,SUM(debit_minor-credit_minor) transaction_amount,
   SUM(base_debit_minor-base_credit_minor) base_amount FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id`).bind(facts.journal_id).all<{account_id:string;transaction_amount:number;base_amount:number}>(),
prepareFinanceInvoices1:(facts:{id:string;})=>db.prepare('SELECT id FROM finance_invoices WHERE id=? FOR UPDATE').bind(facts.id),
prepareFinanceJournalLines1:(facts:{baseDebit:number;baseCredit:number;id:string;ordinal:number;accountId:string;})=>db.prepare('UPDATE finance_journal_lines SET base_debit_minor=?,base_credit_minor=? WHERE journal_id=? AND ordinal=? AND account_id=?').bind(facts.baseDebit,facts.baseCredit,facts.id,facts.ordinal,facts.accountId),
guardRule2:(facts:{key:string;guardId:string;values:string[];})=>assertion(db,facts.key,facts.guardId,"(SELECT SUM(base_debit_minor)=SUM(base_credit_minor) FROM finance_journal_lines WHERE journal_id=?)",facts.values)
};}
