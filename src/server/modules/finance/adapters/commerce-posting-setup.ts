import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {WorkContext} from '../../identity_access/public/authorization';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {requireQueuedCommandAuthority} from '../../identity_access/public/queued-command-authority';
import {ApiError} from '../../runtime/public/http';
import {requireFinanceMoneyScale} from '../public/finance-money-policy';
import {assertProductionModuleEnabled} from '../../pms/public/hospitality-rollout';

type Chart={id:string;name:string;base_currency:string;status:string;version:number;source:string};
type Account={id:string;chart_id:string;code:string;name:string;category:string;currency:string;control_kind:string;status:string;version:number;source:string};
type Mapping={chartId:string;currency:string;receivableAccountId:string;payableAccountId:string;revenueAccountId:string;inventoryAccountId:string;clearingAccountId:string;cogsAccountId:string;inputTaxAccountId:string;outputTaxAccountId:string;cashAccountId:string};
const rules={receivableAccountId:['asset','receivable'],payableAccountId:['liability','payable'],revenueAccountId:['revenue','none'],inventoryAccountId:['asset','none'],clearingAccountId:['liability','none'],cogsAccountId:['expense','none'],inputTaxAccountId:['asset','none'],outputTaxAccountId:['liability','none'],cashAccountId:['asset','bank']} as const;

/** Same Company-level finance.write rule as the original initial ledger setup,
 * plus the original finance.read authority for chart/account references.
 * Current published IAM/selected context is checked without granting Finance. */
export async function requireCommercePostingFinanceAuthority(database:SqlDatabase,actor:WorkContext){
 requirePermission(actor,'finance.read');
 requirePermission(actor,'finance.write');
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_FINANCE_REQUIRED','Use a Company Finance assignment for initial posting configuration.');
 const current=await requireQueuedCommandAuthority(database,actor,'finance.write');
 requirePermission(current,'finance.read');
 await assertProductionModuleEnabled(database,actor.companyId,'procurement');await assertProductionModuleEnabled(database,actor.companyId,'finance');
 return current;
}
export function commercePostingFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_charts,finance_accounts,company_money_settings,company_currency_scales IN SHARE MODE')];}
/** Native owner rows stay private; consumers project only explicit choices. */
export async function commercePostingFinanceReferences(database:SqlDatabase,actor:WorkContext){
 const [charts,accounts]=await Promise.all([
  database.prepare('SELECT c.id,c.name,c.base_currency,c.status,c.version,to_jsonb(c)::text source FROM finance_charts c WHERE c.company_id=? ORDER BY c.id').bind(actor.companyId).all<Chart>(),
  database.prepare('SELECT a.id,a.chart_id,a.code,a.name,a.category,a.currency,a.control_kind,a.status,a.version,to_jsonb(a)::text source FROM finance_accounts a WHERE a.company_id=? ORDER BY a.id').bind(actor.companyId).all<Account>(),
 ]);
 return {charts:charts.results,accounts:accounts.results,image:JSON.stringify([charts.results.map(row=>row.source),accounts.results.map(row=>row.source)])};
}
/** No placeholder or cross-Company account can establish a posting role.
 * Document and functional accounts may differ, as in original foreign goods. */
export async function validateCommercePostingFinanceReferences(database:SqlDatabase,actor:WorkContext,input:Mapping){
 const references=await commercePostingFinanceReferences(database,actor),chart=references.charts.find(row=>row.id===input.chartId&&row.status==='active');
 if(!chart)throw new ApiError(422,'COMMERCE_POSTING_ACCOUNTS_INVALID','Choose an active chart belonging to this Company.');
 const accounts=new Map(references.accounts.map(row=>[row.id,row]));
 for(const field of Object.keys(rules) as Array<keyof typeof rules>){
  const account=accounts.get(input[field]),[category,control]=rules[field];
  if(!account||account.status!=='active'||account.chart_id!==chart.id||account.category!==category||account.control_kind!==control||![input.currency,chart.base_currency].includes(account.currency))
   throw new ApiError(422,'COMMERCE_POSTING_ACCOUNTS_INVALID','Every selected posting role must use an active Company account in the selected chart with its original category, control and currency meaning.');
 }
 await requireFinanceMoneyScale(database,actor.companyId,input.currency);await requireFinanceMoneyScale(database,actor.companyId,chart.base_currency);
 return references;
}
