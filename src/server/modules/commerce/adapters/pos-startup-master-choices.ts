import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {posStartupMappingState} from './pos-posting-setup-sources';
export function posStartupMasterChoiceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_settings,commerce_warehouses,commerce_products IN SHARE MODE')];}
export async function posStartupMasterChoices(database:SqlDatabase,actor:WorkContext){
 const [warehouses,products,mapping]=await Promise.all([
 database.prepare("SELECT id,branch_id,code,name,status,version FROM commerce_warehouses WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY id").bind(actor.companyId,actor.branchId,actor.branchId).all<{id:string;branch_id:string;code:string;name:string;status:string;version:number}>(),
 database.prepare('SELECT id,sku,name,item_type,tracking_mode,unit,price_minor,tax_bps,status,version FROM commerce_products WHERE company_id=? ORDER BY id').bind(actor.companyId).all<{id:string;sku:string;name:string;item_type:string;tracking_mode:string;unit:string;price_minor:number;tax_bps:number;status:string;version:number}>(),
 posStartupMappingState(database,actor.companyId),
 ]);return {warehouses:warehouses.results,products:products.results,mapping};
}
