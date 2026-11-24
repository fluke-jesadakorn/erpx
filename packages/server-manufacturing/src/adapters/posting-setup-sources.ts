import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {manufacturingExistsSelect,manufacturingJsonbAggSql,manufacturingScopedCte} from './manufacturing-read-sql-fragments';
import {manufacturingScopeCte} from './manufacturing-read-tail-r30-sql';
export {manufacturingPurchaseSourceReadLocks as manufacturingPostingSourceReadLocks} from '@erpx/server-manufacturing/adapters/purchases';
const productionAgg=(table:string,alias:string,scope="")=>manufacturingJsonbAggSql(`to_jsonb(${alias})`,`${alias}.id`,`${table} ${alias}`,scope);
const profileImage=productionAgg('manufacturing_accounting_profiles','p',',scope WHERE p.company_id=scope.company_id');
/** Text-only economics guard shared by the posting setup source state. The
 * three EXISTS were byte-identical apart from the source table and the optional
 * cost/quantity predicate; the helper emits each one verbatim on the caller's
 * prepared statement with unchanged bind order and stays outside the RD_*
 * read registry. */
const economicsGuard=(table:string,predicate="")=>`EXISTS(${manufacturingExistsSelect(table,predicate)})`;
/** A specification or an unexecuted planned order is intent, not a production
 * economic fact. Original postings/cost movements and actual issued/reported
 * quantities remain conservative blockers when their mapping is absent. */
export async function manufacturingPostingSourceState(database:SqlDatabase,companyId:string){
 const row=await database.prepare(`${manufacturingScopeCte(false)},${manufacturingScopedCte("orders","manufacturing_orders","o")},${manufacturingScopedCte("postings","manufacturing_order_postings","p")},${manufacturingScopedCte("movements","manufacturing_cost_movements","m")}
 SELECT ${economicsGuard("postings")} OR ${economicsGuard("movements")} OR ${economicsGuard("orders","material_cost_minor<>0 OR labor_cost_minor<>0 OR overhead_cost_minor<>0 OR reported_output_milli<>0 OR reported_scrap_milli<>0")} economics,
 jsonb_build_array(${productionAgg('orders','o')},${productionAgg('postings','p')},${productionAgg('movements','m')})::text economic_image,
 jsonb_build_array(${productionAgg('orders','o')},${productionAgg('postings','p')},${productionAgg('movements','m')},${profileImage})::text image`).bind(companyId).first<{economics:boolean;economic_image:string;image:string}>();
 if(!row)throw new ApiError(503,'MANUFACTURING_POSTING_SOURCE_UNAVAILABLE','The original production posting source is unavailable.');return row;
}
