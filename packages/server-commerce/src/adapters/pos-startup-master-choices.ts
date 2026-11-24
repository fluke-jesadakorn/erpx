import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {posStartupMappingState} from '@erpx/server-commerce/adapters/pos-posting-setup-sources';
import {nativeCommerceReadAll} from './native-commerce-read';
export function posStartupMasterChoiceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_settings,commerce_warehouses,commerce_products IN SHARE MODE')];}
export async function posStartupMasterChoices(database:SqlDatabase,actor:WorkContext){
 const [warehouses,products,mapping]=await Promise.all([
 nativeCommerceReadAll<{id:string;branch_id:string;code:string;name:string;status:string;version:number}>('commerce_rd_pos_startup_warehouses',[actor.companyId,actor.branchId,actor.branchId],database),
 nativeCommerceReadAll<{id:string;sku:string;name:string;item_type:string;tracking_mode:string;unit:string;price_minor:number;tax_bps:number;status:string;version:number}>('commerce_rd_pos_startup_products',[actor.companyId],database),
 posStartupMappingState(database,actor.companyId),
 ]);return {warehouses,products,mapping};
}
