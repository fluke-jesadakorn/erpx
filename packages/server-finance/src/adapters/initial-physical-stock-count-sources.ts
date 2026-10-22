import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryCountSources} from '@erpx/server-finance/adapters/inventory-counts-sources';
/** Same original Count posting configuration and exactly-one open period rule.
 * No accounts, period or quote is created by this source reader. */
export async function initialPhysicalStockCountFinanceSource(database:SqlDatabase,companyId:string,date:string){
 const owner=inventoryCountSources(database),config=await owner.variancePostingConfig(companyId),periods=await owner.postingPeriods(companyId,date,date);
 if(!config)return {config:null,period:periods.results.length===1?periods.results[0]:null,image:JSON.stringify([null,periods.results])};
 const rows=await database.prepare(`SELECT jsonb_build_object('chart',to_jsonb(c),'inventory',to_jsonb(i),'variance',to_jsonb(a),'periods',COALESCE((SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM finance_periods p WHERE p.company_id=c.company_id AND p.state='open' AND p.start_date<=? AND p.end_date>=?),'[]'::jsonb))::text image FROM finance_charts c JOIN finance_accounts i ON i.id=? AND i.company_id=c.company_id AND i.chart_id=c.id AND i.currency=c.base_currency AND i.category='asset' AND i.control_kind='none' AND i.status='active' JOIN finance_accounts a ON a.id=? AND a.company_id=c.company_id AND a.chart_id=c.id AND a.currency=c.base_currency AND a.category='expense' AND a.code='5400' AND a.status='active' WHERE c.id=? AND c.company_id=? AND c.status='active'`).bind(date,date,config.inventory_account_id,config.variance_account_id,config.chart_id,companyId).first<string>('image');
 return {config:rows?config:null,period:periods.results.length===1?periods.results[0]:null,image:rows??JSON.stringify([null,periods.results])};
}
export function initialPhysicalStockCountFinanceLeases(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_periods,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE'),database.prepare('LOCK TABLE finance_journals,finance_journal_lines,finance_book_postings,finance_financial_events,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE')];}
