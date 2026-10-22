import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {financeDocumentCommandContext,financeDocumentFormUnchanged} from '@erpx/feature-finance/lib/document-post-control';
import {paymentReversalAcknowledged} from '@erpx/feature-finance/lib/service-payment-controls';

type Source={base:string;paymentId?:string};
export type FinanceInvoiceInput={partyId:string;invoiceDate:string;dueDate:string;currency:string;completionConfirmed:true;completionNote:string;lines:{description:string;quantity:string;unitPrice:string;taxBps:number}[]};
export type FinanceInvoiceDraft={partyId:string;invoiceDate:string;dueDate:string;currency:string;completionNote:string;completionConfirmed:boolean;lines:{description:string;quantity:string;unitPrice:string;taxBps:number}[]};
type InvoiceOwner={base:string;origin:string;userId:string;companyId:string;assignmentId:string;revision:string};
type StoredInvoiceCommand={version:1;owner:InvoiceOwner;path:'invoices';input:FinanceInvoiceInput;body:string;key:string;uncertain:boolean};
type StoredInvoiceDraft={version:1;owner:InvoiceOwner;draft:FinanceInvoiceDraft};
type Attempt={source:Source;path:string;input:unknown;body:string;key:string;context:string;uncertain:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;restored:boolean;invoiceOwner?:InvoiceOwner};
type Store=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
export type FinanceInvoiceRecovery={pending:boolean;input?:FinanceInvoiceInput;draft?:FinanceInvoiceDraft};
export type FinanceSalesCommands={
 submit:(path:string,input:unknown,completion?:WorkCommandCompletion)=>Promise<{data:Record<string,unknown>;completion?:WorkCommandCompletion;unchanged:()=>boolean;acknowledge:()=>boolean}>;
 retry:()=>Promise<{data:Record<string,unknown>;completion?:WorkCommandCompletion;unchanged:()=>boolean;acknowledge:()=>boolean}>;
 pending:()=>boolean;
 restoreInvoice?:()=>FinanceInvoiceRecovery|null;
 readInvoiceDraft?:()=>FinanceInvoiceDraft|null;
 saveInvoiceDraft?:(draft:FinanceInvoiceDraft)=>void;
 clearInvoiceDraft?:()=>void;
};
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const id=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('Return to the original Finance Company, record and work role to confirm the original request.'),{code:'WORK_CONTEXT_CHANGED'});
const quarantined=()=>Error('A retained invoice request is not bound to this original owner. Its bytes remain preserved; return to its original Company and role to recover it.');
const exact=(value:Record<string,unknown>,keys:string[])=>Object.keys(value).sort().join(',')===keys.slice().sort().join(',');
function invoiceOwner(source:Source|null):InvoiceOwner{
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!source||!node?.isConnected||node.dataset.workStale==='true'||!id(node.dataset.workUser)||!id(node.dataset.workCompany)||!id(node.dataset.workAssignment)||!id(node.dataset.workRevision)||typeof window==='undefined')throw changed();
 return{base:source.base,origin:window.location.origin,userId:node.dataset.workUser,companyId:node.dataset.workCompany,assignmentId:node.dataset.workAssignment,revision:node.dataset.workRevision};
}
function validInvoiceOwner(value:unknown):value is InvoiceOwner{
 if(!record(value)||!exact(value,['base','origin','userId','companyId','assignmentId','revision']))return false;
 return id(value.base)&&value.base.startsWith('/holdings/')&&id(value.origin)&&value.origin.startsWith('http')&&id(value.userId)&&id(value.companyId)&&id(value.assignmentId)&&id(value.revision);
}
function validInvoiceInput(value:unknown):value is FinanceInvoiceInput{
 if(!record(value)||!exact(value,['partyId','invoiceDate','dueDate','currency','completionConfirmed','completionNote','lines'])||!id(value.partyId)||value.partyId.length>200||typeof value.invoiceDate!=='string'||value.invoiceDate.length>20||typeof value.dueDate!=='string'||value.dueDate.length>20||typeof value.currency!=='string'||value.currency.length!==3||value.completionConfirmed!==true||typeof value.completionNote!=='string'||value.completionNote.length>2000||!Array.isArray(value.lines)||value.lines.length<1||value.lines.length>100)return false;
 return value.lines.every(line=>record(line)&&exact(line,['description','quantity','unitPrice','taxBps'])&&typeof line.description==='string'&&line.description.length<=1000&&typeof line.quantity==='string'&&line.quantity.length<=100&&typeof line.unitPrice==='string'&&line.unitPrice.length<=100&&typeof line.taxBps==='number'&&Number.isSafeInteger(line.taxBps)&&line.taxBps>=0&&line.taxBps<=10000);
}
function validInvoiceDraft(value:unknown):value is FinanceInvoiceDraft{
 if(!record(value)||!exact(value,['partyId','invoiceDate','dueDate','currency','completionNote','completionConfirmed','lines'])||typeof value.partyId!=='string'||value.partyId.length>200||typeof value.invoiceDate!=='string'||value.invoiceDate.length>20||typeof value.dueDate!=='string'||value.dueDate.length>20||typeof value.currency!=='string'||value.currency.length!==3||typeof value.completionNote!=='string'||value.completionNote.length>2000||typeof value.completionConfirmed!=='boolean'||!Array.isArray(value.lines)||value.lines.length<1||value.lines.length>100)return false;
 return value.lines.every(line=>record(line)&&exact(line,['description','quantity','unitPrice','taxBps'])&&typeof line.description==='string'&&line.description.length<=500&&typeof line.quantity==='string'&&line.quantity.length<=100&&typeof line.unitPrice==='string'&&line.unitPrice.length<=100&&typeof line.taxBps==='number'&&Number.isSafeInteger(line.taxBps)&&line.taxBps>=0&&line.taxBps<=10000);
}
function decodeInvoiceCommand(raw:string):StoredInvoiceCommand{
 let value:unknown;try{value=JSON.parse(raw);}catch{throw quarantined();}
 if(!record(value)||!exact(value,['version','owner','path','input','body','key','uncertain'])||value.version!==1||!validInvoiceOwner(value.owner)||value.path!=='invoices'||!validInvoiceInput(value.input)||!id(value.body)||value.body.length>100000||!id(value.key)||value.key.length>200||typeof value.uncertain!=='boolean')throw quarantined();
 try{if(!same(JSON.parse(value.body),value.input))throw quarantined();}catch{throw quarantined();}
 return value as StoredInvoiceCommand;
}
function decodeInvoiceDraft(raw:string):StoredInvoiceDraft{
 let value:unknown;try{if(raw.length>128000)throw quarantined();value=JSON.parse(raw);}catch{throw quarantined();}
 if(!record(value)||!exact(value,['version','owner','draft'])||value.version!==1||!validInvoiceOwner(value.owner)||!validInvoiceDraft(value.draft))throw quarantined();
 return value as StoredInvoiceDraft;
}
function invoiceCommandKey(owner:InvoiceOwner){
 // Exclude assignment/revision so a changed role cannot silently start a new
 // invoice while an older same-user/same-Company request is still unresolved.
 return 'erpx-finance-sales-invoice:'+JSON.stringify([owner.origin,owner.base,owner.userId,owner.companyId]);
}
function invoiceDraftKey(owner:InvoiceOwner){return `${invoiceCommandKey(owner)}:draft`;}
function salesReceipt(path:string,input:unknown,key:string,value:unknown):value is Record<string,unknown>{
 if(!record(value)||!id(value.id))return false;
 if(path==='invoices')return value.id===`invoice-${key}-invoice`&&value.status==='draft'&&value.version===1;
 if(path==='defaults')return value.id===`sales-defaults-${key}`&&value.status==='draft'&&value.version===1;
 const decision=/^defaults\/([^/]+)\/decide$/.exec(path);
 if(decision)return record(input)&&value.id===decision[1]&&value.version===Number(input.expectedVersion)+1&&value.status===(input.action==='approve'?'approved':input.action==='reject'?'rejected':null);
 // The original issue operation returns only the retained receipt ID.
 return /^receipts\/[^/]+\/issue$/.test(path);
}
/** Only these original owner commands share this UI completion protocol. Their
 * original response, live context and captured draft finalize together. */
function commands(current:()=>Source|null,correction:boolean,send:typeof workFetch,options?:{invoiceRecovery?:boolean;storage?:()=>Store;nextKey?:()=>string}):FinanceSalesCommands{
 let attempt:Attempt|null=null,busy=false;
 const invoiceRecovery=options?.invoiceRecovery===true&&!correction,storage=options?.storage??(()=>sessionStorage),nextKey=options?.nextKey??(()=>crypto.randomUUID());
 const identity=(source:Source)=>JSON.stringify([source.base,source.paymentId??null]);
 function readStoredInvoice():StoredInvoiceCommand|null{
  if(!invoiceRecovery)return null;
  const owner=invoiceOwner(current()),raw=storage().getItem(invoiceCommandKey(owner));
  if(raw===null){if(attempt?.path==='invoices')throw quarantined();return null;}
  const saved=decodeInvoiceCommand(raw);
  if(!same(saved.owner,owner)||attempt?.path==='invoices'&&(!same(attempt.invoiceOwner,owner)||attempt.key!==saved.key||attempt.body!==saved.body))throw quarantined();
  return saved;
 }
 function loadInvoiceAttempt():Attempt|null{
  if(!invoiceRecovery)return attempt;
  const saved=readStoredInvoice();
  if(!saved){if(attempt?.path==='invoices')throw quarantined();return attempt;}
  if(attempt)return attempt;
  attempt={source:{base:saved.owner.base},path:saved.path,input:saved.input,body:saved.body,key:saved.key,context:[saved.owner.userId,saved.owner.revision,saved.owner.companyId,saved.owner.assignmentId].join(':'),uncertain:saved.uncertain,hadCompletion:true,completion:()=>undefined,restored:true,invoiceOwner:saved.owner};
  return attempt;
 }
 function readInvoiceDraft():FinanceInvoiceDraft|null{
  if(!invoiceRecovery)return null;
  const owner=invoiceOwner(current()),raw=storage().getItem(invoiceDraftKey(owner));if(raw===null)return null;
  const saved=decodeInvoiceDraft(raw);if(!same(saved.owner,owner))throw quarantined();return saved.draft;
 }
 function writeInvoiceCommand(selected:Attempt){
  if(!invoiceRecovery||selected.path!=='invoices')return;
  const owner=invoiceOwner(current());if(!selected.invoiceOwner||!same(owner,selected.invoiceOwner))throw changed();
  const saved:StoredInvoiceCommand={version:1,owner, path:'invoices',input:selected.input as FinanceInvoiceInput,body:selected.body,key:selected.key,uncertain:selected.uncertain};
  const raw=JSON.stringify(saved);if(raw.length>128000)throw Error('The original invoice request is too large to retain safely.');
  storage().setItem(invoiceCommandKey(owner),raw);
 }
 function saveInvoiceDraft(draft:FinanceInvoiceDraft):void{
  if(!invoiceRecovery||!validInvoiceDraft(draft))throw Error('The current invoice draft could not be retained safely.');
  const owner=invoiceOwner(current()),selected=loadInvoiceAttempt();
  if(selected?.path==='invoices'&&(!selected.invoiceOwner||!same(selected.invoiceOwner,owner)))throw changed();
  if(!selected&&!readInvoiceDraft())throw Error('A newer invoice draft can be retained only beside its original uncertain request.');
  const raw=JSON.stringify({version:1,owner,draft} satisfies StoredInvoiceDraft);if(raw.length>128000)throw Error('The newer invoice draft is too large to retain safely.');
  storage().setItem(invoiceDraftKey(owner),raw);
 }
 function clearInvoiceDraft():void{
  if(!invoiceRecovery)return;
  const owner=invoiceOwner(current());if(loadInvoiceAttempt()?.path==='invoices')throw Error('Confirm the original invoice request before discarding its newer draft.');
  storage().removeItem(invoiceDraftKey(owner));
 }
 function restoreInvoice():FinanceInvoiceRecovery|null{
  if(!invoiceRecovery)return null;
  const selected=loadInvoiceAttempt(),draft=readInvoiceDraft();
  return selected?.path==='invoices'?{pending:true,input:selected.input as FinanceInvoiceInput,...(draft?{draft}:{})}:draft?{pending:false,draft}:null;
 }
 function removeInvoiceCommand(selected:Attempt){
  if(!invoiceRecovery||selected.path!=='invoices')return;
  const owner=invoiceOwner(current());if(!selected.invoiceOwner||!same(owner,selected.invoiceOwner))throw changed();
  const saved=readStoredInvoice();if(!saved||saved.key!==selected.key||saved.body!==selected.body)throw quarantined();
  storage().removeItem(invoiceCommandKey(owner));
 }
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Finance command.');
  const source=current();if(!source||identity(source)!==identity(captured.source)||financeDocumentCommandContext()!==captured.context)throw Error('Return to the original Company, record and work role to confirm the original request.');
  const assertWork=captureWorkCommandContext(),wasUncertain=captured.uncertain;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||identity(live)!==identity(captured.source)||financeDocumentCommandContext()!==captured.context)throw Object.assign(Error('Your Finance context changed. The original draft and request are retained.'),{code:'WORK_CONTEXT_CHANGED'});};
  busy=true;
  try{
   const url=`/api/v1${source.base}/finance/${correction?`payments/${encodeURIComponent(source.paymentId??'')}/reverse`:`sales/${captured.path}`}`;
   const response=await send(url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!wasUncertain&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){if(invoiceRecovery&&captured.path==='invoices')removeInvoiceCommand(captured);attempt=null;}throw Object.assign(Error(body.error?.message??'Unable to confirm the original Finance command.'),{code:body.error?.code});}
   if(correction?!paymentReversalAcknowledged(body.data,source.paymentId??''):!salesReceipt(captured.path,captured.input,captured.key,body.data))throw Error('The original Finance receipt is incomplete. Retry its original confirmation.');
   assertCurrent();captured.uncertain=true;if(invoiceRecovery&&captured.path==='invoices')writeInvoiceCommand(captured);const data=body.data as Record<string,unknown>;
   return {data,completion:captured.completion(),unchanged:()=>{
    const currentDraft=invoiceRecovery&&captured.path==='invoices'?readInvoiceDraft():null;
    if(captured.restored)return !currentDraft;
    if(currentDraft)return false;
    const completion=captured.completion();return captured.hadCompletion?!!completion&&financeDocumentFormUnchanged(completion):true;
   },acknowledge(){
    assertCurrent();if(attempt!==captured)return false;const completion=captured.completion();
    if(invoiceRecovery&&captured.path==='invoices')removeInvoiceCommand(captured);
    attempt=null;
    if(typeof window!=='undefined'){if(completion){window.dispatchEvent(new CustomEvent('erpx:finance-command-acknowledged',{detail:completion}));window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,financeAcknowledged:true}}));}announceWorkChange();}return true;
   }};
  }catch(error){if(attempt===captured){captured.uncertain=true;if(invoiceRecovery&&captured.path==='invoices')writeInvoiceCommand(captured);}throw error;}finally{busy=false;}
 }
 const api:FinanceSalesCommands={submit(path:string,input:unknown,completion?:WorkCommandCompletion){
  const source=current();if(!source)throw Error('Return to the original Finance editor.');
  if(invoiceRecovery&&path!=='invoices')throw Error('This invoice recovery boundary accepts only standalone invoice requests.');
  if(!correction&&!invoiceRecovery&&!(path==='invoices'||path==='defaults'||/^defaults\/[^/]+\/decide$/.test(path)||/^receipts\/[^/]+\/issue$/.test(path)))throw Error('This Sales command is not available.');
  const body=JSON.stringify(input),context=financeDocumentCommandContext(),saved=invoiceRecovery?loadInvoiceAttempt():attempt;
  if(saved&&(saved.body!==body||saved.path!==path||identity(source)!==identity(saved.source)||context!==saved.context))throw Error('Confirm the original request first. Your newer draft is retained.');
  if(!attempt){
   const owner=invoiceRecovery?invoiceOwner(source):undefined;
   if(invoiceRecovery&&!validInvoiceInput(input))throw Error('The original invoice request is incomplete and cannot be retained safely.');
   const created:Attempt={source:{...source},path,input:invoiceRecovery?JSON.parse(body) as FinanceInvoiceInput:input,body,key:nextKey(),context,uncertain:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion),restored:false,...(owner?{invoiceOwner:owner}:{})};
   if(invoiceRecovery){writeInvoiceCommand(created);attempt=created;storage().removeItem(invoiceDraftKey(owner!));}else attempt=created;
  }
  return run(attempt!);
 },retry(){const selected=invoiceRecovery?loadInvoiceAttempt():attempt;if(!selected)return Promise.reject(Error('There is no original Finance command to confirm.'));return run(selected);},pending(){return (invoiceRecovery?loadInvoiceAttempt():attempt)!==null;}};
 if(invoiceRecovery){api.restoreInvoice=restoreInvoice;api.readInvoiceDraft=readInvoiceDraft;api.saveInvoiceDraft=saveInvoiceDraft;api.clearInvoiceDraft=clearInvoiceDraft;}
 return api;
}
export function createFinanceSalesCommands(current:()=>{base:string}|null,send:typeof workFetch=workFetch){return commands(current,false,send);}
export function createFinanceSalesInvoiceCommands(current:()=>{base:string}|null,send:typeof workFetch=workFetch,storage:()=>Store=()=>sessionStorage,nextKey=()=>crypto.randomUUID()){return commands(current,false,send,{invoiceRecovery:true,storage,nextKey});}
export function createPaymentCorrectionCommands(current:()=>{base:string;paymentId:string}|null,send:typeof workFetch=workFetch){
 const owner=commands(current,true,send);return {submit:(input:unknown,completion?:WorkCommandCompletion)=>owner.submit('reverse',input,completion),retry:owner.retry,pending:owner.pending};
}
