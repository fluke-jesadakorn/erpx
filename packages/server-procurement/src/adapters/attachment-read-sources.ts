import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {procurementAttachmentCommerceSourceLocks,procurementAttachmentCommerceSourceImage} from '@erpx/server-commerce/public/procurement-attachment-sources';
import {commerceAttachmentFinanceSourceImage} from '@erpx/server-finance/public/commerce-attachment-sources';
import {procurementCommerceSourceReadLocks} from '@erpx/server-procurement/adapters/commerce-source-locks';
import {procurementCommandSourceImage} from '@erpx/server-procurement/adapters/command-source';
const targets=['procurement_rfq','supplier_bill_match'] as const;
export type ProcurementAttachmentReadTarget=typeof targets[number];
export function isProcurementAttachmentReadTarget(target:string):target is ProcurementAttachmentReadTarget{return (targets as readonly string[]).includes(target);}
export function procurementAttachmentReadSourceLocks(database:SqlDatabase){return [...procurementAttachmentCommerceSourceLocks(database),...procurementCommerceSourceReadLocks(database)];}
type Native={image:string;warehouse_id:string|null;product_ids:string[];supplier_ids:string[];order_id:string|null;invoice_id:string|null;receipt_ids:string[]};
export async function procurementAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:ProcurementAttachmentReadTarget,id:string){
 const source=target==='procurement_rfq'?await database.prepare(`SELECT (${procurementCommandSourceImage('rfq','r')})::text image,r.warehouse_id,ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids,ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids,r.order_id,NULL::text invoice_id,ARRAY[]::text[] receipt_ids FROM procurement_rfqs r WHERE r.id=? AND r.company_id=?`).bind(id,companyId).first<Native>()
 :await database.prepare(`SELECT jsonb_build_object('match',to_jsonb(r),'lines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM procurement_bill_match_lines l WHERE l.match_id=r.id),'receipts',(SELECT jsonb_agg(to_jsonb(receipt) ORDER BY receipt.id) FROM procurement_bill_match_receipts receipt WHERE receipt.match_id=r.id),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM procurement_bill_match_events e WHERE e.match_id=r.id))::text image,NULL::text warehouse_id,ARRAY[]::text[] product_ids,ARRAY[r.supplier_id] supplier_ids,r.order_id,r.invoice_id,ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids FROM procurement_bill_matches r WHERE r.id=? AND r.company_id=?`).bind(id,companyId).first<Native>();
 if(!source)return null;
 return JSON.stringify([source.image,await procurementAttachmentCommerceSourceImage(database,{companyId,warehouseId:source.warehouse_id,productIds:source.product_ids,orderId:source.order_id,receiptIds:source.receipt_ids}),await commerceAttachmentFinanceSourceImage(database,{companyId,partyIds:[...new Set(source.supplier_ids)].sort(),invoiceIds:source.invoice_id?[source.invoice_id]:[],journalIds:[]})]);
}
