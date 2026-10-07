import type {Clock} from "../../shared";
import {BusinessRuleError} from "../../shared";
import {supplierBillSchema,supplierBillDecisionSchema,supplierInvoicePolicySchema,requestedBillReceipts,billReceiptScope,billClearingAccount,prepareBillLines,assertBillDecision,assertBillPolicy,assertBillInvoiceHandoff,assertMatchedInvoice,matchedBillLines,billWorkItem,type BillRow,type BillCapture,type BillDecision,type BillPolicy,type BillCapturePlan,type BillWorkFacts} from "../domain/supplier-bills";
type Command={key:string;operation:string;id:string;now:number};
export type BillMutation=Command&(
 | {kind:'capture';input:BillCapture;plan:BillCapturePlan}
 | {kind:'decision';input:BillDecision;bill:BillRow;returnDraft:boolean;variance:boolean;status:'approved'|'returned'}
 | {kind:'policy';input:BillPolicy;previous:'optional'|'required';version:number}
);
export interface BillPorts<Stage>{
 clock:Clock;companyId:string;userId:string;canRead:boolean;canWrite:boolean;canApprove:boolean;canCommerceRead:boolean;samePerson(id:unknown):boolean;
 authorizeRead():Promise<void>;authorizeWrite():Promise<void>;authorizeApprove():Promise<void>;authorizeCommerceRead():Promise<void>;
 replay(key:string,operation:string,id:string,input:unknown):Promise<Record<string,unknown>|null>;
 bill(id:string):Promise<BillRow|null>;workspace():Promise<BillRow[]>;captureOptions():Promise<{fulfillments:BillRow[];lines:BillRow[];currency:string|null}>;
 detail(bill:BillRow):Promise<{lines:BillRow[];receipts:BillRow[];events:BillRow[];accounts:BillRow[];invoicingUserId:string|null}>;
 companyAuthority():Promise<boolean>;policy():Promise<BillRow|null>;policyEvents():Promise<BillRow[]>;
 receipt(id:string):Promise<BillRow|null>;order(id:string):Promise<BillRow>;configuration():Promise<BillRow|null>;postings(journalId:string):Promise<BillRow[]>;receiptLines(ids:string[]):Promise<BillRow[]>;
 variance(id:string):Promise<boolean>;varianceAccount(id:string,chartId:string):Promise<boolean>;
 commit<Result>(mutation:BillMutation,result:Result):Promise<Result>;
 receiptHistory(id:string):Promise<BillRow|null>;matchingRequired():Promise<boolean>;billReceipts(id:string):Promise<BillRow[]>;
 stageInvoice(input:{bill:BillRow;receipts:BillRow[];fulfillment:BillRow;matchVersion:number;invoiceId:string;key:string;now:number}):Promise<Stage[]>;
 billByInvoice(id:string):Promise<BillRow|null>;stagePosting(input:{bill:BillRow;invoice:BillRow;key:string;now:number}):Promise<Stage[]>;
 workFacts(row:BillRow):Promise<BillWorkFacts>;
}
export function createSupplierBillApplication<Stage>(ports:BillPorts<Stage>){
 const get=async(id:string)=>{await ports.authorizeRead();const bill=await ports.bill(id);if(!bill)throw new BusinessRuleError(404,'SUPPLIER_BILL_NOT_FOUND','Supplier invoice review not found in this scope.');return bill;};
 const workspace=async()=>{await ports.authorizeRead();return ports.workspace();};
 return {
 get,workspace,
 async captureOptions(){await ports.authorizeWrite();await ports.authorizeCommerceRead();return ports.captureOptions();},
 async detail(id:string){const bill=await get(id),data=await ports.detail(bill);return {bill,lines:data.lines,receipts:data.receipts,events:data.events,accounts:data.accounts,canReview:ports.canApprove&&ports.canWrite&&!ports.samePerson(bill.created_by)&&(bill.status!=='invoiced'||bill.invoice_status==='draft'&&!ports.samePerson(bill.invoice_created_by)),canPrepare:ports.canWrite&&data.invoicingUserId===ports.userId};},
 async policy(){await ports.authorizeRead();const row=await ports.policy(),canManage=ports.canApprove&&await ports.companyAuthority(),events=canManage?await ports.policyEvents():[];return {matchingMode:String(row?.matching_mode??'optional') as 'optional'|'required',version:Number(row?.version??0),updatedByName:row?.updated_by_name?String(row.updated_by_name):null,updatedAt:row?.updated_at?Number(row.updated_at):null,canManage,events};},
 async savePolicy(raw:unknown,key:string){const input=supplierInvoicePolicySchema.parse(raw);await ports.authorizeApprove();const operation='procurement.invoice-policy.update',id=ports.companyId,prior=await ports.replay(key,operation,id,input);if(prior)return prior;if(!await ports.companyAuthority())throw new BusinessRuleError(403,'COMPANY_FINANCE_AUTHORITY_REQUIRED','A company-wide Finance approver or owner must change supplier invoice matching policy.');const decision=assertBillPolicy(input,await ports.policy());return ports.commit({kind:'policy',key,operation,id,input,...decision,now:ports.clock.now()},{id,version:decision.version,matchingMode:input.matchingMode});},
 async capture(raw:unknown,key:string){
 const input=supplierBillSchema.parse(raw);await ports.authorizeWrite();await ports.authorizeCommerceRead();const id=`bill-match-${key}`,operation='procurement.bill.capture',prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 const requested=requestedBillReceipts(input),rows:BillRow[]=[];for(const selected of requested){const row=await ports.receipt(selected.fulfillmentId);if(!row)throw new BusinessRuleError(404,'RECEIPT_NOT_FOUND','Goods receipt not found.');rows.push(row);}
 const orders=await Promise.all([...new Set(rows.map(x=>String(x.order_id)))].map(id=>ports.order(id))),source=billReceiptScope(input,rows,orders),config=await ports.configuration();if(!config)throw new BusinessRuleError(409,'BILL_POSTING_PROFILE_REQUIRED','Review the Company posting profile before matching supplier invoices.');
 let clearing:string|null=null;for(const receipt of source.receipts)clearing=billClearingAccount(receipt,source.order,config,await ports.postings(String(receipt.journal_id)),clearing);
 const actual=await ports.receiptLines(source.receipts.map(x=>String(x.id))),prepared=prepareBillLines(input,actual),plan:BillCapturePlan={requested,...source,config,clearing:clearing!,...prepared};
 return ports.commit({kind:'capture',key,operation,id,input,plan,now:ports.clock.now()},{id,version:1,receiptCount:source.receipts.length});
 },
 async decide(id:string,raw:unknown,key:string){
 const input=supplierBillDecisionSchema.parse(raw);await ports.authorizeWrite();await ports.authorizeApprove();const operation=`procurement.bill.${input.action}`,prior=await ports.replay(key,operation,id,input);if(prior)return prior;
 const bill=await get(id),returnDraft=assertBillDecision(input,bill,ports.samePerson),variance=await ports.variance(id);
 if(input.action==='approve'&&variance){if(!input.varianceAccountId)throw new BusinessRuleError(422,'PRICE_VARIANCE_ACCOUNT_REQUIRED','Choose the Company-approved expense account for purchase price variance.');if(!await ports.varianceAccount(input.varianceAccountId,String(bill.chart_id)))throw new BusinessRuleError(422,'PRICE_VARIANCE_ACCOUNT_INVALID',"Use an active expense account in this Company chart's functional currency.");}
 const status=input.action==='approve'?'approved':'returned';return ports.commit({kind:'decision',key,operation,id,input,bill,returnDraft,variance,status,now:ports.clock.now()},{id,version:input.expectedVersion+1,status,...(returnDraft?{voidedInvoiceId:bill.invoice_id}:{})});
 },
 async invoiceHandoff(fulfillment:BillRow,input:{matchId?:string;matchVersion?:number;date:string;dueDate:string},invoiceId:string,key:string){const history=await ports.receiptHistory(String(fulfillment.id)),required=await ports.matchingRequired();if(!history&&!input.matchId&&!required)return null;if(!input.matchId)throw new BusinessRuleError(409,'SUPPLIER_BILL_MATCH_REQUIRED',required?'Company policy requires an independently approved supplier invoice match before AP can be prepared.':'Use the approved supplier invoice review for this receipt.');const bill=await get(input.matchId),receipts=await ports.billReceipts(String(bill.id));assertBillInvoiceHandoff(bill,receipts,fulfillment,input);return {bill,steps:await ports.stageInvoice({bill,receipts,fulfillment,matchVersion:input.matchVersion??0,invoiceId,key,now:ports.clock.now()})};},
 async invoicePosting(invoice:BillRow,key:string){const bill=await ports.billByInvoice(String(invoice.id));if(!bill)return null;assertMatchedInvoice(bill,invoice);return {lines:matchedBillLines(bill),steps:await ports.stagePosting({bill,invoice,key,now:ports.clock.now()})};},
 async work(){if(!ports.canRead)return [];const rows=(await workspace()).filter(x=>['review','approved','returned'].includes(String(x.status))),items=[];for(const row of rows){const item=billWorkItem(row,await ports.workFacts(row),ports);if(item)items.push(item);}return items;},
 };
}
