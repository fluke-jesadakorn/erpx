import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {hireConsumptionSchema,type HireConsumptionInput} from '@/server/modules/people_payroll/public/rules/hire-consumption';
import type {hireHandoffWorkspace} from '@/server/modules/people_payroll/public/hire-handoff-workspace';
import {sendTalentHireCommand,validHireReceipt} from './hire-command';
import {talentCommandContext,type TalentAttempt} from './command';

export type HireHandoffData=Omit<Awaited<ReturnType<typeof hireHandoffWorkspace>>,'companyId'>&{companyId?:string};
export type HireHandoffOwner={userId:string;companyId:string};
export type HireConsumptionSource={base:string;owner?:HireHandoffOwner;data:HireHandoffData};
export type HireConsumptionReceipt={id:string;handoffId:string;personId:string;engagementId:string;compensationId:string;status:'consumed';version:1;reconciled:boolean};
type Attempt={request:TalentAttempt;identity:string;completion:ReturnType<typeof retainWorkCommandCompletion>;hadForm:boolean;fields?:string;receipt?:HireConsumptionReceipt};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('The accepted hire or current Work owner changed. Return to the original Company and role to recover the submitted handoff.'),{code:'WORK_CONTEXT_CHANGED'});
const identity=(source:HireConsumptionSource)=>JSON.stringify([source.base,source.owner?.userId,source.owner?.companyId,source.data.handoff.id]);
const fields=(form:HTMLFormElement)=>JSON.stringify(Array.from(new FormData(form),([name,value])=>[name,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]));
/** These exact native errors occur after receipt lookup but before money and
 * commands.commit in people-handoffs.consumeHireHandoff. Never status-wide. */
function firstRollback(status:number,code:unknown){if(typeof code!=='string')return false;return status===422&&code==='HIRE_PAY_GROUP_INVALID'||status===409&&['HIRE_HANDOFF_ALREADY_CONSUMED','HIRE_HANDOFF_SOURCE_CHANGED','HIRE_HANDOFF_PRIVACY_BLOCKED','HIRE_PERSON_CHANGED','HIRE_IDENTITY_RECONCILIATION_REQUIRED'].includes(code);}
export function createHireConsumptionCommands(current:()=>HireConsumptionSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,running=false,recorded:HireConsumptionReceipt|null=null;
 function fence(){
  const source=current(),node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
  if(!source?.owner||!node?.isConnected||node.dataset.workStale==='true'||node.dataset.workUser!==source.owner.userId||node.dataset.workCompany!==source.owner.companyId||!node.dataset.workAssignment||source.data.companyId&&source.data.companyId!==source.owner.companyId)throw changed();
  const assertWork=captureWorkCommandContext(),image=JSON.stringify(source),context=talentCommandContext(),location=window.location.href;
  const assertCurrent=()=>{assertWork();const live=current();if(!live||JSON.stringify(live)!==image||talentCommandContext()!==context||window.location.href!==location)throw changed();};
  assertCurrent();return {source,context,assertCurrent};
 }
 async function run(selected:Attempt){
  if(running)throw Error('Wait for the original hire handoff.');const scope=fence();
  // Workflow fingerprints include authority. Do not rebase a saved key onto a
  // changed Work revision/assignment/session or pretend native replay succeeded.
  if(selected.identity!==identity(scope.source)||selected.request.context!==scope.context)throw changed();
  const wasUncertain=attempt===selected;attempt=selected;running=true;
  try{
   const response=await sendTalentHireCommand(scope.source.base,{...selected.request,completion:selected.completion()},send),body:unknown=await response.json();scope.assertCurrent();
   if(!response.ok){const error=object(body)&&object(body.error)?body.error:null;if(!wasUncertain&&!selected.receipt&&firstRollback(response.status,error?.code)){scope.assertCurrent();attempt=null;}throw Object.assign(Error(typeof error?.message==='string'?error.message:'The original hire result is not confirmed. Retry its original request.'),{status:response.status,code:error?.code});}
   if(!object(body)||!validHireReceipt(selected.request,body.data))throw Error('The HR receipt does not match every original handoff/person/engagement/compensation identifier. Retry the original key.');
   const history=scope.source.data.receipt;if(history&&(history.id!==body.data.id||history.person_id!==body.data.personId||history.engagement_id!==body.data.engagementId||history.compensation_id!==body.data.compensationId))throw Error('The current accepted-hire history contradicts the original native receipt. Keep its key for review.');
   if(selected.receipt&&!same(selected.receipt,body.data))throw Error('The original HR receipt changed. Keep the original request for review.');
   selected.receipt=body.data;const receiptImage=JSON.stringify(body.data);scope.assertCurrent();let consumed=false;
   return {receipt:body.data,assertCurrent:scope.assertCurrent,acknowledge(){scope.assertCurrent();if(consumed||attempt!==selected)return false;const nativeReceipt=selected.receipt;if(JSON.stringify(nativeReceipt)!==receiptImage||!validHireReceipt(selected.request,nativeReceipt))throw Error('The native HR acknowledgement changed before completion. Keep the original key.');const completion=selected.completion(),unchanged=!selected.hadForm||!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion&&fields(completion.form)===selected.fields;recorded=nativeReceipt;attempt=null;consumed=true;if(completion&&unchanged){scope.assertCurrent();completion.form.reset();scope.assertCurrent();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));}scope.assertCurrent();announceWorkChange();return {newer:!unchanged};}};
  }finally{running=false;}
 }
 return {captureCurrent:()=>fence().assertCurrent,pending:()=>attempt!==null,busy:()=>running,
  submit(input:HireConsumptionInput,completion?:WorkCommandCompletion){if(recorded)throw Error('The original accepted hire was already recorded. Review its native receipt before another intake.');const scope=fence(),body=JSON.stringify(input);if(attempt){if(attempt.request.body!==body||attempt.identity!==identity(scope.source)||attempt.request.context!==scope.context)throw Error('Recover the original hire handoff before submitting newer entries.');return run(attempt);}
   hireConsumptionSchema.parse(input);
   const data=scope.source.data;if(!data.canConsume||data.receipt||data.purchase?.settleExisting===false||input.handoffId!==data.handoff.id||input.expectedOfferVersion!==data.handoff.offerVersion||input.expectedCandidateVersion!==data.handoff.candidateVersion)throw Error('Review the current accepted hire before HR intake. Your draft is retained.');
   const group=data.payGroups.find(group=>group.id===input.payGroupId&&group.currency===data.handoff.currency),person=input.reconcilePersonId?data.people.find(person=>person.id===input.reconcilePersonId&&person.version===input.expectedPersonVersion&&person.status==='active'&&!person.merged_into_id):undefined;
   if(!group||input.reconcilePersonId&&!person||data.people.length&&(!person||data.people.some(value=>value.id!==person.id)))throw Error('Choose the actual same-currency pay group and explicitly reconcile the unique active Company identity.');
   const request:TalentAttempt={body,key:crypto.randomUUID(),context:scope.context};return run({request,identity:identity(scope.source),completion:retainWorkCommandCompletion(completion),hadForm:!!completion,fields:completion?fields(completion.form):undefined});
  },retry(){if(!attempt)throw Error('There is no original hire handoff to recover.');return run(attempt);},
 };
}
