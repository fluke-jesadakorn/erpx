import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {rosterCommandSchema,type RosterCommand,type RosterData} from '@erpx/feature-people_payroll/lib/people-roster';
export type RosterOwnerSource={base:string;data:RosterData;initial:RosterData;view?:string;available?:boolean};
export type RosterOwnerDraftState={ownerId:string;revision:number;dirty:boolean;pending:boolean;busy:boolean};
export function rosterOwnerDraftHeld(state:RosterOwnerDraftState|null|undefined){return !!state&&(state.dirty||state.pending||state.busy);}
export function currentRosterOwnerDraft(previous:RosterOwnerDraftState|null|undefined,next:RosterOwnerDraftState){
 if(previous&&(previous.ownerId!==next.ownerId&&rosterOwnerDraftHeld(previous)||previous.ownerId===next.ownerId&&next.revision<previous.revision))return previous;
 if(previous&&previous.ownerId===next.ownerId&&previous.revision===next.revision&&previous.dirty===next.dirty&&previous.pending===next.pending&&previous.busy===next.busy)return previous;
 return next;
}
export type RosterOwnerReceipt={id:string;version:number;state?:string};
type Native={id:string;version:number;state?:string};
type Attempt={base:string;departmentId:string|null;workOwner:string;key:string;body:string;input:RosterCommand;start:string;native:Native;editor:number;edit:number;uncertain:boolean;hadForm:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
/** Version metadata only, from an explicit current native read of the same
 * selected department and exact retained record; never an unknown-command ACK. */
export function rosterOwnerReadVersion(value:unknown,kind:'templates'|'shifts'|'requests',id:string,departmentId:string|null,weekId?:string){
 if(!object(value)||value.departmentId!==departmentId||!Array.isArray(value[kind]))return null;
 const row:unknown=value[kind].find((candidate:unknown)=>object(candidate)&&candidate.id===id);
 if(!object(row)||typeof row.version!=='number'||!Number.isSafeInteger(row.version)||row.version<=0)return null;
 if(kind==='shifts'){if(row.week_id!==weekId)return null;}
 else if(row.department_id!==departmentId)return null;
 return row.version;
}
function changed(){return Object.assign(Error('Your Roster source, account, Company or role changed. Return to the original owner and recover the submitted request; your newer draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});}
function workOwner(){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]):'server';}
function native(source:RosterOwnerSource,input:RosterCommand,key:string):Native{
 const data=source.data,newId=`roster-${key}`;
 if(input.action==='template.save'){
  const row=input.id?data.templates.find(row=>row.id===input.id&&row.department_id===input.departmentId):undefined;
  if(input.id&&(!row||row.version!==input.expectedVersion))throw Error('Refresh the current template before saving.');
  return {id:row?.id??newId,version:(row?.version??0)+1};
 }
 if(input.action==='week.create')return {id:newId,version:1,state:'draft'};
 if(input.action==='week.publish'){
  const row=data.weeks.find(row=>row.id===input.id);if(!row||row.version!==input.expectedVersion)throw Error('Refresh the current week before publishing.');
  return {id:row.id,version:row.version+1,state:'published'};
 }
 if(input.action==='shift.save'){
  const week=data.weeks.find(row=>row.id===input.weekId),row=input.id?data.shifts.find(row=>row.id===input.id&&row.week_id===input.weekId):undefined;
  if(!week||week.version!==input.expectedVersion||input.id&&!row)throw Error('Refresh the current week and shift before saving.');
  // The request guards the WEEK; the receipt advances the SHIFT's own version.
  return {id:row?.id??newId,version:(row?.version??0)+1};
 }
 if(input.action==='shift.cancel'){
  const row=data.shifts.find(row=>row.id===input.id);if(!row||row.version!==input.expectedVersion)throw Error('Refresh the current shift before cancelling.');
  return {id:row.id,version:row.version+1,state:'cancelled'};
 }
 if(input.action==='request.create')return {id:newId,version:1,state:input.kind==='swap'?'pending_coworker':'pending_manager'};
 const row=data.requests.find(row=>row.id===input.id);if(!row||row.version!==input.expectedVersion)throw Error('Refresh the current request before reviewing.');
 return {id:row.id,version:row.version+1,state:input.decision==='accept'?'pending_manager':input.decision==='approve'?'approved':input.decision==='reject'?'rejected':'withdrawn'};
}
function permitted(source:RosterOwnerSource,selected:Attempt,replay:boolean){
 const data=source.data,input=selected.input;
 if(replay)return data.purchase?.history!==false;
 if(data.purchase?.[input.action==='shift.cancel'||input.action==='request.decide'?'settleExisting':'newWork']===false)return false;
 if(input.action==='request.create')return !!data.selfPersonId;
 if(input.action==='request.decide')return data.requests.some(row=>row.id===input.id&&row.permittedActions.includes(input.decision));
 return data.canManage;
}
// Closed action+code mappings, never a status-wide rollback inference. These
// native errors occur before commit inside the original owned transaction (or
// schema validation before entering it); none is an after-COMMIT Work error.
function firstRollback(input:RosterCommand,status:number,code:unknown){
 if(status===422&&code==='VALIDATION_FAILED')return true;
 const copy=input.action==='week.create'&&!!input.copyWeekId;
 const requestCheck=input.action==='request.create'||input.action==='request.decide'&&(input.decision==='accept'||input.decision==='approve');
 if(status===409){
  if(code==='ROSTER_CHANGED')return ['template.save','week.publish','shift.save','shift.cancel','request.create','request.decide'].includes(input.action);
  if(code==='ROSTER_EXISTS')return input.action==='week.create';
  if(code==='ROSTER_TEMPLATE')return input.action==='shift.save';
  if(code==='ROSTER_EMPTY')return input.action==='week.publish';
  if(code==='ROSTER_STARTED')return copy||input.action==='week.publish'||input.action==='shift.save'||input.action==='shift.cancel'||requestCheck;
  if(code==='ROSTER_CONFLICT'||code==='ROSTER_STAFF')return copy||input.action==='week.publish'||input.action==='shift.save'||requestCheck;
  if(code==='ROSTER_FULL')return input.action==='week.publish'||requestCheck;
  if(code==='ROSTER_COVERAGE')return input.action==='week.publish'||input.action==='shift.save'||input.action==='shift.cancel'||input.action==='request.decide'&&input.decision==='approve';
  if(code==='ROSTER_SCOPE')return copy||requestCheck;
  if(code==='ROSTER_DUPLICATE')return input.action==='request.create';
  if(code==='ROSTER_COWORKER_REQUIRED')return input.action==='request.decide'&&(input.decision==='approve'||input.decision==='reject');
  return false;
 }
 if(status!==422)return false;
 if(input.action==='template.save')return code==='ROSTER_TIME';
 if(input.action==='week.create')return code==='ROSTER_WEEK'||code==='ROSTER_TIMEZONE'||copy&&code==='ROSTER_TIME';
 if(input.action==='shift.save')return ['ROSTER_TIME','ROSTER_DATE','ROSTER_CAPACITY','ROSTER_REASON'].includes(String(code));
 if(input.action==='shift.cancel')return code==='ROSTER_REASON';
 if(input.action==='request.create')return code==='ROSTER_REQUEST';
 return input.action==='request.decide'&&input.decision==='reject'&&code==='ROSTER_REASON';
}
function receipt(native:Native,value:unknown):value is RosterOwnerReceipt{return object(value)&&value.id===native.id&&value.version===native.version&&(native.state===undefined?value.state===undefined:value.state===native.state)&&Object.keys(value).every(key=>['id','version',...(native.state===undefined?[]:['state'])].includes(key));}
export function createRosterOwnerCommands(send:typeof workFetch=workFetch,newKey:()=>string=()=>crypto.randomUUID()){
 let read:()=>RosterOwnerSource|null=()=>null,attempt:Attempt|null=null,busy=false,dirty=false,editor=0,edit=0,revision=0;
 function fence(){const source=read(),work=captureWorkCommandContext();if(!source||source.available===false)throw changed();work();const stamp=JSON.stringify(source);return {source,assertCurrent:()=>{work();const current=read();if(!current||JSON.stringify(current)!==stamp)throw changed();}};}
 async function execute(selected:Attempt,replay:boolean){
  if(busy)throw Error('The original Roster request is still running.');
  const {source,assertCurrent}=fence();
  if(source.base!==selected.base||source.data.departmentId!==selected.departmentId||workOwner()!==selected.workOwner)throw changed();
  if(!permitted(source,selected,replay))throw Error('Current Roster access does not permit this operation. The original request is retained.');
  const wasUncertain=selected.uncertain;attempt=selected;busy=true;dirty=true;selected.uncertain=true;revision++;
  try{
   const response=await send(`/api/v1${selected.base}/people/roster`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),envelope:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=object(envelope)&&object(envelope.error)?envelope.error:{};if(!wasUncertain&&firstRollback(selected.input,response.status,error.code)){assertCurrent();attempt=null;revision++;}throw Error(typeof error.message==='string'?error.message:'Unable to save. Recover the original Roster request.');}
   if(!object(envelope)||!receipt(selected.native,envelope.data))throw Error('The native Roster acknowledgement is incomplete. Recover the original request.');
   assertCurrent();let acknowledged=false;
   const unchanged=()=>{const completion=selected.completion();return selected.editor===editor&&selected.edit===edit&&(!selected.hadForm||!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion);};
   return {data:envelope.data,input:selected.input,start:selected.start,completion:selected.completion,assertCurrent,unchanged,acknowledge:()=>{assertCurrent();if(acknowledged||attempt!==selected)return false;acknowledged=true;attempt=null;dirty=!unchanged();revision++;return true;}};
  }finally{busy=false;revision++;}
 }
 return {bind:(current:()=>RosterOwnerSource|null)=>{read=current;},capture:fence,isMounted:()=>read()!==null,
  open(){if(attempt||busy)throw Error('Recover the original Roster request before replacing this editor.');editor++;edit=0;dirty=false;revision++;return editor;},
  changed(){edit++;dirty=true;revision++;},isDirty:()=>dirty||!!attempt,pending:()=>busy,hasPending:()=>!!attempt,
  status:()=>({revision,dirty:dirty||!!attempt,pending:!!attempt,busy}),
  send(raw:unknown,start:string,completion?:WorkCommandCompletion){
   if(attempt||busy)return Promise.reject(Error('Recover the original Roster request first. Your newer draft is retained.'));
   try{const parsed=rosterCommandSchema.safeParse(raw),source=read();if(!parsed.success)throw Error(parsed.error.issues[0]?.message??'Review the Roster fields.');if(!source)throw changed();const key=newKey(),selected:Attempt={base:source.base,departmentId:source.data.departmentId,workOwner:workOwner(),key,body:JSON.stringify(raw),input:parsed.data,start,native:native(source,parsed.data,key),editor,edit,uncertain:false,hadForm:!!completion,completion:retainWorkCommandCompletion(completion)};return execute(selected,false);}catch(error){return Promise.reject(error);}
  },retry:()=>attempt?execute(attempt,true):Promise.reject(Error('No original Roster request needs recovery.')),
 };
}
