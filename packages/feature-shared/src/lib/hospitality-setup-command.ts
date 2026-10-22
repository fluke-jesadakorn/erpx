import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {decimalToMinor} from '@erpx/feature-finance/lib/finance-money';
import {isFirstPmsEditableRejection} from '@erpx/feature-pms/lib/workspace-command-rejections';
import type {HospitalitySetup} from '@erpx/server-pms/public/hospitality-setup';

export type HospitalitySetupOwner={userId:string;companyId:string;branchId:string|null};
export type HospitalitySetupSource={base:string;owner:HospitalitySetupOwner;data:HospitalitySetup;canBootstrapPos:boolean;canReadPos:boolean};
type PmsSetupOperation='property.create'|'property.save'|'room_type.save'|'room.save'|'accounting.bootstrap'|'rate.save';
export type HospitalitySetupPlan=
 |{kind:'pms';operation:PmsSetupOperation;input:Record<string,unknown>;version?:number;after?:string}
 |{kind:'product';input:{sku:string;name:string;price:string;unit:'each';taxBps:0}}
 |{kind:'catalog';productRow:string;input:{kind:'stock'|'recipe';category:string;station:string;barcode:string;recipe:{productId:string;quantityMilli:number}[];modifiers:[];taxInclusive:false;expectedVersion:0}}
 |{kind:'bootstrap';input:{mode:'restaurant';registerName:string;warehouseId:string;registerId?:string;bankAccountId?:string}}
 |{kind:'register';input:{mode:'restaurant';name:string;warehouseId:string}}
 |{kind:'table';input:{registerId:string;name:string;seats:number}};
export type HospitalitySetupCompletion=WorkCommandCompletion&{fields?:string};
export type HospitalitySetupDraft={rowKey:string;label:string;plan:HospitalitySetupPlan};
type BoundOwner={base:string;api:string;origin:string;userId:string;companyId:string;scopeBranchId:string|null;branchId:string;propertyId:string|null;assignmentId:string;revision:string};
export type HospitalitySetupJob=HospitalitySetupDraft&{id:string;key:string;state:'planned'|'uncertain'|'rejected'|'saved'|'cancelled-before-send';sent?:{url:string;body:string};sends:number;receipt?:Record<string,unknown>;confirmation?:'native-current-record'|'native-receipt-history';error?:string};
export type HospitalitySetupQueue={version:1;id:string;owner:BoundOwner;jobs:HospitalitySetupJob[]};
const object=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&!!value;
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const changed=()=>Object.assign(Error('Your hotel setup source or Work context changed. Return to the original Company, branch, hotel and assignment to recover this queue.'),{code:'WORK_CONTEXT_CHANGED'});
const quarantined=()=>Error('An earlier setup queue has no verified original owner, or its bound bytes changed. Its bytes are retained; review original native records before creating more setup work.');
const api=(base:string)=>{if(!/^\/holdings\/[^/?#]+(?:\/branches\/[^/?#]+)?$/.test(base))throw changed();return '/api/v1'+base;};
export const hospitalitySetupQueueKey=(source:HospitalitySetupSource)=>'hospitality-owner-queue:v1:'+JSON.stringify([source.base,source.owner.userId,source.owner.companyId,source.owner.branchId,source.data.branchId]);
export const legacyHospitalitySetupQueueKey=(source:HospitalitySetupSource)=>`hospitality-setup:${source.base}:${source.data.companyId}:${source.data.branchId}:${source.data.propertyId??'new'}`;
function bind(source:HospitalitySetupSource):BoundOwner{
 const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');
 if(!node?.isConnected||node.dataset.workStale==='true'||node.dataset.workUser!==source.owner.userId||node.dataset.workCompany!==source.owner.companyId||source.data.companyId!==source.owner.companyId||!node.dataset.workAssignment||!node.dataset.workRevision||!source.data.branchId||source.owner.branchId&&source.owner.branchId!==source.data.branchId)throw changed();
 return{base:source.base,api:api(source.base),origin:window.location.origin,userId:source.owner.userId,companyId:source.owner.companyId,scopeBranchId:source.owner.branchId,branchId:source.data.branchId,propertyId:source.data.propertyId===null?null:String(source.data.propertyId),assignmentId:node.dataset.workAssignment,revision:node.dataset.workRevision};
}
function validPlan(value:unknown):value is HospitalitySetupPlan{
 if(!object(value)||!object(value.input))return false;
 if(value.kind==='pms')return ['property.create','property.save','room_type.save','room.save','accounting.bootstrap','rate.save'].includes(String(value.operation))&&(value.operation==='property.create'||Number.isSafeInteger(value.version)&&Number(value.version)>0||text(value.after));
 if(value.kind==='product')return text(value.input.sku)&&text(value.input.name)&&text(value.input.price)&&value.input.unit==='each'&&value.input.taxBps===0;
 if(value.kind==='catalog')return text(value.productRow)&&['stock','recipe'].includes(String(value.input.kind))&&typeof value.input.station==='string'&&Array.isArray(value.input.recipe)&&value.input.recipe.every(row=>object(row)&&text(row.productId)&&Number.isSafeInteger(row.quantityMilli)&&Number(row.quantityMilli)>0)&&Array.isArray(value.input.modifiers)&&value.input.modifiers.length===0&&value.input.taxInclusive===false&&value.input.expectedVersion===0;
 if(value.kind==='bootstrap')return value.input.mode==='restaurant'&&text(value.input.registerName)&&text(value.input.warehouseId);
 if(value.kind==='register')return value.input.mode==='restaurant'&&text(value.input.name)&&text(value.input.warehouseId);
 return value.kind==='table'&&text(value.input.registerId)&&text(value.input.name)&&Number.isSafeInteger(value.input.seats)&&Number(value.input.seats)>0&&Number(value.input.seats)<=100;
}
function decode(value:unknown):HospitalitySetupQueue{
 if(!object(value)||value.version!==1||!text(value.id)||!object(value.owner)||!Array.isArray(value.jobs)||value.jobs.length>1000)throw quarantined();
 const owner=value.owner;if(!['base','api','origin','userId','companyId','branchId','assignmentId','revision'].every(key=>text(owner[key]))||owner.scopeBranchId!==null&&!text(owner.scopeBranchId)||owner.propertyId!==null&&!text(owner.propertyId)||owner.api!==api(String(owner.base)))throw quarantined();
 const keys=new Set<string>(),ids=new Set<string>();for(const job of value.jobs){if(!object(job)||!text(job.id)||!text(job.key)||!/^[a-zA-Z0-9:_.-]{8,180}$/.test(job.key)||!text(job.rowKey)||typeof job.label!=='string'||!validPlan(job.plan)||!['planned','uncertain','rejected','saved','cancelled-before-send'].includes(String(job.state))||!Number.isSafeInteger(job.sends)||Number(job.sends)<0||keys.has(job.key)||ids.has(job.id))throw quarantined();keys.add(job.key);ids.add(job.id);if(job.sent&&(!object(job.sent)||typeof job.sent.url!=='string'||typeof job.sent.body!=='string')||job.receipt!==undefined&&!object(job.receipt)||job.state==='saved'&&(!object(job.receipt)||!job.sent||Number(job.sends)<1)||['planned','cancelled-before-send'].includes(String(job.state))&&(job.sent!==undefined||Number(job.sends)!==0)||['uncertain','rejected'].includes(String(job.state))&&(!job.sent||Number(job.sends)<1))throw quarantined();}
 return value as HospitalitySetupQueue;
}
function parseSetup(value:unknown,owner:BoundOwner,propertyId:string|null):HospitalitySetup{
 const permissions=object(value)?value.permissions:null;
 if(!object(value)||value.companyId!==owner.companyId||value.branchId!==owner.branchId||value.propertyId!==(propertyId??null)||!object(permissions)||!['hotel','restaurant','productCreate','financePrepare','financeApprove'].every(key=>typeof permissions[key]==='boolean')||typeof value.companyName!=='string'||typeof value.date!=='string'||typeof value.ready!=='boolean'||typeof value.posConfigured!=='boolean'||!['branches','properties','steps','warehouses','registers','products','tables','charts','accounts','suggestions','roomChargeRequirements','activationRestrictions'].every(key=>Array.isArray(value[key])))throw Error('The current setup read does not match the original Company, branch and hotel.');
 for(const key of ['branches','properties','warehouses','registers','products','tables','charts','accounts'])if(!(value[key] as unknown[]).every(row=>object(row)&&text(row.id)))throw Error('The native setup records are incomplete.');
 if(value.hotel!==null&&(!object(value.hotel)||!object(value.hotel.property)||value.hotel.property.id!==propertyId||value.hotel.property.companyId!==owner.companyId||value.hotel.property.branchId!==owner.branchId||!Number.isSafeInteger(value.hotel.property.version)||!object(value.hotel.property.config)||!Array.isArray(value.hotel.records)||!value.hotel.records.every(row=>object(row)&&text(row.id)&&['room','room_type','rate'].includes(String(row.kind))&&object(row.data))))throw Error('The current hotel setup source is incomplete.');
 if(propertyId&&!value.hotel&&(permissions.hotel||permissions.financePrepare||permissions.financeApprove))throw Error('The current hotel setup source is unavailable.');
 return value as HospitalitySetup;
}
function property(queue:HospitalitySetupQueue){const created=queue.jobs.find(job=>job.plan.kind==='pms'&&job.plan.operation==='property.create'&&job.state==='saved');return queue.owner.propertyId??(created?.receipt&&text(created.receipt.id)?created.receipt.id:null);}
function command(queue:HospitalitySetupQueue,job:HospitalitySetupJob){
 const plan=job.plan,owner=queue.owner;
 if(plan.kind==='pms'){
  if(plan.operation==='property.create'){if(plan.input.branchId!==owner.branchId||owner.propertyId)throw changed();return{url:owner.api+'/pms',body:JSON.stringify({operation:plan.operation,input:plan.input})};}
  const id=property(queue),previous=plan.after?queue.jobs.find(row=>row.rowKey===plan.after&&row.state==='saved'&&row.plan.kind==='pms'):undefined,version=plan.after?previous?.receipt?.version:plan.version;
  if(!id||!Number.isSafeInteger(version)||Number(version)<1)throw Error('Confirm the original preceding hotel row before this setup action.');
  return{url:owner.api+'/pms',body:JSON.stringify({operation:plan.operation,input:plan.input,propertyId:id,expectedVersion:version})};
 }
 if(plan.kind==='product')return{url:owner.api+'/commerce/products',body:JSON.stringify(plan.input)};
 if(plan.kind==='catalog'){
  const product=queue.jobs.find(row=>row.rowKey===plan.productRow&&row.state==='saved'&&row.plan.kind==='product');if(!product?.receipt||!text(product.receipt.id))throw Error('Confirm the original product row before its menu catalogue.');
  return{url:owner.api+'/pos/catalog/save',body:JSON.stringify({...plan.input,productId:product.receipt.id})};
 }
 return{url:owner.api+'/pos/'+(plan.kind==='bootstrap'?'settings/bootstrap':plan.kind==='register'?'register/save':'table/create'),body:JSON.stringify(plan.input)};
}
async function validReceipt(queue:HospitalitySetupQueue,job:HospitalitySetupJob,value:unknown){
 if(!object(value)||!text(value.id)||!job.sent)return false;const plan=job.plan;
 if(plan.kind==='pms'){
  const body:unknown=JSON.parse(job.sent.body);if(!object(body))return false;
  if(plan.operation==='property.create'){const bytes=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(job.key)),id='property-'+Array.from(new Uint8Array(bytes),byte=>byte.toString(16).padStart(2,'0')).join('').slice(0,32);return value.id===id&&value.propertyId===id&&value.version===1;}
  if(value.propertyId!==body.propertyId||value.version!==Number(body.expectedVersion)+1)return false;
  if(plan.operation==='property.save'||plan.operation==='accounting.bootstrap')return value.id===body.propertyId;
  return !text(plan.input.id)||value.id===plan.input.id;
 }
 if(plan.kind==='product')return value.id==='products-'+job.key&&value.version===1;
 if(plan.kind==='catalog'){const body:unknown=JSON.parse(job.sent.body);return object(body)&&value.id===body.productId;}
 if(plan.kind==='bootstrap')return value.id===queue.owner.companyId&&value.mode==='restaurant'&&text(value.registerId)&&text(value.registerName)&&value.warehouseId===plan.input.warehouseId&&text(value.bankAccountId)&&typeof value.registerReused==='boolean'&&Array.isArray(value.accountsCreated)&&value.accountsCreated.every(row=>object(row)&&['id','code','name','category'].every(key=>text(row[key])))&&(!plan.input.registerId||value.registerId===plan.input.registerId)&&(!plan.input.bankAccountId||value.bankAccountId===plan.input.bankAccountId);
 return true;
}
function confirm(job:HospitalitySetupJob,view:HospitalitySetup,receipt:Record<string,unknown>){
 const plan=job.plan,id=receipt.id;
 if(plan.kind==='pms'){
  if(!view.hotel||view.hotel.property.id!==receipt.propertyId||view.hotel.property.version<Number(receipt.version))throw Error('The original hotel receipt is not confirmed in its current native source.');
  if(plan.operation==='property.create'||plan.operation==='property.save'||plan.operation==='accounting.bootstrap')return 'native-current-record' as const;
  const kind=plan.operation==='room.save'?'room':plan.operation==='rate.save'?'rate':'room_type',row=view.hotel.records.find(row=>row.id===id&&row.kind===kind);if(!row)throw Error('The original saved room or room type is not present in this hotel.');return 'native-current-record' as const;
 }
 if(plan.kind==='product'){const row=view.products.find(row=>row.id===id);if(!row)return 'native-receipt-history' as const;if(!Number.isSafeInteger(Number(row.version))||Number(row.version)<1||Number(row.version)===1&&(row.sku!==plan.input.sku||row.name!==plan.input.name||Number(row.price_minor)!==decimalToMinor(plan.input.price)))throw Error('The current product source contradicts its original native receipt.');return 'native-current-record' as const;}
 if(plan.kind==='catalog'){const row=view.products.find(row=>row.id===id);if(!row)return 'native-receipt-history' as const;if(!text(row.kind)||Number(row.catalog_version)<1)throw Error('The current catalogue source contradicts its original native receipt.');return 'native-current-record' as const;}
 if(plan.kind==='table'){if(!view.tables.some(row=>row.id===id&&row.register_id===plan.input.registerId&&row.name===plan.input.name&&Number(row.seats)===plan.input.seats))throw Error('The original table receipt is not confirmed at its restaurant register.');return 'native-current-record' as const;}
 const register=plan.kind==='bootstrap'?receipt.registerId:id;if(plan.kind==='bootstrap'&&!view.posConfigured)throw Error('The current POS source contradicts its native bootstrap receipt.');const originalName=plan.kind==='bootstrap'?receipt.registerName:plan.input.name;return view.registers.some(row=>row.id===register&&row.name===originalName&&row.warehouse_id===plan.input.warehouseId&&row.mode==='restaurant')?'native-current-record' as const:null;
}
function firstRollback(job:HospitalitySetupJob,status:number,code:unknown,owner:BoundOwner){
 if(job.plan.kind==='pms'&&job.sent)return isFirstPmsEditableRejection({base:owner.base,propertyId:propertyId(job.sent.body)},job.sent.body,status,code);
 if(job.plan.kind==='product')return status===422&&code==='VALIDATION_FAILED';
 if(job.plan.kind==='bootstrap')return status===422&&['VALIDATION_FAILED','POS_CURRENCY_MISMATCH','POS_REGISTER_SELECTION_REQUIRED','POS_REGISTER_INVALID','POS_WAREHOUSE_INVALID','POS_WAREHOUSE_SELECTION_REQUIRED','POS_BANK_ACCOUNT_SELECTION_REQUIRED','POS_BANK_ACCOUNT_INVALID'].includes(String(code))||status===409&&['POS_ALREADY_CONFIGURED','POS_COMMERCE_SETUP_REQUIRED','POS_COMMERCE_CHART_REQUIRED','POS_COMMERCE_ACCOUNT_INVALID','POS_RETAIL_REGISTER_EXISTS','POS_WAREHOUSE_INVALID','POS_WAREHOUSE_REQUIRED'].includes(String(code));
 if(status===422&&code==='VALIDATION_FAILED')return true;
 return job.plan.kind==='table'&&status===422&&code==='POS_TABLE_NAME_EXISTS'||job.plan.kind==='catalog'&&(status===409&&code==='POS_VERSION_CONFLICT'||status===422&&['POS_PRODUCT_INVALID','POS_RECIPE_REQUIRED','POS_MODIFIER_DUPLICATE','POS_INGREDIENT_INVALID','POS_RESTAURANT_REQUIRED'].includes(String(code)));
}
function propertyId(body:string){const command:unknown=JSON.parse(body);return object(command)&&text(command.propertyId)?command.propertyId:undefined;}
const fields=(form:HTMLFormElement)=>JSON.stringify(Array.from(new FormData(form),([key,value])=>[key,typeof value==='string'?value:{name:value.name,size:value.size,lastModified:value.lastModified}]));
export function captureHospitalitySetupCompletion(form:HTMLFormElement):HospitalitySetupCompletion{return{form,formEditVersion:form.getAttribute('data-erpx-edit-version'),fields:fields(form)};}
export function createHospitalitySetupCommands(current:()=>HospitalitySetupSource|null,send:typeof workFetch=workFetch,storage:()=>Storage=()=>localStorage){
 let running=false,held:HospitalitySetupQueue|null=null,heldRaw:string|null=null;
 const completions=new Map<string,{completion:ReturnType<typeof retainWorkCommandCompletion>;fields?:string}>();
 function currentSource(){const source=current();if(!source)throw changed();return source;}
 function fence(){const source=currentSource(),owner=bind(source),image=JSON.stringify(source),assertWork=captureWorkCommandContext(),path=window.location.pathname,query=window.location.search;assertWork();return{source,owner,assertCurrent(){assertWork();const fresh=current();if(!fresh||!same(bind(fresh),owner)||JSON.stringify(fresh)!==image||window.location.pathname!==path||window.location.search!==query)throw changed();}};}
 function queue(){const source=currentSource(),raw=storage().getItem(hospitalitySetupQueueKey(source));if(raw===null){if(held)throw quarantined();return null;}if(heldRaw!==null&&raw!==heldRaw)throw quarantined();let value:unknown;try{value=JSON.parse(raw);}catch{throw quarantined();}const result=decode(value),owner=bind(source);if(result.owner.base!==owner.base||result.owner.api!==owner.api||result.owner.origin!==owner.origin||result.owner.userId!==owner.userId||result.owner.companyId!==owner.companyId||result.owner.scopeBranchId!==owner.scopeBranchId||result.owner.branchId!==owner.branchId||owner.propertyId!==result.owner.propertyId&&owner.propertyId!==property(result)||held&&held.id!==result.id)throw quarantined();held=result;heldRaw=raw;return result;}
 function persist(value:HospitalitySetupQueue){const source=currentSource(),prior=queue();if(prior&&prior.id!==value.id)throw quarantined();const raw=JSON.stringify(value);storage().setItem(hospitalitySetupQueueKey(source),raw);held=value;heldRaw=raw;}
 function legacy(){const source=currentSource(),prefix=`hospitality-setup:${source.base}:${source.data.companyId}:${source.data.branchId}:`,keys=new Set(['new',...source.data.properties.map(row=>String(row.id)),String(source.data.propertyId??'new')]);for(const id of keys){const key=prefix+id,raw=storage().getItem(key);if(raw!==null&&raw!=='[]')return{key,bytes:raw.length};}return null;}
 function pending(){return queue()?.jobs.some(job=>job.state==='planned'||job.state==='uncertain')??false;}
 async function confirmRegister(scope:ReturnType<typeof fence>,job:HospitalitySetupJob,receipt:Record<string,unknown>){
  const plan=job.plan;if(plan.kind!=='register'&&plan.kind!=='bootstrap')throw quarantined();
  if(!scope.source.canReadPos)throw Error('The original native register receipt is retained. Its current row is outside the active setup choices; the existing POS settings read requires current pos.read. No successful row is inferred from absence.');
  const response=await send(scope.owner.api+'/pos?tab=settings',{cache:'no-store'}),body:unknown=await response.json();scope.assertCurrent();
  if(!response.ok||!object(body)||!object(body.data)||body.data.companyId!==scope.owner.companyId||body.data.branchId!==scope.owner.scopeBranchId||!Array.isArray(body.data.registers))throw Error('The original native receipt is retained. Current authorized POS register history is unavailable.');
  const id=plan.kind==='bootstrap'?receipt.registerId:receipt.id,row=body.data.registers.find(row=>object(row)&&row.id===id);
  if(!object(row)||row.company_id!==scope.owner.companyId||row.branch_id!==scope.owner.branchId||!Number.isSafeInteger(row.version)||Number(row.version)<1)throw Error('The original register receipt is not confirmed by an actual same-Company, same-branch native POS row.');
  const name=plan.kind==='bootstrap'?receipt.registerName:plan.input.name;
  if(Number(row.version)===1&&(row.name!==name||row.warehouse_id!==plan.input.warehouseId||row.mode!=='restaurant'))throw Error('The first native register row contradicts its original setup receipt.');
  return 'native-receipt-history' as const;
 }
 async function read(scope:ReturnType<typeof fence>,id:string|null){const response=await send(scope.owner.api+'/hospitality/setup?'+new URLSearchParams({branch:scope.owner.branchId,property:id??''}),{cache:'no-store'}),body:unknown=await response.json();scope.assertCurrent();if(!response.ok||!object(body))throw Error(object(body)&&object(body.error)&&typeof body.error.message==='string'?body.error.message:'Current setup read is unavailable. The original jobs are retained.');return parseSetup(body.data,scope.owner,id);}
 function enqueue(drafts:HospitalitySetupDraft[],completion?:HospitalitySetupCompletion,reviewed?:HospitalitySetup){if(running)throw Error('Wait for the original setup save.');const scope=fence();if(reviewed&&!same(reviewed,scope.source.data))throw changed();if(legacy())throw quarantined();let value=queue();if(value&&pending())throw Error('Recover the original unfinished setup rows before submitting newer work.');if(!value)value={version:1,id:crypto.randomUUID(),owner:scope.owner,jobs:[]};if(!same(value.owner,scope.owner)&&value.owner.propertyId!==scope.owner.propertyId&&property(value)!==scope.owner.propertyId)throw changed();if(value.owner.assignmentId!==scope.owner.assignmentId||value.owner.revision!==scope.owner.revision)throw changed();
  if(value.jobs.length+drafts.length>1000)throw Error('Archive the reviewed completed batch before preparing more rows.');
  // Validate every saved dependency before adding any newer correction row.
  for(const draft of drafts){const prior=[...value.jobs].reverse().find(job=>job.rowKey===draft.rowKey);if(prior?.state==='saved'&&!same(prior.plan,draft.plan))throw Error('A previously saved setup row was changed. Its original receipt remains saved. Retain your newer fields and edit that row through its native settings.');}
  const added:string[]=[];for(const draft of drafts){if(!text(draft.rowKey)||!validPlan(draft.plan))throw Error('Review the complete native setup row before saving.');const prior=[...value.jobs].reverse().find(job=>job.rowKey===draft.rowKey);if(prior?.state==='saved')continue;const id=crypto.randomUUID();value.jobs.push({...JSON.parse(JSON.stringify(draft)) as HospitalitySetupDraft,id,key:crypto.randomUUID(),state:'planned',sends:0});added.push(id);}
  if(!added.length)throw Error('These original rows are already saved. Use their native settings to edit them.');scope.assertCurrent();persist(value);const captured=retainWorkCommandCompletion(completion);for(const id of added)completions.set(id,{completion:captured,fields:completion?(completion.fields??fields(completion.form)):undefined});return added;
 }
 async function run(){if(running)throw Error('Wait for the original setup request.');const scope=fence();if(legacy())throw quarantined();const value=queue();if(!value)throw Error('There is no original setup queue.');if(value.owner.assignmentId!==scope.owner.assignmentId||value.owner.revision!==scope.owner.revision)throw changed();if(!value.jobs.some(job=>job.state==='planned'||job.state==='uncertain'))throw Error('No unfinished original setup rows remain.');running=true;let latest=scope.source.data;const completed:HospitalitySetupJob[]=[];
  try{for(const saved of value.jobs.filter(job=>job.state==='saved')){if(!saved.sent||!same(saved.sent,command(value,saved))||!await validReceipt(value,saved,saved.receipt))throw quarantined();scope.assertCurrent();}
   for(const job of value.jobs){if(job.state==='saved'||job.state==='rejected'||job.state==='cancelled-before-send')continue;scope.assertCurrent();const expected=command(value,job);if(job.sent&&!same(job.sent,expected))throw quarantined();if(!job.sent)job.sent=expected;const wasUncertain=job.sends>0;++job.sends;job.state='uncertain';delete job.error;persist(value);
    try{const completion=completions.get(job.id)?.completion(),response=await send(job.sent.url,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':job.key},body:job.sent.body},completion,'caller'),body:unknown=await response.json();scope.assertCurrent();if(!response.ok){const error=object(body)&&object(body.error)?body.error:null;if(!wasUncertain&&!job.receipt&&firstRollback(job,response.status,error?.code,value.owner))job.state='rejected';throw Object.assign(Error(typeof error?.message==='string'?error.message:'The original setup result is unconfirmed.'),{status:response.status,code:error?.code});}
     if(!object(body)||!await validReceipt(value,job,body.data))throw Error('The native setup receipt does not match its original operation, owner and version.');scope.assertCurrent();if(job.receipt&&!same(job.receipt,body.data))throw Error('The original setup receipt changed. Keep its original key.');job.receipt=body.data as Record<string,unknown>;persist(value);const id=job.plan.kind==='pms'&&job.plan.operation==='property.create'?String(job.receipt.id):property(value);latest=await read(scope,id);const confirmed=confirm(job,latest,job.receipt);job.confirmation=confirmed??await confirmRegister(scope,job,job.receipt);scope.assertCurrent();job.state='saved';persist(value);completed.push(job);
    }catch(cause){scope.assertCurrent();job.error=cause instanceof Error?cause.message:'The original row is not confirmed.';persist(value);throw cause;}
   }
   scope.assertCurrent();const finalImage=JSON.stringify(value),viewImage=JSON.stringify(latest);let consumed=false;return{view:latest,jobs:JSON.parse(JSON.stringify(completed)) as HospitalitySetupJob[],assertCurrent:scope.assertCurrent,acknowledge(){scope.assertCurrent();if(consumed)throw Error('The setup completion was already acknowledged.');if(JSON.stringify(queue())!==finalImage||JSON.stringify(latest)!==viewImage)throw quarantined();consumed=true;let newer=false;const forms=new Set<HTMLFormElement>();for(const job of completed){const original=completions.get(job.id),completion=original?.completion();if(!completion||!completion.form.isConnected||completion.form.getAttribute('data-erpx-edit-version')!==completion.formEditVersion||fields(completion.form)!==original?.fields){newer=true;continue;}if(!forms.has(completion.form)){forms.add(completion.form);scope.assertCurrent();window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:completion}));scope.assertCurrent();}completions.delete(job.id);}return{newer};}};
  }finally{running=false;}
 }
 return{enqueue,run,pending,busy:()=>running,queue,legacy,captureCurrent:()=>fence().assertCurrent,async refresh(){const scope=fence(),value=queue();const view=await read(scope,value?property(value):scope.owner.propertyId);return{view,assertCurrent:scope.assertCurrent};},archive(){const scope=fence(),value=queue();if(!value||pending()||running)throw Error('Recover unfinished setup rows before archiving this batch.');scope.assertCurrent();const raw=JSON.stringify(value),key=hospitalitySetupQueueKey(scope.source)+':history:'+value.id;const prior=storage().getItem(key);if(prior!==null&&prior!==raw)throw quarantined();storage().setItem(key,raw);if(storage().getItem(key)!==raw)throw quarantined();storage().removeItem(hospitalitySetupQueueKey(scope.source));held=null;heldRaw=null;completions.clear();},cancelUnsent(){const scope=fence(),value=queue();if(!value||running||value.jobs.some(job=>job.state==='uncertain')||!value.jobs.some(job=>job.state==='rejected'))throw Error('An uncertain native request must be recovered before cancelling unsent rows.');for(const job of value.jobs)if(job.state==='planned'){job.state='cancelled-before-send';job.error='Cancelled before its first send; no native result is claimed.';}scope.assertCurrent();persist(value);}};
}
