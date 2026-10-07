import {z} from "zod";
import type {WorkItem} from "@/lib/work-item";
import {quantityMilli,lineAmounts} from "@/lib/commerce-math";
import {decimalToMinor,minorToDecimal} from "@/lib/finance-money";
import {BusinessRuleError} from "../../shared";
export type BillRow=Record<string,string|number|null>;
export type BillJournalLine={accountId:string;debit:string;credit:string};
const reason = z.string().trim().min(3).max(2000);
const version = z.number().int().positive();
const receiptInputSchema = z.object({ fulfillmentId: z.string().min(1), expectedVersion: version });
const billLineSchema = z.object({
  fulfillmentLineId: z.string().min(1),
  quantity: z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),
  price: z.string().regex(/^\d{1,12}(\.\d{1,2})?$/),
  taxBps: z.number().int().min(0).max(10000),
});

/** Legacy single-receipt input remains accepted while the UI uses receipts[]. */
export const supplierBillSchema = z.object({
  fulfillmentId: z.string().min(1).optional(),
  expectedVersion: version.optional(),
  receipts: z.array(receiptInputSchema).min(1).max(50).optional(),
  reference: z.string().trim().min(2).max(160),
  invoiceDate: z.iso.date(),
  dueDate: z.iso.date(),
  evidence: reason,
  lines: z.array(billLineSchema).min(1).max(200),
}).superRefine((input, context) => {
  const legacy = Boolean(input.fulfillmentId || input.expectedVersion);
  if (legacy && (!input.fulfillmentId || !input.expectedVersion)) {
    context.addIssue({ code: "custom", path: ["fulfillmentId"], message: "Receipt and version must be supplied together." });
  }
  if (Boolean(input.receipts) === legacy) {
    context.addIssue({ code: "custom", path: ["receipts"], message: "Use either receipts or one legacy receipt, not both." });
  }
});
export const supplierBillDecisionSchema = z.object({
  action: z.enum(["approve", "return"]),
  expectedVersion: version,
  reason,
  varianceAccountId: z.string().min(1).optional(),
});
export const supplierInvoicePolicySchema = z.object({
  matchingMode: z.enum(["optional", "required"]),
  expectedVersion: z.number().int().nonnegative(),
  reason,
});


export type BillCapture=z.infer<typeof supplierBillSchema>;export type BillDecision=z.infer<typeof supplierBillDecisionSchema>;export type BillPolicy=z.infer<typeof supplierInvoicePolicySchema>;
export type BillRequestedReceipt={fulfillmentId:string;expectedVersion:number};
export type BillPreparedLine={index:number;line:BillRow;quantity:number;price:number;taxBps:number;subtotal:number;tax:number};
export type BillCapturePlan={requested:BillRequestedReceipt[];receipts:BillRow[];primary:BillRow;order:BillRow;config:BillRow;clearing:string;lines:BillPreparedLine[];expected:number;subtotal:number;tax:number;quantityMatches:boolean};
export type BillWorkFacts={latest:BillRow|null;eligible:number;primary:{userId:string;name:string}|null;flowBlocker:string|null};
export function requestedBillReceipts(input:BillCapture){const requested=input.receipts??[{fulfillmentId:input.fulfillmentId!,expectedVersion:input.expectedVersion!}];if(new Set(requested.map(x=>x.fulfillmentId)).size!==requested.length)throw new BusinessRuleError(422,'DUPLICATE_RECEIPT','Select each goods receipt once.');if(new Set(input.lines.map(x=>x.fulfillmentLineId)).size!==input.lines.length)throw new BusinessRuleError(422,'DUPLICATE_BILL_LINE','Match each receipt line once.');return requested;}
export function billReceiptScope(input:BillCapture,receipts:BillRow[],orders:BillRow[]){
 receipts=[...receipts].sort((a,b)=>String(a.fulfillment_date).localeCompare(String(b.fulfillment_date))||String(a.id).localeCompare(String(b.id)));const primary=receipts[0],byId=new Map(orders.map(x=>[String(x.id),x])),order=byId.get(String(primary.order_id));
 if(!order||order.kind!=='purchase'||receipts.some(x=>x.invoice_id))throw new BusinessRuleError(409,'UNBILLED_PURCHASE_REQUIRED','Select posted, unbilled purchase receipts.');
 if(receipts.some(x=>{const candidate=byId.get(String(x.order_id));return !candidate||candidate.kind!=='purchase'||candidate.party_id!==order.party_id||candidate.currency!==order.currency||candidate.branch_id!==order.branch_id||candidate.department_id!==order.department_id;}))throw new BusinessRuleError(409,'CONSOLIDATED_BILL_SCOPE_MISMATCH','A consolidated supplier invoice must use the same supplier, currency, branch and department across its purchase orders.');
 if(input.invoiceDate<String(receipts.at(-1)!.fulfillment_date)||input.dueDate<input.invoiceDate)throw new BusinessRuleError(422,'BILL_DATES_INVALID','Invoice and due dates must follow every selected receipt.');return {receipts,primary,order};
}
export function billClearingAccount(receipt:BillRow,order:BillRow,config:BillRow|null,postings:BillRow[],previous:string|null){
 if(!config||receipt.journal_status!=='posted'||receipt.journal_currency!==order.currency||receipt.journal_chart_id!==config.chart_id)throw new BusinessRuleError(409,'BILL_POSTING_PROFILE_REQUIRED','Post every selected receipt GL with a compatible Company posting profile first.');
 const clearing=postings.find(x=>Number(x.credit_minor)>0)?.account_id;if(postings.length!==2||!clearing||previous&&previous!==clearing)throw new BusinessRuleError(409,'BILL_SOURCE_POSTING_INVALID','Selected receipts must use the same reviewed goods-receipt clearing account.');return String(clearing);
}
export function prepareBillLines(input:BillCapture,actual:BillRow[]){
 if(input.lines.length!==actual.length||input.lines.some(x=>!actual.some(y=>y.id===x.fulfillmentLineId)))throw new BusinessRuleError(422,'BILL_COMPLETE_RECEIPT_REQUIRED','Match every line from every selected receipt exactly once.');
 const lines=input.lines.map((x,index)=>{const line=actual.find(y=>y.id===x.fulfillmentLineId),quantity=quantityMilli(x.quantity),price=decimalToMinor(x.price);if(!line||quantity<=0)throw new BusinessRuleError(422,'BILL_LINE_INVALID','Use selected receipt lines and positive invoiced quantities.');return {index,line,quantity,price,taxBps:x.taxBps,...lineAmounts(quantity,price,x.taxBps)};});
 const subtotal=lines.reduce((n,x)=>n+x.subtotal,0),tax=lines.reduce((n,x)=>n+x.tax,0),expected=actual.reduce((n,x)=>n+Number(x.subtotal_minor),0),quantityMatches=lines.every(x=>x.quantity===Number(x.line.quantity_milli));
 if(!Number.isSafeInteger(subtotal+tax)||subtotal+tax<=0||lines.some(x=>x.subtotal<=0))throw new BusinessRuleError(422,'BILL_AMOUNT_INVALID','The supplier invoice must have a positive supported total.');return {lines,subtotal,tax,expected,quantityMatches};
}
export function assertBillDecision(input:BillDecision,bill:BillRow,samePerson:(id:unknown)=>boolean){
 if(samePerson(bill.created_by)||bill.status==='invoiced'&&samePerson(bill.invoice_created_by))throw new BusinessRuleError(403,'INDEPENDENT_BILL_REVIEW_REQUIRED','A different Finance reviewer must accept or return the supplier invoice and its AP draft.');
 const returnDraft=input.action==='return'&&bill.status==='invoiced'&&bill.invoice_status==='draft';if(bill.status!=='review'&&!(input.action==='return'&&bill.status==='approved')&&!returnDraft)throw new BusinessRuleError(409,'BILL_REVIEW_REQUIRED','Only unposted supplier invoices can be returned; posted invoices require the source credit workflow.');
 if(input.action==='approve'&&!Number(bill.quantity_matches))throw new BusinessRuleError(409,'BILL_QUANTITY_VARIANCE','Invoiced quantities differ from the selected receipts. Return the invoice for correction; do not approve unreceived quantities.');return returnDraft;
}
export function assertBillPolicy(input:BillPolicy,current:BillRow|null){const version=Number(current?.version??0),previous=String(current?.matching_mode??'optional') as 'optional'|'required';if(version!==input.expectedVersion)throw new BusinessRuleError(409,'STALE_POLICY','The supplier invoice policy changed. Reload and review its current version.');if(previous===input.matchingMode)throw new BusinessRuleError(409,'POLICY_UNCHANGED',`Supplier invoice matching is already ${previous}.`);return {version:version+1,previous};}
export function assertBillInvoiceHandoff(bill:BillRow,receipts:BillRow[],fulfillment:BillRow,input:{date:string;dueDate:string}){if(bill.fulfillment_id!==fulfillment.id||bill.status!=='approved'||!Number(bill.quantity_matches)||input.date!==bill.invoice_date||input.dueDate!==bill.due_date||!receipts.length)throw new BusinessRuleError(409,'SUPPLIER_BILL_MATCH_REQUIRED','Use the approved review, its primary receipt and its invoice dates.');}
export function assertMatchedInvoice(bill:BillRow,invoice:BillRow){if(bill.status!=='invoiced'||invoice.currency!==bill.currency||invoice.kind!=='payable'||invoice.party_id!==bill.supplier_id||invoice.invoice_date!==bill.invoice_date||invoice.due_date!==bill.due_date||invoice.debit_account_id!==bill.clearing_account_id||invoice.credit_account_id!==bill.payable_account_id||invoice.tax_account_id!==bill.tax_account_id||Number(invoice.total_minor)!==Number(bill.subtotal_minor)+Number(bill.tax_minor)||Number(invoice.subtotal_minor)!==Number(bill.subtotal_minor)||Number(invoice.tax_minor)!==Number(bill.tax_minor))throw new BusinessRuleError(409,'SUPPLIER_BILL_SOURCE_CHANGED','Review the matched invoice source before posting.');}
export function matchedBillLines(bill: BillRow): BillJournalLine[] {
  const expected = Number(bill.expected_subtotal_minor), subtotal = Number(bill.subtotal_minor), tax = Number(bill.tax_minor), variance = subtotal - expected;
  const lines: BillJournalLine[] = [];
  if (expected) lines.push({ accountId: String(bill.clearing_account_id), debit: minorToDecimal(expected), credit: "0" });
  if (variance) lines.push({ accountId: String(bill.variance_account_id), debit: variance > 0 ? minorToDecimal(variance) : "0", credit: variance < 0 ? minorToDecimal(-variance) : "0" });
  if (tax) lines.push({ accountId: String(bill.tax_account_id), debit: minorToDecimal(tax), credit: "0" });
  lines.push({ accountId: String(bill.payable_account_id), debit: "0", credit: minorToDecimal(subtotal + tax) });
  return lines;
}


export function billWorkItem(row:BillRow,facts:BillWorkFacts,authority:{userId:string;canWrite:boolean;canApprove:boolean;canCommerceRead:boolean;samePerson(id:unknown):boolean}):WorkItem|null{
 const correction=row.status==='returned';if(correction&&(facts.latest?.id!==row.id||facts.latest.status!=='returned'))return null;const review=row.status==='review';let canAct=authority.canWrite&&authority.canApprove&&!authority.samePerson(row.created_by),blocker:string|null=null,responsible='Independent Finance reviewer';
 if(review){if(!facts.eligible)blocker='No independent Finance reviewer; Company administrator must repair role coverage';}else{blocker=correction?facts.primary?null:'The purchase order invoicing owner is missing or no longer eligible. Reassign with a reason.':facts.flowBlocker;canAct=authority.canWrite&&(!correction||authority.canCommerceRead)&&!blocker&&facts.primary?.userId===authority.userId;responsible=facts.primary?.name??'Assigned purchase order invoicing owner';}
 return {id:String(row.id),type:'supplier_bill',actionKind:review&&canAct?'approval':'work',title:`Supplier invoice · ${row.reference}`,detail:`${row.order_number} · ${row.receipt_count} receipt(s) · ${row.currency} ${minorToDecimal(Number(row.subtotal_minor)+Number(row.tax_minor))}`,href:correction?`/commerce/orders/${row.order_id}`:`/supplier-bills/${row.id}`,status:String(row.status),updatedAt:Number(row.updated_at),queue:canAct?'todo':'waiting',canAct,blocker,responsibleQueue:responsible,nextAction:review?Number(row.quantity_matches)?'Review invoice and purchase price variance':'Return invoice with quantity discrepancy':correction?'Obtain corrected supplier invoice and resubmit from the receipt':'Prepare matched AP invoice',dueAt:Date.parse(String(row.due_date)+'T00:00:00Z'),relationships:[canAct?'assigned':'team']};
}
