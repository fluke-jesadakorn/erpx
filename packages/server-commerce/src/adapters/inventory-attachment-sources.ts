import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeCommerceReadFirst,RD_INVENTORY_ATTACHMENT_WAREHOUSES,RD_INVENTORY_ATTACHMENT_PRODUCTS,RD_INVENTORY_ATTACHMENT_STOCK,RD_INVENTORY_ATTACHMENT_RECEIPT} from './native-commerce-read';
/** Query-free native leases. Only actual Inventory FKs select these private
 * point images; they cannot grant Commerce collections or source ownership. */
export function inventoryAttachmentCommerceSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_warehouses,commerce_products,commerce_stock,commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines IN SHARE ROW EXCLUSIVE MODE');}
export async function inventoryAttachmentCommerceSourceImage(database:SqlDatabase,input:{companyId:string;warehouseIds:readonly string[];productIds:readonly string[];receiptId:string|null}){
 const warehouses=await nativeCommerceReadFirst<string>(RD_INVENTORY_ATTACHMENT_WAREHOUSES,[input.companyId,JSON.stringify(input.warehouseIds)],database);
 const products=await nativeCommerceReadFirst<string>(RD_INVENTORY_ATTACHMENT_PRODUCTS,[input.companyId,JSON.stringify(input.productIds)],database);
 const stock=await nativeCommerceReadFirst<string>(RD_INVENTORY_ATTACHMENT_STOCK,[input.companyId,JSON.stringify(input.warehouseIds),JSON.stringify(input.productIds)],database);
 const receipt=input.receiptId?await nativeCommerceReadFirst<{image:string;journal_id:string|null}>(RD_INVENTORY_ATTACHMENT_RECEIPT,[input.companyId,input.receiptId],database):null;
 return {image:JSON.stringify([warehouses,products,stock,receipt?.image??null]),journalId:receipt?.journal_id??null};
}
