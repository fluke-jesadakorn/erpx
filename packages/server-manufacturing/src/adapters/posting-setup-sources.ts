import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
export {manufacturingPurchaseSourceReadLocks as manufacturingPostingSourceReadLocks} from '@erpx/server-manufacturing/adapters/purchases';
/** A specification or an unexecuted planned order is intent, not a production
 * economic fact. Original postings/cost movements and actual issued/reported
 * quantities remain conservative blockers when their mapping is absent. */
export async function manufacturingPostingSourceState(database:SqlDatabase,companyId:string){
 const row=await database.prepare(`WITH scope AS(SELECT ?::text company_id),orders AS MATERIALIZED(SELECT o.* FROM manufacturing_orders o,scope WHERE o.company_id=scope.company_id),postings AS MATERIALIZED(SELECT p.* FROM manufacturing_order_postings p,scope WHERE p.company_id=scope.company_id),movements AS MATERIALIZED(SELECT m.* FROM manufacturing_cost_movements m,scope WHERE m.company_id=scope.company_id)
 SELECT EXISTS(SELECT 1 FROM postings) OR EXISTS(SELECT 1 FROM movements) OR EXISTS(SELECT 1 FROM orders WHERE material_cost_minor<>0 OR labor_cost_minor<>0 OR overhead_cost_minor<>0 OR reported_output_milli<>0 OR reported_scrap_milli<>0) economics,
 jsonb_build_array((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM orders o),(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM postings p),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM movements m))::text economic_image,
 jsonb_build_array((SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM orders o),(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM postings p),(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM movements m),(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM manufacturing_accounting_profiles p,scope WHERE p.company_id=scope.company_id))::text image`).bind(companyId).first<{economics:boolean;economic_image:string;image:string}>();
 if(!row)throw new ApiError(503,'MANUFACTURING_POSTING_SOURCE_UNAVAILABLE','The original production posting source is unavailable.');return row;
}
