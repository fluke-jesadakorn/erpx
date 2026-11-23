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
it('shares two canonical source-probe projections across the Assets record-purchase reads (48->46)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../packages/server-assets/src/adapters/record-purchases.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 expect(adapter).toContain('const assetSourceVisibleSql=(table:string,alias:string,idValue:string,tail="")=>');
 expect(adapter).toContain('const assetSourceReadSql=(from:string,idColumn:string,tail="")=>');
 // Each canonical helper owns one read projection; its comment adds no KPI token.
 expect(count(adapter,'EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${idValue}${tail})')).toBe(1);
 expect(count(adapter,'SELECT 1 FROM ${from} WHERE ${idColumn}=?${tail}')).toBe(1);
 // No duplicate raw source-probe literal remains at any of the four call sites.
 expect(count(adapter,'SELECT 1 FROM ${definition.table}')).toBe(0);
 expect(adapter).toContain('visible=assetSourceVisibleSql(definition.table,alias,id,` AND ${alias}.company_id=${company} AND ${scope.where}`)');
 expect(adapter).toContain('${assetSourceVisibleSql(definition.table,"actual_asset_source",id)}');
 expect(adapter).toContain('database.prepare(assetSourceReadSql(definition.table,"id"))');
 expect(adapter).toContain('database.prepare(assetSourceReadSql(`${definition.table} asset_record_source`,"id",` AND ${scope.where}`))');
});
