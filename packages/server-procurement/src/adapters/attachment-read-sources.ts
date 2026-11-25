import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {procurementAttachmentCommerceSourceLocks,procurementAttachmentCommerceSourceImage} from '@erpx/server-commerce/public/procurement-attachment-sources';
import {commerceAttachmentFinanceSourceImage} from '@erpx/server-finance/public/commerce-attachment-sources';
import {procurementCommerceSourceReadLocks} from '@erpx/server-procurement/adapters/commerce-source-locks';
import {procurementCommandSourceImage} from '@erpx/server-procurement/adapters/command-source';
import {jsonbAggBy} from './read-tail-r21';
import {procurementAttachmentReadOuterFrame} from './read-tail-r45';
import {arraySelectBy} from './read-tail-r55';
const targets=['procurement_rfq','supplier_bill_match'] as const;
export type ProcurementAttachmentReadTarget=typeof targets[number];
export function isProcurementAttachmentReadTarget(target:string):target is ProcurementAttachmentReadTarget{return (targets as readonly string[]).includes(target);}
export function procurementAttachmentReadSourceLocks(database:SqlDatabase){return [...procurementAttachmentCommerceSourceLocks(database),...procurementCommerceSourceReadLocks(database)];}
type Native={image:string;warehouse_id:string|null;product_ids:string[];supplier_ids:string[];order_id:string|null;invoice_id:string|null;receipt_ids:string[]};
export async function procurementAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:ProcurementAttachmentReadTarget,id:string){
 const source=target==='procurement_rfq'?await database.prepare(`${procurementAttachmentReadOuterFrame("procurement_rfqs", `(${procurementCommandSourceImage('rfq','r')})`, "r.warehouse_id", `${arraySelectBy("product_id","procurement_rfq_lines","rfq_id=r.id AND product_id IS NOT NULL")} product_ids`, `${arraySelectBy("supplier_id","procurement_rfq_offers","rfq_id=r.id")} supplier_ids`, "NULL::text invoice_id", "ARRAY[]::text[] receipt_ids")}`).bind(id,companyId).first<Native>()
 :await database.prepare(`${procurementAttachmentReadOuterFrame("procurement_bill_matches", `jsonb_build_object('match',to_jsonb(r),'lines',${jsonbAggBy("procurement_bill_match_lines","l","match_id","r.id","l.id")},'receipts',${jsonbAggBy("procurement_bill_match_receipts","receipt","match_id","r.id","receipt.id")},'events',${jsonbAggBy("procurement_bill_match_events","e","match_id","r.id","e.id")})`, "NULL::text warehouse_id", "ARRAY[]::text[] product_ids", "ARRAY[r.supplier_id] supplier_ids", "r.invoice_id", "ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids")}`).bind(id,companyId).first<Native>();
 if(!source)return null;
 return JSON.stringify([source.image,await procurementAttachmentCommerceSourceImage(database,{companyId,warehouseId:source.warehouse_id,productIds:source.product_ids,orderId:source.order_id,receiptIds:source.receipt_ids}),await commerceAttachmentFinanceSourceImage(database,{companyId,partyIds:[...new Set(source.supplier_ids)].sort(),invoiceIds:source.invoice_id?[source.invoice_id]:[],journalIds:[]})]);
}
