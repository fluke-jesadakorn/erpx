import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import type {importBankStatement,reconcileBankTransaction} from '@/server/modules/finance/public/finance';

export type FinanceBankRequest =
  {kind:'import';input:Parameters<typeof importBankStatement>[1]} |
  {kind:'reconcile';id:string;input:Parameters<typeof reconcileBankTransaction>[2]};
export type FinanceBankDraft =
  {kind:'import';fields:{bankAccount:string;number:string;date:string;opening:string;closing:string};rows:{externalId:string;transactionDate:string;description:string;amount:string}[]} |
  {kind:'reconcile';id:string;selectedPayment:string;proof:string};
export type FinanceBankScope = {base:string;ownerKey:string;source:string;draft:()=>number;draftRequest?:()=>FinanceBankRequest|null;canStart:boolean;canRecover:boolean};
type Owner = {base:string;ownerKey:string;origin:string;userId:string;companyId:string;assignmentId:string;revision:string};
type Durable = {version:1;owner:Owner;request:FinanceBankRequest;body:string;key:string;uncertain:boolean};
type DurableDraft = {version:1;owner:Owner;draft:FinanceBankDraft};
type Attempt = Durable & {draft:number;fields?:string;completion:ReturnType<typeof retainWorkCommandCompletion>;restored:boolean};
type Store = Pick<Storage,'getItem'|'setItem'|'removeItem'>;
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const integer=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value);
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('Return to the original Bank record, Company and work role to recover this request. Your draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});
const quarantined=()=>Error('The retained Bank request is not bound to this original owner. Keep its bytes and return to its original Company and role.');
function owner(scope:FinanceBankScope|null):Owner {
  const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
  if(!scope||!node?.isConnected||node.dataset.workStale==='true'||!text(node.dataset.workUser)||!text(node.dataset.workCompany)||!text(node.dataset.workAssignment)||!text(node.dataset.workRevision))throw changed();
  return {base:scope.base,ownerKey:scope.ownerKey,origin:window.location.origin,userId:node.dataset.workUser,companyId:node.dataset.workCompany,assignmentId:node.dataset.workAssignment,revision:node.dataset.workRevision};
}
function validRequest(value:unknown):value is FinanceBankRequest {
  if(!object(value)||!object(value.input))return false;
  const input=value.input;
  if(value.kind==='import')return ['bankAccount','statementNumber','statementDate','opening','closing','sourceHash'].every(key=>typeof input[key]==='string')&&input.settlementPaymentId===undefined&&Array.isArray(input.rows)&&input.rows.length<=500&&input.rows.every(row=>object(row)&&['externalId','transactionDate','amount','currency'].every(key=>typeof row[key]==='string')&&(row.description===undefined||typeof row.description==='string'));
  return value.kind==='reconcile'&&text(value.id)&&integer(input.expectedVersion)&&input.expectedVersion>0&&input.matchedType==='payment'&&typeof input.matchedId==='string'&&typeof input.proof==='string'&&Array.isArray(input.attachmentSetIds)&&input.attachmentSetIds.length<=20&&input.attachmentSetIds.every(text);
}
function validOwner(value:unknown):value is Owner {return object(value)&&Object.keys(value).sort().join(',')==='assignmentId,base,companyId,origin,ownerKey,revision,userId'&&Object.values(value).every(text);}
function validDraft(value:unknown):value is FinanceBankDraft {
  if(!object(value))return false;
  if(value.kind==='import'){
    if(Object.keys(value).sort().join(',')!=='fields,kind,rows'||!object(value.fields)||Object.keys(value.fields).sort().join(',')!=='bankAccount,closing,date,number,opening'||!Array.isArray(value.rows)||value.rows.length<1||value.rows.length>500)return false;
    if(!Object.values(value.fields).every(item=>typeof item==='string'&&item.length<=200))return false;
    return value.rows.every(row=>object(row)&&Object.keys(row).sort().join(',')==='amount,description,externalId,transactionDate'&&typeof row.externalId==='string'&&row.externalId.length<=300&&typeof row.transactionDate==='string'&&row.transactionDate.length<=20&&typeof row.description==='string'&&row.description.length<=2000&&typeof row.amount==='string'&&row.amount.length<=100);
  }
  return value.kind==='reconcile'&&Object.keys(value).sort().join(',')==='id,kind,proof,selectedPayment'&&text(value.id)&&value.id.length<=200&&typeof value.selectedPayment==='string'&&value.selectedPayment.length<=200&&typeof value.proof==='string'&&value.proof.length<=2000;
}
function decode(raw:string):Durable {
  let value:unknown;try{value=JSON.parse(raw);}catch{throw quarantined();}
  if(!object(value)||Object.keys(value).sort().join(',')!=='body,key,owner,request,uncertain,version'||value.version!==1||!validOwner(value.owner)||!validRequest(value.request)||!text(value.body)||!text(value.key)||typeof value.uncertain!=='boolean')throw quarantined();
  try{if(!same(JSON.parse(value.body),value.request.input))throw quarantined();}catch{throw quarantined();}
  return value as Durable;
}
function decodeDraft(raw:string):DurableDraft {
  let value:unknown;try{if(raw.length>128_000)throw quarantined();value=JSON.parse(raw);}catch{throw quarantined();}
  if(!object(value)||Object.keys(value).sort().join(',')!=='draft,owner,version'||value.version!==1||!validOwner(value.owner)||!validDraft(value.draft))throw quarantined();
  return value as DurableDraft;
}
const storageKey=(value:Owner)=>'erpx-finance-bank:'+JSON.stringify([value.origin,value.base,value.ownerKey,value.userId,value.companyId]);
const draftStorageKey=(value:Owner)=>`${storageKey(value)}:draft`;
function route(request:FinanceBankRequest){return request.kind==='import'?'statements/import':`transactions/${encodeURIComponent(request.id)}/reconcile`;}
function fields(form:HTMLFormElement){return JSON.stringify(Array.from(new FormData(form),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]));}
/** Exactly the two original bank owner contracts. Import returns a count and
 * status, not a version; reconciliation returns the bank movement version. */
export function validFinanceBankReceipt(request:FinanceBankRequest,key:string,value:unknown):value is Record<string,unknown> {
  if(!object(value))return false;
  return request.kind==='import'?value.id===`statement-${key}`&&value.status==='imported'&&value.imported===request.input.rows.length&&value.settlementPaymentId===undefined&&value.refundInstructionId===undefined&&value.payrollRunId===undefined:
    value.id===request.id&&value.status==='matched'&&value.version===request.input.expectedVersion+1;
}
function firstRollback(request:FinanceBankRequest,status:number,code:unknown,wasUncertain:boolean){
  if(status!==422)return false;
  const rejected=String(code);
  // The route parser and this reconciliation check run before native receipt
  // replay. They prove only that a first submission was rejected; after an
  // uncertain submission they cannot rule out a previously committed result.
  if(rejected==='VALIDATION_FAILED'||request.kind==='reconcile'&&rejected==='MATCH_INPUT_INVALID')return !wasUncertain;
  if(request.kind==='reconcile'){
    // These attachment checks run only after the owner has looked for a prior
    // command receipt. Parser and MATCH_INPUT_INVALID failures can bypass replay.
    return ['ATTACHMENT_SETS_REQUIRED','ATTACHMENT_BINDING_CONTRACT_INVALID','ATTACHMENT_POLICY_UNKNOWN',
      'ATTACHMENT_RETENTION_POLICY_UNKNOWN','ATTACHMENT_TARGET_UNSUPPORTED','ATTACHMENT_SET_TARGET','ATTACHMENT_BINDING_CONTEXT',
      'ATTACHMENT_SET_EMPTY','ATTACHMENT_REQUIRED','ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED'].includes(rejected);
  }
  // application/bank validates these after absent replay and before commitImport.
  return ['STATEMENT_SIZE','BANK_ACCOUNT_INVALID','CURRENCY_SCALE_REQUIRED','FINANCE_CURRENCY_SCALE_UNSUPPORTED','STATEMENT_UNBALANCED',
    'DUPLICATE_TRANSACTION','STATEMENT_ROW_INVALID','INVALID_MONEY','MONEY_SCALE','MONEY_OVERFLOW','MANUAL_RATE_REQUIRED',
    'FX_RATE_PRECISION','FX_RATE_REQUIRED'].includes(rejected);
}
/** Closed Bank owner recovery; it never substitutes a new request for an
 * uncertain original, and uses native authority and receipt replay unchanged. */
export function createFinanceBankCommands(current:()=>FinanceBankScope|null,send:typeof workFetch=workFetch,storage:()=>Store=()=>sessionStorage,nextKey=()=>crypto.randomUUID()) {
  let attempt:Attempt|null=null,running=false;
  function pending():Durable|null {
    const native=owner(current()),raw=storage().getItem(storageKey(native));
    if(raw===null){if(attempt)throw quarantined();return null;}
    const saved=decode(raw);
    if(!same(saved.owner,native)||attempt&&(!same(saved.owner,attempt.owner)||saved.key!==attempt.key||saved.body!==attempt.body||!same(saved.request,attempt.request)))throw quarantined();
    return saved;
  }
  function readDraft():FinanceBankDraft|null {
    const native=owner(current()),command=pending(),key=draftStorageKey(native),raw=storage().getItem(key);
    if(!command){if(raw!==null)storage().removeItem(key);return null;}
    if(raw===null)return null;
    const saved=decodeDraft(raw);
    if(!same(saved.owner,native)||saved.draft.kind!==command.request.kind||saved.draft.kind==='reconcile'&&command.request.kind==='reconcile'&&saved.draft.id!==command.request.id)throw quarantined();
    return saved.draft;
  }
  function saveDraft(draft:FinanceBankDraft):void {
    const native=owner(current()),command=pending(),key=draftStorageKey(native),store=storage();
    if(!command||!validDraft(draft)||draft.kind!==command.request.kind||draft.kind==='reconcile'&&command.request.kind==='reconcile'&&draft.id!==command.request.id){store.removeItem(key);throw Error('A newer Bank draft can be retained only beside its original uncertain request.');}
    const value:DurableDraft={version:1,owner:native,draft},raw=JSON.stringify(value);
    if(raw.length>128_000){store.removeItem(key);throw Error('The newer Bank draft is too large to retain safely.');}
    // Replace atomically from the caller's point of view. If storage refuses a
    // new value, discard the old snapshot so a reload cannot present it as the
    // user's latest edits.
    store.removeItem(key);
    try{store.setItem(key,raw);}catch(error){try{store.removeItem(key);}catch{/* Preserve the quota/storage error. */}throw error;}
  }
  function clearDraft():void {const native=owner(current());storage().removeItem(draftStorageKey(native));}
  function persist(selected:Attempt){const native=owner(current());if(!same(native,selected.owner))throw changed();const old=storage().getItem(storageKey(native));if(old!==null){const saved=decode(old);if(!same(saved.owner,selected.owner)||saved.key!==selected.key||saved.body!==selected.body||!same(saved.request,selected.request))throw quarantined();}const {draft:_draft,fields:_fields,completion:_completion,restored:_restored,...saved}=selected;void _draft;void _fields;void _completion;void _restored;storage().setItem(storageKey(native),JSON.stringify(saved));}
  function lease(){const scope=current();if(!scope)throw changed();const native=owner(scope),source=scope.source,canStart=scope.canStart,canRecover=scope.canRecover,work=captureWorkCommandContext(),path=window.location.pathname,query=window.location.search;
    const assertCurrent=()=>{work();const now=current();if(!now||!same(owner(now),native)||now.source!==source||now.canStart!==canStart||now.canRecover!==canRecover||window.location.pathname!==path||window.location.search!==query)throw changed();};assertCurrent();return{scope,native,assertCurrent};}
  async function execute(selected:Attempt){
    if(running)throw Error('Wait for the original Bank request.');
    const captured=lease(),expectedOwner=selected.request.kind==='import'?'statement-import':`transaction:${selected.request.id}`;
    if(!same(captured.native,selected.owner)||!captured.scope.canRecover||captured.scope.ownerKey!==expectedOwner)throw changed();
    const wasUncertain=selected.uncertain;selected.uncertain=true;persist(selected);running=true;
    try{
      const response=await send(`/api/v1${selected.owner.base}/finance/bank/${route(selected.request)}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller');
      const value:unknown=await response.json();captured.assertCurrent();
      if(!response.ok){const error=object(value)&&object(value.error)?value.error:{};
        if(firstRollback(selected.request,response.status,error.code,wasUncertain)){const saved=pending();if(!saved||saved.key!==selected.key)throw quarantined();storage().removeItem(storageKey(selected.owner));storage().removeItem(draftStorageKey(selected.owner));attempt=null;}
        throw Object.assign(Error(typeof error.message==='string'?error.message:'The original Bank request is not confirmed. Retry it safely.'),{status:response.status,code:error.code});
      }
      if(!object(value)||!validFinanceBankReceipt(selected.request,selected.key,value.data))throw Error('The native Bank receipt does not match the original statement or movement. Recover its original request.');
      const data=value.data;let consumed=false;
      return {data,request:selected.request,assertCurrent:captured.assertCurrent,acknowledge(){
        captured.assertCurrent();if(consumed||attempt!==selected)return false;const saved=pending();if(!saved||saved.key!==selected.key||saved.body!==selected.body)throw quarantined();
        const completion=selected.completion(),unchanged=!selected.restored&&(!completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&current()?.draft()===selected.draft&&fields(completion.form)===selected.fields);
        storage().removeItem(storageKey(selected.owner));storage().removeItem(draftStorageKey(selected.owner));attempt=null;consumed=true;return{unchanged,completion:selected.restored?undefined:completion};
      }};
    }finally{running=false;}
  }
  return {lease,pending,readDraft,saveDraft,clearDraft,busy:()=>running,
    async submit(request:FinanceBankRequest,form:HTMLFormElement){
      const scope=lease();if(running||pending())throw Error('Recover the original Bank request before submitting another body. Your newer draft is retained.');
      if(!scope.scope.canStart||!validRequest(request)||scope.scope.ownerKey!==(request.kind==='import'?'statement-import':`transaction:${request.id}`))throw Error('Review the current Bank record and available Company workflow before submitting.');
      const input=JSON.parse(JSON.stringify(request)) as FinanceBankRequest,completion:WorkCommandCompletion={form,formEditVersion:form.getAttribute('data-erpx-edit-version')};
      attempt={version:1,owner:scope.native,request:input,body:JSON.stringify(input.input),key:nextKey(),uncertain:false,draft:scope.scope.draft(),fields:fields(form),completion:retainWorkCommandCompletion(completion),restored:false};persist(attempt);return execute(attempt);
    },
    async retry(form?:HTMLFormElement){
      const saved=pending();if(!saved)return Promise.reject(Error('There is no original Bank request to recover.'));
      if(!attempt){
        const scope=lease(),draft=scope.scope.draft(),draftRequest=scope.scope.draftRequest?.()??null;
        // A pristine editor after reload may still show its empty defaults. Any
        // edited, non-matching form is retained while the exact old command is
        // recovered; a matching restored import can be completed normally.
        const restored= draft>0&&!same(draftRequest,saved.request);
        const completion=form?{form,formEditVersion:form.getAttribute('data-erpx-edit-version')}:undefined;
        attempt={...saved,uncertain:true,draft,fields:form?fields(form):undefined,completion:retainWorkCommandCompletion(completion),restored};
      }
      return execute(attempt);
    },
  };
}
