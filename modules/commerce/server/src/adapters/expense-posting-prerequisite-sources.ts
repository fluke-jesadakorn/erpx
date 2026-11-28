import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {manufacturingPostingInventorySourceLocks,manufacturingPostingInventoryState} from '@erpx/inventory-server/public/manufacturing-posting-setup-sources';
import {manufacturingPostingSourceReadLocks,manufacturingPostingSourceState} from '@erpx/manufacturing-server/public/posting-setup-sources';
import {expensePostingSetupSourceLocks,expensePostingSetupSources} from '@erpx/people-payroll-server/public/expense-posting-setup-sources';
import {expensePostingFinanceReadLocks,expensePostingFinanceState} from '@erpx/finance-server/public/expense-posting-setup-sources';
import {validateCommercePostingFinanceReferences} from '@erpx/finance-server/public/commerce-posting-setup';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {posStartupCommerceReadLocks} from '@erpx/commerce-server/adapters/pos-posting-setup-sources';
import {manufacturingPostingDocumentState} from '@erpx/commerce-server/adapters/sales-goods-posting-setup';
import type {CommercePostingSetupConfiguration} from '@erpx/commerce-server/domain/posting-setup';
import {nativeCommerceReadFirst,RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE} from './native-commerce-read';

export function expensePostingPrerequisiteSourceLocks(database:SqlDatabase){return [...posStartupCommerceReadLocks(database),...manufacturingPostingInventorySourceLocks(database),...manufacturingPostingSourceReadLocks(database),...expensePostingSetupSourceLocks(database),...expensePostingFinanceReadLocks(database)];}
/** Company economic identity stays private and independent of the one newly
 * created Finance reference. A receipt is never a substitute for these rows. */
export async function expensePostingPrerequisiteSources(database:SqlDatabase,actor:WorkContext){
 const [row,documents,inventory,production,expense,finance]=await Promise.all([
  nativeCommerceReadFirst<{source:string}>(RD_EXPENSE_PREREQUISITE_SETTINGS_SOURCE,[actor.companyId],database),
  manufacturingPostingDocumentState(database,actor.companyId),manufacturingPostingInventoryState(database,actor.companyId),manufacturingPostingSourceState(database,actor.companyId),expensePostingSetupSources(database,actor.companyId),expensePostingFinanceState(database,actor),
 ]);
 const raw=row?JSON.parse(row.source) as Record<string,unknown>:null;
 const fields={chartId:'chart_id',currency:'currency',receivableAccountId:'receivable_account_id',payableAccountId:'payable_account_id',revenueAccountId:'revenue_account_id',inventoryAccountId:'inventory_account_id',clearingAccountId:'clearing_account_id',cogsAccountId:'cogs_account_id',inputTaxAccountId:'input_tax_account_id',outputTaxAccountId:'output_tax_account_id',cashAccountId:'cash_account_id'} as const;
 const mapping=raw?Object.fromEntries(Object.entries(fields).map(([name,column])=>[name,String(raw[column])])) as CommercePostingSetupConfiguration:null;
 let mappingValid=true;if(mapping){try{await validateCommercePostingFinanceReferences(database,actor,mapping);}catch(error){if(error instanceof ApiError&&error.status===422)mappingValid=false;else throw error;}}
 const history=documents.goods||inventory.economics||production.economics||expense.economics||finance.expenseEconomics||finance.historyInvalid;
 const defaultsRequired=finance.defaultsBlocked||documents.history&&!finance.defaults;
 return {configured:!!mapping,mappingValid,chartId:mapping?.chartId??finance.defaults?.chart_id??null,currency:mapping?.currency??finance.defaults?.currency??null,defaults:finance.expectedSalesDefaults,history,defaultsRequired,
  image:JSON.stringify([raw,documents.image,inventory.image,production.image,expense.image,finance.retainedFinancialImage,finance.retainedExpenseFinancialImage,finance.expectedSalesDefaults,finance.service,finance.defaultsBlocked,finance.historyInvalid,finance.expenseEconomics])};
}
