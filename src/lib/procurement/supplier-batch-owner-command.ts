import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {supplierBatchSchema,supplierDecisionSchema,supplierExportSchema,supplierResultSchema,supplierConfirmSchema,assertSupplierBatchUnique,prepareSupplierBatchItems} from '@/server/modules/procurement/public/supplier-payment-inputs';
import type {supplierBatchWorkspace} from '@/server/modules/procurement/public/supplier-payments';
import type {z} from 'zod';

export type SupplierBatchView=Awaited<ReturnType<typeof supplierBatchWorkspace>>;
/** Native SSR identity only; never a session token, authority image or grant. */
export type SupplierBatchOwner={companyId:string;userId:string;assignmentId:string;branchId:string|null;departmentId:string|null};
export type SupplierBatchCurrent={base:string;owner?:SupplierBatchOwner;data:SupplierBatchView|null;invoiceId?:string;canSettle:boolean;canHistory:boolean;draft?:()=>{revision:number;forms:readonly HTMLFormElement[]}};
type Request={kind:'create';input:z.infer<typeof supplierBatchSchema>}|{kind:'approve'|'cancel';id:string;input:z.infer<typeof supplierDecisionSchema>}|{kind:'export';id:string;input:z.infer<typeof supplierExportSchema>}|{kind:'result';id:string;input:z.infer<typeof supplierResultSchema>}|{kind:'confirm';id:string;input:z.infer<typeof supplierConfirmSchema>};
type Identity=SupplierBatchOwner&{base:string;origin:string;pathname:string;query:string;revision:string;invoiceId:string|null};
type Target={batchId:string|null;instructionId?:string;invoiceId?:string;itemCount?:number};
export type SupplierBatchReceipt=Record<string,unknown>&{id:string};
export type SupplierBatchAttempt={path:string;body:string;key:string;context:string;request:Request;owner:Identity;target:Target};
type Attempt=SupplierBatchAttempt&{completion:ReturnType<typeof retainWorkCommandCompletion>;fields:string|null;onSaved?:Saved;uncertain:boolean;draftRevision:number|null;hadOtherDraft:boolean};
type Saved=(result:SupplierBatchReceipt,unchanged:boolean)=>void;
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const text=(v:unknown):v is string=>typeof v==='string'&&v.length>0;
const positive=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>0;
const changed=()=>Object.assign(Error('Return to the original supplier batch, Company and work role. The original body, key and newer draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});
const formImage=(completion?:WorkCommandCompletion)=>completion?.form.isConnected?JSON.stringify([...new FormData(completion.form)].map(([name,value])=>[name,typeof value==='string'?value:[value.name,value.size,value.type,value.lastModified]])):null;
function requestFor(path:string,raw:unknown):Request{
 if(path==='supplier-batches')return{kind:'create',input:supplierBatchSchema.parse(raw)};
 const parts=path.split('/');if(parts.length!==3||!text(parts[1])||!/^[a-zA-Z0-9:_.-]+$/.test(parts[1]))throw Error('Select an original supplier batch action.');
 if(parts[0]==='supplier-batches'){
  if(parts[2]==='approve'||parts[2]==='cancel')return{kind:parts[2],id:parts[1],input:supplierDecisionSchema.parse(raw)};
  if(parts[2]==='export')return{kind:'export',id:parts[1],input:supplierExportSchema.parse(raw)};
 }
 if(parts[0]==='supplier-instructions'){
  if(parts[2]==='result')return{kind:'result',id:parts[1],input:supplierResultSchema.parse(raw)};
  if(parts[2]==='confirm')return{kind:'confirm',id:parts[1],input:supplierConfirmSchema.parse(raw)};
 }
 throw Error('Select an original supplier batch action.');
}
function identity(source:SupplierBatchCurrent|null,requireData=true):Identity{
 const owner=source?.owner,node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!source||!owner||requireData&&!source.data||owner.branchId||owner.departmentId||source.data&&source.data.userId!==owner.userId||!node?.isConnected||node.dataset.workStale==='true'||node.dataset.workUser!==owner.userId||node.dataset.workCompany!==owner.companyId||node.dataset.workAssignment!==owner.assignmentId||!text(node.dataset.workRevision))throw changed();
 if(source.data?.batch&&source.data.batch.company_id!==owner.companyId)throw changed();
 return{...owner,base:source.base,origin:window.location.origin,pathname:window.location.pathname,query:window.location.search,revision:node.dataset.workRevision,invoiceId:source.invoiceId??null};
}
function retainedTarget(source:SupplierBatchCurrent,attempt:Attempt){const data=source.data;if(!data)throw changed();if(data.batch?.id!==attempt.target.batchId&&!(data.batch===null&&attempt.target.batchId===null))throw changed();if(attempt.target.instructionId&&!data.items.some(row=>row.id===attempt.target.instructionId&&row.batch_id===attempt.target.batchId&&row.invoice_id===attempt.target.invoiceId))throw changed();}
function prepareTarget(source:SupplierBatchCurrent,request:Request):Target{
 const data=source.data;if(!data||!source.canSettle)throw Error('This supplier action is unavailable. Your draft is retained.');
 if(request.kind==='create'){
  assertSupplierBatchUnique(request.input);prepareSupplierBatchItems(request.input);
  if(data.batch||!data.canCreate||!data.accounts.some(row=>row.id===request.input.bankAccountId))throw Error('Review the current Company bank ledger and supplier source.');
  const seen=new Set<string>();for(const item of request.input.items){const row=data.invoices.find(row=>row.id===item.invoiceId);if(!row||row.company_id!==source.owner?.companyId||Number(row.version)!==item.expectedVersion||source.invoiceId&&item.invoiceId!==source.invoiceId||seen.has(item.invoiceId)||Number(item.amount)<=0)throw Error('Review each original invoice once at its current version and positive amount.');seen.add(item.invoiceId);}
  return{batchId:null};
 }
 const batch=data.batch;if(!batch||batch.company_id!==source.owner?.companyId)throw changed();
 const pay=data.canPay&&batch.created_by!==data.userId&&batch.approved_by!==data.userId;
 if(request.kind==='approve'||request.kind==='cancel'||request.kind==='export'){
  if(batch.id!==request.id||Number(batch.version)!==request.input.expectedVersion)throw Error('Review the current supplier batch version before submitting.');
  const allowed=request.kind==='approve'?data.canApprove&&batch.created_by!==data.userId&&batch.status==='draft':request.kind==='cancel'?data.canCreate&&(batch.created_by===data.userId||data.canApprove)&&['draft','approved'].includes(String(batch.status)):pay&&batch.status==='approved';
  if(!allowed)throw Error('Use the original independent Company Finance or Treasury action.');return{batchId:String(batch.id),itemCount:data.items.length};
 }
 const item=data.items.find(row=>row.id===request.id&&row.batch_id===batch.id);if(!item||Number(item.version)!==request.input.expectedVersion||!['exported','unknown'].includes(String(item.status)))throw Error('Review the current unresolved supplier instruction.');
 if(request.kind==='confirm'){
  if(!pay)throw Error('A current independent Treasury payer must confirm this debit.');
  const bank=data.bank.find(row=>row.id===request.input.bankTransactionId);if(!bank||Number(bank.version)!==request.input.bankVersion||bank.currency!==batch.currency||bank.bank_account!==batch.bank_account_id||Number(bank.amount_minor)!==-Number(item.amount_minor)||String(bank.transaction_date)<String(batch.planned_date)||!data.periods.some(row=>row.id===request.input.periodId))throw Error('Select the current exact debit, bank version and open period for this instruction.');
 }else if(request.kind==='result'&&(request.input.outcome==='unknown'?!pay:!data.canApprove||data.userId===batch.exported_by))throw Error('Use the original independent bank-result review.');
 return{batchId:String(batch.id),instructionId:String(item.id),invoiceId:String(item.invoice_id)};
}
export function validSupplierExport(value:unknown):value is Record<string,unknown>&{content:string;sha256:string}{return object(value)&&text(value.content)&&typeof value.sha256==='string'&&/^[a-f0-9]{64}$/.test(value.sha256);}
export async function verifySupplierExport(value:unknown){if(!validSupplierExport(value))throw Error('Supplier export acknowledgement is incomplete.');const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(value.content)),actual=[...new Uint8Array(digest)].map(byte=>byte.toString(16).padStart(2,'0')).join('');if(actual!==value.sha256)throw Error('The supplier export does not match its retained SHA-256. Retry the original export.');return value;}
export function validSupplierBatchOwnerReceipt(attempt:SupplierBatchAttempt,value:unknown):value is SupplierBatchReceipt{
 if(!object(value)||!text(value.id))return false;const request=attempt.request;
 if(request.kind==='create')return value.id===`supplier-batch-${attempt.key}`&&value.version===1&&value.status==='draft';
 if(value.id!==request.id)return false;
 if(request.kind==='export')return validSupplierExport(value)&&positive(value.rows)&&value.rows===attempt.target.itemCount&&value.confirmationRequired===true; // Native export intentionally has no version.
 if(value.version!==request.input.expectedVersion+1)return false;
 if(request.kind==='approve'||request.kind==='cancel')return value.status===(request.kind==='approve'?'approved':'cancelled');
 if(request.kind==='result')return value.status===request.input.outcome;
 return value.status==='confirmed'&&value.paymentId===`supplier-payment-${attempt.key}`&&value.journalId===`journal-${attempt.key}`;
}
/** Exact application/domain throws before ports.commit (see source-proof).
 * No authority, Work, database, package, 404, generic409 or status-wide release. */
function firstRollback(request:Request,status:number,code:unknown){const closed:Record<Request['kind'],readonly string[]>={create:['422:DUPLICATE_INVOICE','422:INVALID_AMOUNT','422:BANK_ACCOUNT_INVALID'],approve:['403:INDEPENDENT_APPROVER_REQUIRED','409:BATCH_NOT_EDITABLE'],cancel:['403:CANCEL_FORBIDDEN','409:BATCH_NOT_EDITABLE'],export:['409:BATCH_NOT_APPROVED'],result:['403:INDEPENDENT_REVIEW_REQUIRED','409:INSTRUCTION_NOT_PENDING'],confirm:['409:INSTRUCTION_NOT_PENDING','422:BANK_EVIDENCE_MISMATCH','422:SUPPLIER_ATTACHMENT_PURPOSE']};return closed[request.kind].includes(`${status}:${String(code)}`);}

/** One mounted owner; no module registry, storage, new authority or native API. */
export function createSupplierBatchOwnerCommands(current:()=>SupplierBatchCurrent|null,send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let attempt:Attempt|null=null,running=false,created:{owner:string;id:string}|null=null,needsReview:string|null=null;
 const stable=(owner:Identity)=>{const {revision:_revision,...value}=owner;void _revision;return JSON.stringify(value);};
 function captureCurrent(){const source=current(),owner=identity(source),snapshot=JSON.stringify(source),work=captureWorkCommandContext();work();return()=>{work();if(!same(owner,identity(current()))||JSON.stringify(current())!==snapshot)throw changed();};}
 function captureOwner(){const owner=identity(current(),false),work=captureWorkCommandContext();work();return()=>{work();if(!same(owner,identity(current(),false)))throw changed();};}
 async function run(selected:Attempt){
  if(running)throw Error('Wait for the original supplier payment request.');const source=current();if(!source||!same(identity(source),selected.owner))throw changed();retainedTarget(source,selected);if(attempt===selected&&!source.canHistory)throw Error('Original supplier history is currently unavailable. The request is retained.');const assertCurrent=captureCurrent(),wasUncertain=selected.uncertain;assertCurrent();attempt=selected;running=true;
  try{
   selected.uncertain=true;
   const response=await send(`/api/v1${selected.owner.base}/finance/${selected.path}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};assertCurrent();
   if(!response.ok){if(!wasUncertain&&firstRollback(selected.request,response.status,body.error?.code)){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'The supplier outcome is unconfirmed. Retry its original request.'),{code:body.error?.code});}
   if(!validSupplierBatchOwnerReceipt(selected,body.data))throw Error('Supplier acknowledgement does not match the original native operation.');
   if(selected.request.kind==='export'){await verifySupplierExport(body.data);assertCurrent();}
   const data=body.data;let consumed=false;
   return{data,attempt:selected,assertCurrent,acknowledge(){assertCurrent();if(consumed||attempt!==selected)return false;const completion=selected.completion(),draft=current()?.draft?.(),unchanged=!selected.hadOtherDraft&&(selected.draftRevision===null||draft?.revision===selected.draftRevision)&&(!completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&formImage(completion)===selected.fields);consumed=true;if(!unchanged)needsReview=stable(selected.owner);if(selected.request.kind==='create'){const {revision:_revision,...stableOwner}=selected.owner;void _revision;created={owner:JSON.stringify(stableOwner),id:data.id};}attempt=null;selected.onSaved?.(data,unchanged);return{unchanged,completion:unchanged?completion:undefined,created:selected.request.kind==='create',base:selected.owner.base};}};
  }finally{running=false;}
 }
 return{captureCurrent,captureOwner,reviewDraft(){if(attempt||running)throw Error('Recover the original supplier action before discarding.');const owner=identity(current(),false);if(needsReview===stable(owner))needsReview=null;},busy:()=>running,hasRetained:()=>attempt!==null,
  pending(){if(!attempt)return null;try{return same(attempt.owner,identity(current(),false))?attempt:null;}catch{return null;}},
  run(path:string,raw:unknown,completion?:WorkCommandCompletion,onSaved?:Saved){const request=requestFor(path,raw),body=JSON.stringify(request.input);if(attempt){if(attempt.path!==path||attempt.body!==body)throw Error('Recover the original supplier request first. Your newer fields are retained.');return run(attempt);}const source=current();if(!source)throw changed();const owner=identity(source);if(needsReview===stable(owner))throw Error('The original supplier action is confirmed. Explicitly discard the newer draft and review current source before another request.');const {revision:_revision,...stableOwner}=owner;void _revision;if(request.kind==='create'&&created?.owner===JSON.stringify(stableOwner))throw Error('The original supplier batch is already recorded. Review that native batch before another preparation.');const target=prepareTarget(source,request),selected:Attempt={path,body,key:newKey(),context:JSON.stringify(owner),request,owner,target,completion:retainWorkCommandCompletion(completion),fields:formImage(completion),onSaved,uncertain:false,draftRevision:source.draft?.().revision??null,hadOtherDraft:source.draft?.().forms.some(form=>form!==completion?.form)??false};return run(selected);},
  retry(){if(!attempt)throw Error('There is no original supplier payment request.');return run(attempt);},
  async readExport(){const source=current(),owner=identity(source),batch=source?.data?.batch;if(!source?.canHistory||!batch||!source.data?.canPay||batch.created_by===owner.userId||batch.approved_by===owner.userId||typeof batch.export_hash!=='string'||!/^[a-f0-9]{64}$/.test(batch.export_hash))throw Error('Original export recovery requires current independent Treasury history.');const assertCurrent=captureCurrent(),response=await send(`/api/v1${owner.base}/finance/supplier-batches/${encodeURIComponent(String(batch.id))}/export`,{cache:'no-store'}),body=await response.json() as {data?:unknown;error?:{message?:string}};assertCurrent();if(!response.ok)throw Error(body.error?.message??'Original export is unavailable.');if(!object(body.data)||body.data.id!==batch.id||body.data.confirmationRequired!==true||body.data.rows!==source.data.items.length||!positive(body.data.rows))throw Error('The recovered export is not this exact native batch receipt.');const data=await verifySupplierExport(body.data);assertCurrent();if(data.sha256!==batch.export_hash)throw Error('The recovered file does not match this original batch export.');return{data:{content:data.content,sha256:data.sha256},assertCurrent};},
 };
}
