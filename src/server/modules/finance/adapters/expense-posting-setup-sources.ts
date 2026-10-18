import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {expensePostingSetupNativeSources} from '../../people_payroll/public/expense-posting-setup-sources';
import {requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import type {ExpensePostingSetupInput} from '../../commerce/public/expense-posting-setup';
import {salesGoodsPostingFinanceState,salesGoodsPostingEffectiveService} from './sales-goods-posting-setup';
import {validateCommercePostingFinanceReferences} from './commerce-posting-setup';
import {approvedSalesPostingDefaultMatches} from './approved-sales-posting-defaults';

/** Original Company Finance setup authority; no Supply/Sales/Stock module or
 * purchase is inferred from the physical Commerce mapping table. */
export async function requireExpensePostingFinanceAuthority(database:SqlDatabase,actor:WorkContext,permission:'finance.write'|'finance.approve'='finance.write'){
 requirePermission(actor,'finance.read');requirePermission(actor,permission);
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment for Expense posting setup.');
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');
 if(current.scope.kind!=='holding'||current.branchId||current.departmentId||current.companyId!==actor.companyId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use the original Company Finance assignment.');
 for(const feature of ['people_payroll','finance'])await assertProductionModuleEnabled(database,actor.companyId,feature);return current;
}
export function expensePostingFinanceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,finance_periods,finance_book_postings,finance_invoices,finance_invoice_line_versions,finance_allocations,finance_payments,finance_payment_reversals,finance_customer_receipts,finance_journals,finance_journal_lines,finance_financial_events,company_money_settings,company_currency_scales,company_fx_rates,money_transaction_snapshots IN SHARE MODE')];}
export async function expensePostingFinanceState(database:SqlDatabase,actor:WorkContext){
 const [original,expense]=await Promise.all([
  salesGoodsPostingFinanceState(database,actor),
  database.prepare(`WITH scope AS(SELECT ?::text company_id),claims AS MATERIALIZED(SELECT e.* FROM (${expensePostingSetupNativeSources}) e,scope WHERE e.company_id=scope.company_id),invoices AS MATERIALIZED(SELECT i.* FROM finance_invoices i,scope WHERE i.company_id=scope.company_id AND (i.source_type='expense_claim' OR EXISTS(SELECT 1 FROM claims e WHERE e.invoice_id=i.id))),primary_journals AS MATERIALIZED(SELECT j.* FROM finance_journals j,scope WHERE j.company_id=scope.company_id AND (j.source_type IN('expense_claim','expense_claim_reversal') OR EXISTS(SELECT 1 FROM claims e WHERE e.journal_id=j.id OR e.reversal_journal_id=j.id) OR EXISTS(SELECT 1 FROM invoices i WHERE i.journal_id=j.id))),books AS MATERIALIZED(SELECT b.* FROM finance_book_postings b JOIN primary_journals j ON j.id=b.primary_journal_id,scope WHERE b.company_id=scope.company_id),journals AS MATERIALIZED(SELECT j.* FROM finance_journals j,scope WHERE j.company_id=scope.company_id AND (EXISTS(SELECT 1 FROM primary_journals p WHERE p.id=j.id) OR EXISTS(SELECT 1 FROM books b WHERE b.secondary_journal_id=j.id)))
  SELECT EXISTS(SELECT 1 FROM invoices WHERE journal_id IS NOT NULL OR status NOT IN('draft','void')) OR EXISTS(SELECT 1 FROM primary_journals WHERE status IN('posted','reversed')) economics,jsonb_build_array((SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM invoices i),(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM journals j),(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.primary_journal_id,b.secondary_journal_id) FROM books b),(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.journal_id,l.id) FROM finance_journal_lines l JOIN journals j ON j.id=l.journal_id))::text image`).bind(actor.companyId).first<{economics:boolean;image:string}>(),
 ]);
 if(!expense)throw Error('Original Expense Finance source unavailable.');return {...original,expenseEconomics:expense.economics,retainedExpenseFinancialImage:expense.image,image:JSON.stringify([original.image,original.expectedSalesDefaults,expense.image])};
}
export async function validateExpensePostingFinanceState(database:SqlDatabase,actor:WorkContext,input:ExpensePostingSetupInput,requiresApprovedDefaults:boolean){
 const state=await expensePostingFinanceState(database,actor);
 if(state.historyInvalid||state.expenseEconomics)throw new ApiError(409,'EXPENSE_POSTING_HISTORY_REQUIRES_REVIEW','Reconcile the original Expense and Sales financial source before initial setup.');
 await validateCommercePostingFinanceReferences(database,actor,input);
 if(state.defaultsBlocked||requiresApprovedDefaults&&!state.defaults)throw new ApiError(409,'SALES_DEFAULTS_REQUIRED','Restore or independently approve current Sales defaults before adding posting roles to existing documents.');
 if(JSON.stringify(input.expectedSalesDefaults??null)!==JSON.stringify(state.expectedSalesDefaults)||!approvedSalesPostingDefaultMatches(state.defaults,input))throw new ApiError(409,'EXPENSE_POSTING_DEFAULTS_CHANGED','Retain the independently approved Sales chart, currency, receivable, revenue and VAT meaning.');
 return state;
}
export {salesGoodsPostingEffectiveService as expensePostingEffectiveService};
