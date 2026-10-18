import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {talentCommandContext,talentFormUnchanged} from './command';
import type {HireHandoffPayGroupInput,HireHandoffPayGroupResult} from '@/server/modules/people_payroll/public/hire-handoff-pay-group';
import type {hireHandoffWorkspace} from '@/server/modules/people_payroll/public/hire-handoff-workspace';
export type HirePayGroupSource={base:string;companyId:string;handoffId:string;offerVersion:number;candidateVersion:number;currency:string};
type Workspace=Awaited<ReturnType<typeof hireHandoffWorkspace>>;
type View={companyId:string;canConsume:boolean;handoff:Pick<Workspace['handoff'],'id'|'offerVersion'|'candidateVersion'|'currency'>;payGroups:Workspace['payGroups'];receipt:Pick<NonNullable<Workspace['receipt']>,'id'|'person_id'|'engagement_id'|'compensation_id'>|null};
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value),text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const owner=(source:HirePayGroupSource)=>JSON.stringify([source.base,source.companyId,source.handoffId]),stamp=(source:HirePayGroupSource)=>JSON.stringify([owner(source),source.offerVersion,source.candidateVersion,source.currency]);
export function validHirePayGroupView(value:unknown,source:HirePayGroupSource):value is View{
 if(!row(value)||value.companyId!==source.companyId||!row(value.handoff)||value.handoff.id!==source.handoffId||value.handoff.offerVersion!==source.offerVersion||value.handoff.candidateVersion!==source.candidateVersion||value.handoff.currency!==source.currency||typeof value.canConsume!=='boolean'||!Array.isArray(value.payGroups))return false;
 if(!value.payGroups.every(group=>row(group)&&['id','name','currency','frequency'].every(key=>text(group[key]))&&group.currency===source.currency))return false;
 return value.receipt===null||row(value.receipt)&&['id','person_id','engagement_id','compensation_id'].every(key=>text((value.receipt as Record<string,unknown>)[key]));
}
type Attempt={source:HirePayGroupSource;work:string;input:HireHandoffPayGroupInput;body:string;key:string;unknown:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;hadCompletion:boolean};
export function createHirePayGroupCommands(current:()=>HirePayGroupSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original pay-group request.');const source=current();if(!source||owner(source)!==owner(captured.source)||talentCommandContext()!==captured.work)throw Error('Return to the original accepted hire and work role before retrying.');const before={...source},assertWork=captureWorkCommandContext(),wasUnknown=captured.unknown;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||stamp(live)!==stamp(before)||talentCommandContext()!==captured.work)throw Object.assign(Error('The accepted hire context changed. Your original pay-group request and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};assertCurrent();busy=true;
  try{
   const url=`/api/v1${before.base}/people/hire-handoffs/${encodeURIComponent(before.handoffId)}/pay-groups`,response=await send(url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};assertCurrent();
   if(!response.ok){if(!wasUnknown&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED')attempt=null;throw Object.assign(Error(body.error?.message??'The accepted-hire pay group was not created.'),{code:body.error?.code});}
   const receipt=body.data;if(!row(receipt)||!text(receipt.id)||receipt.handoffId!==captured.source.handoffId||receipt.status!=='active'||receipt.currency!==captured.input.input.currency)throw Error('The pay-group acknowledgement is incomplete. Retry the original request.');
   const refreshed=await send(url,{cache:'no-store'}),decoded=await refreshed.json() as {data?:unknown;error?:{message?:string}};assertCurrent();if(!refreshed.ok||!validHirePayGroupView(decoded.data,before)||!decoded.data.receipt&&!decoded.data.payGroups.some(group=>group.id===receipt.id&&group.name===captured.input.input.name&&group.currency===captured.input.input.currency&&group.frequency===captured.input.input.frequency))throw Error(decoded.error?.message??'The pay group was saved. Reload the original request to confirm its current source.');
   captured.unknown=true;return {receipt:{id:receipt.id,handoffId:captured.source.handoffId,status:'active',currency:captured.input.input.currency} satisfies HireHandoffPayGroupResult,view:decoded.data,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&talentFormUnchanged(completion):true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));announceWorkChange();return true;}};
  }catch(error){if(attempt===captured)captured.unknown=true;throw error;}finally{busy=false;}
 }
 return {submit(input:HireHandoffPayGroupInput,completion?:WorkCommandCompletion){const source=current();if(!source||input.expectedOfferVersion!==source.offerVersion||input.expectedCandidateVersion!==source.candidateVersion||input.input.currency!==source.currency)throw Error('Review the actual accepted hire before creating its pay group.');const body=JSON.stringify(input);if(attempt&&(attempt.body!==body||owner(source)!==owner(attempt.source)))throw Error('Resolve the original pay-group request first. Your newer draft is retained.');attempt??={source:{...source},work:talentCommandContext(),input,body,key:crypto.randomUUID(),unknown:false,completion:retainWorkCommandCompletion(completion),hadCompletion:!!completion};return run(attempt);},retry(){if(!attempt)throw Error('There is no original pay-group request to retry.');return run(attempt);},pending:()=>!!attempt};
}
