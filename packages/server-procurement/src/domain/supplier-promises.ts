import {z} from "zod";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {quantityMilli} from "@erpx/feature-commerce/lib/commerce-math";
import {BusinessRuleError} from "@erpx/server-shared/index";
export type PromiseRow=Record<string,string|number|null>;
export const supplierPromiseSchema=z.object({expectedVersion:z.number().int().positive(),kind:z.enum(['confirmed','withdrawn']),confirmationDate:z.iso.date(),supplierReference:z.string().trim().min(3).max(160),evidence:z.string().trim().min(5).max(2000),attachmentSetIds:z.array(z.string().min(1)).max(10).default([]),lines:z.array(z.object({orderLineId:z.string().min(1),quantity:z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),promisedDate:z.iso.date()})).max(100)});
export type SupplierPromiseInput=z.infer<typeof supplierPromiseSchema>;
export type PromiseLine={ordinal:number;orderLineId:string;quantityMilli:number;fulfilledSnapshot:number;promisedDate:string};
export function assertPromiseOrder(order:PromiseRow,input:SupplierPromiseInput){
 if(!["confirmed","partially_fulfilled"].includes(String(order.state)))throw new BusinessRuleError(409,"APPROVED_PURCHASE_REQUIRED","Supplier confirmations require an approved, outstanding purchase order.");
 if(input.confirmationDate<String(order.order_date))throw new BusinessRuleError(422,"PROMISE_DATE_INVALID","Supplier confirmation cannot precede the purchase order.");
 if(input.kind==="withdrawn"&&input.lines.length||input.kind==="confirmed"&&!input.lines.length)throw new BusinessRuleError(422,"PROMISE_LINES_REQUIRED","A confirmation needs dated quantities; a withdrawal must have no lines.");
}
export function preparePromiseLines(input:SupplierPromiseInput,source:PromiseRow[]):PromiseLine[]{
 const totals=new Map<string,number>(),dates=new Set<string>();
 return input.lines.map((line,ordinal)=>{const original=source.find(s=>s.id===line.orderLineId),qty=quantityMilli(line.quantity),duplicate=`${line.orderLineId}:${line.promisedDate}`;
 if(!original)throw new BusinessRuleError(422,"PROMISE_LINE_INVALID","Select a line from this purchase order.");
 if(qty<=0||dates.has(duplicate)||line.promisedDate<input.confirmationDate)throw new BusinessRuleError(422,"PROMISE_LINE_INVALID","Use positive quantities, a unique line/date and a date on or after supplier confirmation.");dates.add(duplicate);
 const total=(totals.get(line.orderLineId)??0)+qty;totals.set(line.orderLineId,total);
 if(!Number.isSafeInteger(total)||total>Number(original.quantity_milli)-Number(original.fulfilled_milli))throw new BusinessRuleError(409,"PROMISE_QUANTITY_EXCEEDED","Confirmed quantities exceed the outstanding source order quantity.");
 return {ordinal,orderLineId:line.orderLineId,quantityMilli:qty,fulfilledSnapshot:Number(original.fulfilled_milli),promisedDate:line.promisedDate};});
}
export function promiseDetailLines(lines:PromiseRow[],open:PromiseRow[]):(PromiseRow&{outstanding_milli:number;unconfirmed_milli:number})[]{return lines.map(line=>{const outstanding=Math.max(0,Number(line.quantity_milli)-Number(line.fulfilled_milli)),promised=open.filter(p=>p.order_line_id===line.id).reduce((n,p)=>n+Number(p.remaining_milli),0);return {...line,outstanding_milli:outstanding,unconfirmed_milli:Math.max(0,outstanding-promised)};});}
export type PromiseEvidenceSet={id:string;purpose_key:string;component_kind:string;component_ref:string|null};
export function assertPromiseEvidence(input:SupplierPromiseInput,sets:PromiseEvidenceSet[]){if(sets.length!==new Set(input.attachmentSetIds).size||sets.some(set=>set.purpose_key!=="supplier_promise"||set.component_kind!=="action"||set.component_ref!=="supplier-promise"))throw new BusinessRuleError(422,"SUPPLIER_PROMISE_ATTACHMENT_PURPOSE","Choose only supplier promise evidence prepared for this confirmation.");}
export function promiseWorkItems(records:PromiseRow[],canPurchase:boolean):WorkItem[]{
 return records.map(o=>{const canAct=canPurchase,unconfirmed=Math.max(0,Number(o.outstanding)-Number(o.promised));return {id:String(o.id),type:'supplier_promise',title:`Supplier follow-up · ${o.number}`,detail:`${Number(o.outstanding)/1000} outstanding · ${unconfirmed/1000} not confirmed by supplier`,href:`/commerce/orders/${o.id}#supplier-promises`,status:unconfirmed?'awaiting_supplier_confirmation':'awaiting_receipt',updatedAt:Number(o.updated_at),queue:canAct?'todo':'waiting',canAct,dueAt:Date.parse((unconfirmed>0&&String(o.due_date)<String(o.followup_date)?String(o.due_date):String(o.followup_date))+'T00:00:00Z'),responsibleQueue:'Purchasing team',nextAction:unconfirmed?'Obtain supplier confirmation for remaining quantities':'Follow up promised delivery and actual receipts',blocker:Number(o.candidates)?null:'No eligible purchasing operator; Company administrator must repair role coverage',relationships:[canAct?'assigned':'team']};});}
