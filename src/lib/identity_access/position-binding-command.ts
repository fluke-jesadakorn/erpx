import {workFetch,type WorkCommandCompletion} from '@/lib/work_projection/work-fetch';
import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {retainWorkCommandCompletion} from '@/lib/work_projection/command-completion';
import {announceWorkChange} from '@/lib/work_projection/work-refresh';
import {packageDefinition,type AccessPackageDefinition,type PeopleLifecycleData} from '@/lib/people_payroll/people-lifecycle';

export type PositionBindingInput={positionId:string;expectedVersion:number;packageId:string;revision:number};
export type PositionBindingSource={base:string;source:string;draft:number;data:PeopleLifecycleData;ownerSnapshot:PeopleLifecycleData;selection:Pick<PositionBindingInput,'positionId'|'packageId'|'revision'>|null};
export type PositionBindingPreview={input:PositionBindingInput;token:string;people:string[];definition:AccessPackageDefinition;source:string;draft:number};
type Kind='preview'|'bind';
type Attempt={kind:Kind;owner:string;workOwner:string;input:PositionBindingInput;body:string;key:string;source:string;draft:number;uncertain:boolean;receipt?:{id:string;version:number};acknowledgedSnapshot?:WeakRef<PeopleLifecycleData>;completion:ReturnType<typeof retainWorkCommandCompletion>;hadCompletion:boolean};
export type PositionBindingPending={kind:Kind;positionId:string;packageId:string;acknowledged:boolean;running:boolean};
/** View-owned only: the caller retains unknown requests when its drawer unmounts. */
export function createPositionBindingCommandStore(){
 let attempt:Attempt|null=null,running=false,pending:PositionBindingPending|null=null;
 const listeners=new Set<()=>void>();
 function publish(){pending=attempt?{kind:attempt.kind,positionId:attempt.input.positionId,packageId:attempt.input.packageId,acknowledged:!!attempt.receipt,running}:null;for(const listener of listeners)listener();}
 return {read:()=>attempt,save:(value:Attempt|null)=>{attempt=value;publish();},busy:()=>running,setBusy:(value:boolean)=>{running=value;publish();},pending:()=>pending,hasPending:()=>attempt!==null,subscribe:(listener:()=>void)=>{listeners.add(listener);return()=>{listeners.delete(listener);};}};
}
export type PositionBindingCommandStore=ReturnType<typeof createPositionBindingCommandStore>;
export type PositionBindingOutcome={kind:'bind-awaiting-source';receipt:{id:string;version:number}}|{kind:'preview';preview:PositionBindingPreview;unchanged:boolean}|{kind:'bind';receipt:{id:string;version:number};data:PeopleLifecycleData;unchanged:boolean;changedSinceAssignment:boolean};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const positive=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>0;
export const positionBindingCanPrepare=(data:PeopleLifecycleData)=>data.canPrepare&&(data.purchases?.identity.newWork??true);
export const positionBindingCanPreview=(data:PeopleLifecycleData)=>data.canPrepare&&(data.purchases?.identity.history??true);
function workOwner(){const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workCompany,node.dataset.workAssignment]):'server';}
function changed(){return Object.assign(Error('Your position source, account or work role changed. Review the current owner before recovering the original request; your newer draft is retained.'),{code:'WORK_CONTEXT_CHANGED'});}
function completionUnchanged(completion?:WorkCommandCompletion){return !completion||completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion;}
const nullableString=(value:unknown)=>value===null||typeof value==='string';
const strings=(value:unknown)=>Array.isArray(value)&&value.every(item=>typeof item==='string');
const integer=(value:unknown)=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
const rows=(value:unknown,check:(row:Record<string,unknown>)=>boolean)=>Array.isArray(value)&&value.every(row=>object(row)&&check(row));
function snapshot(value:unknown):value is PeopleLifecycleData{
 if(!object(value)||typeof value.canPrepare!=='boolean'||typeof value.canManage!=='boolean'||typeof value.canReview!=='boolean'||!object(value.settings))return false;
 const settings=value.settings;
 if(!integer(settings.version)||!nullableString(settings.itAssigneeId)||!strings(settings.onboardingTasks)||!strings(settings.offboardingTasks))return false;
 if(value.purchases!==undefined&&(!object(value.purchases)||!['directory','lifecycle','identity'].every(kind=>{const access=value.purchases&&object(value.purchases)?value.purchases[kind]:null;return object(access)&&typeof access.newWork==='boolean'&&typeof access.settleExisting==='boolean'&&typeof access.history==='boolean'&&typeof access.revision==='string';})))return false;
 return rows(value.positions,row=>typeof row.id==='string'&&positive(row.version)&&typeof row.title==='string'&&typeof row.requirements==='string'&&nullableString(row.package_id)&&(row.package_revision===null||integer(row.package_revision))&&nullableString(row.branch_id)&&nullableString(row.department_id)&&typeof row.level_id==='string'&&integer(row.active)&&object(row.gpo)&&typeof row.gpo.mapped==='boolean'&&[null,'pending','directory_applied','blocked'].includes(row.gpo.directoryStatus as string|null))
  &&rows(value.packages,row=>typeof row.id==='string'&&integer(row.version)&&integer(row.published_revision)&&object(row.definition)&&nullableString(row.definition.connectionId)&&Array.isArray(row.definition.directoryGroupIds)&&Array.isArray(row.definition.windowsPolicies)&&packageDefinition.safeParse(row.definition).success)
  &&rows(value.roster,row=>typeof row.id==='string'&&typeof row.engagementId==='string'&&typeof row.name==='string'&&typeof row.status==='string'&&nullableString(row.positionId)&&nullableString(row.managerPersonId)&&['none','awaiting_identity','queued','directory_applied','blocked'].includes(String(row.gpoDelivery))&&(row.itStatus===undefined||['waiting','working','ready','problem'].includes(String(row.itStatus))))
  &&rows(value.nodes,row=>typeof row.id==='string'&&typeof row.name==='string'&&typeof row.kind==='string')
  &&rows(value.managers,row=>typeof row.id==='string'&&typeof row.name==='string')
  &&rows(value.tasks,row=>typeof row.id==='string'&&nullableString(row.position_id)&&nullableString(row.case_id)&&typeof row.title==='string'&&typeof row.status==='string'&&nullableString(row.assignee_id)&&positive(row.version)&&typeof row.kind==='string'&&typeof row.updated_at==='number')
  &&rows(value.cases,row=>typeof row.id==='string');
}

/** Original position Preview/Bind only. Unknown sends retain first body/key/form.
 * Admin deliberately cannot GET generic People lifecycle. Bind is confirmed only
 * by the caller's fresh native SSR owner snapshot, never an optimistic merge. */
export function createPositionBindingCommands(current:()=>PositionBindingSource|null,send:typeof workFetch=workFetch,store:PositionBindingCommandStore=createPositionBindingCommandStore()){
 function fence(){const value=current(),work=captureWorkCommandContext();if(!value)throw changed();const before={...value};return {before,assertCurrent:()=>{work();const now=current();if(!now||now.base!==before.base||now.source!==before.source||now.ownerSnapshot!==before.ownerSnapshot)throw changed();}};}
 function sameOwner(source:PositionBindingSource,selected:Attempt){if(source.base!==selected.owner||workOwner()!==selected.workOwner||source.data.purchases?.identity.history===false)throw changed();}
 async function json(url:string,init:RequestInit,assertCurrent:()=>void,completion?:WorkCommandCompletion){
  const response=await send(url,init,completion,'caller'),value:unknown=await response.json();assertCurrent();
  if(!response.ok){const error=object(value)&&object(value.error)?value.error:{};throw Object.assign(Error(typeof error.message==='string'?error.message:'Unable to prepare this position.'),{status:response.status,code:error.code});}
  if(!object(value)||!('data'in value))throw Error('The original position acknowledgement is incomplete. Recover the same request.');return value.data;
 }
 function selectedInput(source:PositionBindingSource,input:PositionBindingInput){return source.selection?.positionId===input.positionId&&source.selection.packageId===input.packageId&&source.selection.revision===input.revision;}
 function inputCurrent(source:PositionBindingSource,input:PositionBindingInput){const row=source.data.positions.find(position=>position.id===input.positionId),pkg=source.data.packages.find(item=>item.id===input.packageId);return !!row&&!!row.active&&row.version===input.expectedVersion&&!!pkg&&pkg.published_revision===input.revision&&selectedInput(source,input);}
 async function confirm():Promise<Extract<PositionBindingOutcome,{kind:'bind'}>|null>{
  const selected=store.read();if(!selected?.receipt||store.busy())return null;
  const {before,assertCurrent}=fence();sameOwner(before,selected);
  // A local selection/render is not a server read. The parent passes the exact
  // lifecycle prop received by its SSR owner, stable across local drawer changes.
  if(before.ownerSnapshot===selected.acknowledgedSnapshot?.deref())return null;
  const fresh=before.ownerSnapshot;if(!snapshot(fresh))throw Error('The refreshed position history is incomplete. Recover the acknowledged assignment.');
  const position=fresh.positions.find(row=>row.id===selected.input.positionId);
  if(!position||position.version<selected.receipt.version)return null;
  if(position.version===selected.receipt.version&&(position.package_id!==selected.input.packageId||position.package_revision!==selected.input.revision))throw Error('The refreshed position has not confirmed the original binding. Keep the acknowledged request.');
  assertCurrent();if(store.read()!==selected)throw changed();
  const completion=selected.completion(),unchanged=before.draft===selected.draft&&selectedInput(before,selected.input)&&position.version===selected.receipt.version&&(!selected.hadCompletion||!!completion)&&completionUnchanged(completion);store.save(null);
  if(typeof window!=='undefined'){if(unchanged)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));announceWorkChange();}
  return {kind:'bind',receipt:selected.receipt,data:fresh,unchanged,changedSinceAssignment:position.version!==selected.receipt.version};
 }
 async function execute():Promise<PositionBindingOutcome>{
  if(!store.read()||store.busy())throw Error('The original position request is already running or unavailable.');
  const selected=store.read()!,{before,assertCurrent}=fence();sameOwner(before,selected);
  if(selected.receipt)return await confirm()??{kind:'bind-awaiting-source',receipt:selected.receipt};
  store.setBusy(true);const wasUncertain=selected.uncertain;
  try{
   selected.uncertain=true;
   const value=await json(`/api/v1${selected.owner}/people/lifecycle/position/${selected.kind}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},assertCurrent,selected.completion());
   if(selected.kind==='preview'){
    const definition=object(value)?packageDefinition.safeParse(value.definition):null;
    if(!object(value)||typeof value.token!=='string'||!value.token||!Array.isArray(value.people)||!value.people.every(name=>typeof name==='string')||!definition?.success)throw Error('The position preview is incomplete. Recover the original preview.');
    assertCurrent();if(store.read()!==selected)throw changed();store.save(null);
    return {kind:'preview',preview:{input:{...selected.input},token:value.token,people:value.people,definition:definition.data,source:before.source,draft:selected.draft},unchanged:before.source===selected.source&&before.draft===selected.draft&&inputCurrent(before,selected.input)&&(!selected.hadCompletion||!!selected.completion())&&completionUnchanged(selected.completion())};
   }
   if(!object(value)||typeof value.id!=='string'||!positive(value.version)||value.id!==selected.input.positionId||value.version!==selected.input.expectedVersion+1)throw Error('The position receipt does not match the original assignment. Recover the same request.');
   selected.receipt={id:value.id,version:value.version};selected.acknowledgedSnapshot=new WeakRef(before.ownerSnapshot);selected.uncertain=false;store.save(selected);
   return {kind:'bind-awaiting-source',receipt:selected.receipt};
  }catch(cause){
   const error=cause as {status?:number;code?:string};
   // Bind may have committed before a first typed authority error was returned.
   // Keep its exact request; only readonly Preview can release an ephemeral attempt.
   if(selected.kind==='preview'&&!selected.receipt&&!wasUncertain&&error.status&&error.status<500&&!['WORK_CONTEXT_CHANGED','IDEMPOTENCY_CONFLICT'].includes(error.code??''))store.save(null);
   throw cause;
  }finally{store.setBusy(false);}
 }
 function begin(kind:Kind,input:PositionBindingInput,token:string|undefined,completion?:WorkCommandCompletion){
  if(store.read()||store.busy())return Promise.reject(Error('Recover the original position request first. Your newer draft is retained.'));
  const now=current();if(!now||!(kind==='preview'?positionBindingCanPreview(now.data):positionBindingCanPrepare(now.data))||!inputCurrent(now,input))return Promise.reject(changed());
  const copied={...input},body=JSON.stringify({...copied,...(kind==='bind'?{previewToken:token}:{})});
  store.save({kind,owner:now.base,workOwner:workOwner(),input:copied,body,key:crypto.randomUUID(),source:now.source,draft:now.draft,uncertain:false,completion:retainWorkCommandCompletion(completion),hadCompletion:!!completion});return execute();
 }
 return {
  preview:(input:PositionBindingInput,completion?:WorkCommandCompletion)=>begin('preview',input,undefined,completion),
  bind:(preview:PositionBindingPreview,completion?:WorkCommandCompletion)=>{const now=current();if(!now||now.source!==preview.source||now.draft!==preview.draft||!inputCurrent(now,preview.input)||!preview.token)return Promise.reject(changed());return begin('bind',preview.input,preview.token,completion);},
  retry:execute,confirm,hasPending:store.hasPending,pending:store.pending,busy:store.busy,
 };
}
