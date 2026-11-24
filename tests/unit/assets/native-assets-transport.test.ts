import {describe,it,expect,vi} from 'vitest';
import {readFile} from 'node:fs/promises';
import {capabilityDelivery} from '../../../packages/catalogue/src/capability-delivery';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'})}));
import {createNativeAssetsClient,type NativeAssetsPageContext} from '../../../packages/feature-assets/src/adapters/server/native-assets';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44991',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
  ...overrides,
});
const actor:NativeAssetsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['assets'],
  iam:{permissions:['records.read','records.write']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',assignmentVersion:3,revision:7,
  role:'owner',assignmentScopeKind:'holding',modules:['assets'],
  permissions:['records.read','records.write'],expiresAt:9_007_199_254_740_991};
const asset={id:'asset-1',company_id:'company',branch_id:'branch',department_id:null,code:'A-1',
  name:'Asset One',category:'equipment',currency:'THB',cost_minor:0,acquired_date:'2026-01-01',
  warranty_end:null,finance_asset_id:null,finance_asset_version:null,condition:'good',
  lifecycle_status:'registered',placement:null,custodian_id:null,custodian_name:null,
  retirement_reason:null,version:1,created_by:'user',created_at:0,updated_at:0,can_review:true};
const workspace={items:[asset],people:[{id:'user',name:'Asset Owner'}],
  nodes:[{id:'branch',kind:'branch',display_name:'Branch',branch_id:null}],
  financeAssets:[],canLinkFinance:false,canCreate:true,actorId:'user'};
const detail={...workspace,asset,finance:null,
  history:[{id:'event-1',asset_id:'asset-1',version:1,action:'register',actor_id:'user',
    reason:'Asset registered',condition:'good',lifecycle_status:'registered',placement:null,
    custodian_id:null,created_at:0}],
  maintenance:[]};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const readInput={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};

describe('native Assets transport',()=>{
 it('forwards the configured Holding, session and current CAS to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  const result=await client.read(incoming(),actor,readInput);
  expect(result.items[0]).toMatchObject({id:'asset-1',code:'A-1',condition:'good'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
 });

 it('reads detail by id and forwards branch/department workspace selectors',async()=>{
  const scoped=actor.scope.kind==='holding'
    ? {...actor,scope:{kind:'branch' as const,branchSlug:'branch'}}
    : actor;
  const transport=vi.fn().mockResolvedValue(ok({data:{...detail,asset:{...asset,branch_id:'branch'}},context:{...context,branchSlug:'branch'}}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await client.detail(incoming(),{...scoped,authority:actor.authority},
    {holdingSlug:'acme',id:'asset-1',branchSlug:'branch'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets?id=asset-1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('branch');
 });

 it('returns the complete native detail payload with Finance snapshot and Maintenance rows',async()=>{
  const finance={id:'finance-1',asset_number:'FA-1',name:'Asset One',status:'in_service',currency:'THB',cost_minor:125050,accumulated_depreciation_minor:0,version:2};
  const work={id:'work-1',title:'Inspect asset',status:'open',priority:'normal',owner_id:'user',scheduled_at:0,version:1};
  const transport=vi.fn().mockResolvedValue(ok({data:{...detail,finance,maintenance:[work]},context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  const result=await client.detail(incoming(),actor,{...readInput,id:'asset-1'});
  expect(result.finance).toEqual(finance);
  expect(result.maintenance).toEqual([work]);
  expect(result.items).toEqual(workspace.items);
  expect(result.history).toEqual(detail.history);
  expect(result.asset).toEqual(asset);
 });

 it('sends one keyed command with origin and returns the owner acknowledgement',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'asset-1',version:2,status:'in_service'},context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  const result=await client.command(incoming(),actor,{
    holdingSlug:'acme',
    body:{id:'asset-1',action:'condition',expectedVersion:1,condition:'degraded',reason:'Inspected'},
    idempotencyKey:'asset-command-1',
    origin:'https://acme.example.com',
  });
  expect(result).toEqual({id:'asset-1',version:2,status:'in_service'});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('idempotency-key')).toBe('asset-command-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body)).toMatchObject({id:'asset-1',action:'condition',expectedVersion:1});
 });

 it('accepts the retained create acknowledgement without fabricating a status',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'asset-create-1',version:1},context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{holdingSlug:'acme',body:{code:'A'},idempotencyKey:'asset-create-1'}))
    .resolves.toEqual({id:'asset-create-1',version:1});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'ASSET_FINANCE_DISPOSAL_REQUIRED',message:'Finance must post the linked asset disposal before physical retirement can be approved.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{id:'asset-1',action:'retire-approve',expectedVersion:2,reason:'Approve'},
    idempotencyKey:'asset-command-2',origin:'https://acme.example.com',
  })).rejects.toMatchObject({
    code:'ASSET_FINANCE_DISPOSAL_REQUIRED',
    status:409,
    message:'Finance must post the linked asset disposal before physical retirement can be approved.',
  });
 });

 it('rejects an unconfigured host, a changed context and a malformed response before returning data',async()=>{
  const client=createNativeAssetsClient(JSON.parse(config()),vi.fn());
  await expect(client.read(incoming({host:'evil.example.com'}),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_AUTHORITY_INVALID'});
  const changed=createNativeAssetsClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:workspace,context:{...context,revision:8}}),
  ));
  await expect(changed.read(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_CONTEXT_CHANGED'});
  const malformed=createNativeAssetsClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:{...workspace,items:[{...asset,extra:'field'}]},context}),
  ));
  await expect(malformed.read(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_RESPONSE_INVALID'});
 });

 it('rejects reserved loopback ports, insecure Holding origins and an invalid command key',async()=>{
  expect(()=>createNativeAssetsClient(JSON.parse(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeAssetsClient(JSON.parse(config({privateBaseUrl:'http://10.0.0.1:44991'})),vi.fn())).toThrow();
  expect(()=>createNativeAssetsClient(JSON.parse(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  const transport=vi.fn();
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{},idempotencyKey:'short',origin:'https://acme.example.com',
  })).rejects.toMatchObject({code:'NATIVE_ASSETS_COMMAND_KEY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('keeps the financial-assets accounting seam Finance-owned and outside the asset-register API',async()=>{
  const routeUrl=new URL('../../../packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts',import.meta.url);
  const pathUrl=new URL('../../../packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts',import.meta.url);
  const assetUrl=new URL('../../../packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/assets/route.ts',import.meta.url);
  const [route,pathRoute,assetRoute]=await Promise.all([routeUrl,pathUrl,assetUrl].map(url=>readFile(url,'utf8')));
  expect(capabilityDelivery('assets','asset-register')?.api).toBe('packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/assets/route.ts');
  expect(assetRoute).toContain('ownedAssetCommand');
  expect(assetRoute).not.toContain('financial-assets');
  expect(route).toContain('Finance-owned Assets accounting seam');
  expect(route).toContain('@erpx/server-finance/public/assets-financial-workflow');
  expect(route).not.toContain('@erpx/server-assets/');
  expect(pathRoute).toContain('Finance-owned Assets accounting seam');
  expect(pathRoute).toContain('@erpx/server-finance/public/assets-financial-workflow');
  expect(pathRoute).not.toContain('@erpx/server-assets/');
 });

 it('routes maintenance creation through the native owner command without a TS create branch',async()=>{
  const routeUrl=new URL('../../../packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/maintenance/route.ts',import.meta.url);
  const ownedUrl=new URL('../../../packages/feature-assets/src/adapters/server/native-maintenance-owned.ts',import.meta.url);
  const [route,owned]=await Promise.all([readFile(routeUrl,'utf8'),readFile(ownedUrl,'utf8')]);
  expect(route).toContain('ownedMaintenanceCommand');
  expect(route).not.toContain('createMaintenanceWork');
  expect(route).not.toContain('changeMaintenanceWork');
  expect(route).not.toContain('nativeMaintenanceTransitionAvailable');
  expect(owned).toContain('nativeMaintenanceCommand');
  expect(owned).toContain("purpose: 'settle-existing'");
  expect(owned).not.toContain('nativeMaintenanceTransitionAvailable');
 });
});

/** Asset source cutover oracle: the source facts consumed by Records and
 * Support come only from the mounted native Assets contract. No owner SQL for
 * `operational_assets` remains in TypeScript and digest rechecks fail closed. */
describe('Asset source-record policy is native-only',()=>{
 const key='ERPX_NATIVE_ASSETS_CONFIG';
 const config=()=>JSON.stringify({privateBaseUrl:'http://127.0.0.1:44991',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}]});
 const sourceActor={
  userId:'owner',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,
  assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['assets'],iam:{permissions:['records.read']},
 } as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const context={userId:'owner',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',assignmentVersion:3,revision:7,
  role:'owner',assignmentScopeKind:'holding',modules:['assets'],permissions:['records.read'],expiresAt:9_007_199_254_740_991};
 const row=(id='source',version=3)=>({id,company_id:'company',branch_id:null,department_id:null,
  name:'Asset source',lifecycle_status:'in_service',version});
 const ok=(data:unknown)=>new Response(JSON.stringify({data,context}),{status:200,headers:{'content-type':'application/json'}});
 const policy=async()=>await import('../../../packages/server-records/src/adapters/source-record-policy');
 const mockRuntime=()=>vi.doMock('../../../packages/server-runtime/src/public/runtime',()=>({
  holdingResources:async()=>({write:{prepare:()=>{throw Error('Legacy Asset source SQL must not execute');}}}),
 }));
 let spy:ReturnType<typeof vi.spyOn>;
 it('reads the asset source through the native owner with a digest recheck and no TypeScript SQL',async()=>{
  vi.resetModules();mockRuntime();vi.stubEnv(key,config());
  spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({asset:row()}));
  try{
   const p=await policy();
   const source=await p.readSourceRecord(sourceActor,{kind:'asset',id:'source',version:3});
   expect(source).toMatchObject({id:'source',version:3,title:'Asset source',status:'in_service',href:'/assets/source'});
   expect(spy).toHaveBeenCalledTimes(2);
   expect(String(spy.mock.calls[0][0])).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets?id=source');
  }finally{spy.mockRestore();vi.unstubAllEnvs();vi.doUnmock('../../../packages/server-runtime/src/public/runtime');vi.resetModules();}
 });
 it('fails closed for same-unit callers, source guards and a changed digest',async()=>{
  vi.resetModules();mockRuntime();vi.stubEnv(key,config());
  try{
   const p=await policy();
   const db={prepare:vi.fn()} as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source'},db)).rejects.toMatchObject({status:503,code:'ASSET_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
   await expect(p.sourceRecordGuards(db,sourceActor,'key',{kind:'asset',id:'source',version:3,href:'/assets/'} as never)).rejects.toMatchObject({status:503,code:'ASSET_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
   spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({asset:row()}));
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source',version:2})).rejects.toMatchObject({status:409,code:'SOURCE_RECORD_CHANGED'});
   spy.mockRestore();
   spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({asset:row()}));
   spy.mockResolvedValueOnce(ok({asset:row()})).mockResolvedValueOnce(ok({asset:row('source',4)}));
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source'})).rejects.toMatchObject({status:409,code:'SOURCE_RECORD_CHANGED'});
  }finally{spy?.mockRestore();vi.unstubAllEnvs();vi.doUnmock('../../../packages/server-runtime/src/public/runtime');vi.resetModules();}
 });
 it('uses native asset identifiers in the support ticket source predicate',async()=>{
  vi.stubEnv(key,config());
  spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({items:[row('asset-1'),row('asset-2')]}));
  try{
   const {supportTicketSourceReadPredicate}=await import('../../../packages/server-support/src/adapters/ticket-source-read-policy');
   const predicate=await supportTicketSourceReadPredicate(sourceActor);
   expect(predicate.sql).toContain("'asset'");
   expect(predicate.bindings).toContain('asset-1');
   expect(predicate.bindings).toContain('asset-2');
   expect(predicate.sql).not.toContain('operational_assets');
   expect(spy).toHaveBeenCalledTimes(2);
  }finally{spy.mockRestore();vi.unstubAllEnvs();}
 });
});
it('retires the TS Service asset picker read now served by the mounted native workspace port',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/field-service-references.ts',import.meta.url),'utf8');
 const barrel=await readFile(new URL('../../../packages/server-assets/src/public/field-service-references.ts',import.meta.url),'utf8');
 const native=await readFile(new URL('../../../packages/server-assets/src/adapters/field_service_workspace.rs',import.meta.url),'utf8');
 for(const retired of ['fieldServiceAssets','fieldServiceAssetReferencePredicate','fieldServiceAssetSourceReadLocks','ServiceAssetReference']){
  expect(adapter).not.toContain(retired);
  expect(barrel).not.toContain(retired);
 }
 for(const live of ['fieldServiceAssetSourceRows','fieldServiceAssetAttachmentSourceLocks','fieldServiceAssetAttachmentSourceImage']){
  expect(adapter).toContain(live);
  expect(barrel).toContain(live);
 }
 expect(adapter).not.toContain("lifecycle_status NOT IN ('retired','retirement_pending')");
 expect(native).toContain('pub struct ServiceAssetReference');
 expect(native).toContain('picker: Vec<ServiceAssetReference>');
 expect(native).toContain('read_service_workspace_assets');
});
it('shares one parameterized org-node label projection across the Assets executive breakdowns (50->49)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/executive-metric-projections.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 expect(adapter).toContain('const assetNodeDisplayNameLabel = (column: "branch_id" | "department_id", fallback: string) =>');
 expect(count(adapter,'SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes})')).toBe(1);
 expect(adapter).toContain('assetNodeDisplayNameLabel("branch_id", "Company-wide")');
 expect(adapter).toContain('assetNodeDisplayNameLabel("department_id", "Unassigned")');
 expect(count(adapter,'? assetBranchDisplayNameLabel')).toBe(2);
 expect(adapter).toContain('breakdown === "branch" ? assetBranchDisplayNameLabel');
 expect(adapter).toContain('breakdown === "department" ? assetDepartmentDisplayNameLabel');
 expect(adapter).not.toContain("WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')");
});
it('shares one canonical permission-probe SQL with the purchase and metric callers (52->50)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const helper=await readFile(new URL('../../../packages/server-assets/src/adapters/asset-scope-probe.ts',import.meta.url),'utf8');
 const purchases=await readFile(new URL('../../../packages/server-assets/src/adapters/purchases.ts',import.meta.url),'utf8');
 const metrics=await readFile(new URL('../../../packages/server-assets/src/adapters/metric-purchases.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const probe="allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id)";
 expect(helper).toContain('export const assetPermissionProbeSql = (alias: string, permission: string) =>');
 expect(count(helper,probe)).toBe(1);
 for(const caller of [purchases,metrics]){
  expect(caller).toContain('assetPermissionProbeSql');
  expect(count(caller,probe)).toBe(0);
 }
 expect(purchases).toContain('assetPermissionProbeSql("assets_scope",permission)');
 expect(metrics).toContain('assetPermissionProbeSql("asset_metric_scope","records.read")');
 expect(metrics).toContain('assignmentReadSql("asset_metric_owned","records.read")');
});
it('shares one parameterized asset descriptor SQL across the policy and remaining-operational reads (49->48)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const policy=await readFile(new URL('../../../packages/server-assets/src/adapters/attachments-policy-facts.ts',import.meta.url),'utf8');
 const operational=await readFile(new URL('../../../packages/server-assets/src/adapters/attachments-operational-facts.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const literal='SELECT company_id,${branch} AS branch_id,${department} AS department_id,${version} AS version FROM ${table}';
 expect(policy).toContain('export const assetAttachmentDescriptorSql = (table:string,branch:string,department:string,version:string,extra="") =>');
 expect(count(policy,literal)).toBe(1);
 expect(count(operational,literal)).toBe(0);
 expect(policy).toContain('assetAttachmentDescriptorSql(descriptor.table,branch,department,version,extra)');
 expect(operational).toContain('assetAttachmentDescriptorSql(moduleTarget.table,moduleTarget.branch,moduleTarget.department ?? "NULL::text","version")');
 expect(operational).toContain('import {assetAttachmentDescriptorSql} from "./attachments-policy-facts"');
});
it('shares one canonical source-probe projection across the Assets record-purchase reads (46->44)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/record-purchases.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 // The canonical probe owns the single read token; its comment adds no KPI token.
 expect(adapter.match(/SELECT/g)).toHaveLength(2);
 expect(adapter).toContain('const assetSourceProbeSql=(from:string,where:string)=>');
 expect(count(adapter,'SELECT 1 FROM ${from} WHERE ${where}')).toBe(1);
 expect(adapter).toContain('assetSourceVisibleSql=(table:string,alias:string,idValue:string,tail="")=>`EXISTS(${assetSourceProbeSql(');
 expect(adapter).toContain('assetSourceReadSql=(from:string,idColumn:string,tail="")=>assetSourceProbeSql(from,');
 // No duplicate raw source-probe literal remains at any of the three call sites.
 expect(count(adapter,'SELECT 1 FROM ${table} ${alias}')).toBe(0);
 expect(count(adapter,'SELECT 1 FROM ${from} WHERE ${idColumn}')).toBe(0);
 expect(adapter).toContain('visible=assetSourceVisibleSql(definition.table,alias,id,` AND ${alias}.company_id=${company} AND ${scope.where}`)');
 expect(adapter).toContain('${assetSourceVisibleSql(definition.table,"actual_asset_source",id)}');
 expect(adapter).toContain('database.prepare(assetSourceReadSql(definition.table,"id"))');
 expect(adapter).toContain('database.prepare(assetSourceReadSql(`${definition.table} asset_record_source`,"id",` AND ${scope.where}`))');
 expect(adapter).toContain('${assetSourceProbeSql("asset_source_clock",purchasedCapabilityPredicate(');
});
it('shares one canonical aggregate projection across the Assets executive breakdowns (45->44)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/executive-metric-projections.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const literal='SELECT ${label} label,count(*)::text value FROM';
 // The canonical helper owns the single aggregate projection; its comment adds
 // no "SELECT" token to the retirement KPI.
 expect(count(adapter,literal)).toBe(1);
 expect(adapter).toContain('const assetMetricBreakdownProjectionSql = (label: string) =>');
 // No duplicate raw projection literal remains at either breakdown call site.
 expect(count(adapter,'`SELECT ${label} label,count(*)::text value FROM (${source.sql}) a')).toBe(0);
 expect(count(adapter,'`SELECT ${label} label,count(*)::text value FROM operational_asset_events e')).toBe(0);
 expect(count(adapter,'${assetMetricBreakdownProjectionSql(label)} (${source.sql}) a')).toBe(1);
 expect(count(adapter,'${assetMetricBreakdownProjectionSql(label)} operational_asset_events e')).toBe(1);
 // The already-shared org-node label family remains intact.
 expect(count(adapter,'SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes})')).toBe(1);
});
it('shares one fixed owner predicate for attachment owner version guards (46->45)',async()=>{
  const {readFile}=await import('node:fs/promises');
  const base=new URL('../../../packages/server-assets/src/adapters/',import.meta.url);
  const guard=await readFile(new URL('attachment-owner-version-guard.ts',base),'utf8');
  const service=await readFile(new URL('attachments-service-facts.ts',base),'utf8');
  const operational=await readFile(new URL('attachments-operational-facts.ts',base),'utf8');
  const predicate='EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?';
  // The canonical helper owns the single guard predicate; its comment must
  // not smuggle a second "SELECT" token into the retirement KPI.
  expect(guard.match(/SELECT/g)).toHaveLength(1);
  expect(guard).toContain('export const attachmentOwnerVersionGuardSql');
  expect(guard).toContain(predicate);
  for(const consumer of [service,operational]){
   expect(consumer).toContain('import {attachmentOwnerVersionGuardSql} from "./attachment-owner-version-guard"');
   expect(consumer).not.toContain('EXISTS(SELECT 1 FROM ${');
  }
  expect(service).toContain('attachmentOwnerVersionGuardSql(item.table, item.extra)');
  expect(operational).toContain('attachmentOwnerVersionGuardSql(table)');
});
it('shares one canonical existence probe across the Assets package-settlement read (11->7)',async()=>{
  const {readFile}=await import('node:fs/promises');
  const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/package-obligations.ts',import.meta.url),'utf8');
  const count=(text:string,needle:string)=>text.split(needle).length-1;
  // The owner-local source probe is the single existence statement; neither this
  // adapter nor its comments may smuggle another "SELECT" token into the KPI.
  expect(adapter.match(/SELECT/g)).toHaveLength(7);
  expect(adapter).toContain('import {assetSourceProbeSql} from "./record-purchases"');
  expect(adapter).toContain('const assetExistsSql = (from:string,where:string) =>');
  expect(count(adapter,'EXISTS(SELECT 1 FROM ${from} WHERE ${where})')).toBe(0);
  expect(count(adapter,'`EXISTS(${assetSourceProbeSql(from,where)})`')).toBe(1);
  expect(adapter).toContain('const assetSourceExistsSql = (source:string,alias:string,where:string) =>');
  // No duplicate raw probe literal remains at the owner or finance-source call sites.
  expect(count(adapter,'EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE ${where})')).toBe(0);
  expect(count(adapter,'NOT EXISTS(SELECT 1 FROM (${assetsSettlementFinancialAssets}) financial')).toBe(0);
  expect(count(adapter,'assetSourceExistsSql(')).toBe(3);
  expect(count(adapter,'assetExistsSql(')).toBe(2);
  expect(adapter).toContain('assetSourceExistsSql(placements,"branch"');
  expect(adapter).toContain('assetSourceExistsSql(placements,"department"');
  expect(adapter).toContain('assetSourceExistsSql(refs,"source"');
  expect(adapter).toContain('NOT ${assetExistsSql(`(${assetsSettlementFinancialAssets}) financial JOIN');
  // The exported source object used by the Maintenance consumer is unchanged in shape.
  expect(adapter).toContain('export const assetPackageSettlementSources={assets:');
  expect(adapter).toContain('links:"SELECT * FROM operational_asset_finance_links"} as const;');
});
it('shares one canonical lineage aggregate projection across the Assets financial source (42->39)',async()=>{
   const {readFile}=await import('node:fs/promises');
   const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/financial-workflow-sources.ts',import.meta.url),'utf8');
   const count=(text:string,needle:string)=>text.split(needle).length-1;
   // The canonical helper owns the single JSON aggregate literal; its comment must
   // not smuggle another "SELECT" token into the retirement KPI.
   expect(adapter.match(/SELECT/g)).toHaveLength(6);
   expect(adapter).toContain('const assetPhysicalAggSql = (alias:string,from:string) =>');
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})')).toBe(1);
   // All four lineage call sites use the helper with their owner-owned FROM fragment.
   expect(count(adapter,'assetPhysicalAggSql(')).toBe(4);
   expect(adapter).toContain('assetPhysicalAggSql("a","physical a")');
   expect(adapter).toContain('assetPhysicalAggSql("e","operational_asset_events e JOIN physical a ON a.id=e.asset_id")');
   expect(adapter).toContain('assetPhysicalAggSql("l","operational_asset_finance_links l WHERE l.finance_asset_id=?")');
   expect(adapter).toContain('assetPhysicalAggSql("w","maintenance_work_orders w JOIN physical a ON a.id=w.asset_id")');
   // No raw aggregate subquery remains at any of the four call sites.
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM physical a)')).toBe(0);
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id)')).toBe(0);
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id)')).toBe(0);
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(w) ORDER BY w.id)')).toBe(0);
   // The byte-identical composed image and its source guards remain intact.
   expect(adapter).toContain("jsonb_build_object('assets',${assetPhysicalAggSql(\"a\",\"physical a\")},'events',");
   expect(adapter).toContain('EXISTS(SELECT 1 FROM operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id WHERE l.finance_asset_id=? AND a.id IS NULL)');
   expect(adapter).toContain('export function assetsFinancialPhysicalSourceLocks');
});
it('shares one canonical current-assignment scope probe across the Assets private attachment authority and current-scope checks (35->33)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const base=new URL('../../../packages/server-assets/src/adapters/',import.meta.url);
 const helper=await readFile(new URL('asset-scope-tuple.ts',base),'utf8');
 const authority=await readFile(new URL('attachment-purchases.ts',base),'utf8');
 const read=await readFile(new URL('attachment-read.ts',base),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const tuple='(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
 // The canonical helpers own the single tuple and probe statements; no comment adds a KPI token.
 expect(helper.match(/SELECT/g)).toHaveLength(2);
 expect(helper).toContain('export const assetScopeTupleSql = (alias: string) =>');
 expect(helper).toContain('export const assetCurrentScopeProbeSql = (projection:string,from:string,tail="") =>');
 expect(count(helper,tuple)).toBe(1);
 expect(count(helper,'SELECT ${projection} FROM${from}${tail}')).toBe(1);
 for(const caller of [authority,read]){
  expect(caller).toMatch(/import \{assetCurrentScopeProbeSql,assetScopeTupleSql\} from ['"]\.\/asset-scope-tuple['"]/);
  expect(count(caller,tuple)).toBe(0);
 }
 // No raw current-scope statement remains at either call site; both use the canonical probe.
 expect(count(read,'SELECT 1 FROM${assetScopeTupleSql("workforce_actor_scope")}')).toBe(0);
 expect(count(authority,'(SELECT ${assignmentReadSql("asset_file_scope","records.write")} FROM ${assetScopeTupleSql("asset_file_scope")})')).toBe(0);
 expect(read).toContain('assetCurrentScopeProbeSql("1",assetScopeTupleSql("workforce_actor_scope"),` WHERE ${currentScope.where}`)');
 expect(authority).toContain('assetCurrentScopeProbeSql(assignmentReadSql("asset_file_scope","records.write"),` ${assetScopeTupleSql("asset_file_scope")}`)');
 expect(authority).toContain('${assetScopeTupleSql("asset_file_scope")}');
 expect(read).toContain('assetScopeTupleSql("workforce_actor_scope")');
 // The r3 permission-probe and r12 record-purchase probes stay untouched.
 const probe=await readFile(new URL('asset-scope-probe.ts',base),'utf8');
 expect(count(probe,tuple)).toBe(1);
 expect(probe).toContain('allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id)');
 const purchaseProbe=await readFile(new URL('record-purchases.ts',base),'utf8');
 expect(purchaseProbe).toContain('const assetSourceProbeSql=(from:string,where:string)=>`SELECT 1 FROM ${from} WHERE ${where}`');
});
it('shares the canonical lineage aggregate with the Assets purchase owner source (36->35)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const base=new URL('../../../packages/server-assets/src/adapters/',import.meta.url);
 const aggregate=await readFile(new URL('financial-workflow-sources.ts',base),'utf8');
 const purchases=await readFile(new URL('purchases.ts',base),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 // The r10 helper owns the single aggregate token; exporting it lets the
 // separate purchase owner source reuse the same literal without a second one.
 expect(aggregate).toContain('export const assetPhysicalAggSql = (alias:string,from:string) =>');
 expect(purchases.match(/SELECT/g)).toHaveLength(2);
 expect(purchases).toContain('import {assetPhysicalAggSql} from "./financial-workflow-sources"');
 expect(count(purchases,'${assetPhysicalAggSql("link","operational_asset_finance_links link WHERE link.asset_id=r.id")}')).toBe(1);
 // No duplicate raw finance-link aggregate remains at the purchase call site.
 expect(count(purchases,'(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM operational_asset_finance_links link WHERE link.asset_id=r.id)')).toBe(0);
 // Byte parity: the helper template composes the exact prior purchase SQL.
 const alias="link",from="operational_asset_finance_links link WHERE link.asset_id=r.id";
 expect(`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})`).toBe('(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM operational_asset_finance_links link WHERE link.asset_id=r.id)');
 // The distinct r10 consumer and the closed r12 scope tuple stay untouched.
 expect(count(aggregate,'assetPhysicalAggSql(')).toBe(4);
});
