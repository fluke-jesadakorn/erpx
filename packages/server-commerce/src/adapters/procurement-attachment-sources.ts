import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryAttachmentCommerceSourceImage} from '@erpx/server-commerce/adapters/inventory-attachment-sources';
import {commerceAttachmentReadSourceLocks,commerceAttachmentReadSourceImage} from '@erpx/server-commerce/adapters/attachment-read-sources';
export {commerceAttachmentReadSourceLocks as procurementAttachmentCommerceSourceLocks};
/** Only raw Procurement FKs enter this named point reader. Original RFQ,
 * supplier/receipt and independent Finance policy remain owner decisions. */
export async function procurementAttachmentCommerceSourceImage(database:SqlDatabase,input:{companyId:string;warehouseId:string|null;productIds:readonly string[];orderId:string|null;receiptIds:readonly string[]}){
 const setup=await inventoryAttachmentCommerceSourceImage(database,{companyId:input.companyId,warehouseIds:input.warehouseId?[input.warehouseId]:[],productIds:input.productIds,receiptId:null}),order=input.orderId?await commerceAttachmentReadSourceImage(database,input.companyId,'purchase_order',input.orderId):null;
 const receipts:Array<readonly [string,string|null]>=[];
 for(const id of [...input.receiptIds].sort())receipts.push([id,await commerceAttachmentReadSourceImage(database,input.companyId,'commerce_fulfillment',id)]);
 return JSON.stringify([setup.image,order,receipts]);
}
