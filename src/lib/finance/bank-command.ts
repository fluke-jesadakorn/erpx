import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import type {importBankStatement,reconcileBankTransaction} from '@/server/modules/finance/public/finance';

export type FinanceBankRequest =
  {kind:'import';input:Parameters<typeof importBankStatement>[1]} |
  {kind:'reconcile';id:string;input:Parameters<typeof reconcileBankTransaction>[2]};
export type FinanceBankScope = {base:string;ownerKey:string;source:string;draft:()=>number;draftRequest?:()=>FinanceBankRequest|null;canStart:boolean;canRecover:boolean};
type Owner = {base:string;ownerKey:string;origin:string;userId:string;companyId:string;assignmentId:string;revision:string};
type Durable = {version:1;owner:Owner;request:FinanceBankRequest;body:string;key:string;uncertain:boolean};
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
function decode(raw:string):Durable {
  let value:unknown;try{value=JSON.parse(raw);}catch{throw quarantined();}
  if(!object(value)||Object.keys(value).sort().join(',')!=='body,key,owner,request,uncertain,version'||value.version!==1||!validOwner(value.owner)||!validRequest(value.request)||!text(value.body)||!text(value.key)||typeof value.uncertain!=='boolean')throw quarantined();
  try{if(!same(JSON.parse(value.body),value.request.input))throw quarantined();}catch{throw quarantined();}
  return value as Durable;
}
const storageKey=(value:Owner)=>'erpx-finance-bank:'+JSON.stringify([value.origin,value.base,value.ownerKey,value.userId,value.companyId]);
function route(request:FinanceBankRequest){return request.kind==='import'?'statements/import':`transactions/${encodeURIComponent(request.id)}/reconcile`;}
function fields(form:HTMLFormElement){return JSON.stringify(Array.from(new FormData(form),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]));}
/** Exactly the two original bank owner contracts. Import returns a count and
 * status, not a version; reconciliation returns the bank movement version. */
export function validFinanceBankReceipt(request:FinanceBankRequest,key:string,value:unknown):value is Record<string,unknown> {
  if(!object(value))return false;
  return request.kind==='import'?value.id===`statement-${key}`&&value.status==='imported'&&value.imported===request.input.rows.length&&value.settlementPaymentId===undefined&&value.refundInstructionId===undefined&&value.payrollRunId===undefined:
    value.id===request.id&&value.status==='matched'&&value.version===request.input.expectedVersion+1;
}
function firstRollback(request:FinanceBankRequest,status:number,code:unknown){
  if(status!==422)return false;
  // The two exact HTTP parsers reject before invoking their native owners.
  if(code==='VALIDATION_FAILED')return true;
  if(request.kind==='reconcile')return code==='MATCH_INPUT_INVALID';
  // application/bank validates these before commitImport, after absent replay.
  return ['STATEMENT_SIZE','BANK_ACCOUNT_INVALID','STATEMENT_UNBALANCED','DUPLICATE_TRANSACTION','STATEMENT_ROW_INVALID','INVALID_MONEY','MONEY_SCALE','MONEY_OVERFLOW'].includes(String(code));
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
        if(!wasUncertain&&firstRollback(selected.request,response.status,error.code)){const saved=pending();if(!saved||saved.key!==selected.key)throw quarantined();storage().removeItem(storageKey(selected.owner));attempt=null;}
        throw Object.assign(Error(typeof error.message==='string'?error.message:'The original Bank request is not confirmed. Retry it safely.'),{status:response.status,code:error.code});
      }
      if(!object(value)||!validFinanceBankReceipt(selected.request,selected.key,value.data))throw Error('The native Bank receipt does not match the original statement or movement. Recover its original request.');
      const data=value.data;let consumed=false;
      return {data,request:selected.request,assertCurrent:captured.assertCurrent,acknowledge(){
        captured.assertCurrent();if(consumed||attempt!==selected)return false;const saved=pending();if(!saved||saved.key!==selected.key||saved.body!==selected.body)throw quarantined();
        const completion=selected.completion(),unchanged=!selected.restored&&(!completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&current()?.draft()===selected.draft&&fields(completion.form)===selected.fields);
        storage().removeItem(storageKey(selected.owner));attempt=null;consumed=true;return{unchanged,completion};
      }};
    }finally{running=false;}
  }
  return {lease,pending,busy:()=>running,
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
