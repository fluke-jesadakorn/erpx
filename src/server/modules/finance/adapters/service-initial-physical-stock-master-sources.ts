import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commerceFinanceJoinedFacts} from './commerce-joined-facts';
/** Private original quote fields captured by the original Commerce application.
 * This is not an input or a caller-selected source authority. */
export type ServiceInitialPhysicalStockProductMoney={
 input:{sourceType:string;sourceId:string;amountKind:string;transactionDate:string;currency:string;currencyScale:number;amountUnits:string};
 amountUnits:string;usdUnits:string;currencyScale:number;rateId:string|null;usedLatestFallback:boolean;
};
export function serviceInitialPhysicalStockMasterFinanceLeases(database:SqlDatabase){return [
 database.prepare('LOCK TABLE finance_charts,finance_accounts,finance_sales_defaults,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE'),
 database.prepare('LOCK TABLE money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE'),
];}
/** The existing Finance settings reader remains authoritative for the actual
 * document currency, approved defaults and active chart. No collections escape. */
export async function serviceInitialPhysicalStockMasterFinanceSource(database:SqlDatabase,companyId:string,chartId:string,currency:string){
 const [effective,reference]=await Promise.all([
  commerceFinanceJoinedFacts(database).readCommerceSettings1({companyId}),
  database.prepare(`SELECT jsonb_build_object('chart',(SELECT to_jsonb(c) FROM finance_charts c WHERE c.id=? AND c.company_id=?),'settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id=?),'scale',(SELECT to_jsonb(s) FROM company_currency_scales s WHERE s.company_id=? AND s.currency=?),'rates',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM company_fx_rates r WHERE r.company_id=? AND r.currency=?),'[]'::jsonb))::text image`).bind(chartId,companyId,companyId,companyId,currency,companyId,currency).first<string>('image'),
 ]);return {effective,image:JSON.stringify([effective,reference])};
}
type MoneyRow={company_id:string;source_type:string;source_id:string;amount_kind:string;transaction_date:string;currency:string;currency_scale:number;amount_units:string;usd_units:string;fx_rate_id:string|null;used_latest_fallback:boolean;captured_at:number;source:string};
export async function serviceInitialPhysicalStockProductMoneySource(database:SqlDatabase,companyId:string,productId:string){
 return (await database.prepare(`SELECT s.company_id,s.source_type,s.source_id,s.amount_kind,s.transaction_date::text,s.currency,s.currency_scale,s.amount_units::text,s.usd_units::text,s.fx_rate_id,s.used_latest_fallback,s.captured_at,to_jsonb(s)::text source FROM money_transaction_snapshots s WHERE s.company_id=? AND s.source_type='commerce_product_revision' AND s.source_id=? ORDER BY s.amount_kind`).bind(companyId,`${productId}:1`).all<MoneyRow>()).results;
}
export function serviceInitialPhysicalStockProductMoneyMatches(rows:Awaited<ReturnType<typeof serviceInitialPhysicalStockProductMoneySource>>,companyId:string,productId:string,expected:ServiceInitialPhysicalStockProductMoney){
 const row=rows[0],input=expected.input,source=row?JSON.parse(row.source) as Record<string,unknown>:null;
 const nativeQuote=source&&['original_source_type','original_source_id','original_amount_kind','carried_source_type','carried_source_id','carried_amount_kind','carried_allocation_offset_units'].every(field=>source[field]===null);
 return !!nativeQuote&&rows.length===1&&row.company_id===companyId&&row.source_type==='commerce_product_revision'&&input.sourceType===row.source_type&&row.source_id===`${productId}:1`&&input.sourceId===row.source_id&&row.amount_kind==='unit_price'&&input.amountKind===row.amount_kind&&row.transaction_date===input.transactionDate&&row.currency===input.currency&&Number(row.currency_scale)===expected.currencyScale&&Number(row.currency_scale)===input.currencyScale&&row.amount_units===expected.amountUnits&&row.amount_units===input.amountUnits&&row.usd_units===expected.usdUnits&&row.fx_rate_id===expected.rateId&&row.used_latest_fallback===expected.usedLatestFallback&&Number.isSafeInteger(Number(row.captured_at));
}
