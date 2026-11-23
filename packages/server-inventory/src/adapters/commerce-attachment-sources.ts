import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeInventoryReadFirst,INVENTORY_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE} from './native-inventory-read';
export function commerceAttachmentStockSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE inventory_order_reservations,inventory_fulfillment_lot_allocations,inventory_lots,inventory_lot_balances IN SHARE ROW EXCLUSIVE MODE');}
/** Actual persisted line IDs originate in the Commerce owner. A source label
 * cannot manufacture a reservation, receipt allocation or lot relationship.
 * Owner-native read: planned by `RD_COMMERCE_ATTACHMENT_STOCK_IMAGE` and
 * executed on the caller's unit; this module carries no SQL for it. */
export function commerceAttachmentStockSourceImage(database:SqlDatabase,input:{companyId:string;orderLineIds:readonly string[];fulfillmentLineIds:readonly string[];lotIds:readonly string[]}){
  return nativeInventoryReadFirst<{image:string}>(
    INVENTORY_RD_COMMERCE_ATTACHMENT_STOCK_IMAGE,
    [JSON.stringify(input.fulfillmentLineIds),input.companyId,JSON.stringify(input.lotIds),JSON.stringify(input.orderLineIds)],
    database,
  ).then(row=>row?.image??null);
}
