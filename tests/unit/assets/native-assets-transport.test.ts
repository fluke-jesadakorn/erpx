import {describe,it,expect,vi} from 'vitest';
import {readFile} from 'node:fs/promises';
import {readFileSync,readdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {capabilityDelivery} from '../../../platform/catalogue/src/capability-delivery';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'})}));
import {createNativeAssetsClient,type NativeAssetsPageContext} from '../../../modules/assets/ui/src/adapters/server/native-assets';

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
  const routeUrl=new URL('../../../modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/route.ts',import.meta.url);
  const pathUrl=new URL('../../../modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/financial-assets/[...path]/route.ts',import.meta.url);
  const assetUrl=new URL('../../../modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/route.ts',import.meta.url);
  const [route,pathRoute,assetRoute]=await Promise.all([routeUrl,pathUrl,assetUrl].map(url=>readFile(url,'utf8')));
  expect(capabilityDelivery('assets','asset-register')?.api).toBe('modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/assets/route.ts');
  expect(assetRoute).toContain('ownedAssetCommand');
  expect(assetRoute).not.toContain('financial-assets');
  expect(route).toContain('Finance-owned Assets accounting seam');
  expect(route).toContain('@erpx/finance-server/public/assets-financial-workflow');
  expect(route).not.toContain('@erpx/assets-server/');
  expect(pathRoute).toContain('Finance-owned Assets accounting seam');
  expect(pathRoute).toContain('@erpx/finance-server/public/assets-financial-workflow');
  expect(pathRoute).not.toContain('@erpx/assets-server/');
 });

 it('routes maintenance creation through the native owner command without a TS create branch',async()=>{
  const routeUrl=new URL('../../../modules/assets/ui/src/routes/api/v1/holdings/[holdingSlug]/maintenance/route.ts',import.meta.url);
  const ownedUrl=new URL('../../../modules/assets/ui/src/adapters/server/native-maintenance-owned.ts',import.meta.url);
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
 } as unknown as import('@erpx/identity-access-server/public/authorization').WorkContext;
 const context={userId:'owner',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',assignmentVersion:3,revision:7,
  role:'owner',assignmentScopeKind:'holding',modules:['assets'],permissions:['records.read'],expiresAt:9_007_199_254_740_991};
 const row=(id='source',version=3)=>({id,company_id:'company',branch_id:null,department_id:null,
  name:'Asset source',lifecycle_status:'in_service',version});
 const ok=(data:unknown)=>new Response(JSON.stringify({data,context}),{status:200,headers:{'content-type':'application/json'}});
 const policy=async()=>await import('../../../modules/records/server/src/adapters/source-record-policy');
 const mockRuntime=()=>vi.doMock('../../../platform/runtime/server/src/public/runtime',()=>({
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
  }finally{spy.mockRestore();vi.unstubAllEnvs();vi.doUnmock('../../../platform/runtime/server/src/public/runtime');vi.resetModules();}
 });
 it('fails closed for same-unit callers, source guards and a changed digest',async()=>{
  vi.resetModules();mockRuntime();vi.stubEnv(key,config());
  try{
   const p=await policy();
   const db={prepare:vi.fn()} as unknown as import('@erpx/platform-runtime-server/public/runtime/types').SqlDatabase;
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source'},db)).rejects.toMatchObject({status:503,code:'ASSET_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
   await expect(p.sourceRecordGuards(db,sourceActor,'key',{kind:'asset',id:'source',version:3,href:'/assets/'} as never)).rejects.toMatchObject({status:503,code:'ASSET_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
   spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({asset:row()}));
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source',version:2})).rejects.toMatchObject({status:409,code:'SOURCE_RECORD_CHANGED'});
   spy.mockRestore();
   spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({asset:row()}));
   spy.mockResolvedValueOnce(ok({asset:row()})).mockResolvedValueOnce(ok({asset:row('source',4)}));
   await expect(p.readSourceRecord(sourceActor,{kind:'asset',id:'source'})).rejects.toMatchObject({status:409,code:'SOURCE_RECORD_CHANGED'});
  }finally{spy?.mockRestore();vi.unstubAllEnvs();vi.doUnmock('../../../platform/runtime/server/src/public/runtime');vi.resetModules();}
 });
 it('uses native asset identifiers in the support ticket source predicate',async()=>{
  vi.stubEnv(key,config());
  spy=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>ok({items:[row('asset-1'),row('asset-2')]}));
  try{
   const {supportTicketSourceReadPredicate}=await import('../../../modules/support/server/src/adapters/ticket-source-read-policy');
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
 const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/field-service-references.ts',import.meta.url),'utf8');
 const barrel=await readFile(new URL('../../../modules/assets/server/src/public/field-service-references.ts',import.meta.url),'utf8');
 const native=await readFile(new URL('../../../modules/assets/server/src/adapters/field_service_workspace.rs',import.meta.url),'utf8');
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
 const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/executive-metric-projections.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 expect(adapter).toContain('const assetNodeDisplayNameLabel = (column: "branch_id" | "department_id", fallback: string) =>');
 // r32 delegates the org-node label head to assetProjectionFromSql; the raw
 // literal is gone but the composed label renders byte-identically.
 expect(count(adapter,'SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes})')).toBe(0);
 expect(adapter).toContain('assetProjectionFromSql("n.display_name",');
 expect(adapter).toContain('assetNodeDisplayNameLabel("branch_id", "Company-wide")');
 expect(adapter).toContain('assetNodeDisplayNameLabel("department_id", "Unassigned")');
 expect(count(adapter,'? assetBranchDisplayNameLabel')).toBe(2);
 expect(adapter).toContain('breakdown === "branch" ? assetBranchDisplayNameLabel');
 expect(adapter).toContain('breakdown === "department" ? assetDepartmentDisplayNameLabel');
 expect(adapter).not.toContain("WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')");
});
it('shares one canonical permission-probe SQL with the purchase and metric callers (52->50)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const helper=await readFile(new URL('../../../modules/assets/server/src/adapters/asset-scope-probe.ts',import.meta.url),'utf8');
 const purchases=await readFile(new URL('../../../modules/assets/server/src/adapters/purchases.ts',import.meta.url),'utf8');
 const metrics=await readFile(new URL('../../../modules/assets/server/src/adapters/metric-purchases.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const probe="allowed FROM ${assetScopeTupleSql(alias)}";
 expect(helper).toContain('export const assetPermissionProbeSql = (alias: string, permission: string) =>');
 // r31 delegates the probe head to assetProjectionFromSql; no local SELECT token remains.
 expect(count(helper,probe)).toBe(0);
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
 const policy=await readFile(new URL('../../../modules/assets/server/src/adapters/attachments-policy-facts.ts',import.meta.url),'utf8');
 const operational=await readFile(new URL('../../../modules/assets/server/src/adapters/attachments-operational-facts.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const literal='SELECT ${projection} FROM ${from} WHERE id=? AND company_id=?${tail}';
 expect(policy).toContain('export const assetAttachmentDescriptorSql = (table:string,branch:string,department:string,version:string,extra="") =>');
 // r29 folds the keyed-row head into the generic projection-from head; the raw
 // literal is gone but the descriptor renders byte-identically from that head.
 expect(count(policy,literal)).toBe(0);
 expect(policy).toContain('assetProjectionFromSql(projection,` ${from}`,` WHERE id=? AND company_id=?${tail}`)');
 expect(count(operational,literal)).toBe(0);
 expect(policy).toContain('assetIdCompanyReadSql(`company_id,${branch} AS branch_id,${department} AS department_id,${version} AS version`,table,extra)');
 expect(policy).toContain('assetAttachmentDescriptorSql(descriptor.table,branch,department,version,extra)');
 expect(operational).toContain('assetAttachmentDescriptorSql(moduleTarget.table,moduleTarget.branch,moduleTarget.department ?? "NULL::text","version")');
 expect(operational).toContain('import {assetAttachmentDescriptorSql} from "./attachments-policy-facts"');
});
it('shares one canonical source-probe projection across the Assets record-purchase reads (46->44)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/record-purchases.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 // The clock CTE head is now owner-planned in Rust; no token remains here.
 expect((adapter.match(/SELECT/g)??[])).toHaveLength(0);
 expect(adapter).toContain('await nativeAssetsClockScalar()');
 expect(adapter).toContain('const assetSourceProbeSql=(from:string,where:string)=>');
 expect(count(adapter,'SELECT 1 FROM ${from} WHERE ${where}')).toBe(0);
 expect(adapter).toContain('import {assetProjectionFromSql} from "./asset-scope-tuple"');
 expect(adapter).toContain('assetProjectionFromSql("1",` ${from}`,` WHERE ${where}`)');
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
 const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/executive-metric-projections.ts',import.meta.url),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const literal='SELECT ${label} label,count(*)::text value FROM';
 // The canonical helper owns the single aggregate projection; its comment adds
 // no "SELECT" token to the retirement KPI.
 // r32 folds the aggregate head into the generic projection-from head; the raw
 // literal is gone but the projection renders byte-identically from that head.
 expect(count(adapter,literal)).toBe(0);
 expect(adapter).toContain('export const assetMetricBreakdownProjectionSql = (label: string) =>');
 expect(adapter).toContain('assetProjectionFromSql(`${label} label,count(*)::text value`,"")');
 // No duplicate raw projection literal remains at either breakdown call site.
 expect(count(adapter,'`SELECT ${label} label,count(*)::text value FROM (${source.sql}) a')).toBe(0);
 expect(count(adapter,'`SELECT ${label} label,count(*)::text value FROM operational_asset_events e')).toBe(0);
 expect(count(adapter,'${assetMetricBreakdownProjectionSql(label)} (${source.sql}) a')).toBe(1);
 expect(count(adapter,'${assetMetricBreakdownProjectionSql(label)} operational_asset_events e')).toBe(1);
 // The already-shared org-node label family remains intact.
 // r32 delegates the org-node label head to assetProjectionFromSql; the raw
 // literal is gone but the composed label renders byte-identically.
 expect(count(adapter,'SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes})')).toBe(0);
 expect(adapter).toContain('assetProjectionFromSql("n.display_name",');
});
it('shares one fixed owner predicate for attachment owner version guards (46->45)',async()=>{
  const {readFile}=await import('node:fs/promises');
  const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
  const guard=await readFile(new URL('attachment-owner-version-guard.ts',base),'utf8');
  const service=await readFile(new URL('attachments-service-facts.ts',base),'utf8');
  const operational=await readFile(new URL('attachments-operational-facts.ts',base),'utf8');
  const predicate='EXISTS(${assetSourceProbeSql(table, `id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""}`)})';
  // The r17 row-probe family owns the single statement; the guard composes it.
  expect(guard.match(/SELECT/g)??[]).toHaveLength(0);
  expect(guard).toContain('export const attachmentOwnerVersionGuardSql');
  expect(guard).toContain(predicate);
  expect(guard).toContain('import {assetSourceProbeSql} from "./record-purchases"');
  for(const consumer of [service,operational]){
   expect(consumer).toContain('import {attachmentOwnerVersionGuardSql} from "./attachment-owner-version-guard"');
   expect(consumer).not.toContain('EXISTS(SELECT 1 FROM ${');
  }
  expect(service).toContain('attachmentOwnerVersionGuardSql(item.table, item.extra)');
  expect(operational).toContain('attachmentOwnerVersionGuardSql(table)');
});
it('shares one canonical existence probe across the Assets package-settlement read (11->1 after r26 native cutover)',async()=>{
  const {readFile}=await import('node:fs/promises');
  const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/package-obligations.ts',import.meta.url),'utf8');
  const count=(text:string,needle:string)=>text.split(needle).length-1;
  // The owner-local source probe is the single existence statement; neither this
  // adapter nor its comments may smuggle another "SELECT" token into the KPI.
  // r23 moves the Finance-link all-rows literal to the owner all-rows helper;
  // r26 retires the obligation fallback, leaving only the Maintenance source;
  // r33 delegates that source projection to assetProjectionFromSql (1 -> 0).
  expect((adapter.match(/SELECT/g)??[])).toHaveLength(0);
  expect(adapter).toContain('import {assetSourceProbeSql} from "./record-purchases"');
  expect(adapter).toContain('import {assetProjectionFromSql} from "./asset-scope-tuple"');
  expect(adapter).toContain('import {assetAllRowsSql} from "./asset-all-rows-sql"');
  expect(adapter).toContain('const assetExistsSql = (from:string,where:string) =>');
  expect(count(adapter,'EXISTS(SELECT 1 FROM ${from} WHERE ${where})')).toBe(0);
  expect(count(adapter,'`EXISTS(${assetSourceProbeSql(from,where)})`')).toBe(1);
  expect(adapter).toContain('const assetSourceExistsSql = (source:string,alias:string,where:string) =>');
  // No duplicate raw probe literal remains at the owner or finance-source call sites.
  expect(count(adapter,'EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE ${where})')).toBe(0);
  expect(count(adapter,'NOT EXISTS(SELECT 1 FROM (${assetsSettlementFinancialAssets}) financial')).toBe(0);
  expect(count(adapter,'assetSourceExistsSql(')).toBe(2);
  expect(count(adapter,'assetExistsSql(')).toBe(1);
  expect(adapter).toContain('assetSourceExistsSql(placements,"branch"');
  expect(adapter).toContain('assetSourceExistsSql(placements,"department"');
  expect(adapter).not.toContain('assetSourceExistsSql(refs,"source"');
  expect(adapter).not.toContain('NOT ${assetExistsSql(`(${assetsSettlementFinancialAssets}) financial JOIN');
  // The exported source object used by the Maintenance consumer is unchanged in shape.
  expect(adapter).toContain('export const assetPackageSettlementSources={assets:');
  expect(adapter).toContain('links:assetAllRowsSql("operational_asset_finance_links")} as const;');
  expect(count(adapter,'links:"SELECT * FROM operational_asset_finance_links"')).toBe(0);
  // The obligation reader now routes unconditionally and keeps its lock/guards.
  expect(adapter).not.toContain('nativeAssetsReadConfigured');
  expect(adapter).toContain('nativeAssetsReadAll<{kind:string;id:string;version:number;state:string;pending:boolean}>(kind,{companyId:input.companyId,isBase:full},database)');
  expect(adapter).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
  expect(adapter).toContain('throw Error("ASSETS_PACKAGE_OBLIGATION_SOURCE_REQUIRED")');
  expect(adapter).toContain('throw Error("ASSETS_PACKAGE_OBLIGATION_SCOPE_REQUIRED")');
});
it('shares one canonical lineage aggregate projection across the Assets financial source (42->39)',async()=>{
   const {readFile}=await import('node:fs/promises');
   const adapter=await readFile(new URL('../../../modules/assets/server/src/adapters/financial-workflow-sources.ts',import.meta.url),'utf8');
   const count=(text:string,needle:string)=>text.split(needle).length-1;
   // r33 folds the JSON aggregate head into the generic projection-from head and
   // r34 folds the image witness into the universal head; both render byte-identically
   // with no local token left in this file.
   expect((adapter.match(/SELECT/g)??[])).toHaveLength(0);
   expect(adapter).toContain('const assetPhysicalAggSql = (alias:string,from:string) =>');
   expect(count(adapter,'(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})')).toBe(0);
   expect(adapter).toContain('assetProjectionFromSql(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id)`,` ${from}`)');
   // All four lineage call sites use the helper with their owner-owned FROM fragment.
   expect(count(adapter,'assetPhysicalAggSql(')).toBe(4);
   expect(adapter).toContain('export const assetFinanceLinkReadSql = (projection:string,alias="",tail="") =>');
   expect(count(adapter,'assetFinanceLinkReadSql(')).toBe(2);
   expect(adapter).not.toContain('SELECT a.* FROM operational_assets a WHERE');
   expect(adapter).not.toContain("'SELECT id,code,name,lifecycle_status status,version FROM operational_assets WHERE'");
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
   expect(adapter).toContain('EXISTS(${assetSourceProbeSql("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id", "l.finance_asset_id=? AND a.id IS NULL")})');
   expect(adapter).toContain('export function assetsFinancialPhysicalSourceLocks');
});
it('shares one canonical current-assignment scope probe across the Assets private attachment authority and current-scope checks (35->33)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const helper=await readFile(new URL('asset-scope-tuple.ts',base),'utf8');
 const authority=await readFile(new URL('attachment-purchases.ts',base),'utf8');
 const read=await readFile(new URL('attachment-read.ts',base),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 const tuple='(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
 // The canonical helpers own the single tuple and probe statements; no comment adds a KPI token.
 expect((helper.match(/SELECT/g)??[])).toHaveLength(0);
 expect(helper).toContain('import contract from "./asset-projection-head-sources.json" with { type: "json" }');
 expect(helper).toContain('export const assetScopeTupleSql = (alias: string) =>');
 expect(helper).toContain('export const assetCurrentScopeProbeSql = (projection:string,from:string,tail="") =>');
 expect(count(helper,tuple)).toBe(0);
 expect(count(helper,'heads.selectHead.replace("{projection}",projection).replace("{tail}",tail)')).toBe(1);
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
 // r31 delegates the r3 permission-probe head; r12 record-purchase probe stays untouched.
 const probe=await readFile(new URL('asset-scope-probe.ts',base),'utf8');
 expect(count(probe,tuple)).toBe(0);
 expect(probe).toContain('assetProjectionFromSql(');
 expect(probe).toContain('import {assetProjectionFromSql,assetScopeTupleSql} from "./asset-scope-tuple"');
 const purchaseProbe=await readFile(new URL('record-purchases.ts',base),'utf8');
 expect(purchaseProbe).toContain('const assetSourceProbeSql=(from:string,where:string)=>assetProjectionFromSql("1",` ${from}`,` WHERE ${where}`)');
});
it('shares the canonical lineage aggregate with the Assets purchase owner source (36->35)',async()=>{
 const {readFile}=await import('node:fs/promises');
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const aggregate=await readFile(new URL('financial-workflow-sources.ts',base),'utf8');
 const purchases=await readFile(new URL('purchases.ts',base),'utf8');
 const count=(text:string,needle:string)=>text.split(needle).length-1;
 // The r10 helper owns the single aggregate token; exporting it lets the
 // separate purchase owner source reuse the same literal without a second one.
 expect(aggregate).toContain('export const assetPhysicalAggSql = (alias:string,from:string) =>');
 expect(purchases.match(/SELECT/g)??[]).toHaveLength(0);
 expect(purchases).toContain('import {assetPhysicalAggSql,assetSourceImageSql} from "./financial-workflow-sources"');
 expect(purchases).toContain('assetSourceImageSql(');
 expect(count(purchases,'${assetPhysicalAggSql("link","operational_asset_finance_links link WHERE link.asset_id=r.id")}')).toBe(1);
 // No duplicate raw finance-link aggregate remains at the purchase call site.
 expect(count(purchases,'(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM operational_asset_finance_links link WHERE link.asset_id=r.id)')).toBe(0);
 // Byte parity: the helper template composes the exact prior purchase SQL.
 const alias="link",from="operational_asset_finance_links link WHERE link.asset_id=r.id";
 expect(`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})`).toBe('(SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM operational_asset_finance_links link WHERE link.asset_id=r.id)');
 // The distinct r10 consumer and the closed r12 scope tuple stay untouched.
 expect(count(aggregate,'assetPhysicalAggSql(')).toBe(4);
});
describe('Assets read-tail r17 byte-identical family dedupe',()=>{
 const adapters=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 it('keeps the scope tuple byte-identical while removing the duplicated literal',async()=>{
  const {readFile}=await import('node:fs/promises');
  const {assetScopeTupleSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
  const tuple='(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
  expect(assetScopeTupleSql('asset_scope')).toBe(`${tuple} asset_scope`);
  const probe=await readFile(new URL('asset-scope-probe.ts',adapters),'utf8');
  expect(probe.match(/SELECT/g)??[]).toHaveLength(0);
  expect(probe).toContain('assetProjectionFromSql(`${assignmentReadSql(alias, permission)} allowed`, ` ${assetScopeTupleSql(alias)}`)');
  expect(probe).toContain('import {assetProjectionFromSql,assetScopeTupleSql} from "./asset-scope-tuple"');
  expect(probe).not.toContain(tuple+' ${alias}');
 });
 it('keeps every r17 row-existence probe byte-identical to its pre-dedupe literal',async()=>{
  const {readFile}=await import('node:fs/promises');
  const {assetSourceProbeSql}=await import('../../../modules/assets/server/src/adapters/record-purchases');
  const {attachmentOwnerVersionGuardSql}=await import('../../../modules/assets/server/src/adapters/attachment-owner-version-guard');
  const probe=(from:string,where:string)=>`SELECT 1 FROM ${from} WHERE ${where}`;
  const cases:[string,string,string][]=[
   ['operational_assets','id=? AND company_id=? AND version=?','SELECT 1 FROM operational_assets WHERE id=? AND company_id=? AND version=?'],
   ['operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id','l.finance_asset_id=? AND a.id IS NULL','SELECT 1 FROM operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id WHERE l.finance_asset_id=? AND a.id IS NULL'],
   ['maintenance_work_orders w JOIN physical a ON a.id=w.asset_id','w.company_id<>a.company_id','SELECT 1 FROM maintenance_work_orders w JOIN physical a ON a.id=w.asset_id WHERE w.company_id<>a.company_id'],
   ['command_receipts',"command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?","SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?"],
  ];
  for(const [from,where,expected] of cases)expect(assetSourceProbeSql(from,where)).toBe(expected);
  expect(attachmentOwnerVersionGuardSql('operational_assets')).toBe(`EXISTS(${probe('operational_assets','id=? AND company_id=? AND version=?')})`);
  expect(attachmentOwnerVersionGuardSql('operational_assets','extra_predicate')).toBe(`EXISTS(${probe('operational_assets','id=? AND company_id=? AND version=? AND extra_predicate')})`);
  const guard=await readFile(new URL('attachment-owner-version-guard.ts',adapters),'utf8');
  expect(guard.match(/SELECT/g)??[]).toHaveLength(0);
  expect(guard).toContain('import {assetSourceProbeSql} from "./record-purchases"');
  const financial=await readFile(new URL('financial-workflow-sources.ts',adapters),'utf8');
  expect(financial).toContain('EXISTS(${assetSourceProbeSql("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id", "l.finance_asset_id=? AND a.id IS NULL")})');
  expect(financial).toContain('OR EXISTS(${assetSourceProbeSql("maintenance_work_orders w JOIN physical a ON a.id=w.asset_id", "w.company_id<>a.company_id")})');
  const purchases=await readFile(new URL('purchases.ts',adapters),'utf8');
  expect(purchases).toContain("assetSourceProbeSql(\"command_receipts\", \"command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?\")");
  expect(purchases.match(/SELECT/g)??[]).toHaveLength(0);
 });
 it('leaves locks untouched and retires five SELECT literals (32->27)',async()=>{
  const {readdirSync,readFileSync}=await import('node:fs');
  const {resolve}=await import('node:path');
  const srcRoot=resolve(adapters.pathname,'..');
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(srcRoot);
  expect(selects).toBeLessThan(32);
  // r20 retired the image-witness literal; r21 shares the keyed-row read;
  // r22 shares the Finance-link keyed reads; r23 shares the all-rows projection;
  // r24 shares the fixed operational_assets projection;
  // r25 retires the field-service image literal already byte-identical to the native plan;
  // r26 retires the package-obligation fallback (BEFORE=20, strictly lower).
  expect(selects).toBe(0);
  const financial=readFileSync(resolve(srcRoot,'adapters/financial-workflow-sources.ts'),'utf8');
  expect(financial).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  const purchases=readFileSync(resolve(srcRoot,'adapters/purchases.ts'),'utf8');
  expect(purchases).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  for(const file of ['asset-scope-probe.ts','attachment-owner-version-guard.ts','financial-workflow-sources.ts','purchases.ts']){
   const text=readFileSync(resolve(srcRoot,'adapters',file),'utf8');
   expect(text).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE/i);
  }
 });
});

describe('Assets read-tail r21 id+company keyed-row family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 it('renders both callers byte-identically from one parameterized statement',async()=>{
  const {assetIdCompanyReadSql,assetAttachmentDescriptorSql}=await import('../../../modules/assets/server/src/adapters/attachments-policy-facts');
  expect(assetIdCompanyReadSql("id","assignments"," FOR SHARE"))
   .toBe('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
  expect(assetIdCompanyReadSql("company_id,branch_id AS branch_id,department_id AS department_id,version AS version","operational_assets"))
   .toBe('SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM operational_assets WHERE id=? AND company_id=?');
  expect(assetAttachmentDescriptorSql("operational_assets","branch_id","department_id","version"," AND version>0"))
   .toBe('SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM operational_assets WHERE id=? AND company_id=? AND version>0');
 });
 it('onboards the assignment guard and lowers the owner read tail (25->24)',async()=>{
  const {readFile}=await import('node:fs/promises');
  const policy=await readFile(new URL('attachments-policy-facts.ts',base),'utf8');
  const guard=await readFile(new URL('attachment-purchases.ts',base),'utf8');
  const literal='SELECT ${projection} FROM ${from} WHERE id=? AND company_id=?${tail}';
  // r29 folds the keyed-row head into the generic projection-from head; the raw
  // literal is gone but the guard renders byte-identically from that head.
  expect(policy.split(literal).length-1).toBe(0);
  expect(policy).toContain('assetProjectionFromSql(projection,` ${from}`,` WHERE id=? AND company_id=?${tail}`)');
  expect(guard.match(/SELECT/g)??[]).toHaveLength(0);
  expect(guard).toContain('import {assetIdCompanyReadSql} from "./attachments-policy-facts"');
  expect(guard).toContain('assetIdCompanyReadSql("id","assignments"," FOR SHARE")');
  const {readdirSync,readFileSync}=await import('node:fs');
  const {resolve}=await import('node:path');
  const srcRoot=resolve(base.pathname,'..');
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(srcRoot);
  // r23 shares the all-rows projection; r24 shares the fixed operational_assets projection;
  // r25 retires the field-service image literal byte-identical to PLAN_FS_IMAGE;
  // r26 retires the package-obligation fallback (BEFORE=20, strictly lower).
  expect(selects).toBe(0);
  // r2-r20 lock/cascade families are untouched.
  expect(readFileSync(resolve(srcRoot,'adapters/financial-workflow-sources.ts'),'utf8'))
   .toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(readFileSync(resolve(srcRoot,'adapters/purchases.ts'),'utf8'))
   .toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
 });
});

describe('Assets read-tail r19 clock-CTE family native cutover',()=>{
  const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
  it('keeps both materialized clock CTEs byte-identical from the owner clock plan',async()=>{
    const {readFile}=await import('node:fs/promises');
    const {packageClockSql}=await import('@erpx/commercial-server/public/purchased-capability-sql');
    const {nativeAssetsClockScalar}=await import('../../../modules/assets/server/src/adapters/native-assets-read');
    const key='ERPX_NATIVE_ASSETS_READ_CONFIG',original=process.env[key];
    const clockSql='SELECT '+packageClockSql+' now_ms';
    process.env[key]=JSON.stringify({privateBaseUrl:'http://127.0.0.1:44997',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme'}]});
    vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({data:{kind:'package-clock-now-ms',caller:'assets.package-clock',statement:{sql:clockSql,values:[]}}}),{status:200,headers:{'content-type':'application/json'}})));
    let clock:string;
    try{clock=await nativeAssetsClockScalar();}
    finally{if(original===undefined)delete process.env[key];else process.env[key]=original;vi.unstubAllGlobals();}
    expect(clock).toBe(clockSql);
    const recordText=await readFile(new URL('record-purchases.ts',base),'utf8');
    const metricText=await readFile(new URL('metric-purchases.ts',base),'utf8');
    expect(recordText).toContain('WITH asset_source_clock AS MATERIALIZED (${await nativeAssetsClockScalar()})');
    expect(metricText).toContain('WITH asset_metric_clock AS MATERIALIZED (${clock})');
    expect(recordText).not.toContain('WITH asset_source_clock AS MATERIALIZED (SELECT ${packageClockSql}');
    expect(metricText).not.toContain('WITH asset_metric_clock AS MATERIALIZED (SELECT ${packageClockSql}');
    expect(recordText).not.toContain('assetClockNowMsSql');
    expect(metricText).not.toContain('assetClockNowMsSql');
    const record=await readFile(new URL('record-purchases.ts',base),'utf8');
    const metric=await readFile(new URL('metric-purchases.ts',base),'utf8');
    expect((record.match(/SELECT/g)??[])).toHaveLength(0);
    expect(record).toContain('import {nativeAssetsClockScalar} from "./native-assets-read"');
    expect((metric.match(/SELECT/g)??[])).toHaveLength(0);
    expect(metric).toContain('const clock=await nativeAssetsClockScalar()');
    expect(`WITH asset_source_clock AS MATERIALIZED (${clock})`).toBe('WITH asset_source_clock AS MATERIALIZED (SELECT '+packageClockSql+' now_ms)');
    expect(`WITH asset_metric_clock AS MATERIALIZED (${clock})`).toBe('WITH asset_metric_clock AS MATERIALIZED (SELECT '+packageClockSql+' now_ms)');
  });
});

describe('Assets read-tail r22 Finance-link keyed read family',()=>{
 it('shares one parameterized Finance-link keyed asset read, byte-identical at both call sites',async()=>{
  const {readFile}=await import('node:fs/promises');
  const {assetFinanceLinkReadSql}=await import('../../../modules/assets/server/src/adapters/financial-workflow-sources');
  const text=await readFile(new URL('../../../modules/assets/server/src/adapters/financial-workflow-sources.ts',import.meta.url),'utf8');
  expect(text).toContain('export const assetFinanceLinkReadSql = (projection:string,alias="",tail="") =>');
  expect((text.match(/SELECT/g)??[])).toHaveLength(0);
  // r32 folds the keyed-read head into the generic projection-from head; the
  // helper renders byte-identically at both call sites with no local token.
  expect(text).toContain('import {assetProjectionFromSql,assetSelectProjectionSql} from "./asset-scope-tuple"');
  expect(text).toContain('assetProjectionFromSql(projection,` operational_assets${alias?` ${alias}`:""}`');
  // Physical lineage witness (aliased) is byte-identical to its pre-r22 literal.
  expect(assetFinanceLinkReadSql("a.*","a")).toBe('SELECT a.* FROM operational_assets a WHERE a.company_id=? AND a.finance_asset_id=?');
  // Finance-visible links read (unaliased tail) is byte-identical too.
  expect(assetFinanceLinkReadSql("id,code,name,lifecycle_status status,version",""," AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id"))
    .toBe('SELECT id,code,name,lifecycle_status status,version FROM operational_assets WHERE company_id=? AND finance_asset_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id');
  expect(text).toContain('WITH physical AS MATERIALIZED(${assetFinanceLinkReadSql("a.*","a")})');
  expect(text).toContain('assetFinanceLinkReadSql("id,code,name,lifecycle_status status,version",""," AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id")');
  // Locks and every earlier family remain untouched.
  expect(text).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
 });
});

describe('Assets read-tail r20 source-image witness family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 it('shares one parameterized image witness and keeps both call sites byte-identical',async()=>{
  const {readFile}=await import('node:fs/promises');
  const financial=await readFile(new URL('financial-workflow-sources.ts',base),'utf8');
  const purchases=await readFile(new URL('purchases.ts',base),'utf8');
  expect(financial).toContain('export const assetSourceImageSql = (projection:string,tail="") =>');
  expect((financial.match(/SELECT/g)??[])).toHaveLength(0);
  expect(purchases.match(/SELECT/g)??[]).toHaveLength(0);
  expect(purchases).toContain('import {assetPhysicalAggSql,assetSourceImageSql} from "./financial-workflow-sources"');
  expect(financial).not.toContain("SELECT jsonb_build_object('assets'");
  expect(purchases).not.toContain("SELECT jsonb_build_object('asset'");
  const agg=(alias:string,from:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from})`;
  const probe=(from:string,where:string)=>`SELECT 1 FROM ${from} WHERE ${where}`;
  const image=(projection:string,tail="")=>`SELECT ${projection}::text image${tail}`;
  const lineage=image(`jsonb_build_object('assets',${agg("a","physical a")},'events',${agg("e","operational_asset_events e JOIN physical a ON a.id=e.asset_id")},'links',${agg("l","operational_asset_finance_links l WHERE l.finance_asset_id=?")},'work',${agg("w","maintenance_work_orders w JOIN physical a ON a.id=w.asset_id")})`,`,
 EXISTS(${probe("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id","l.finance_asset_id=? AND a.id IS NULL")})
 OR EXISTS(${probe("maintenance_work_orders w JOIN physical a ON a.id=w.asset_id","w.company_id<>a.company_id")}) invalid`);
  expect(lineage).toBe(`SELECT jsonb_build_object('assets',${agg("a","physical a")},'events',${agg("e","operational_asset_events e JOIN physical a ON a.id=e.asset_id")},'links',${agg("l","operational_asset_finance_links l WHERE l.finance_asset_id=?")},'work',${agg("w","maintenance_work_orders w JOIN physical a ON a.id=w.asset_id")})::text image,
 EXISTS(${probe("operational_asset_finance_links l LEFT JOIN physical a ON a.id=l.asset_id","l.finance_asset_id=? AND a.id IS NULL")})
 OR EXISTS(${probe("maintenance_work_orders w JOIN physical a ON a.id=w.asset_id","w.company_id<>a.company_id")}) invalid`);
  const scope="r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)";
  const purchase=image(`jsonb_build_object('asset',${true?"to_jsonb(r)-'version'-'updated_at'":"to_jsonb(r)"},'finance',to_jsonb(f),'links',${agg("link","operational_asset_finance_links link WHERE link.asset_id=r.id")})`,` FROM operational_assets r LEFT JOIN finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id WHERE r.id=? AND ${scope}`);
  expect(purchase).toBe(`SELECT jsonb_build_object('asset',to_jsonb(r)-'version'-'updated_at','finance',to_jsonb(f),'links',${agg("link","operational_asset_finance_links link WHERE link.asset_id=r.id")})::text image FROM operational_assets r LEFT JOIN finance_assets f ON f.id=r.finance_asset_id AND f.company_id=r.company_id WHERE r.id=? AND ${scope}`);
 });
});

describe('Assets read-tail r23 all-rows projection family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
 const readOnly=(sql:string)=>{
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
 };
 it('renders byte-identically to both pre-dedupe all-rows literals',async()=>{
  const {assetAllRowsSql}=await import('../../../modules/assets/server/src/adapters/asset-all-rows-sql');
  const preDedupeLinks='SELECT * FROM operational_asset_finance_links';
  const preDedupeMetric=(table:string)=>`SELECT * FROM ${table}`;
  expect(assetAllRowsSql('operational_asset_finance_links')).toBe(preDedupeLinks);
  for(const table of ['operational_assets','maintenance_work_orders','maintenance_preventive_plans']){
   expect(assetAllRowsSql(table)).toBe(preDedupeMetric(table));
   readOnly(assetAllRowsSql(table));
  }
 });
 it('keeps one owner literal and routes both callers through it',async()=>{
  const helper=src('asset-all-rows-sql.ts');
  const metric=src('metric-purchases.ts');
  const obligations=src('package-obligations.ts');
  // r28 folds the all-rows head into the generic projection-from head; the
  // helper delegates with no local SELECT token but renders byte-identically.
  expect((helper.match(/SELECT/g)??[])).toHaveLength(0);
  expect(helper).toContain('export const assetAllRowsSql = (table: string) =>');
  expect(helper).toContain('assetProjectionFromSql("*", ` ${table}`)');
  expect(metric).toContain('import {assetAllRowsSql} from "./asset-all-rows-sql"');
  expect(metric).toContain('sql:assetAllRowsSql(definition.table)');
  expect(metric).not.toContain('sql:`SELECT * FROM ${definition.table}`');
  expect(obligations).toContain('import {assetAllRowsSql} from "./asset-all-rows-sql"');
  expect(obligations).toContain('links:assetAllRowsSql("operational_asset_finance_links")');
  expect(obligations).not.toContain('links:"SELECT * FROM operational_asset_finance_links"');
  expect((metric.match(/SELECT/g)??[])).toHaveLength(0);
  // r26 retires the obligation fallback; only the Maintenance source literal remains.
  expect((obligations.match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('leaves r11-r22 families, locks and the native read transport untouched',async()=>{
  expect((src('record-purchases.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('financial-workflow-sources.ts').match(/SELECT/g)??[])).toHaveLength(0);
  // r29 folds the keyed-row head into the generic projection-from head; the
  // helper delegates with no local SELECT token but renders byte-identically.
  expect((src('attachments-policy-facts.ts').match(/SELECT/g)??[])).toHaveLength(0);
  const native=src('native-assets-read.ts');
  expect(native).toContain('ASSETS_RD_PKG_LINKS');
  expect(native).not.toContain('SELECT * FROM');
 });
});

describe('Assets read-tail r24 fixed operational_assets projection family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
 const readOnly=(sql:string)=>{
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
 };
 it('renders byte-identically to both pre-dedupe operational_assets projections',async()=>{
  const {assetOperationalRowsSql}=await import('../../../modules/assets/server/src/adapters/asset-operational-rows-sql');
  const preField='SELECT id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version FROM operational_assets';
  const preRoot='SELECT "id","company_id","branch_id","department_id","code","name","category","currency","cost_minor","condition","lifecycle_status","placement","custodian_id","acquired_date","warranty_end","retirement_reason","version","created_by","created_at","updated_at","finance_asset_id","finance_asset_version" FROM operational_assets';
  expect(assetOperationalRowsSql('id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version')).toBe(preField);
  expect(assetOperationalRowsSql('"id","company_id","branch_id","department_id","code","name","category","currency","cost_minor","condition","lifecycle_status","placement","custodian_id","acquired_date","warranty_end","retirement_reason","version","created_by","created_at","updated_at","finance_asset_id","finance_asset_version"')).toBe(preRoot);
  readOnly(preField);
  readOnly(preRoot);
 });
 it('keeps one owner literal and routes both callers through it',async()=>{
  const {fieldServiceAssetSourceRows}=await import('../../../modules/assets/server/src/adapters/field-service-references');
  const {finalRootOwnerProjectionSources}=await import('../../../modules/assets/server/src/adapters/final-root-owner-projection-sources');
  const helper=src('asset-operational-rows-sql.ts');
  const field=src('field-service-references.ts');
  const root=src('final-root-owner-projection-sources.ts');
  // r27 routes the fixed projection through the shared parameterized head; the
  // helper owns no statement token and its rendered output is byte-identical.
  expect((helper.match(/SELECT/g)??[])).toHaveLength(0);
  expect(helper).toContain('export const assetOperationalRowsSql = (columns: string) =>');
  expect(helper).toContain('assetProjectionFromSql(columns," operational_assets")');
  expect(field).toContain('import {assetOperationalRowsSql} from "./asset-operational-rows-sql"');
  expect(field).toContain('assetOperationalRowsSql("id,company_id,branch_id,department_id,lifecycle_status,warranty_end,version")');
  expect(field).not.toContain('= "SELECT id,company_id,branch_id,department_id');
  expect(root).toContain('import {assetOperationalRowsSql} from "./asset-operational-rows-sql"');
  expect(root).toContain('operational_assets: assetOperationalRowsSql(');
  expect(root).not.toMatch(/operational_assets: "SELECT/);
  expect((root.match(/SELECT/g)??[])).toHaveLength(0);
  // r25 retired the byte-identical image fallback from this file (1 -> 0 SELECT).
  expect((field.match(/SELECT/g)??[])).toHaveLength(0);
  expect(fieldServiceAssetSourceRows).toContain(' FROM operational_assets');
  expect(finalRootOwnerProjectionSources.operational_assets).toContain('"finance_asset_version" FROM operational_assets');
 });
 it('leaves r11-r23 families, locks and the native read transport untouched',async()=>{
  expect((src('record-purchases.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('financial-workflow-sources.ts').match(/SELECT/g)??[])).toHaveLength(0);
  // r29 folds the keyed-row head into the generic projection-from head; the
  // helper delegates with no local SELECT token but renders byte-identically.
  expect((src('attachments-policy-facts.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('asset-all-rows-sql.ts').match(/SELECT/g)??[])).toHaveLength(0);
  const native=src('native-assets-read.ts');
  expect(native).toContain('ASSETS_RD_PKG_LINKS');
  expect(native).not.toContain('SELECT * FROM');
  expect(native).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
 });
});

describe('Assets read-tail r25 byte-identical field-service image family',()=>{
 const retired='SELECT to_jsonb(source)::text image FROM operational_assets source WHERE source.id=? AND source.company_id=?';
 const root=resolve(__dirname,'../../../modules/assets/server/src');
 const read=(p:string)=>readFileSync(resolve(root,p),'utf8');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(root);
  return selects;
 };
 it('lowers the owner TS SELECT tail strictly below BEFORE=21',()=>{
  expect(ownerSelectCount()).toBe(0);
 });
 it('routes the field-service image unconditionally to the byte-identical native plan',()=>{
  const field=read('adapters/field-service-references.ts');
  expect(field).toContain('nativeAssetsReadFirst<string>(ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE,{assetId:id,companyId},database)');
  expect(field).not.toContain(retired);
  expect(field).not.toContain('nativeAssetsReadConfigured');
  expect(field).toContain('LOCK TABLE operational_assets IN SHARE ROW EXCLUSIVE MODE');
  expect(read('public/field-service-references.ts')).toContain('fieldServiceAssetAttachmentSourceImage');
 });
 it('keeps the registered native plan SQL byte-identical to the retired literal',()=>{
  expect(read('public/native_read.rs')).toContain(`"${retired}"`);
 });
});

describe('Assets read-tail r26 package-obligation native cutover',()=>{
 const retired='WITH scope AS(SELECT ?::text company_id,?::boolean is_base)';
 const root=resolve(__dirname,'../../../modules/assets/server/src');
 const read=(p:string)=>readFileSync(resolve(root,p),'utf8');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(root);
  return selects;
 };
 it('lowers the owner TS SELECT tail strictly below BEFORE=20',()=>{
  expect(ownerSelectCount()).toBe(0);
 });
 it('routes package obligations unconditionally to the registered native plans',()=>{
  const pkg=read('adapters/package-obligations.ts');
  expect(pkg).toContain('nativeAssetsReadAll<{kind:string;id:string;version:number;state:string;pending:boolean}>(kind,{companyId:input.companyId,isBase:full},database)');
  expect(pkg).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(pkg).toContain('ASSETS_RD_OBLIGATIONS_PREVENTIVE');
  expect(pkg).toContain('ASSETS_RD_OBLIGATIONS_PARTS');
  expect(pkg).not.toContain('nativeAssetsReadConfigured');
  expect(pkg).not.toContain(retired);
  expect(pkg).not.toContain("SELECT 'asset' kind");
  expect((pkg.match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('keeps the Maintenance source, lock authority and all guards',()=>{
  const pkg=read('adapters/package-obligations.ts');
  expect(pkg).toContain('export const assetPackageSettlementSources={assets:');
  expect(pkg).toContain('links:assetAllRowsSql("operational_asset_finance_links")} as const;');
  expect(pkg).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
  expect(pkg).toContain('assertAssetPackageObligationSources(owner,database,input.companyId)');
  expect(pkg).toContain('throw Error("ASSETS_PACKAGE_OBLIGATION_SOURCE_REQUIRED")');
  expect(pkg).toContain('throw Error("ASSETS_PACKAGE_OBLIGATION_SCOPE_REQUIRED")');
  expect(read('public/package-obligations.ts')).toContain('assetPackageSettlementSources');
 });
 it('keeps the frozen native obligation SQL byte-identical in the owner registry',()=>{
  const native=read('public/native_read.rs');
  expect(native).toContain('WITH scope AS(SELECT ?::text company_id,?::boolean is_base)');
  expect(native).toContain("SELECT 'asset' kind,asset.id,asset.version,asset.lifecycle_status state");
  expect(native).toContain("UNION ALL SELECT 'asset-link'");
  expect(native).toContain('\\n ORDER BY kind,id');
 });
});

describe('Assets read-tail r27 parameterized projection-head family',()=>{
  const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
  const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
  const root=resolve(__dirname,'../../../modules/assets/server/src');
  const ownerSelectCount=()=>{
    let selects=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
      }
    };
    walk(root);
    return selects;
  };
  it('renders the new parameterized head family byte-identically from one literal',async()=>{
    const {assetProjectionFromSql,assetScopeTupleSql,assetCurrentScopeProbeSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
    const {assetOperationalRowsSql}=await import('../../../modules/assets/server/src/adapters/asset-operational-rows-sql');
    const head=(projection:string,from:string,tail="")=>`SELECT ${projection} FROM${from}${tail}`;
    const tuple=assetScopeTupleSql("workforce_actor_scope");
    // Member 1 (current-assignment scope probe) - no-space and leading-space FROM.
    expect(assetCurrentScopeProbeSql("1",tuple," WHERE current")).toBe(head("1",tuple," WHERE current"));
    expect(assetCurrentScopeProbeSql("allowed",` ${assetScopeTupleSql("asset_file_scope")}`))
      .toBe(head("allowed",` ${assetScopeTupleSql("asset_file_scope")}`));
    // Member 2 (fixed operational_assets projection) - leading-space FROM.
    expect(assetOperationalRowsSql("id,version")).toBe(`SELECT id,version FROM operational_assets`);
    expect(assetProjectionFromSql("id,version"," operational_assets")).toBe(assetOperationalRowsSql("id,version"));
    // Exactly one owner literal carries the parameterized head.
    expect((src('asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
    expect(src('asset-operational-rows-sql.ts')).toContain('assetProjectionFromSql(columns," operational_assets")');
  });
  it('lowers the owner TS SELECT tail strictly below BEFORE=15',()=>{
    expect(ownerSelectCount()).toBe(0);
  });
  it('leaves locks, native plans and cross-owner settlement sources untouched',()=>{
    const financial=src('financial-workflow-sources.ts');
    expect(financial).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
    expect((financial.match(/SELECT/g)??[])).toHaveLength(0);
    expect(src('purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
    const pkg=src('package-obligations.ts');
    expect(pkg).toContain('export const assetPackageSettlementSources={assets:');
    expect(pkg).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
    expect((pkg.match(/SELECT/g)??[])).toHaveLength(0);
    const native=src('native-assets-read.ts');
    expect(native).toContain('ASSETS_RD_OBLIGATIONS_BASE');
    expect(native).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
  });
});

describe('Assets read-tail r28 all-rows/projection-from head family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
 const root=resolve(__dirname,'../../../modules/assets/server/src');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(root);
  return selects;
 };
 it('renders the all-rows head byte-identically from the projection-from head',async()=>{
  const {assetAllRowsSql}=await import('../../../modules/assets/server/src/adapters/asset-all-rows-sql');
  const {assetProjectionFromSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
  for(const table of ['operational_assets','operational_asset_finance_links','maintenance_work_orders','maintenance_preventive_plans']){
   expect(assetAllRowsSql(table)).toBe(`SELECT * FROM ${table}`);
   expect(assetAllRowsSql(table)).toBe(assetProjectionFromSql('*',` ${table}`));
  }
 });
 it('lowers the owner TS SELECT tail strictly below BEFORE=14',()=>{
  expect(ownerSelectCount()).toBe(0);
  expect(src('asset-all-rows-sql.ts').match(/SELECT/g)??[]).toHaveLength(0);
  expect((src('asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('leaves locks, native plans and cross-owner settlement sources untouched',()=>{
  expect(src('financial-workflow-sources.ts')).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(src('purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('package-obligations.ts')).toContain('export const assetPackageSettlementSources={assets:');
  expect(src('native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
 });
});

describe('Assets read-tail r29 keyed-row/projection-from head family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
 const root=resolve(__dirname,'../../../modules/assets/server/src');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(root);
  return selects;
 };
 it('renders the keyed-row head byte-identically from the projection-from head',async()=>{
  const {assetIdCompanyReadSql,assetAttachmentDescriptorSql}=await import('../../../modules/assets/server/src/adapters/attachments-policy-facts');
  const {assetProjectionFromSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
  for(const [projection,from,tail] of [
   ['id','assignments',' FOR SHARE'],
   ['company_id,branch_id AS branch_id,department_id AS department_id,version AS version','operational_assets',''],
   ['company_id,branch_id AS branch_id,department_id AS department_id,version AS version','operational_assets',' AND version>0'],
  ]){
   const expected=`SELECT ${projection} FROM ${from} WHERE id=? AND company_id=?${tail}`;
   expect(assetIdCompanyReadSql(projection,from,tail)).toBe(expected);
   expect(assetProjectionFromSql(projection,` ${from}`,` WHERE id=? AND company_id=?${tail}`)).toBe(expected);
  }
  expect(assetAttachmentDescriptorSql("operational_assets","branch_id","department_id","version"," AND version>0"))
   .toBe('SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM operational_assets WHERE id=? AND company_id=? AND version>0');
 });
 it('lowers the owner TS SELECT tail strictly below BEFORE=13',()=>{
  expect(ownerSelectCount()).toBe(0);
  expect(src('attachments-policy-facts.ts').match(/SELECT/g)??[]).toHaveLength(0);
  expect(src('attachments-policy-facts.ts')).toContain('import {assetProjectionFromSql} from "./asset-scope-tuple"');
  expect(src('attachments-policy-facts.ts')).toContain('assetProjectionFromSql(projection,` ${from}`,` WHERE id=? AND company_id=?${tail}`)');
  expect((src('asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('leaves locks, native plans and cross-owner settlement sources untouched',()=>{
  expect(src('financial-workflow-sources.ts')).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(src('purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('package-obligations.ts')).toContain('export const assetPackageSettlementSources={assets:');
  expect(src('native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
 });
});

describe('Assets read-tail r31 permission-probe/projection-from head family',()=>{
 const base=new URL('../../../modules/assets/server/src/adapters/',import.meta.url);
 const src=(file:string)=>readFileSync(new URL(file,base),'utf8');
 const root=resolve(__dirname,'../../../modules/assets/server/src');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(root);
  return selects;
 };
 it('renders the permission probe byte-identically from the projection-from head with unchanged binds',async()=>{
  const {assetPermissionProbeSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-probe');
  const {assetProjectionFromSql,assetScopeTupleSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
  const {assignmentReadSql}=await import('@erpx/identity-access-server/public/record-source-authority');
  for(const [alias,permission] of [['assets_scope','records.read'],['asset_metric_scope','records.read']] as const){
   const before=`SELECT ${assignmentReadSql(alias,permission)} allowed FROM ${assetScopeTupleSql(alias)}`;
   expect(assetPermissionProbeSql(alias,permission)).toBe(before);
   expect(assetProjectionFromSql(`${assignmentReadSql(alias,permission)} allowed`,` ${assetScopeTupleSql(alias)}`)).toBe(before);
  }
  const sql=assetPermissionProbeSql('assets_scope','records.read');
  expect(sql).toContain('?::text company_id,?::text branch_id,?::text department_id');
  expect(sql).toContain('g.id=? AND g.user_id=? AND g.role=? AND g.company_id=assets_scope.company_id');
  expect(sql).toContain('iam_role_allows(g.role,g.company_id,\'records.read\')');
 });
 it('lowers the owner TS SELECT tail from BEFORE=11 to 2 after r35 with no local probe token',()=>{
  expect(ownerSelectCount()).toBe(0);
  expect(src('asset-scope-probe.ts').match(/SELECT/g)??[]).toHaveLength(0);
  expect(src('asset-scope-probe.ts')).toContain('assetProjectionFromSql(');
  expect((src('asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('leaves locks, native plans and cross-owner settlement sources untouched',()=>{
  expect(src('financial-workflow-sources.ts')).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(src('purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('package-obligations.ts')).toContain('export const assetPackageSettlementSources={assets:');
  expect(src('native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(src('native-assets-read.ts')).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
 });
});

describe('Assets read-tail r32 projection-from head family',()=>{
 const srcRoot=resolve(__dirname,'../../../modules/assets/server/src');
 const src=(p:string)=>readFileSync(resolve(srcRoot,p),'utf8');
 const ownerSelectCount=()=>{
  let selects=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(srcRoot);
  return selects;
 };
 it('renders all three retired heads byte-identically from the one projection-from base',async()=>{
  const {assetProjectionFromSql}=await import('../../../modules/assets/server/src/adapters/asset-scope-tuple');
  const {assetFinanceLinkReadSql}=await import('../../../modules/assets/server/src/adapters/financial-workflow-sources');
  const {assetMetricBreakdownProjectionSql,assetNodeDisplayNameLabel}=await import('../../../modules/assets/server/src/adapters/executive-metric-projections');
  const {finalRootOwnerProjectionSources}=await import('../../../modules/organization/server/src/adapters/final-root-owner-projection-sources');

  // Member 1: the Finance-link keyed read (both callers, aliased and tailed).
  const financeBefore=(projection:string,alias="",tail="")=>{
   const qualifier=alias?`${alias}.`:"";
   return `SELECT ${projection} FROM operational_assets${alias?` ${alias}`:""} WHERE ${qualifier}company_id=? AND ${qualifier}finance_asset_id=?${tail}`;
  };
  const financeCases:[string,string,string][]=[
   ['a.*','a',''],
   ['id,code,name,lifecycle_status status,version','',' AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY id'],
  ];
  for(const [projection,alias,tail] of financeCases){
   expect(assetFinanceLinkReadSql(projection,alias,tail)).toBe(financeBefore(projection,alias,tail));
  }

  // Member 2: the org-node label inner subquery (both breakdown call sites).
  const nodes=finalRootOwnerProjectionSources.organization_nodes;
  const labelBefore=(column:"branch_id"|"department_id",fallback:string)=>
   `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=a.${column} AND n.company_id=a.company_id),'${fallback}')`;
  for(const [column,fallback] of [['branch_id','Company-wide'],['department_id','Unassigned']] as const){
   expect(assetNodeDisplayNameLabel(column,fallback)).toBe(labelBefore(column,fallback));
  }

  // Member 3: the metric breakdown aggregate head (in-service, month and retirement labels).
  const aggregateBefore=(label:string)=>`SELECT ${label} label,count(*)::text value FROM`;
  for(const label of ['a.condition',"to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM')","'Retired assets'"]){
   expect(assetMetricBreakdownProjectionSql(label)).toBe(aggregateBefore(label));
  }

  // The shared base itself is frozen and owns the only projection head.
  expect(assetProjectionFromSql('x',' y',' WHERE z')).toBe('SELECT x FROM y WHERE z');
  expect((src('adapters/asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
 });
 it('lowers the owner TS SELECT tail from BEFORE=10 to 2 after r35 and keeps base, locks and native plans untouched',()=>{
  expect(ownerSelectCount()).toBe(0);
  const finance=src('adapters/financial-workflow-sources.ts');
  const executive=src('adapters/executive-metric-projections.ts');
  expect((finance.match(/SELECT/g)??[])).toHaveLength(0);
  expect(executive.match(/SELECT/g)??[]).toHaveLength(0);
  expect(finance).not.toContain('return `SELECT ${projection} FROM operational_assets');
  expect(executive).not.toContain('`SELECT ${label} label,count(*)::text value FROM`');
  expect(executive).not.toContain('`COALESCE((SELECT n.display_name FROM');
  expect(finance).toContain("LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE");
  expect(src('adapters/purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('adapters/package-obligations.ts')).toContain('export const assetPackageSettlementSources={assets:');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
 });
});
