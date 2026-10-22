import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function posStartupSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE pos_settings,pos_registers,pos_catalog,pos_register_catalog,pos_sales,pos_shifts,pos_payments,pos_refunds,pos_postings IN SHARE MODE')];}
export async function posStartupSourceState(database:SqlDatabase,companyId:string){
 const state=await database.prepare(`SELECT
 EXISTS(SELECT 1 FROM pos_settings WHERE company_id=?) OR EXISTS(SELECT 1 FROM pos_sales WHERE company_id=?) OR EXISTS(SELECT 1 FROM pos_shifts WHERE company_id=?) OR EXISTS(SELECT 1 FROM pos_postings WHERE company_id=?) economics,
 jsonb_build_object('settings',(SELECT to_jsonb(s) FROM pos_settings s WHERE s.company_id=?),'registers',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM pos_registers r WHERE r.company_id=?),'catalog',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.product_id) FROM pos_catalog c WHERE c.company_id=?),'registerCatalog',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.register_id,c.product_id) FROM pos_register_catalog c JOIN pos_registers r ON r.id=c.register_id WHERE r.company_id=?),'sales',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM pos_sales s WHERE s.company_id=?),'shifts',(SELECT jsonb_agg(to_jsonb(s) ORDER BY s.id) FROM pos_shifts s WHERE s.company_id=?),'payments',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM pos_payments p WHERE p.company_id=?),'refunds',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM pos_refunds r WHERE r.company_id=?),'postings',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.source_key) FROM pos_postings p WHERE p.company_id=?))::text image`)
 .bind(...Array.from({length:13},()=>companyId)).first<{economics:boolean;image:string}>();
 if(!state)throw Error('Original POS startup source unavailable');return state;
}
