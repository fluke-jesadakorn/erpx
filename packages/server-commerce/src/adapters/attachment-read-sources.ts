import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {commerceAttachmentFinanceSourceLocks,commerceAttachmentFinanceSourceImage} from '@erpx/server-finance/public/commerce-attachment-sources';
import {commerceAttachmentStockSourceLocks,commerceAttachmentStockSourceImage} from '@erpx/server-inventory/public/commerce-attachment-sources';
import {commerceAttachmentPosSourceLocks,commerceAttachmentPosSourceImage} from '@erpx/server-pos/public/commerce-attachment-sources';
import { fulfillmentParentGuardSql } from './attachment-parent-guard-sql';
import { nativeCommerceReadFirst, RD_ATTACHMENT_PRODUCT_IMAGE } from './native-commerce-read';
const targets=['purchase_order','purchase_receipt','commerce_order','commerce_order_delivery','commerce_fulfillment','commerce_return','product'] as const;
export type CommerceAttachmentReadTarget=typeof targets[number];
export function isCommerceAttachmentReadTarget(target:string):target is CommerceAttachmentReadTarget{return (targets as readonly string[]).includes(target);}
/** Query-free source leases, never a Company advisory/data observation. */
export function commerceAttachmentReadSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_products,commerce_warehouses IN SHARE ROW EXCLUSIVE MODE'),...commerceAttachmentFinanceSourceLocks(database),commerceAttachmentStockSourceLocks(database),commerceAttachmentPosSourceLocks(database)];}
const rows=(table:string,predicate:string)=>`(SELECT jsonb_agg(to_jsonb(child) ORDER BY to_jsonb(child)::text) FROM ${table} child WHERE ${predicate})`;
/** Byte-identical parent guards shared by the paired attachment row sources.
 * Text only: each rows() call still composes its own fragment and the single
 * outer prepare/execute is unchanged. */
const FULFILLMENT_LINE_PARENT_GUARD=fulfillmentParentGuardSql('id','fulfillment_id');
const RETURN_LINE_PARENT_GUARD='EXISTS(SELECT 1 FROM commerce_returns r WHERE r.id=child.return_id AND r.order_id=o.id AND r.company_id=o.company_id)';
type Native={image:string;order_id:string;party_id:string;invoice_ids:string[];journal_ids:(string|null)[];order_line_ids:string[];fulfillment_line_ids:string[];lot_ids:string[]};
/** Aliases select their actual original physical owner, not a claimed source
 * label. Every original kind/Company/record decision remains in the full policy. */
export async function commerceAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:CommerceAttachmentReadTarget,id:string):Promise<string|null>{
 if(target==='product'){
  const product=await database.prepare('SELECT to_jsonb(p)::text image FROM commerce_products p WHERE p.id=? AND p.company_id=?').bind(id,companyId).first<string>('image');
  return product?JSON.stringify([product,await commerceAttachmentPosSourceImage(database,companyId,id)]):null;
 }
 const join=target==='commerce_fulfillment'?'JOIN commerce_fulfillments selected ON selected.order_id=o.id':target==='commerce_return'?'JOIN commerce_returns selected ON selected.order_id=o.id AND selected.company_id=o.company_id':'';
 const predicate=target==='commerce_fulfillment'||target==='commerce_return'?'selected.id=?':"o.id=?";
 const kind=target==='purchase_order'||target==='purchase_receipt'?" AND o.kind='purchase'":target==='commerce_order_delivery'?" AND o.kind='sales'":'';
 const source=await database.prepare(`SELECT jsonb_build_object('order',to_jsonb(o),'warehouse',to_jsonb(w),'lines',(SELECT jsonb_agg(jsonb_build_object('line',to_jsonb(l),'product',to_jsonb(p)) ORDER BY l.id) FROM commerce_order_lines l LEFT JOIN commerce_products p ON p.id=l.product_id AND p.company_id=o.company_id WHERE l.order_id=o.id),
 'fulfillments',${rows('commerce_fulfillments','child.order_id=o.id')},'fulfillmentLines',${rows('commerce_fulfillment_lines',FULFILLMENT_LINE_PARENT_GUARD)},'evidence',${rows('commerce_fulfillment_evidence',FULFILLMENT_LINE_PARENT_GUARD)},'valuations',${rows('commerce_goods_valuations','child.company_id=o.company_id AND '+fulfillmentParentGuardSql('id','fulfillment_id'))},
 'returns',${rows('commerce_returns','child.order_id=o.id AND child.company_id=o.company_id')},'returnLines',${rows('commerce_return_lines',RETURN_LINE_PARENT_GUARD)},'returnLots',${rows('commerce_return_lot_allocations','EXISTS(SELECT 1 FROM commerce_return_lines l JOIN commerce_returns r ON r.id=l.return_id AND r.order_id=o.id AND r.company_id=o.company_id WHERE l.id=child.return_line_id)')},'returnMoves',${rows('commerce_return_stock_moves',RETURN_LINE_PARENT_GUARD)},
 'invoiceCarrying',${rows('commerce_invoice_goods_carrying','child.company_id=o.company_id AND '+fulfillmentParentGuardSql('invoice_id','invoice_id'))},'stock',${rows('commerce_stock','child.warehouse_id=o.warehouse_id AND EXISTS(SELECT 1 FROM commerce_order_lines l WHERE l.order_id=o.id AND l.product_id=child.product_id)')})::text image,
 o.id order_id,o.party_id,ARRAY(SELECT invoice_id FROM commerce_fulfillments WHERE order_id=o.id AND invoice_id IS NOT NULL UNION SELECT invoice_id FROM commerce_returns WHERE order_id=o.id AND company_id=o.company_id) invoice_ids,
 ARRAY(SELECT journal_id FROM commerce_fulfillments WHERE order_id=o.id AND journal_id IS NOT NULL UNION SELECT stock_journal_id FROM commerce_returns WHERE order_id=o.id AND company_id=o.company_id UNION SELECT credit_journal_id FROM commerce_returns WHERE order_id=o.id AND company_id=o.company_id UNION SELECT goods_fx_journal_id FROM commerce_returns WHERE order_id=o.id AND company_id=o.company_id) journal_ids,
 ARRAY(SELECT id FROM commerce_order_lines WHERE order_id=o.id) order_line_ids,ARRAY(SELECT l.id FROM commerce_fulfillment_lines l JOIN commerce_fulfillments f ON f.id=l.fulfillment_id WHERE f.order_id=o.id) fulfillment_line_ids,ARRAY(SELECT a.lot_id FROM commerce_return_lot_allocations a JOIN commerce_return_lines l ON l.id=a.return_line_id JOIN commerce_returns r ON r.id=l.return_id AND r.order_id=o.id AND r.company_id=o.company_id) lot_ids
 FROM commerce_orders o ${join} LEFT JOIN commerce_warehouses w ON w.id=o.warehouse_id AND w.company_id=o.company_id WHERE ${predicate} AND o.company_id=?${kind}`).bind(id,companyId).first<Native>();
 if(!source)return null;
 return JSON.stringify([source.image,await commerceAttachmentFinanceSourceImage(database,{companyId,partyIds:[source.party_id],invoiceIds:source.invoice_ids,journalIds:source.journal_ids.filter((value):value is string=>!!value)}),await commerceAttachmentStockSourceImage(database,{companyId,orderLineIds:source.order_line_ids,fulfillmentLineIds:source.fulfillment_line_ids,lotIds:source.lot_ids})]);
}
