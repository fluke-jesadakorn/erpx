import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/public/posting-setup';
import {commerceFinanceJoinedFacts} from '@erpx/server-finance/adapters/commerce-joined-facts';

type Edition={id:string;version:number;status:string;chart_id:string;currency:string;receivable_account_id:string;revenue_account_id:string;tax_account_id:string|null;source:string};
const serviceFields=['chart_id','currency','receivable_account_id','revenue_account_id','output_tax_account_id'] as const;
/** The original independently reviewed Sales intent is an owned reference,
 * not a Sales purchase or an authority inferred from another product. */
export function approvedSalesPostingDefaultSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_sales_defaults IN SHARE MODE')];}
export async function approvedSalesPostingEffectiveService(database:SqlDatabase,companyId:string){const effective=await commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId});return effective?Object.fromEntries(serviceFields.map(field=>[field,effective[field]??null])):null;}
export async function approvedSalesPostingDefaults(database:SqlDatabase,companyId:string){
 const [editions,effective]=await Promise.all([
  database.prepare('SELECT d.id,d.version,d.status,d.chart_id,d.currency,d.receivable_account_id,d.revenue_account_id,d.tax_account_id,to_jsonb(d)::text source FROM finance_sales_defaults d WHERE d.company_id=? ORDER BY d.id').bind(companyId).all<Edition>(),
  commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId}),
 ]);
 const defaults=editions.results.find(row=>row.id===effective?.approved_sales_defaults_id&&row.status==='approved')??null;
 return {editions:editions.results,defaults,expectedSalesDefaults:defaults?{id:defaults.id,version:Number(defaults.version)}:null,service:effective?Object.fromEntries(serviceFields.map(field=>[field,effective[field]??null])):null,defaultsBlocked:!defaults&&editions.results.some(row=>row.status==='approved'),image:JSON.stringify(editions.results.map(row=>row.source))};
}
/** A reviewed null VAT remains null in the original effective reader. The
 * genuine nine-role mapping still retains its own non-null VAT account. */
export function approvedSalesPostingDefaultMatches(defaults:Edition|null,input:CommercePostingSetupConfiguration){return !defaults||input.chartId===defaults.chart_id&&input.currency===defaults.currency&&input.receivableAccountId===defaults.receivable_account_id&&input.revenueAccountId===defaults.revenue_account_id&&(defaults.tax_account_id===null||input.outputTaxAccountId===defaults.tax_account_id);}
