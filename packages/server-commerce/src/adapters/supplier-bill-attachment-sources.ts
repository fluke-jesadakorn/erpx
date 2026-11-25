import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceAttachmentReadSourceLocks,commerceAttachmentReadSourceImage} from '@erpx/server-commerce/adapters/attachment-read-sources';
import {nativeCommerceReadAll,RD_SUPPLIER_BILL_RECEIPT_IDS} from './native-commerce-read';
export {commerceAttachmentReadSourceLocks as supplierBillAttachmentCommerceSourceLocks};
/** Only the original fulfillment.invoice_id FK, actual Purchase kind and legal
 * Company select a Finance bill's Commerce history. No source label grants. */
export async function supplierBillAttachmentCommerceSourceImage(database:SqlDatabase,companyId:string,invoiceId:string){
 const receipts=await nativeCommerceReadAll<{id:string}>(RD_SUPPLIER_BILL_RECEIPT_IDS,[companyId,invoiceId],database);
 const images:Array<readonly [string,string|null]>=[];
 for(const receipt of receipts)images.push([receipt.id,await commerceAttachmentReadSourceImage(database,companyId,'commerce_fulfillment',receipt.id)]);
 return JSON.stringify(images);
}
