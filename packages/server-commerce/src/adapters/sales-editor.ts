import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {checkPurchasedCapability,companyPurchaseReadLocks,commercePackageEnforcementMode} from '@erpx/server-commercial/public/purchase-command-boundary';
import {salesEditorFinanceReferences,salesEditorFinanceSourceReadLocks} from '@erpx/server-finance/public/sales-editor-sources';
import {CommerceSalesFacts} from '@erpx/server-sales/public/commerce-facts';
import {commerceOwnerPricingSourceReadLocks} from '@erpx/server-sales/public/commerce-owner-sources';
import {requireCommerceCurrentAuthority} from '@erpx/server-commerce/adapters/purchase-boundary';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';

export type SalesEditorOptions={
 currency:string;
 parties:{id:string;name:string;kind:string;status:string;currency:string}[];
 products:{id:string;sku:string;name:string;status:string;item_type:string;price_minor:number;tax_bps:number}[];
 warehouses:{id:string;name:string;status:string}[];
 nodes:[];
 priceLists:{id:string;name:string;customer_id:string|null;valid_from:string;valid_until:string}[];
 priceListItems:{price_list_id:string;product_id:string;price_minor:number;tax_bps:number}[];
};
/** Initial Sales editor content only: no order collection, supplier, stock,
 * movement, cost or private Finance snapshot is serialized to OrderEditor. */
export async function salesEditorOptions(actor:WorkContext,kind:'quote'|'sales'):Promise<SalesEditorOptions>{
 requirePermission(actor,'commerce.read');requirePermission(actor,'commerce.sales');
 const write=(await holdingResources(actor.holdingId)).write,key=`sales-editor:${randomUUID()}`,mode=await commercePackageEnforcementMode();
 if(!isTransactionalSqlDatabase(write))throw new ApiError(503,'TRANSACTION_REQUIRED','Sales editor references require their owned Holding read.');
 return write.transaction(async database=>{
 const purchase=companyPurchaseReadLocks(database,actor.companyId);
 await database.batch([purchase[0],database.prepare('LOCK TABLE commerce_settings,commerce_products,commerce_warehouses IN SHARE MODE'),...salesEditorFinanceSourceReadLocks(database),...commerceOwnerPricingSourceReadLocks(database)]);
 await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(purchase.slice(1));
 const current=async()=>{await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:kind==='quote'?'sales.quotes':'sales.orders',purpose:'new-work'});await requireCommerceCurrentAuthority(database,actor,['commerce.read','commerce.sales'],null,key);await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');};
 await current();
 const finance=await salesEditorFinanceReferences(database,actor.companyId);
 if(!finance)throw new ApiError(409,'COMMERCE_SETUP_REQUIRED','Prepare and independently approve service Sales posting defaults.');
 const products=(await database.prepare('SELECT id,sku,name,status,item_type,price_minor,tax_bps FROM commerce_products WHERE company_id=? AND status=\'active\' ORDER BY sku').bind(actor.companyId).all<SalesEditorOptions['products'][number]>()).results;
 const warehouses=(await database.prepare('SELECT id,name,status FROM commerce_warehouses WHERE company_id=? AND status=\'active\' AND (?::text IS NULL OR branch_id=?) ORDER BY name').bind(actor.companyId,actor.branchId,actor.branchId).all<SalesEditorOptions['warehouses'][number]>()).results;
 const sales=CommerceSalesFacts(database),lists=(await sales.readSalesPriceLists1({companyId:actor.companyId})).results,items=(await sales.readSalesPriceListItems1({companyId:actor.companyId})).results;
 await current();
 return {currency:finance.currency,parties:finance.customers,products,warehouses,nodes:[],priceLists:lists.map(item=>({id:String(item.id),name:String(item.name),customer_id:item.customer_id===null?null:String(item.customer_id),valid_from:String(item.valid_from),valid_until:String(item.valid_until)})),priceListItems:items.map(item=>({price_list_id:String(item.price_list_id),product_id:String(item.product_id),price_minor:Number(item.price_minor),tax_bps:Number(item.tax_bps)}))};
 });
}
