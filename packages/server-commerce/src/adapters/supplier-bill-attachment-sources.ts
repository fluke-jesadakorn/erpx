import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceAttachmentReadSourceLocks,commerceAttachmentReadSourceImage} from '@erpx/server-commerce/adapters/attachment-read-sources';
export {commerceAttachmentReadSourceLocks as supplierBillAttachmentCommerceSourceLocks};
/** Only the original fulfillment.invoice_id FK, actual Purchase kind and legal
 * Company select a Finance bill's Commerce history. No source label grants. */
export async function supplierBillAttachmentCommerceSourceImage(database:SqlDatabase,companyId:string,invoiceId:string){
 const receipts=(await database.prepare("SELECT f.id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase' WHERE f.invoice_id=? ORDER BY f.id").bind(companyId,invoiceId).all<{id:string}>()).results;
 const images:Array<readonly [string,string|null]>=[];
 for(const receipt of receipts)images.push([receipt.id,await commerceAttachmentReadSourceImage(database,companyId,'commerce_fulfillment',receipt.id)]);
 return JSON.stringify(images);
}
