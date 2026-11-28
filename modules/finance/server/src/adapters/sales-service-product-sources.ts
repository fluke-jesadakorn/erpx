import { aggByCompany } from './finance-read-tail-r20-sql';
import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {commerceFinanceJoinedFacts} from '@erpx/finance-server/adapters/commerce-joined-facts';
import { MONEY_TXN_SNAPSHOT_SOURCE_SQL } from './finance-read-tail-r18-sql';
/** Private original quote fields captured by the original Commerce application.
 * This is not an input or a caller-selected source authority. */
export type SalesServiceProductMoney={
 input:{sourceType:string;sourceId:string;amountKind:string;transactionDate:string;currency:string;currencyScale:number;amountUnits:string};
 amountUnits:string;usdUnits:string;currencyScale:number;rateId:string|null;usedLatestFallback:boolean;
};
export function salesServiceProductFinanceLeases(database:SqlDatabase,write=true){return [
 database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE'),
 database.prepare(`LOCK TABLE money_transaction_snapshots IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`),
];}
/** The existing Finance settings reader remains authoritative for the actual
 * document currency, approved defaults and active chart. No collections escape. */
export async function salesServiceProductFinanceSource(database:SqlDatabase,companyId:string){
 const effective=await commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId});
 const reference=await database.prepare(`SELECT jsonb_build_object('charts',COALESCE(${aggByCompany('c','finance_charts','id')},'[]'::jsonb),'accounts',COALESCE(${aggByCompany('a','finance_accounts','id')},'[]'::jsonb),'defaults',COALESCE(${aggByCompany('d','finance_sales_defaults','id')},'[]'::jsonb),'settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id=?),'scales',COALESCE(${aggByCompany('s','company_currency_scales','currency')},'[]'::jsonb),'rates',COALESCE(${aggByCompany('r','company_fx_rates','id')},'[]'::jsonb))::text image`).bind(companyId,companyId,companyId,companyId,companyId,companyId).first<string>('image');
 return {effective,image:JSON.stringify([effective,reference])};
}
type MoneyRow={company_id:string;source_type:string;source_id:string;amount_kind:string;transaction_date:string;currency:string;currency_scale:number;amount_units:string;usd_units:string;fx_rate_id:string|null;used_latest_fallback:boolean;captured_at:number;source:string};
export async function salesServiceProductMoneySource(database:SqlDatabase,companyId:string,productId:string){
 return (await database.prepare(MONEY_TXN_SNAPSHOT_SOURCE_SQL).bind(companyId,`${productId}:1`).all<MoneyRow>()).results;
}
export function salesServiceProductMoneyMatches(rows:Awaited<ReturnType<typeof salesServiceProductMoneySource>>,companyId:string,productId:string,expected:SalesServiceProductMoney){
 const row=rows[0],input=expected.input,source=row?JSON.parse(row.source) as Record<string,unknown>:null;
 const nativeQuote=source&&['original_source_type','original_source_id','original_amount_kind','carried_source_type','carried_source_id','carried_amount_kind','carried_allocation_offset_units'].every(field=>source[field]===null);
 return !!nativeQuote&&rows.length===1&&row.company_id===companyId&&row.source_type==='commerce_product_revision'&&input.sourceType===row.source_type&&row.source_id===`${productId}:1`&&input.sourceId===row.source_id&&row.amount_kind==='unit_price'&&input.amountKind===row.amount_kind&&row.transaction_date===input.transactionDate&&row.currency===input.currency&&Number(row.currency_scale)===expected.currencyScale&&Number(row.currency_scale)===input.currencyScale&&row.amount_units===expected.amountUnits&&row.amount_units===input.amountUnits&&row.usd_units===expected.usdUnits&&row.fx_rate_id===expected.rateId&&row.used_latest_fallback===expected.usedLatestFallback&&Number.isSafeInteger(Number(row.captured_at));
}
