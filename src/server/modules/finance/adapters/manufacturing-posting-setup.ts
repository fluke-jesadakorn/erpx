import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {salesGoodsPostingFinanceState,validateSalesGoodsPostingFinanceState} from './sales-goods-posting-setup';
import type {ManufacturingPostingSetupInput} from '../../commerce/public/manufacturing-posting-setup';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';
/** Existing Company Finance authority, independently of personal module lists.
 * This closed constructor never buys or discovers the Finance management UI. */
export async function requireManufacturingPostingFinanceAuthority(database:SqlDatabase,actor:WorkContext,permission:'finance.read'|'finance.write'|'finance.approve'='finance.write'){
 requirePermission(actor,'finance.read');requirePermission(actor,permission);
 if(actor.beta||actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment for initial production posting configuration.');
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');
 if(current.beta||current.scope.kind!=='holding'||current.branchId||current.departmentId||current.companyId!==actor.companyId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment.');
 for(const feature of ['manufacturing','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
// Reuse the complete original Finance-owned nine-role and independently
// approved Sales-default/service-history proof without requiring Sales purchase.
export {commercePostingFinanceSourceReadLocks as manufacturingPostingFinanceReferenceLocks} from './commerce-posting-setup';
export {salesGoodsPostingFinanceSourceLocks as manufacturingPostingFinanceHistoryLocks,salesGoodsPostingFinanceState as manufacturingPostingFinanceState,salesGoodsPostingEffectiveService as manufacturingPostingEffectiveService} from './sales-goods-posting-setup';

export {commercePostingFinanceReferences as manufacturingPostingFinanceReferences} from './commerce-posting-setup';
/** Current shared reader order; only Finance-owned sources are declared here. */
export function manufacturingPostingFinanceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,finance_periods,finance_book_postings,finance_invoices,finance_invoice_line_versions,finance_allocations,finance_payments,finance_payment_reversals,finance_customer_receipts,finance_journals,finance_journal_lines,finance_financial_events,company_money_settings,company_currency_scales,company_fx_rates,money_transaction_snapshots IN SHARE MODE')];}
export async function validateManufacturingPostingFinanceState(database:SqlDatabase,actor:WorkContext,input:ManufacturingPostingSetupInput,requiresApprovedDefaults:boolean){
 await validateSalesGoodsPostingFinanceState(database,actor,input,requiresApprovedDefaults);const state=await salesGoodsPostingFinanceState(database,actor);
 if(state.references.charts.find(chart=>chart.id===input.chartId)?.base_currency!=='THB')throw new ApiError(422,'MANUFACTURING_CURRENCY_ADAPTER','The original production accounting adapter requires a THB functional chart.');
}
