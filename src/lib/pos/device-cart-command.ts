import {captureWorkCommandContext} from '@/lib/work_projection/command-context';
import {posReceiptHref} from '@/lib/pos/receipt-route';
import {workFetch} from '@/lib/work_projection/work-fetch';
import type {PosWorkspace} from '@/server/modules/pos/public/pos';

export type PosDeviceDraftReference={storageKey:string;key:string};
export type PosDeviceDraft={key:string;productId:string;name:string;quantityMilli:number;modifierIds:string[];note:string;discountMinor:number;expectedCatalogVersion:number;expectedPriceMinor:number;saleId:string|null;saleVersion:number|null;submission?:PosDeviceSubmission;imageDigest?:string};
type Input={id:string;expectedVersion:number;productId:string;quantityMilli:number;modifierIds:string[];note:string;discountMinor:number;expectedCatalogVersion:number;expectedPriceMinor:number};
type Owner={api:string;scope:string;origin:string;userId:string;companyId:string;branchId:string|null;registerId:string;assignmentId:string};
export type PosDeviceSource=Omit<Owner,'assignmentId'|'origin'>&{canSell:boolean;selection:string};
type PosDeviceSubmission={version:1;owner:Owner;body:string;key:string;draftImage:string;uncertain:boolean;applied:boolean};
export type PosDevicePending={op:'sale.add';input:Input;select:true;fingerprint:string;key:string;deviceDraft:PosDeviceDraftReference;body:string;submission:PosDeviceSubmission;released?:boolean};
type StoragePort=Pick<Storage,'getItem'|'setItem'>;
type FetchPort=(input:RequestInfo|URL,init?:RequestInit,completion?:undefined,acknowledgement?:'caller')=>Promise<Response>;
type Acknowledgement={reference:PosDeviceDraftReference;image:string};
type Retirement={key:string;imageDigest:string;saleId:string;version:number};
const digest=async(value:string)=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(value))),byte=>byte.toString(16).padStart(2,'0')).join('');
function retirement(storage:StoragePort,storageKey:string,key:string):Retirement|null{
 const value:unknown=JSON.parse(storage.getItem(storageKey+':completed:'+key)??'null');if(value===null)return null;
 if(!row(value)||value.key!==key||typeof value.imageDigest!=='string'||!/^[a-f0-9]{64}$/.test(value.imageDigest)||!text(value.saleId)||!integer(value.version,1))throw Error('Keep the saved device completion before sending more drafts.');
 return {key,imageDigest:value.imageDigest,saleId:value.saleId,version:value.version};
}
function normalized(drafts:PosDeviceDraft[],storage:StoragePort,storageKey:string){return drafts.flatMap(draft=>{const done=retirement(storage,storageKey,draft.key);if(!done)return [draft];if(!draft.imageDigest)throw Error('An older cart copy needs its original completion. Keep the saved cart before retrying.');return draft.imageDigest===done.imageDigest?[]:[{...draft,key:'device-'+draft.imageDigest,submission:undefined}];});}
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const integer=(value:unknown,min=0):value is number=>Number.isSafeInteger(value)&&Number(value)>=min;
const changed=()=>Object.assign(Error('Return to the original account, Company, work role and register before retrying this device draft.'),{code:'WORK_CONTEXT_CHANGED'});
const same=(a:Owner,b:Owner)=>JSON.stringify(a)===JSON.stringify(b);
export const posDeviceDraftStorageKey=(source:Pick<Owner,'userId'|'companyId'|'branchId'|'registerId'>)=>`erpx-pos-draft:${source.userId}:${source.companyId}:${source.branchId??'company'}:${source.registerId}`;
export function posDeviceDraftImage(draft:PosDeviceDraft){return JSON.stringify({key:draft.key,productId:draft.productId,name:draft.name,quantityMilli:draft.quantityMilli,modifierIds:draft.modifierIds,note:draft.note,discountMinor:draft.discountMinor,expectedCatalogVersion:draft.expectedCatalogVersion,expectedPriceMinor:draft.expectedPriceMinor,saleId:draft.saleId,saleVersion:draft.saleVersion});}
function validDraft(d:unknown):d is PosDeviceDraft{return row(d)&&text(d.key)&&text(d.productId)&&typeof d.name==='string'&&integer(d.quantityMilli,1)&&integer(d.expectedCatalogVersion,1)&&integer(d.expectedPriceMinor)&&integer(d.discountMinor)&&typeof d.note==='string'&&Array.isArray(d.modifierIds)&&d.modifierIds.every(text)&&(d.saleId===null||text(d.saleId))&&(d.saleVersion===null||integer(d.saleVersion,1));}
function input(value:unknown):value is Input{return row(value)&&Object.keys(value).sort().join(',')==='discountMinor,expectedCatalogVersion,expectedPriceMinor,expectedVersion,id,modifierIds,note,productId,quantityMilli'&&text(value.id)&&text(value.productId)&&integer(value.expectedVersion,1)&&integer(value.quantityMilli,1)&&integer(value.discountMinor)&&integer(value.expectedCatalogVersion,1)&&integer(value.expectedPriceMinor)&&typeof value.note==='string'&&value.note.length<=500&&Array.isArray(value.modifierIds)&&value.modifierIds.length<=30&&value.modifierIds.every(text);}
function submission(value:unknown):value is PosDeviceSubmission{
 if(!row(value)||value.version!==1||!row(value.owner)||!text(value.body)||!text(value.draftImage)||!text(value.key)||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(value.key)||typeof value.uncertain!=='boolean'||typeof value.applied!=='boolean')return false;
 const owner=value.owner;if(!['api','scope','origin','userId','companyId','registerId','assignmentId'].every(field=>text(owner[field]))||!(owner.branchId===null||text(owner.branchId)))return false;
 try{return input(JSON.parse(value.body));}catch{return false;}
}
export function readPosDeviceDrafts(storage:StoragePort,key:string):PosDeviceDraft[]{
 const value:unknown=JSON.parse(storage.getItem(key)??'[]');if(!Array.isArray(value))throw Error('The device cart could not be read. Keep the saved cart and retry.');
 for(const d of value)if(row(d)&&d.submission!==undefined&&(!submission(d.submission)||!text(d.key)||!text(d.productId)||typeof d.name!=='string'||!integer(d.quantityMilli,1)||!integer(d.expectedCatalogVersion,1)||!integer(d.expectedPriceMinor)||!integer(d.discountMinor)||typeof d.note!=='string'||!Array.isArray(d.modifierIds)||!d.modifierIds.every(text)||(d.saleId!==null&&!text(d.saleId))||(d.saleVersion!==null&&!integer(d.saleVersion,1))))throw Error('Keep the original device submission; its saved cart row is incomplete.');
 const drafts=value.filter((d):d is PosDeviceDraft=>row(d)&&text(d.key)&&text(d.productId)&&typeof d.name==='string'&&integer(d.quantityMilli,1)&&integer(d.expectedCatalogVersion,1)&&integer(d.expectedPriceMinor)&&integer(d.discountMinor)&&typeof d.note==='string'&&Array.isArray(d.modifierIds)&&d.modifierIds.every(text)&&(d.saleId===null||text(d.saleId))&&(d.saleVersion===null||integer(d.saleVersion,1))).map(d=>{if(d.submission!==undefined&&!submission(d.submission))throw Error('The original device submission could not be read. Keep its saved data; do not send a new add.');return d;});
 return normalized(drafts,storage,key);
}
/** A stale mounted cart must not overwrite a submitted first body/key. */
export async function writePosDeviceDrafts(storage:StoragePort,key:string,drafts:PosDeviceDraft[],assertCurrent:()=>void=()=>{}){
 assertCurrent();const before=readPosDeviceDrafts(storage,key);
 const stamped=await Promise.all(drafts.map(async draft=>({...draft,imageDigest:await digest(posDeviceDraftImage(draft))})));
 assertCurrent();
 // Re-read both records after hashing: no stale render may undo a completion
 // published while its digest was pending. Readers repeat this normalization.
 const retained=readPosDeviceDrafts(storage,key);
 // Another tab may have saved newer unsent fields while these digests awaited.
 // Submission metadata can advance independently; its original body is merged below.
 if(JSON.stringify(before.map(posDeviceDraftImage))!==JSON.stringify(retained.map(posDeviceDraftImage)))throw Error('The device cart changed while saving. Keep its newer saved drafts.');
 const next=normalized(stamped,storage,key).map(draft=>{const original=retained.find(item=>item.key===draft.key)?.submission;return original?{...draft,submission:original}:draft;});
 for(const original of retained)if(original.submission&&!next.some(draft=>draft.key===original.key))next.push(original);
 assertCurrent();storage.setItem(key,JSON.stringify(next));return next;
}
export function retirePosDeviceDraft(drafts:PosDeviceDraft[],ack:Acknowledgement):PosDeviceDraft[]{return drafts.flatMap(draft=>draft.key!==ack.reference.key?[draft]:posDeviceDraftImage(draft)===ack.image?[]:[{...draft,submission:undefined}]);}
export function subscribePosDeviceDrafts(storageKey:string,receive:(ack:Acknowledgement)=>void){
 const listener=(event:Event)=>{const ack=(event as CustomEvent<Acknowledgement>).detail;if(ack?.reference.storageKey===storageKey)receive(ack);};window.addEventListener('erpx:pos-device-draft-saved',listener);return()=>window.removeEventListener('erpx:pos-device-draft-saved',listener);
}
export function subscribePosDevicePending(storageKey:string,receive:(draft:PosDeviceDraft)=>void){const listener=(event:Event)=>{const detail=(event as CustomEvent<{storageKey:string;draft:PosDeviceDraft}>).detail;if(detail?.storageKey===storageKey)receive(detail.draft);};window.addEventListener('erpx:pos-device-draft-pending',listener);return()=>window.removeEventListener('erpx:pos-device-draft-pending',listener);}
function pending(draft:PosDeviceDraft,storageKey:string):PosDevicePending|null{if(!draft.submission)return null;const captured=draft.submission;if(!submission(captured)||captured.key!==draft.key)throw Error('Keep the original saved device submission before adding more items.');const parsed:unknown=JSON.parse(captured.body);if(!input(parsed))throw Error('The original device add is incomplete.');return {op:'sale.add',input:parsed,select:true,fingerprint:'sale.add'+captured.body,key:captured.key,deviceDraft:{storageKey,key:draft.key},body:captured.body,submission:captured};}
function owner(source:PosDeviceSource|null):Owner{
 const boundary=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!source||!source.canSell||!boundary||!boundary.isConnected||boundary.dataset.workStale==='true'||boundary.dataset.workCompany!==source.companyId||!boundary.dataset.workAssignment||boundary.dataset.workUser&&boundary.dataset.workUser!==source.userId)throw changed();
 return {api:source.api,scope:source.scope,origin:window.location.origin,userId:source.userId,companyId:source.companyId,branchId:source.branchId,registerId:source.registerId,assignmentId:boundary.dataset.workAssignment};
}
function validView(value:unknown,source:Owner,request:Input,version:number):value is PosWorkspace{
 if(!row(value)||value.companyId!==source.companyId||value.userId!==source.userId||value.branchId!==source.branchId||!row(value.permissions)||value.permissions['pos.sell']!==true||!row(value.sale)||value.sale.id!==request.id||value.sale.register_id!==source.registerId||!integer(value.sale.version,version))return false;
 return ['registers','products','warehouses','shifts','tables','sales','lines','payments','transferEvidence','roomChargeStays','kitchen','refunds','cashMovements','stock','jobs','reports','paymentMix','bankTransactions','customers','branches','dayCloses','accounts','charts','bankAccounts','settlements'].every(field=>Array.isArray(value[field]));
}
/** Device-only adds share the real native key/body across global Retry and reload.
 * No Work revision/session is persisted: native replay rechecks the same current
 * account/assignment, and each invocation pins its freshly rendered authority. */
export function createPosDeviceDraftCommands(current:()=>PosDeviceSource|null,send:FetchPort=workFetch,storage:()=>StoragePort=()=>localStorage){
 const saved=(reference:PosDeviceDraftReference)=>readPosDeviceDrafts(storage(),reference.storageKey).find(draft=>draft.key===reference.key);
 function persist(captured:PosDevicePending){const drafts=readPosDeviceDrafts(storage(),captured.deviceDraft.storageKey),draft=drafts.find(row=>row.key===captured.deviceDraft.key);if(!draft){if(retirement(storage(),captured.deviceDraft.storageKey,captured.key))return;throw Error('The original device draft is no longer in this register cart.');}draft.submission=captured.submission;storage().setItem(captured.deviceDraft.storageKey,JSON.stringify(drafts));}
 function prepare(value:unknown,reference:PosDeviceDraftReference):PosDevicePending{
  const source=owner(current());if(reference.storageKey!==posDeviceDraftStorageKey(source))throw changed();
  const draft=saved(reference);if(!draft)throw Error('Keep the original device draft before sending it.');
  const previous=pending(draft,reference.storageKey);if(previous){if(!same(previous.submission.owner,source))throw changed();if(JSON.stringify(value)!==previous.body)throw Error('Recover the original device add before sending newer edits.');return previous;}
  if(!input(value)||value.productId!==draft.productId||value.quantityMilli!==draft.quantityMilli||value.note!==draft.note||value.discountMinor!==draft.discountMinor||value.expectedPriceMinor!==draft.expectedPriceMinor||value.expectedCatalogVersion!==draft.expectedCatalogVersion||JSON.stringify(value.modifierIds)!==JSON.stringify(draft.modifierIds))throw Error('Review the saved device draft before submitting it.');
  const captured:PosDeviceSubmission={version:1,owner:source,body:JSON.stringify(value),key:draft.key,draftImage:posDeviceDraftImage(draft),uncertain:false,applied:false};
  if(!submission(captured))throw Error('The device draft needs its original command identity.');draft.submission=captured;storage().setItem(reference.storageKey,JSON.stringify(readPosDeviceDrafts(storage(),reference.storageKey).map(item=>item.key===draft.key?draft:item)));
  window.dispatchEvent(new CustomEvent('erpx:pos-device-draft-pending',{detail:{storageKey:reference.storageKey,draft:{...draft,submission:{...captured,owner:{...source}}}}}));
  const prepared=pending(draft,reference.storageKey);if(!prepared)throw Error('The original device submission could not be saved.');return prepared;
 }
 function outstanding(){const source=current();if(!source)return null;const key=posDeviceDraftStorageKey(source);for(const draft of readPosDeviceDrafts(storage(),key)){const captured=pending(draft,key);if(captured)return captured;}return null;}
 function restore(value:unknown):PosDevicePending|null{
  if(!row(value)||value.op!=='sale.add'||!submission(value.submission)||!row(value.deviceDraft)||!text(value.deviceDraft.storageKey)||value.deviceDraft.key!==value.key||value.key!==value.submission.key||value.body!==value.submission.body)return null;
  const parsed:unknown=JSON.parse(value.submission.body);if(!input(parsed)||JSON.stringify(value.input)!==value.submission.body)return null;const image:unknown=JSON.parse(value.submission.draftImage);if(!validDraft(image)||image.key!==value.submission.key||(['productId','quantityMilli','note','discountMinor','expectedCatalogVersion','expectedPriceMinor'] as const).some(field=>parsed[field]!==image[field])||JSON.stringify(parsed.modifierIds)!==JSON.stringify(image.modifierIds))return null;
  return {op:'sale.add',input:parsed,select:true,fingerprint:'sale.add'+value.submission.body,key:value.submission.key,deviceDraft:{storageKey:value.deviceDraft.storageKey,key:value.submission.key},body:value.submission.body,submission:value.submission};
 }
 async function run(captured:PosDevicePending){
  const source=owner(current());if(!same(source,captured.submission.owner)||captured.deviceDraft.storageKey!==posDeviceDraftStorageKey(source))throw changed();
  if(!restore(captured))throw Error('Keep the complete original device submission before retrying.');
  const selected=current()!.selection,path=window.location.pathname,search=window.location.search,receiptURL=new URL(posReceiptHref(window.location.href,captured.input.id),window.location.origin),receiptSaleURL=new URL(receiptURL),location=document.querySelector<HTMLElement>('[data-front-app]')?.dataset.frontLocation,assertWork=captureWorkCommandContext();
  receiptSaleURL.searchParams.set('tab','sale');
  const assertCurrent=()=>{assertWork();const originalRoute=window.location.pathname===path&&window.location.search===search,ownReceiptRoute=window.location.pathname===receiptURL.pathname&&(window.location.search===receiptURL.search||window.location.search===receiptSaleURL.search);if(!same(source,owner(current()))||current()?.selection!==selected||!originalRoute&&!ownReceiptRoute||document.querySelector<HTMLElement>('[data-front-app]')?.dataset.frontLocation!==location)throw changed();};
  assertCurrent();let original=saved(captured.deviceDraft)?.submission;
  if(!original&&!retirement(storage(),captured.deviceDraft.storageKey,captured.key)){
   const recovered:unknown=JSON.parse(captured.submission.draftImage);if(!validDraft(recovered)||recovered.key!==captured.key)throw Error('Keep the original saved device cart before retrying.');
   const rows=readPosDeviceDrafts(storage(),captured.deviceDraft.storageKey);rows.push({...recovered,submission:captured.submission});storage().setItem(captured.deviceDraft.storageKey,JSON.stringify(rows));original=captured.submission;
  }
  if(original&&JSON.stringify(original)!==JSON.stringify(captured.submission))throw Error('Reload the original saved device add before retrying.');
  const previousUncertain=captured.submission.uncertain,request=captured.input;assertCurrent();captured.submission.uncertain=true;persist(captured);
  try{
   const response=await send(`${source.api}/sale/add`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},undefined,'caller'),body:unknown=await response.json();assertCurrent();
   if(!response.ok){const failure=row(body)&&row(body.error)?body.error:null;if(!previousUncertain&&!captured.submission.applied&&((response.status===409&&['POS_CATALOG_CHANGED','POS_VERSION_CONFLICT','POS_SALE_LOCKED'].includes(String(failure?.code)))||(response.status===422&&['POS_PRODUCT_INVALID','POS_MODIFIER_INVALID','POS_RESTAURANT_REQUIRED'].includes(String(failure?.code))))){
    captured.released=true;const drafts=readPosDeviceDrafts(storage(),captured.deviceDraft.storageKey);storage().setItem(captured.deviceDraft.storageKey,JSON.stringify(drafts.map(draft=>draft.key===captured.deviceDraft.key?{...draft,submission:undefined}:draft)));const rejected=drafts.find(draft=>draft.key===captured.deviceDraft.key);if(rejected)window.dispatchEvent(new CustomEvent('erpx:pos-device-draft-pending',{detail:{storageKey:captured.deviceDraft.storageKey,draft:{...rejected,submission:undefined}}}));
   }throw Error(typeof failure?.message==='string'?failure.message:'The original device add was not confirmed.');}
   if(!row(body)||!row(body.data)||body.data.id!==request.id||body.data.version!==request.expectedVersion+1)throw Error('The original POS add receipt is incomplete. Retry the saved device add.');
   const receipt=body.data;captured.submission.applied=true;persist(captured);
   const responseView=await send(`${source.api}?${new URLSearchParams({sale:request.id,tab:'sale'})}`,{cache:'no-store'}),refreshed:unknown=await responseView.json();assertCurrent();
   if(!responseView.ok||!row(refreshed)||!validView(refreshed.data,source,request,Number(receipt.version)))throw Error('The add is saved. Retry its original command to load the current bill before removing the device draft.');
   const view=refreshed.data;let acknowledged=false;
   return {receipt,view,assertCurrent,async acknowledge(){assertCurrent();if(acknowledged)return false;
    const imageDigest=await digest(captured.submission.draftImage);assertCurrent();
    const retained=saved(captured.deviceDraft)?.submission,already=retirement(storage(),captured.deviceDraft.storageKey,captured.key);if(!retained&&already&&already.imageDigest===imageDigest&&already.saleId===request.id&&already.version===receipt.version){window.dispatchEvent(new CustomEvent('erpx:pos-device-draft-saved',{detail:{reference:captured.deviceDraft,image:captured.submission.draftImage}}));assertCurrent();acknowledged=true;return {newer:false};}if(!retained||retained.key!==captured.key||retained.body!==captured.body||!same(retained.owner,source))throw Error('The original device completion changed. Keep the pending command.');
    const ack:Acknowledgement={reference:captured.deviceDraft,image:captured.submission.draftImage},drafts=readPosDeviceDrafts(storage(),captured.deviceDraft.storageKey),newer=drafts.some(draft=>draft.key===captured.deviceDraft.key&&posDeviceDraftImage(draft)!==ack.image);
    const done:Retirement={key:captured.key,imageDigest,saleId:request.id,version:Number(receipt.version)},prior=retirement(storage(),captured.deviceDraft.storageKey,captured.key);if(prior&&JSON.stringify(prior)!==JSON.stringify(done))throw Error('Keep the original saved device completion.');
    if(!prior)storage().setItem(captured.deviceDraft.storageKey+':completed:'+captured.key,JSON.stringify(done));
    // Journal first; readers filter a stale physical row even if another tab
    // writes between this queue update and the mounted completion notification.
    storage().setItem(captured.deviceDraft.storageKey,JSON.stringify(retirePosDeviceDraft(drafts,ack)));window.dispatchEvent(new CustomEvent('erpx:pos-device-draft-saved',{detail:ack}));assertCurrent();acknowledged=true;return {newer};}};
  }catch(error){const draft=saved(captured.deviceDraft);if(draft?.submission){captured.submission.uncertain=true;persist(captured);}throw error;}
 }
 return {prepare,pending:outstanding,restore,run,isPending:(captured:PosDevicePending)=>captured.released!==true};
}
