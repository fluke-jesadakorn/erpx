import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {salesGoodsPostingFinanceState,validateSalesGoodsPostingFinanceState} from '@erpx/server-finance/adapters/sales-goods-posting-setup';
import type {PosPostingSetupInput} from '@erpx/server-commerce/public/pos-posting-setup';
import {assertProductionModuleEnabled} from '@erpx/server-pms/public/hospitality-rollout';
/** Original Company Finance setup authority, without Finance discovery or a
 * personal Finance module requirement. The actual POS owner still separately
 * requires its three original bootstrap permissions and independent review. */
export async function requirePosStartupFinanceAuthority(database:SqlDatabase,actor:WorkContext,permission:'finance.read'|'finance.write'|'finance.approve'='finance.write'){
 requirePermission(actor,'finance.read');requirePermission(actor,permission);
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment for POS posting setup.');
 const current=await requireQueuedCommandAuthority(database,actor,permission);requirePermission(current,'finance.read');
 if(current.scope.kind!=='holding'||current.branchId||current.departmentId||current.companyId!==actor.companyId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a current Company Finance assignment.');
 for(const feature of ['pos','inventory','finance'] as const)await assertProductionModuleEnabled(database,actor.companyId,feature);
 return current;
}
export {salesGoodsPostingFinanceState as posStartupFinanceState,salesGoodsPostingEffectiveService as posStartupEffectiveService} from '@erpx/server-finance/adapters/sales-goods-posting-setup';
export function posStartupFinanceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,finance_periods,finance_book_postings,finance_invoices,finance_invoice_line_versions,finance_allocations,finance_payments,finance_payment_reversals,finance_customer_receipts,finance_journals,finance_journal_lines,finance_financial_events,company_money_settings,company_currency_scales,company_fx_rates,money_transaction_snapshots IN SHARE MODE')];}
export async function validatePosStartupFinanceState(database:SqlDatabase,actor:WorkContext,input:PosPostingSetupInput,requiresApprovedDefaults:boolean){
 await validateSalesGoodsPostingFinanceState(database,actor,input,requiresApprovedDefaults);
 const state=await salesGoodsPostingFinanceState(database,actor),chart=state.references.charts.find(row=>row.id===input.chartId);
 if(chart?.base_currency!==input.currency)throw new ApiError(422,'POS_CURRENCY_MISMATCH','The original POS bootstrap requires document currency to equal the active chart functional currency.');
}
