import {z} from "zod";
import type {WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {quantityMilli,lineAmounts} from "@erpx/feature-commerce/lib/commerce-math";
import {decimalToMinor} from "@erpx/feature-finance/lib/finance-money";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {exactConversion} from "@erpx/server-procurement/domain/quantity-conversion";
import type {requestQuotationSource} from '@erpx/server-records/public/employee-requests';
import type {orderSchema} from '@erpx/server-commerce/public/commerce';
export type RfqRow=Record<string,string|number|null>;
export type RfqSource=Awaited<ReturnType<typeof requestQuotationSource>>;
export type RfqOrderInput=z.infer<typeof orderSchema>;
const text=z.string().trim().min(3).max(2000),amount=z.string().regex(/^\d{1,12}(\.\d{1,2})?$/),quantity=z.string().regex(/^\d{1,12}(\.\d{1,3})?$/);
export const rfqCreateSchema=z.object({requestId:z.string().min(1).optional(),requestVersion:z.number().int().positive().optional(),title:text,warehouseId:z.string().min(1),neededDate:z.iso.date(),demandEvidence:text,lines:z.array(z.object({requestItemOrdinal:z.number().int().min(0).optional(),requestQuantity:quantity.optional(),requestUnit:z.string().trim().min(1).max(30).optional(),requestUnitsPerProductUnit:quantity.optional(),productId:z.string().min(1),quantity})).min(1).max(20)});
export const rfqActionSchema=z.discriminatedUnion('action',[
 z.object({action:z.literal('offer'),expectedVersion:z.number().int().positive(),attachmentSetIds:z.array(z.string().min(1)).max(10).default([]),supplierId:z.string().min(1),reference:text,evidence:text,arrivalDate:z.iso.date(),validUntil:z.iso.date(),lines:z.array(z.object({lineId:z.string().min(1),price:amount,taxBps:z.number().int().min(0).max(10000)})).min(1).max(20)}),
 z.object({action:z.enum(['submit','approve','return','cancel']),expectedVersion:z.number().int().positive(),offerId:z.string().min(1).optional(),reason:text})
]);
export type RfqCreate=z.infer<typeof rfqCreateSchema>;export type RfqAction=z.infer<typeof rfqActionSchema>;
export type RfqDetail={rfq:RfqRow;currentRequestVersion:number|null;sourceBlocked:boolean;lines:RfqRow[];offers:RfqRow[];current:RfqRow[];offerLines:RfqRow[];events:RfqRow[];canPrepare:boolean;canApprove:boolean};
export type RfqMapping={ordinal:number;requestMilli:number;requestUnit:string;factorMilli:number};
export type RfqDemand={ordinal:number;input:RfqCreate["lines"][number];product:RfqRow;quantityMilli:number;mapping:RfqMapping|null};
export type RfqOfferLine={i:number;l:Extract<RfqAction,{action:"offer"}>["lines"][number];price:number;subtotal:number;tax:number};
export function sourceBlocked(rfq:RfqRow,source:RfqRow|null){return !!rfq.request_id&&rfq.status!=="ordered"&&(!source||source.state!=="in_progress"||Number(source.evidence_version)!==Number(rfq.request_evidence_version)||source.items_json!==rfq.request_items_json||JSON.parse(String(source.approval_json)).decision!=="approved");}
export function currentRfqOffers(offers:RfqRow[]){return offers.filter((offer,i,list)=>list.findIndex(prior=>prior.supplier_id===offer.supplier_id)===i);}
export function independentRfqReviewer(rfq:RfqRow,offers:RfqRow[],samePerson:(id:unknown)=>boolean){const selected=offers.find(offer=>offer.id===rfq.selected_offer_id);return !samePerson(rfq.created_by)&&!samePerson(rfq.submitted_by)&&(!selected||!samePerson(selected.recorded_by));}
export function mapRfqDemand(input:RfqCreate,source:RfqSource|null):RfqMapping[]{
 if(source&&new Set(input.lines.map(line=>line.requestItemOrdinal)).size!==input.lines.length)throw new BusinessRuleError(422,"RFQ_REQUEST_MAPPING_REQUIRED","Map each selected requisition item at most once in this quotation request.");
 return source?input.lines.map(line=>{const ordinal=line.requestItemOrdinal,item=ordinal===undefined?undefined:source.items[ordinal],requestMilli=quantityMilli(line.requestQuantity??line.quantity),productMilli=quantityMilli(line.quantity),factorMilli=quantityMilli(line.requestUnitsPerProductUnit??"1");
 if(!item||!item.quantity||requestMilli<=0||productMilli<=0||factorMilli<=0||requestMilli>item.remainingMilli||!exactConversion(requestMilli,productMilli,factorMilli)||(line.requestUnit&&line.requestUnit!==(item.unit||"each")))throw new BusinessRuleError(422,"RFQ_REQUEST_MAPPING_REQUIRED","Choose an unallocated request quantity and record an exact conversion to the product purchasing unit.");return {ordinal:ordinal!,requestMilli,requestUnit:item.unit||"each",factorMilli};}):[];
}
export function assertRfqSetup(input:RfqCreate,source:RfqSource|null,warehouse:RfqRow|null,config:RfqRow|null){
 if(!warehouse||!config||(source&&(warehouse.branch_id!==source.request.branch_id||(source.request.needed_date&&input.neededDate!==source.request.needed_date))))throw new BusinessRuleError(422,"RFQ_SETUP_REQUIRED","Choose an active warehouse and configure commerce currency.");
 if(new Set(input.lines.map(line=>line.productId)).size!==input.lines.length)throw new BusinessRuleError(422,"RFQ_DUPLICATE_PRODUCT","Use one demand line per product.");return {warehouse,config};
}
export function rfqDemand(input:RfqCreate["lines"][number],product:RfqRow|null,ordinal:number,mapping:RfqMapping|null):RfqDemand{const quantity=quantityMilli(input.quantity);if(!product||quantity<=0)throw new BusinessRuleError(422,"RFQ_DEMAND_INVALID","Select active products with positive quantities.");return {ordinal,input,product,quantityMilli:quantity,mapping};}
export function prepareRfqOffer(input:Extract<RfqAction,{action:"offer"}>,detail:RfqDetail,supplier:RfqRow|null,today:string){
 if(!supplier||input.validUntil<today)throw new BusinessRuleError(422,"RFQ_OFFER_INVALID","Choose an active supplier in the RFQ currency and an unexpired quote.");
 if(input.lines.length!==detail.lines.length||new Set(input.lines.map(line=>line.lineId)).size!==detail.lines.length)throw new BusinessRuleError(422,"RFQ_COMPLETE_OFFER_REQUIRED","Quote every demand line exactly once for a comparable offer.");
 const prepared=input.lines.map((line,i)=>{const demand=detail.lines.find(x=>x.id===line.lineId);if(!demand)throw new BusinessRuleError(422,"RFQ_FOREIGN_LINE","Use this RFQ demand only.");const price=decimalToMinor(line.price),totals=lineAmounts(Number(demand.quantity_milli),price,line.taxBps);return {i,l:line,price,...totals};}),subtotal=prepared.reduce((n,l)=>n+l.subtotal,0),tax=prepared.reduce((n,l)=>n+l.tax,0);
 if(!Number.isSafeInteger(subtotal+tax))throw new BusinessRuleError(422,"RFQ_AMOUNT_RANGE","The combined amount exceeds supported precision.");return {supplier,lines:prepared,subtotal,tax};
}
export function assertRfqOrder(detail:RfqDetail,input:RfqOrderInput,orderId:string,editing:boolean,today:string){const r=detail.rfq,selected=detail.offers.find(offer=>offer.id===r.selected_offer_id);
 if(input.kind!=="purchase"||!selected||(!editing&&r.status!=="approved")||(editing&&r.order_id!==orderId))throw new BusinessRuleError(409,"RFQ_AWARD_REQUIRED","An independently approved RFQ award is required.");
 if(!editing&&r.request_id&&input.requestId!==r.request_id)throw new BusinessRuleError(409,"RFQ_REQUEST_LINK_REQUIRED","The awarded PO must also hand off its approved requisition.");
 const lines=detail.offerLines.filter(line=>line.offer_id===selected.id);
 if(input.partyId!==selected.supplier_id||input.warehouseId!==r.warehouse_id||input.dueDate!==r.needed_date||input.lines.length!==detail.lines.length||input.lines.some(line=>{const demand=detail.lines.find(x=>x.product_id===line.productId),price=lines.find(x=>x.rfq_line_id===demand?.id);return !demand||!price||quantityMilli(line.quantity)!==Number(demand.quantity_milli)||decimalToMinor(line.price)!==Number(price.price_minor)||line.taxBps!==Number(price.tax_bps);}))throw new BusinessRuleError(409,"RFQ_AWARD_MISMATCH","Supplier, warehouse, required date, quantities, prices and taxes must match the approved award.");
 if(!editing&&(String(selected.valid_until)<today||input.orderDate>String(selected.valid_until)))throw new BusinessRuleError(409,"RFQ_QUOTE_EXPIRED","Obtain a new quote and independent award before ordering.");return selected;
}
export function rfqWorkItems(records:RfqRow[],canPurchase:boolean,canApprove:boolean,samePerson:(id:unknown)=>boolean):WorkItem[]{
 return records.map(r=>{const review=r.status==='review',canAct=canPurchase&&(!review||(canApprove&&![r.created_by,r.submitted_by,r.offer_recorder].some(p=>samePerson(p)))),nextAction=!Number(r.source_ready)?'Resolve the source requisition or cancel this quotation request':review?'Independently review supplier comparison and award':r.status==='approved'?'Create purchase order from approved offer':'Collect quotations and propose an award';return {id:String(r.id),type:'procurement_rfq',actionKind:review&&canAct?'approval':'work',title:`Supplier quotations · ${r.title}`,detail:String(r.demand_evidence),href:`/procurement-rfqs/${r.id}`,status:String(r.status),updatedAt:Number(r.updated_at),queue:canAct?'todo':'waiting',canAct,dueAt:Date.parse(String(r.needed_date)+'T00:00:00Z'),responsibleQueue:review?'Independent purchasing manager':'Purchasing team',nextAction,blocker:!Number(r.source_ready)?'Source requisition is no longer approved and ready with the same demand':Number(r.eligible_count)>0?null:'No eligible recipient; Company administrator must repair purchasing role coverage',relationships:[canAct?'assigned':'team']};});}
