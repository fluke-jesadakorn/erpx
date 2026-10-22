import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import type {PosWorkspace} from '@erpx/server-pos/public/pos';
import {isFirstPosWorkspaceRollback} from '@erpx/feature-pos/lib/workspace-command-rejections';

export type PosWorkspaceCompletion={captured?:true;form?:HTMLFormElement;formEditVersion?:string|null;fields?:string;notifySaved?:boolean;current?():boolean;acknowledged?(receipt:Record<string,unknown>):void|(()=>void|Promise<unknown>)};
export type PosWorkspaceCommandSource={api:string;scope:string;userId:string;companyId:string;branchId:string|null;registerId:string;selection:string;tab:string;cursor:string;q:string;finance:boolean;frontMode:boolean;recordOnly:boolean;data:PosWorkspace};
export type PosWorkspaceCommandOwner={api:string;scope:string;origin:string;userId:string;companyId:string;branchId:string|null;assignmentId:string;registerId:string;selection:string;frontApp:string|null;frontLocation:string|null};
export type PosWorkspacePending={version:1;kind:'pos-workspace-command';owner:PosWorkspaceCommandOwner;op:string;input:Record<string,unknown>;body:string;select:boolean;fingerprint:string;key:string;uncertain:boolean;links:{saleId?:string};receipt?:Record<string,unknown>};
type Store=Pick<Storage,'getItem'|'setItem'|'removeItem'>;
type Attempt=PosWorkspacePending&{completion:ReturnType<typeof retainWorkCommandCompletion>;hadForm:boolean;onAcknowledged?:PosWorkspaceCompletion['acknowledged'];completionCurrent?:()=>boolean;formImage?:string;notifySaved:boolean;callbackDone:boolean;followUp?:()=>void|Promise<unknown>};
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const int=(value:unknown,min=0):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=min;
function validOAuth(value:unknown,id:string){if(!text(value))return false;try{const url=new URL(value);return url.origin==='https://connect.stripe.com'&&url.pathname==='/oauth/authorize'&&url.searchParams.get('state')===id;}catch{return false;}}
const uuid=(value:unknown)=>typeof value==='string'&&/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i.test(value);
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('Your POS Company, role or source changed. Return to the original owner to recover this request; newer entries are retained.'),{code:'WORK_CONTEXT_CHANGED'});
export const posLegacyCommandMessage='This saved POS command has no verified original Company and role. Its original bytes are preserved. Review the original command and source; it cannot safely be assigned to the currently selected Company or sent with a new key.';
function owner(source:PosWorkspaceCommandSource|null):PosWorkspaceCommandOwner{
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]'),front=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-front-app]');
 if(!source||!node||!node.isConnected||node.dataset.workStale==='true'||!text(node.dataset.workAssignment)||node.dataset.workCompany!==source.companyId||node.dataset.workUser!==source.userId)throw changed();
 return {api:source.api,scope:source.scope,origin:window.location.origin,userId:source.userId,companyId:source.companyId,branchId:source.branchId,assignmentId:node.dataset.workAssignment,registerId:source.registerId,selection:source.selection,frontApp:front?.dataset.frontApp??null,frontLocation:front?.dataset.frontLocation??null};
}
function validOwner(value:unknown):value is PosWorkspaceCommandOwner{return row(value)&&['api','scope','origin','userId','companyId','assignmentId'].every(key=>text(value[key]))&&(value.branchId===null||text(value.branchId))&&typeof value.registerId==='string'&&typeof value.selection==='string'&&(value.frontApp===null||text(value.frontApp))&&(value.frontLocation===null||typeof value.frontLocation==='string');}
export function decodePosWorkspacePending(value:unknown):PosWorkspacePending|null{
 if(!row(value)||value.version!==1||value.kind!=='pos-workspace-command'||!validOwner(value.owner)||!text(value.op)||!row(value.input)||!text(value.body)||typeof value.select!=='boolean'||!text(value.key)||typeof value.uncertain!=='boolean'||!row(value.links)||Object.keys(value.links).some(key=>key!=='saleId')||value.links.saleId!==undefined&&!text(value.links.saleId)||value.fingerprint!==value.op+value.body||value.receipt!==undefined&&!row(value.receipt))return null;
 try{if(!same(JSON.parse(value.body),value.input))return null;}catch{return null;}
 return value as PosWorkspacePending;
}
/** Original native results are intentionally heterogeneous. Never demand a
 * universal expectedVersion+1, and never confuse a generated target with its
 * source sale/shift/product. No result fields are synthesized here. */
export function validPosWorkspaceReceipt(op:string,input:Record<string,unknown>,companyId:string,value:unknown,links:{saleId?:string}={}):value is Record<string,unknown>{
 if(!row(value)||!text(value.id)||op!=='connection.oauth'&&'url' in value)return false;
 const id=value.id,existing=()=>id===input.id,generated=()=>uuid(id),version=()=>int(input.expectedVersion,1)&&value.version===input.expectedVersion+1;
 if(op==='sale.create')return generated();
 if(op==='sale.split')return generated();
 if(op==='sale.merge')return existing()&&version();
 if(op==='sale.complete')return existing()&&value.state==='completed'&&(value.invoiceId===null||text(value.invoiceId));
 if(op==='sale.charge_to_room')return existing()&&value.state==='completed'&&value.tender==='room_charge'&&value.propertyId===input.propertyId&&value.stayId===input.stayId&&text(value.guestAccountEntryId);
 if(op==='sale.approve_discount')return existing();
 if(['sale.add','sale.remove','sale.update_line','sale.customer','sale.discount','sale.move_table','sale.cancel','sale.hold','sale.resume'].includes(op))return existing()&&version();
 if(op==='refund.request')return generated();
 if(op==='refund.approve'||op==='refund.reject')return existing();
 if(op==='payment.create')return generated()&&value.saleId===input.id;
 if(op==='payment.verify_transfer')return existing()&&text(value.saleId)&&(!links.saleId||value.saleId===links.saleId)&&value.state==='succeeded'&&value.bankTransactionId===input.bankTransactionId;
 if(op==='payment.cancel_transfer')return existing()&&text(value.saleId)&&(!links.saleId||value.saleId===links.saleId)&&value.state==='cancelled';
 if(op==='payment.retry'||op==='payment.cancel')return existing();
 const kitchen:Record<string,string>={'kitchen.send':'queued','kitchen.start':'preparing','kitchen.ready':'ready','kitchen.deliver':'delivered','kitchen.cancel':'cancelled'};
 if(op in kitchen)return existing()&&value.state===kitchen[op];
 if(op==='shift.open'||op==='cash.create')return generated();
 if(['shift.close','shift.review','cash.approve'].includes(op))return existing();
 if(op==='day.prepare')return generated()&&value.state==='prepared';
 if(op==='day.confirm')return existing()&&value.state==='confirmed';
 if(op==='room_charge_reversal.request')return generated()&&value.saleId===input.id&&value.state==='requested'&&value.version===1;
 if(op==='room_charge_reversal.approve_pos')return existing()&&text(value.saleId)&&(!links.saleId||value.saleId===links.saleId)&&value.state==='pos_approved'&&version();
 if(op==='room_charge_reversal.reject_pos'||op==='room_charge_reversal.reject_pms')return existing()&&text(value.saleId)&&(!links.saleId||value.saleId===links.saleId)&&value.state==='rejected'&&version();
 if(op==='room_charge_reversal.approve_pms')return existing()&&text(value.saleId)&&(!links.saleId||value.saleId===links.saleId)&&value.state==='room_charge_reversed'&&text(value.reversalEntryId)&&int(value.restoredCostMinor)&&int(value.wastedCostMinor);
 if(op==='settings.bootstrap')return id===companyId&&value.mode===(input.mode??'retail')&&['registerId','registerName','warehouseId','bankAccountId'].every(key=>text(value[key]))&&typeof value.registerReused==='boolean'&&Array.isArray(value.accountsCreated);
 if(op==='settings.save'||op==='accounting.prepare')return id===companyId&&value.state==='prepared';
 if(op==='accounting.approve')return id===companyId&&value.state==='approved'&&version();
 if(op==='settings.operations')return id===companyId&&version();
 if(op==='register.save')return text(input.id)?existing():generated();
 if(op==='table.create'||op==='connection.create'||op==='connection.refresh'||op==='settlement.refresh')return generated();
 if(op==='catalog.save')return id===input.productId;
 if(op==='catalog.image')return id===input.productId&&value.imageId===input.imageId;
 if(op==='customer.create')return generated()&&value.name===String(input.name??'').trim();
 if(op==='connection.oauth')return generated()&&value.livemode===(input.livemode??false)&&validOAuth(value.url,id);
 if(['settlement.map_external','settlement.match','exception.attach_provider','exception.retry'].includes(op))return existing();
 return false;
}
function validView(value:unknown,source:PosWorkspaceCommandSource):value is PosWorkspace{return row(value)&&value.companyId===source.companyId&&value.userId===source.userId&&value.branchId===source.branchId&&row(value.permissions)&&['registers','products','warehouses','shifts','tables','sales','lines','payments','transferEvidence','roomChargeStays','kitchen','refunds','cashMovements','stock','jobs','reports','paymentMix','bankTransactions','customers','branches','dayCloses','accounts','charts','bankAccounts','settlements'].every(key=>Array.isArray(value[key]));}
function formImage(form:HTMLFormElement){return JSON.stringify(Array.from(new FormData(form),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,type:value.type,lastModified:value.lastModified}]));}
function nativeLinks(source:PosWorkspaceCommandSource,op:string,input:Record<string,unknown>):{saleId?:string}{
 if(op==='payment.create')return {saleId:String(input.id)};
 if(op.startsWith('payment.')){const payment=source.data.payments.find(row=>row.id===input.id);if(!payment||!text(payment.sale_id))throw Error('Review the actual POS payment before submitting.');return{saleId:payment.sale_id};}
 if(op==='room_charge_reversal.request')return{saleId:String(input.id)};
 return {};
}
function captureForm(explicit?:PosWorkspaceCompletion):WorkCommandCompletion|undefined{const form=explicit?.captured?explicit.form:explicit?.form??document.activeElement?.closest<HTMLFormElement>('form');return form?{form,formEditVersion:explicit&&'formEditVersion' in explicit?explicit.formEditVersion??null:form.getAttribute('data-erpx-edit-version')}:undefined;}
/** One mounted controller owns this protocol; only native owner identity and
 * original request are durable. Current Work revision is freshly guarded for
 * each invocation, and remains the normal workFetch header source. */
export function createPosWorkspaceCommands(current:()=>PosWorkspaceCommandSource|null,storageKey:string,send:typeof workFetch=workFetch,storage:()=>Store=()=>sessionStorage){
 let attempt:Attempt|null=null,busy=false;
 function stored(){const bytes=storage().getItem(storageKey);if(!bytes)return null;let raw:unknown;try{raw=JSON.parse(bytes);}catch{throw Error(posLegacyCommandMessage);}const value=decodePosWorkspacePending(raw);if(!value)throw Error(posLegacyCommandMessage);return value;}
 function persist(selected:Attempt){const saved=stored();if(saved&&(saved.key!==selected.key||saved.body!==selected.body||!same(saved.owner,selected.owner)||saved.receipt!==undefined&&!same(saved.receipt,selected.receipt)))throw Error('Keep the original saved POS command before sending another.');const {completion:_completion,hadForm:_form,onAcknowledged:_callback,completionCurrent:_current,formImage:_image,notifySaved:_notify,callbackDone:_done,followUp:_follow,...durable}=selected;void _completion;void _form;void _callback;void _current;void _image;void _notify;void _done;void _follow;storage().setItem(storageKey,JSON.stringify(durable));}
 function pending(){const saved=stored();if(!saved)return null;if(attempt&&(saved.key!==attempt.key||saved.body!==attempt.body||!same(saved.owner,attempt.owner)))throw Error('The saved POS request changed. Keep both original records.');return saved;}
 function fence(){const source=current();if(!source)throw changed();const originalOwner=owner(source),assertWork=captureWorkCommandContext(),image=JSON.stringify(source),path=window.location.pathname,search=window.location.search,front=document.querySelector('[data-front-app]');assertWork();return{source,originalOwner,assertOwnerCurrent(createdSale?:string){assertWork();const live=current();if(!live||live.registerId!==source.registerId||live.selection!==source.selection&&live.selection!==createdSale||live.tab!==source.tab||!same({...owner(live),selection:originalOwner.selection},originalOwner)||window.location.pathname!==path||window.location.search!==search)throw changed();},assertCurrent(){assertWork();const live=current();if(!live||JSON.stringify(live)!==image||!same(owner(live),originalOwner)||window.location.pathname!==path||window.location.search!==search||front&&(front.isConnected===false||document.querySelector('[data-front-app]')!==front))throw changed();}};}
 async function run(op:string,input:unknown,select=false,completion?:PosWorkspaceCompletion){
  if(busy)throw Error('Wait for the original POS request.');const {source,originalOwner,assertCurrent,assertOwnerCurrent}=fence();if(!row(input))throw Error('Review the original POS command input.');
  const body=JSON.stringify(input),saved=stored();if(saved&&!attempt)attempt={...saved,uncertain:true,completion:()=>undefined,hadForm:false,notifySaved:false,callbackDone:true};
  if(attempt&&(attempt.op!==op||attempt.body!==body||attempt.select!==select||!same(attempt.owner,originalOwner)))throw Error('Return to the original POS Company and role, then retry its original command. Your newer entries are retained.');
  if(!attempt){const captured=captureForm(completion);attempt={version:1,kind:'pos-workspace-command',owner:originalOwner,op,input:JSON.parse(body),body,select,fingerprint:op+body,key:crypto.randomUUID(),uncertain:false,links:nativeLinks(source,op,input),completion:retainWorkCommandCompletion(captured),hadForm:!!captured,onAcknowledged:completion?.acknowledged,completionCurrent:completion?.current,formImage:captured?(completion?.fields??formImage(captured.form)):undefined,notifySaved:completion?.notifySaved!==false,callbackDone:false};}
  const selected=attempt,wasUncertain=selected.uncertain;assertCurrent();persist(selected);selected.uncertain=true;persist(selected);busy=true;
  try{
   const response=await send(`${source.api}/${selected.op.replace(".",'/')}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':selected.key},body:selected.body},selected.completion(),'caller'),body:unknown=await response.json();assertCurrent();
   if(!response.ok){const error=row(body)&&row(body.error)?body.error:null;if(!wasUncertain&&!selected.receipt&&isFirstPosWorkspaceRollback(selected.op,response.status,error?.code)){const saved=pending();if(!saved||saved.key!==selected.key||saved.body!==selected.body||!same(saved.owner,selected.owner))throw Error('The saved POS request changed.');assertCurrent();storage().removeItem(storageKey);attempt=null;}throw Object.assign(Error(typeof error?.message==='string'?error.message:'The original POS command was not confirmed.'),{status:response.status,code:error?.code});}
   if(!row(body)||!validPosWorkspaceReceipt(selected.op,selected.input,source.companyId,body.data,selected.links))throw Error('The POS acknowledgement does not match the original native command. Retry its saved key.');
   const receipt=body.data;if(selected.receipt&&!same(selected.receipt,receipt))throw Error('The original saved POS receipt changed. Keep its key for review.');selected.receipt=receipt;persist(selected);assertCurrent();
   const sale=selected.select||selected.op==='sale.create'?String(receipt.id):source.selection,query=new URLSearchParams({...(sale?{sale}:{}),...(source.finance?{finance:'1'}:{}),q:source.frontMode&&!source.recordOnly?(new URL(window.location.href).searchParams.get('q')??''):source.q,tab:source.tab,cursor:source.cursor});
   let view:PosWorkspace|undefined;
   if(!text(receipt.url)){const refreshed=await send(`${source.api}?${query}`,{cache:'no-store'}),parsed:unknown=await refreshed.json();assertCurrent();if(!refreshed.ok||!row(parsed)||!validView(parsed.data,source)||sale&&(!row(parsed.data.sale)||parsed.data.sale.id!==sale))throw Error('The command is saved. Retry its original key to confirm the current POS owner before completing.');view=parsed.data;}
   assertCurrent();let consumed=false;return{receipt,view,assertCurrent,acknowledge(){assertCurrent();if(consumed||attempt!==selected)return false;const saved=pending();if(!saved||saved.key!==selected.key||saved.body!==selected.body||!same(saved.owner,selected.owner)||!same(saved.receipt,receipt))throw Error('The saved POS completion changed. Keep the original request.');const captured=selected.completion(),unchanged=(!selected.hadForm||!!captured&&captured.form.isConnected&&captured.form.getAttribute('data-erpx-edit-version')===captured.formEditVersion&&formImage(captured.form)===selected.formImage)&&(selected.completionCurrent?.()??true);if(!selected.callbackDone){const after=unchanged?selected.onAcknowledged?.(receipt):undefined;if(typeof after==='function')selected.followUp=after;selected.callbackDone=true;}assertCurrent();storage().removeItem(storageKey);attempt=null;consumed=true;if(captured&&unchanged&&selected.notifySaved)window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:captured}));announceWorkChange();const follow=selected.followUp;return{newer:!unchanged,afterAcknowledged:follow?()=>{assertOwnerCurrent(selected.op==='sale.create'?String(receipt.id):undefined);return follow();}:undefined};}};
  }finally{busy=false;}
 }
 return{run,pending};
}
