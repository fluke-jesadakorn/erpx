import {workFetch, type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';

type Context={user:string;company:string;assignment:string;revision:string;assignmentVersion:string};
type Attempt={endpoint:string;body:string;key:string;context:Context;kind:string;recordId:string;flowVersion:number;recordVersion:number;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>;uncertain:boolean;busy:boolean};
const attempts=new Map<string,Attempt>();
const stamp=(context:Context)=>JSON.stringify([context.user,context.company,context.assignment,context.revision,context.assignmentVersion]);
const scope=(endpoint:string,context:Context)=>JSON.stringify([endpoint,stamp(context)]);
export function renderedFlowContext():Context {
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 const {workUser:user,workCompany:company,workAssignment:assignment,workRevision:revision,workAssignmentVersion:assignmentVersion}=node?.dataset??{};
 if(!node?.isConnected||node.dataset.workStale==='true'||!user||!company||!assignment||!revision||!assignmentVersion)throw Object.assign(Error('Return to the original work context before assigning an owner.'),{code:'WORK_CONTEXT_CHANGED'});
 return {user,company,assignment,revision,assignmentVersion};
}
const validView=(value:unknown,attempt:Attempt,native:boolean):boolean=>{
 if(!value||typeof value!=='object'||Array.isArray(value))return false;
 const view=value as Record<string,unknown>;
 return view.kind===attempt.kind&&view.recordId===attempt.recordId&&Number.isSafeInteger(view.version)&&Number.isSafeInteger(view.recordVersion)&&Number(view.version)>=0&&Number(view.recordVersion)>0&&(native||(view.version===Math.max(1,attempt.flowVersion)+1&&view.recordVersion===attempt.recordVersion))&&Array.isArray(view.steps)&&Array.isArray(view.history)&&typeof view.canManage==='boolean'&&typeof view.canAct==='boolean';
};
/** Retain the exact request after an unknown result, including across remounts. */
export function createFlowAssignmentCommands(send:typeof workFetch=workFetch,context:()=>Context=renderedFlowContext,newKey:()=>string=()=>crypto.randomUUID()){
 const current=()=>context();
 const pending=(endpoint:string)=>attempts.get(scope(endpoint,current()))??null;
 async function run(attempt:Attempt){
  if(stamp(current())!==stamp(attempt.context))throw Object.assign(Error('Return to the original account, Company and role before retrying.'),{code:'WORK_CONTEXT_CHANGED'});
  if(attempt.busy)throw Error('The original owner assignment is still in progress.');
  const assertCurrent=captureWorkCommandContext(),alreadyUncertain=attempt.uncertain;let uncertain=true;attempt.busy=true;
  try{
   const headers=new Headers({'Content-Type':'application/json','Idempotency-Key':attempt.key,'x-erpx-context-company':attempt.context.company,'x-erpx-context-assignment':attempt.context.assignment,'x-erpx-context-revision':attempt.context.revision,'x-erpx-context-assignment-version':attempt.context.assignmentVersion});
   const response=await send(attempt.endpoint,{method:'POST',headers,body:attempt.body},attempt.completion(),'caller');
   const envelope=await response.json().catch(()=>null);
   if(!response.ok){uncertain=alreadyUncertain||response.status>=500||[401,403,409].includes(response.status);throw Error(envelope?.error?.message??'Assignment failed.');}
   const native=response.headers.get('x-erpx-native-count')==='1';
   if(!validView(envelope?.data,attempt,native))throw Error('The owner assignment was not acknowledged. Retry the original request.');
   if(native&&(!envelope?.command||envelope.command.id!==attempt.recordId||envelope.command.version!==Math.max(1,attempt.flowVersion)+1))throw Error('The original owner assignment receipt was not acknowledged. Retry the original request.');
   assertCurrent();if(stamp(current())!==stamp(attempt.context))throw Error('Return to the original work context before retrying.');
   attempts.delete(scope(attempt.endpoint,attempt.context));uncertain=false;
   const completion=attempt.completion(),unchanged=!attempt.hadCompletion||!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion;
   if(completion&&unchanged)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));
   if(unchanged)announceWorkChange();return {unchanged};
  }catch(error){attempt.uncertain=uncertain;if(!uncertain)attempts.delete(scope(attempt.endpoint,attempt.context));throw error;}finally{attempt.busy=false;}
 }
 return {pending,submit(endpoint:string,kind:string,recordId:string,flowVersion:number,input:unknown,completion?:WorkCommandCompletion){
  const captured=current(),key=scope(endpoint,captured),body=JSON.stringify(input),existing=attempts.get(key);
  const recordVersion=typeof input==='object'&&input!==null&&'recordVersion' in input?(input as {recordVersion:unknown}).recordVersion:undefined;if(!Number.isSafeInteger(recordVersion)||Number(recordVersion)<1)throw Error('A current Flow record version is required.');
  if(existing&&(existing.body!==body||existing.kind!==kind||existing.recordId!==recordId))throw Error('Retry the original owner assignment first. Your newer edits are retained.');
  if(!existing&&attempts.size>=64)throw Error('Confirm a previous owner assignment before starting another.');
  const attempt=existing??{endpoint,body,key:newKey(),context:captured,kind,recordId,flowVersion,recordVersion:Number(recordVersion),hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion),uncertain:false,busy:false};attempts.set(key,attempt);return run(attempt);
 },async retry(endpoint:string){const attempt=pending(endpoint);if(!attempt)throw Error('Return to the original work context to retry this owner assignment.');return run(attempt);}};
}
