import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {decimalToMinor} from '@/lib/finance/finance-money';
import type {ManufacturingPurchaseControls} from './purchase-controls';

export type ManufacturingOrderActor={userId:string;companyId:string;assignmentId:string;branchId:string|null;departmentId:string|null};
export type ManufacturingOrderCurrent={base:string;actor:ManufacturingOrderActor;order:Record<string,unknown>;operations:readonly Record<string,unknown>[];materialLots:readonly Record<string,unknown>[];canWrite:boolean;canReview:boolean;purchase?:ManufacturingPurchaseControls};
export type ManufacturingOrderAction='reserve'|'issue'|'complete-operation'|'report'|'pass-qc'|'fail-qc'|'rework'|'scrap'|'cancel';
export type ManufacturingOrderRequest=Record<string,unknown>&{entity:'order';id:string;action:ManufacturingOrderAction;expectedVersion:number;reason:string};
export type ManufacturingOrderReceipt={id:string;status:string;version:number;journalId:string|null};
type Journal='none'|'native'|'nullable-native';
type Attempt={owner:string;href:string;work:string;body:string;key:string;journal:Journal;formSource:string|null;completion:ReturnType<typeof retainWorkCommandCompletion>;uncertain:boolean};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const actions:readonly ManufacturingOrderAction[]=['reserve','issue','complete-operation','report','pass-qc','fail-qc','rework','scrap','cancel'];
const states:Record<ManufacturingOrderAction,string>={reserve:'reserved',issue:'in_progress','complete-operation':'in_progress',report:'awaiting_qc','pass-qc':'completed','fail-qc':'qc_failed',rework:'in_progress',scrap:'cancelled',cancel:'cancelled'};
const starts:Record<ManufacturingOrderAction,readonly string[]>={reserve:['planned'],issue:['reserved'],'complete-operation':['in_progress'],report:['in_progress'],'pass-qc':['awaiting_qc'],'fail-qc':['awaiting_qc'],rework:['qc_failed'],scrap:['qc_failed'],cancel:['planned','reserved']};
export function manufacturingOrderOwner(source:ManufacturingOrderCurrent){const row=source.order;return JSON.stringify([source.base,source.actor,row.id,row.company_id,row.branch_id,row.warehouse_id,row.recommendation_id,row.specification_id,row.specification_version]);}
export function manufacturingOrderImage(source:ManufacturingOrderCurrent){return JSON.stringify([source.base,source.actor,source.order,source.operations,source.materialLots,source.canWrite,source.canReview,source.purchase]);}
function changed(){return Object.assign(Error('Return to the original Manufacturing order, Company and selected work revision to recover the original change. Your draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});}
function work(source:ManufacturingOrderCurrent){
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!node?.isConnected||node.dataset.workStale==='true'||node.dataset.workUser!==source.actor.userId||node.dataset.workCompany!==source.actor.companyId||node.dataset.workAssignment!==source.actor.assignmentId||!node.dataset.workRevision)throw changed();
 return JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workRevision]);
}
function request(raw:unknown):ManufacturingOrderRequest{
 if(!object(raw)||raw.entity!=='order'||typeof raw.id!=='string'||!raw.id||!actions.includes(raw.action as ManufacturingOrderAction)||!Number.isSafeInteger(raw.expectedVersion)||Number(raw.expectedVersion)<1||typeof raw.reason!=='string'||raw.reason.trim().length<3||raw.reason.trim().length>3000)throw Error('Review the original order, action, revision and reason before submitting.');
 return raw as ManufacturingOrderRequest;
}
function journal(source:ManufacturingOrderCurrent,input:ManufacturingOrderRequest):Journal{
 if(input.action==='issue')return 'nullable-native'; // Native issue valuation is computed from locked Stock/lot facts, never guessed by the client.
 if(input.action==='report'){
  if(typeof input.laborCost!=='string'||typeof input.overheadCost!=='string'||typeof input.outputQuantity!=='string'||typeof input.scrapQuantity!=='string')throw Error('Enter the actual output, scrap and conversion costs before submitting.');
  decimalToMinor(input.outputQuantity,3);decimalToMinor(input.scrapQuantity,3);
  const labor=decimalToMinor(input.laborCost),overhead=decimalToMinor(input.overheadCost);
  if(labor<0||overhead<0||!Number.isSafeInteger(labor+overhead))throw Error('Production costs must be nonnegative and within supported precision.');
  return labor+overhead===0?'none':'native';
 }
 if(input.action==='pass-qc'||input.action==='scrap'){
  const amounts=['material_cost_minor','labor_cost_minor','overhead_cost_minor'].map(name=>{const value=source.order[name];if((typeof value!=='number'&&typeof value!=='string')||typeof value==='string'&&!/^\d+$/.test(value))throw changed();const number=Number(value);if(!Number.isSafeInteger(number)||number<0)throw changed();return number;});
  const total=amounts.reduce((sum,value)=>sum+value,0);if(!Number.isSafeInteger(total))throw changed();return total===0?'none':'native';
 }
 return 'none';
}
function validReceipt(value:unknown,input:ManufacturingOrderRequest,key:string,expected:Journal):value is ManufacturingOrderReceipt{
 return object(value)&&value.id===input.id&&Number.isSafeInteger(value.version)&&value.version===input.expectedVersion+1&&value.status===states[input.action]&&(expected==='none'?value.journalId===null:expected==='native'?value.journalId===`journal-${key}`:value.journalId===null||value.journalId===`journal-${key}`);
}
/** Closed, source-traced rejections occur before executeCommand/COMMIT. A prior
 * uncertain send, generic conflict or Work/authority error can never release it. */
function firstRollback(input:ManufacturingOrderRequest,status:number,error:Record<string,unknown>){
 const code=error.code;
 if(status===409&&code==='CONFLICT'&&(error.message==='Order changed. Reload and retry.'||['pass-qc','fail-qc'].includes(input.action)&&error.message==='QC must be independent from output reporting.'||input.action==='scrap'&&error.message==='The reporter cannot approve failed output disposal.'))return true;
 if(status===422&&code==='VALIDATION_FAILED')return true;
 if(status===409&&code==='INVALID_MANUFACTURING_TRANSITION')return true;
 if(input.action==='report'&&status===422&&['MANUFACTURING_OUTPUT_REQUIRED','SERIAL_QUANTITY_INVALID'].includes(String(code)))return true;
 if(input.action==='issue'&&status===409&&code==='MATERIAL_RESERVATION_REQUIRED')return true;
 if(input.action==='issue'&&status===422&&code==='MANUFACTURING_LOT_ALLOCATION_REQUIRED')return true;
 if(input.action==='pass-qc'&&status===422&&['MANUFACTURING_OUTPUT_IDENTITY','MANUFACTURING_OUTPUT_EXPIRY'].includes(String(code)))return true;
 if(input.action==='complete-operation'&&status===404&&code==='MANUFACTURING_OPERATION_NOT_FOUND')return true;
 if(status===422&&code==='MANUFACTURING_ATTACHMENT_ACTION')return true;
 if(status===422&&['report','pass-qc','fail-qc','rework'].includes(input.action)&&code==='MANUFACTURING_ATTACHMENT_PURPOSE')return true;
 if(status===422&&['issue','report','pass-qc','scrap'].includes(input.action)&&['MANUFACTURING_ACCOUNTING_REQUIRED','MANUFACTURING_PERIOD_REQUIRED'].includes(String(code)))return true;
 return false;
}
export function createManufacturingOrderCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>ManufacturingOrderCurrent|null=()=>null,pending:Attempt|null=null,busy=false;
 function capture(ownerOnly=false){
  const source=read();if(!source)throw changed();const assertWork=captureWorkCommandContext(),stamp=ownerOnly?manufacturingOrderOwner(source):manufacturingOrderImage(source),href=window.location.href,selectedWork=work(source);assertWork();
  return()=>{assertWork();const current=read();if(!current||(ownerOnly?manufacturingOrderOwner(current):manufacturingOrderImage(current))!==stamp||window.location.href!==href||work(current)!==selectedWork)throw changed();};
 }
 async function execute(attempt:Attempt,replay:boolean){
  if(busy)throw Error('The original Manufacturing change is still running.');
  const source=read(),assertCurrent=capture();if(!source||manufacturingOrderOwner(source)!==attempt.owner||window.location.href!==attempt.href||work(source)!==attempt.work)throw changed();
  const input=request(JSON.parse(attempt.body)),row=source.order;
  if(row.id!==input.id||row.company_id!==source.actor.companyId||source.actor.branchId!==null&&row.branch_id!==source.actor.branchId)throw changed();
  const review=['pass-qc','fail-qc','scrap'].includes(input.action);
  if(!source.canWrite||review&&!source.canReview||source.purchase?.[replay?'history':input.action==='reserve'?'newWork':'settleExisting']===false)throw Error('Current Manufacturing authority, purchase and original order history are required. The original request is retained.');
  if(!replay&&(Number(row.version)!==input.expectedVersion||!starts[input.action].includes(String(row.status))))throw Error('The order changed. Reopen the current native order before sending a new change.');
  assertCurrent();busy=true;pending=attempt;const alreadyUncertain=attempt.uncertain;
  try{
   const response=await send(`/api/v1${source.base}/production`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},attempt.completion(),'caller');
   const original:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=object(original)&&object(original.error)?original.error:{};if(!alreadyUncertain&&firstRollback(input,response.status,error))pending=null;throw Error(typeof error.message==='string'?error.message:'Recover the original Manufacturing change before editing its request.');}
   if(!object(original)||!validReceipt(original.data,input,attempt.key,attempt.journal))throw Error('The native Manufacturing acknowledgement is incomplete. Retry the original change.');
   const data=original.data;let consumed=false;
   return{data,assertCurrent,completion:attempt.completion,unchanged:()=>{const completion=attempt.completion();return !!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&completion.form.getAttribute('data-manufacturing-order-draft')===attempt.formSource;},acknowledge:()=>{assertCurrent();if(consumed||pending!==attempt)return false;consumed=true;pending=null;return true;}};
  }catch(error){if(pending===attempt)attempt.uncertain=true;throw error;}finally{busy=false;}
 }
 return{
  bind:(source:()=>ManufacturingOrderCurrent|null)=>{read=source;},captureCurrent:()=>capture(),captureOwner:()=>capture(true),
  send:(raw:unknown,completion:WorkCommandCompletion)=>{if(pending||busy)return Promise.reject(Error('Recover the original Manufacturing change before sending newer fields. Your draft is retained.'));try{const source=read();if(!source)throw changed();const input=request(raw),body=JSON.stringify(input),expectedJournal=journal(source,input);return execute({owner:manufacturingOrderOwner(source),href:window.location.href,work:work(source),body,key:newKey(),journal:expectedJournal,formSource:completion.form.getAttribute('data-manufacturing-order-draft'),completion:retainWorkCommandCompletion(completion),uncertain:false},false);}catch(error){return Promise.reject(error);}},
  retry:()=>pending?execute(pending,true):Promise.reject(Error('No original Manufacturing change needs recovery.')),
  hasPending:()=>!!pending,busy:()=>busy,pendingForCurrent:()=>{const source=read();return !!pending&&!!source&&manufacturingOrderOwner(source)===pending.owner?request(JSON.parse(pending.body)):null;},
 };
}
