import {workFetch,type WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {retainWorkCommandCompletion} from '@erpx/feature-work_projection/lib/command-completion';
import {announceWorkChange} from '@erpx/feature-work_projection/lib/work-refresh';
import {validPostingPrerequisites} from '@erpx/feature-commerce/lib/posting-setup-prerequisite-command';
import type {ServicePartsPrerequisitesView,ServicePartsChartInput,ServicePartsAccountInput,ServicePartsPeriodInput,ServicePartsRateInput} from '@erpx/server-finance/public/service-parts-prerequisites';
import type {ServicePartsPostingSetupInput,ServicePartsPostingSetupView,ServicePartsExistingPostingSetupInput} from '@erpx/server-commerce/public/service-parts-posting-setup';
import type {ServiceInitialPhysicalStockProductInput,ServiceInitialPhysicalStockWarehouseInput} from '@erpx/server-commerce/public/service-initial-physical-stock-masters';
import type {ServicePartsStockWorkspace} from '@erpx/server-commerce/public/service-parts-posting-setup';
export type ServicePartsView={prerequisites:ServicePartsPrerequisitesView;mapping:ServicePartsPostingSetupView;workspace:ServicePartsStockWorkspace};
export type ServicePartsRequest={kind:'existing';body:ServicePartsExistingPostingSetupInput}|{kind:'charts';body:ServicePartsChartInput}|{kind:'accounts';body:ServicePartsAccountInput}|{kind:'periods';body:ServicePartsPeriodInput}|{kind:'rates';body:ServicePartsRateInput}|{kind:'mapping';body:ServicePartsPostingSetupInput}|{kind:'products';body:ServiceInitialPhysicalStockProductInput}|{kind:'warehouses';body:ServiceInitialPhysicalStockWarehouseInput};
export type ServicePartsKind=ServicePartsRequest['kind'];
export type ServicePartsSource={base:string;companyId:string;contextKey:string|null};
export const servicePartsPostingAccountFields=['receivableAccountId','payableAccountId','revenueAccountId','inventoryAccountId','clearingAccountId','cogsAccountId','inputTaxAccountId','outputTaxAccountId','cashAccountId'] as const;
const row=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
const text=(value:unknown):value is string=>typeof value==='string'&&value.length>0;
const context=(value:unknown)=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const work=()=>{const node=typeof document==='undefined'?null:document.querySelector<HTMLElement>('[data-work-revision]');return node?JSON.stringify([node.dataset.workUser,node.dataset.workRevision,node.dataset.workCompany,node.dataset.workAssignment,node.dataset.workAssignmentVersion]):'server';};
export function validServicePartsPostingSetup(value:unknown,companyId:string):value is ServicePartsPostingSetupView{
 if(!row(value)||value.companyId!==companyId||!['available','schemaAvailable','sourceReady','canConfigure','canConfirmExisting'].every(key=>typeof value[key]==='boolean')||!Array.isArray(value.charts)||!Array.isArray(value.accounts))return false;
 if(!value.charts.every(item=>row(item)&&['id','name','baseCurrency'].every(field=>text(item[field])))||!value.accounts.every(item=>row(item)&&['id','chartId','code','name','category','currency','controlKind'].every(field=>text(item[field]))))return false;
 if(![null,'authority-required','purchase-required','schema-required','history-requires-review','invalid-existing-mapping','sales-defaults-required'].includes(value.unavailableReason as string|null))return false;
 const expected=value.expectedSalesDefaults,approved=value.approvedSalesDefaults;
 const legacy=expected===undefined&&approved===undefined,empty=expected===null&&approved===null;
 if(!legacy&&!empty&&(!row(expected)||!text(expected.id)||!Number.isSafeInteger(expected.version)||Number(expected.version)<1||!row(approved)||!['id','chartId','currency','receivableAccountId','revenueAccountId'].every(field=>text(approved[field]))||!Number.isSafeInteger(approved.version)||Number(approved.version)<1||!(approved.taxAccountId===null||text(approved.taxAccountId))||approved.id!==expected.id||approved.version!==expected.version))return false;
 if(!value.available&&!legacy&&!empty)return false;
 if(!value.available)return value.configured===null&&value.version===null&&value.contextKey===null&&value.configuration===null&&!value.canConfigure&&!value.canConfirmExisting&&!value.sourceReady&&value.charts.length===0&&value.accounts.length===0;
 if(!(value.contextKey===null||context(value.contextKey)))return false;
 return (value.configured===false&&value.version===0&&value.configuration===null||value.configured===true&&(value.version===0||value.version===1)&&row(value.configuration)&&['chartId','currency',...servicePartsPostingAccountFields].every(field=>text((value.configuration as Record<string,unknown>)[field])))&&(!value.sourceReady||value.schemaAvailable===true&&value.configured===true&&value.version===1)&&(!value.canConfigure||value.schemaAvailable===true&&value.configured===false&&context(value.contextKey))&&(!value.canConfirmExisting||value.schemaAvailable===true&&value.configured===true&&!value.sourceReady&&context(value.contextKey));
}
export function validServicePartsView(value:unknown,companyId:string):value is ServicePartsView{
 if(!row(value)||!validPostingPrerequisites(value.prerequisites,companyId)||!validServicePartsPostingSetup(value.mapping,companyId)||!row(value.workspace))return false;
 const w=value.workspace,flags=['available','schemaAvailable','sourceReady','canCreateWarehouse','canCreateProduct'];
 if(w.companyId!==companyId||!flags.every(field=>typeof w[field]==='boolean')||!(w.currency===null||typeof w.currency==='string'&&/^[A-Z]{3}$/.test(w.currency))||!Array.isArray(w.branches)||!w.branches.every(item=>row(item)&&text(item.id)&&text(item.name))||![null,'stock-authority-required','purchase-required','schema-required','service-posting-required'].includes(w.unavailableReason as string|null))return false;
 return w.available?(!(w.canCreateWarehouse||w.canCreateProduct)||w.schemaAvailable===true&&w.sourceReady===true):w.currency===null&&!w.sourceReady&&!w.canCreateWarehouse&&!w.canCreateProduct&&w.branches.length===0;
}
export function servicePartsContext(view:ServicePartsView,kind:ServicePartsKind){return kind==='mapping'||kind==='existing'?view.mapping.contextKey:kind==='products'||kind==='warehouses'?JSON.stringify(view.workspace):view.prerequisites.contextKey;}
export function servicePartsAllowed(view:ServicePartsView,kind:ServicePartsKind){if(kind==='mapping')return view.mapping.available&&view.mapping.canConfigure&&!view.mapping.configured;if(kind==='existing')return view.mapping.available&&view.mapping.canConfirmExisting&&view.mapping.configured===true&&!view.mapping.sourceReady;const p=view.prerequisites,w=view.workspace;return kind==='products'?w.available&&w.canCreateProduct:kind==='warehouses'?w.available&&w.canCreateWarehouse:p.available&&({charts:p.canCreateChart,accounts:p.canCreateAccount,periods:p.canCreatePeriod,rates:p.canConfirmRate})[kind];}
export function validServicePartsReceipt(value:unknown,request:ServicePartsRequest,companyId:string,key:string):value is Record<string,unknown>&{id:string}{
 if(!row(value)||!text(value.id))return false;
 if(request.kind==='mapping'||request.kind==='existing')return value.id===companyId&&value.status===(request.kind==='mapping'?'configured':'confirmed-existing')&&value.version===1;
 if(request.kind==='products'||request.kind==='warehouses')return value.id===`${request.kind}-${key}`&&value.version===1;
 const input=request.body.input;
 if(request.kind==='rates')return 'effectiveDate' in input&&value.currency===input.currency&&value.effectiveDate===input.effectiveDate&&value.currencyPerUsd===input.currencyPerUsd&&typeof value.confirmedAt==='number'&&Number.isFinite(value.confirmedAt);
 if(value.companyId!==companyId)return false;
 if(request.kind==='charts')return 'baseCurrency' in input&&value.version===1&&value.name===input.name.trim()&&value.baseCurrency===input.baseCurrency.toUpperCase()&&value.accountingBasis===input.accountingBasis.trim();
 if(request.kind==='periods')return 'startDate' in input&&value.version===0&&value.state==='open'&&value.name===input.name.trim()&&value.startDate===input.startDate&&value.endDate===input.endDate;
 return 'chartId' in input&&value.version===1&&value.chartId===input.chartId&&value.code===input.code.trim()&&value.name===input.name.trim()&&value.category===input.category&&typeof value.currency==='string'&&/^[A-Z]{3}$/.test(value.currency)&&(!input.currency||value.currency===input.currency.toUpperCase())&&value.controlKind===input.controlKind&&value.isControl===input.isControl;
}
const same=(a:ServicePartsSource,b:ServicePartsSource)=>a.base===b.base&&a.companyId===b.companyId;
type Attempt={source:ServicePartsSource;work:string;request:ServicePartsRequest;body:string;key:string;uncertain:boolean;applied:boolean;hadCompletion:boolean;completion:ReturnType<typeof retainWorkCommandCompletion>};
export function createServicePartsCommands(current:(kind:ServicePartsKind)=>ServicePartsSource|null,send:typeof workFetch=workFetch){
 let attempt:Attempt|null=null,busy=false;
 async function run(captured:Attempt){
  if(busy)throw Error('Wait for the original Service parts preparation request.');const before=current(captured.request.kind);if(!before||!same(before,captured.source)||work()!==captured.work)throw Error('Return to the original Company and work role before retrying Service parts preparation.');
  const source={...before},assertWork=captureWorkCommandContext(),priorUncertain=captured.uncertain,assertCurrent=()=>{assertWork();const live=current(captured.request.kind);if(!live||!same(source,live)||source.contextKey!==live.contextKey||work()!==captured.work)throw Object.assign(Error('The Service parts preparation context changed. Your original request and draft are retained.'),{code:'WORK_CONTEXT_CHANGED'});};busy=true;let sent=false;
  try{assertCurrent();sent=true;
   const response=await send(`/api/v1${source.base}/field-service/parts-setup${captured.request.kind==='mapping'?'':captured.request.kind==='products'||captured.request.kind==='warehouses'?`/masters/${captured.request.kind}`:`/${captured.request.kind}`}`,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':captured.key},body:captured.body},captured.completion(),'caller'),body=await response.json() as {data?:unknown;error?:{code?:string;message?:string}};
   if(!response.ok){if(!priorUncertain&&!captured.applied&&response.status<500&&body.error?.code!=='WORK_CONTEXT_CHANGED'){assertCurrent();attempt=null;}throw Object.assign(Error(body.error?.message??'Service parts preparation was not confirmed.'),{code:body.error?.code});}
   if(!validServicePartsReceipt(body.data,captured.request,source.companyId,captured.key))throw Error('The original Service parts preparation receipt is incomplete. Retry the same request.');const receipt=body.data;captured.applied=true;assertCurrent();
   const view=await readServiceParts(()=>current(captured.request.kind),send);
   if(captured.request.kind==='mapping'||captured.request.kind==='existing'){
    const mapping=view.mapping.configuration;if(!mapping||!view.mapping.configured||!view.mapping.sourceReady)throw Error('The saved Service parts mapping is unavailable. Retry the original confirmation.');
    if(captured.request.kind==='mapping'){const input=captured.request.body;if(['chartId','currency',...servicePartsPostingAccountFields].some(field=>mapping[field as keyof typeof mapping]!==input[field as keyof typeof input]))throw Error('The saved mapping does not match the original request.');}
   }else if(captured.request.kind!=='products'&&captured.request.kind!=='warehouses'&&!view.prerequisites[captured.request.kind].some(item=>item.id===receipt.id))throw Error('The saved reference is unavailable in current Service parts status. Retry the original confirmation.');
   assertCurrent();captured.uncertain=true;return{view,receipt,submitted:captured.request,unchanged:()=>{const completion=captured.completion();return captured.hadCompletion?!!completion&&completion.form.isConnected&&completion.form.getAttribute('data-erpx-edit-version')===completion.formEditVersion:true;},acknowledge(){assertCurrent();if(attempt!==captured)return false;attempt=null;const completion=captured.completion();if(completion&&typeof window!=='undefined')window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{...completion,commerceAcknowledged:true}}));announceWorkChange();return true;}};
  }catch(error){if(attempt===captured){if(!sent&&!priorUncertain&&!captured.applied)attempt=null;else captured.uncertain=true;}throw error;}finally{busy=false;}
 }
 return{submit(request:ServicePartsRequest,completion?:WorkCommandCompletion){const source=current(request.kind);if(!source||!source.contextKey||('contextKey' in request.body&&request.body.contextKey!==source.contextKey))throw Error('Review current Service parts preparation references before submitting. Your draft is retained.');const body=JSON.stringify(request.body);if(attempt&&(attempt.request.kind!==request.kind||attempt.body!==body||!same(source,attempt.source)||attempt.work!==work()))throw Error('Confirm the original Service parts preparation request first. Your newer draft is retained.');attempt??={source:{...source},work:work(),request,body,key:crypto.randomUUID(),uncertain:false,applied:false,hadCompletion:!!completion,completion:retainWorkCommandCompletion(completion)};return run(attempt);},retry(){if(!attempt)throw Error('There is no original Service parts preparation request to retry.');return run(attempt);},pending:()=>attempt!==null,applied:()=>attempt?.applied??false};
}
export async function readServiceParts(current:()=>ServicePartsSource|null,send:typeof workFetch=workFetch){
 const before=current();if(!before)throw Error('The Service parts scope is unavailable.');const source={...before},stamp=work(),assertWork=captureWorkCommandContext();assertWork();
 const resources=['','/prerequisites','/stock'] as const,values=await Promise.all(resources.map(async suffix=>{const response=await send(`/api/v1${source.base}/field-service/parts-setup${suffix}`,{cache:'no-store'}),body=await response.json() as {data?:unknown;error?:{message?:string}};if(!response.ok)throw Error(body.error?.message??'Service parts status is unavailable.');return body.data;}));
 assertWork();const live=current();if(!live||!same(source,live)||source.contextKey!==live.contextKey||work()!==stamp)throw Object.assign(Error('The Service parts scope changed. Keep the original draft.'),{code:'WORK_CONTEXT_CHANGED'});
 const value={mapping:values[0],prerequisites:values[1],workspace:values[2]};if(!validServicePartsView(value,source.companyId))throw Error('The Service parts status is incomplete.');return value;
}
