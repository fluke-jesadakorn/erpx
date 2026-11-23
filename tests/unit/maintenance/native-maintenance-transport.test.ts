import {describe,it,expect,vi,beforeEach,afterEach} from 'vitest';
import {
  createNativeMaintenanceClient,
  nativeMaintenanceConfiguration,
  type NativeMaintenancePageContext,
} from '../../../packages/feature-assets/src/adapters/server/native-maintenance';
import { nativeMaintenanceAvailable } from '../../../packages/feature-assets/src/adapters/server/native-maintenance-page';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'})}));
vi.mock('@erpx/server-identity_access/public/iam-permissions',()=>({requirePermission:()=>{}}));
vi.mock('../../../packages/server-runtime/src/public/runtime',()=>({holdingResources:async()=>({write:{prepare:()=>{throw Error('Legacy Maintenance source SQL must not execute');}}})}));
/** Exercise the Next route with its server dependencies replaced, so the
 * native-only selection and closed failure policy are directly observable. */
const routeMocks=vi.hoisted(()=>({
  nativeCommand:vi.fn(),
  actor:{userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding' as const},authority:{revision:7,assignmentVersion:3},modules:['maintenance'],iam:{permissions:['records.read','records.write','records.review']}},
}));
vi.mock('@erpx/feature-assets/adapters/server/native-maintenance-owned',()=>({
  ownedMaintenanceCommand:routeMocks.nativeCommand,
  ownedMaintenanceWorkspace:vi.fn(),
  ownedMaintenanceDetail:vi.fn(),
}));
vi.mock('@erpx/server-identity_access/public/authorization',()=>({
  requireWorkContext:async()=>routeMocks.actor,
}));
vi.mock('@erpx/server-manage_erpx/public/public-origin',()=>({
  requestPublicOrigin:()=> 'https://acme.example.com',
}));
vi.mock('@erpx/feature-work_projection/lib/destination-links',()=>({
  canonicalDestinationLinks:(value:unknown)=>value,
}));
vi.mock('@erpx/server-runtime/public/http',()=>({
  ApiError:class ApiError extends Error{
    constructor(readonly status:number,readonly code:string,message:string){super(message);this.name='ApiError';}
  },
  errorResponse:(error:unknown)=>{
    const value=error as {status?:number;code?:string;message?:string};
    return Response.json({error:{code:value.code??'INTERNAL',message:value.message??'Request failed.'}},{status:value.status??500});
  },
}));
const maintenanceRoute=await import('../../../packages/feature-assets/src/routes/api/v1/holdings/[holdingSlug]/maintenance/route');

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44992',
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
const actor:NativeMaintenancePageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['maintenance'],
  iam:{permissions:['records.read','records.write','records.review']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',assignmentVersion:3,revision:7,
  role:'owner',assignmentScopeKind:'holding',modules:['maintenance'],
  permissions:['records.read','records.write','records.review'],expiresAt:9_007_199_254_740_991};
const work={id:'work',company_id:'company',branch_id:'branch',department_id:null,asset_id:'asset',
  title:'Synthetic work',priority:'normal',status:'in_progress',owner_id:'user',scheduled_at:null,
  downtime_started_at:null,downtime_ended_at:null,blocked_reason:null,completion_evidence:null,
  resulting_condition:null,version:1,created_by:'user',created_at:0,updated_at:0,
  asset_code:'A-1',asset_name:'Asset One',owner_name:'Owner',owner_eligible:1,can_review:1};
const asset={id:'asset',code:'A-1',name:'Asset One',version:1,condition:'good',lifecycle_status:'in_service'};
const workspace={items:[work],assets:[asset],people:[{id:'user',name:'Owner'}],
  nodes:[{id:'branch',kind:'branch',display_name:'Branch',branch_id:null}],canCreate:true};
const event={id:'event-1',work_order_id:'work',version:1,action:'create',actor_id:'user',
  reason:'Synthetic work opened',status:'in_progress',owner_id:'user',scheduled_at:null,
  downtime_started_at:null,downtime_ended_at:null,created_at:0};
const detail={...workspace,work,history:[event],consumptions:[],spares:{stock:[],lots:[]}};
const plan={id:'plan',company_id:'company',branch_id:'branch',department_id:null,asset_id:'asset',
  title:'Synthetic plan',priority:'normal',owner_id:'user',interval_days:30,time_zone:'Asia/Bangkok',
  first_due_local:'2026-11-10T09:00',next_due_local:'2026-11-10T09:00',next_due_at:1_800_000_000_000,
  lead_days:7,status:'draft',version:1,created_by:'user',edited_by:'user',approved_by:null,
  approved_at:null,created_at:0,updated_at:0,asset_code:'A-1',asset_name:'Asset One',
  owner_name:'Owner',owner_eligible:1};
const planDetail={plan,actorId:'user',canWrite:true,canReview:true,assetVersion:1,
  people:[{id:'user',name:'Owner'}],history:[],occurrences:[]};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const readInput={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};

describe('native Maintenance transport',()=>{
 it('forwards the configured Holding, session and current CAS to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  const result=await client.workspace(incoming(),actor,readInput);
  expect(result.items[0]).toMatchObject({id:'work',asset_code:'A-1',status:'in_progress'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
 });

 it('reads work detail, plan list and plan detail with branch selectors',async()=>{
  const scoped={...actor,scope:{kind:'branch' as const,branchSlug:'north'}};
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:detail,context:{...context,branchSlug:'north'}}))
    .mockResolvedValueOnce(ok({data:[plan],context:{...context,branchSlug:'north'}}))
    .mockResolvedValueOnce(ok({data:planDetail,context:{...context,branchSlug:'north'}}));
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  const d=await client.detail(incoming(),scoped,{...readInput,id:'work',branchSlug:'north'});
  expect(d.work.id).toBe('work');
  expect(d.history).toHaveLength(1);
  const plans=await client.plans(incoming(),scoped,{...readInput,branchSlug:'north'});
  expect(plans[0]).toMatchObject({id:'plan',asset_code:'A-1'});
  const p=await client.planDetail(incoming(),{...scoped,authority:actor.authority},{...readInput,id:'plan',branchSlug:'north'});
  expect(p.plan.status).toBe('draft');
  expect(transport.mock.calls[0][1].toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance?id=work');
  expect(transport.mock.calls[1][1].toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance/plans');
  expect(transport.mock.calls[2][1].toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance/plans?id=plan');
  expect(transport.mock.calls[0][2].get('x-erpx-workspace-branch')).toBe('north');
 });

 it('sends one keyed command with origin and returns the owner acknowledgement',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'work',version:2,status:'completed'},context}));
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  const result=await client.command(incoming(),actor,{
    holdingSlug:'acme',
    body:{id:'work',action:'complete',expectedVersion:1,resultingCondition:'good',evidence:'Repaired',reason:'Completed repairs'},
    idempotencyKey:'maintenance-command-1',
    origin:'https://acme.example.com',
  });
  expect(result).toEqual({id:'work',version:2,status:'completed'});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('idempotency-key')).toBe('maintenance-command-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body)).toMatchObject({id:'work',action:'complete',expectedVersion:1});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'MAINTENANCE_OWNER_REQUIRED',message:'The current owner or an eligible reviewer must perform this action.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{id:'work',action:'start',expectedVersion:1,reason:'Start work'},
    idempotencyKey:'maintenance-command-2',origin:'https://acme.example.com',
  })).rejects.toMatchObject({
    code:'MAINTENANCE_OWNER_REQUIRED',status:403,
    message:'The current owner or an eligible reviewer must perform this action.',
  });
 });

 it('maps a configured native unavailability to a closed command failure',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'MAINTENANCE_COMMAND_UNAVAILABLE',message:'Native Maintenance commands are not enabled yet.'}}),
    {status:503,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{id:'work',action:'block',expectedVersion:1,reason:'Waiting for parts'},
    idempotencyKey:'maintenance-command-3',origin:'https://acme.example.com',
  })).rejects.toMatchObject({
    code:'MAINTENANCE_COMMAND_UNAVAILABLE',status:503,
    message:'Native Maintenance commands are not enabled yet.',
  });
  expect(transport).toHaveBeenCalledTimes(1);
 });

 it('rejects an unconfigured host, a changed context and a malformed response before returning data',async()=>{
  const client=createNativeMaintenanceClient(JSON.parse(config()),vi.fn());
  await expect(client.workspace(incoming({host:'evil.example.com'}),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_MAINTENANCE_AUTHORITY_INVALID'});
  const changed=createNativeMaintenanceClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:workspace,context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_MAINTENANCE_CONTEXT_CHANGED'});
  const malformed=createNativeMaintenanceClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:{...workspace,items:[{...work,extra:'field'}]},context}),
  ));
  await expect(malformed.workspace(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_MAINTENANCE_RESPONSE_INVALID'});
 });

 it('fails closed when the native Maintenance configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  delete process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  expect(nativeMaintenanceAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_MAINTENANCE_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins, malformed config and a short key',async()=>{
  expect(()=>createNativeMaintenanceClient(nativeMaintenanceConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeMaintenanceClient(nativeMaintenanceConfiguration(config({privateBaseUrl:'http://10.0.0.1:44992'})),vi.fn())).toThrow();
  expect(()=>createNativeMaintenanceClient(nativeMaintenanceConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeMaintenanceConfiguration('{not-json')).toThrow();
  const transport=vi.fn();
  const client=createNativeMaintenanceClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{},idempotencyKey:'short',origin:'https://acme.example.com',
  })).rejects.toMatchObject({code:'NATIVE_MAINTENANCE_COMMAND_KEY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });
});

const workRequest=(body:Record<string,unknown>,key:string|null='maintenance-command-1')=>new Request(
  'https://acme.example.com/api/v1/holdings/acme/maintenance',
  {method:'POST',headers:{origin:'https://acme.example.com','content-type':'application/json',...(key!==null?{'idempotency-key':key}:{})},body:JSON.stringify(body)},
);
const routeContext={params:Promise.resolve({holdingSlug:'acme'})};

describe('Maintenance work command route selection',()=>{
 beforeEach(()=>{vi.resetAllMocks();});

 it('forwards schedule and issue-part transitions on the native path',async()=>{
  routeMocks.nativeCommand.mockResolvedValue({id:'work',version:2,status:'scheduled'});
  const schedule={id:'work',action:'schedule',expectedVersion:1,ownerId:'user',scheduledAt:1_800_000_000_000,reason:'Plan the repair'};
  const issue={id:'work',action:'issue-part',expectedVersion:2,warehouseId:'warehouse',productId:'product',quantityMilli:1000,entryDate:'2026-11-06',reason:'Issue the spare'};
  await maintenanceRoute.POST(workRequest(schedule,'schedule-key-1'),routeContext);
  await maintenanceRoute.POST(workRequest(issue,'parts-key-1'),routeContext);
  expect(routeMocks.nativeCommand).toHaveBeenNthCalledWith(1,routeMocks.actor,'acme',schedule,'schedule-key-1');
  expect(routeMocks.nativeCommand).toHaveBeenNthCalledWith(2,routeMocks.actor,'acme',issue,'parts-key-1');
 });

 it('closes when the native owner command is unavailable',async()=>{
  routeMocks.nativeCommand.mockResolvedValue(undefined);
  const body={id:'work',action:'start',expectedVersion:1,reason:'Start work'};
  const response=await maintenanceRoute.POST(workRequest(body,'maintenance-command-1'),routeContext);
  expect(response.status).toBe(503);
  expect((await response.json()).error.code).toBe('NATIVE_MAINTENANCE_UNAVAILABLE');
  expect(routeMocks.nativeCommand).toHaveBeenCalledWith(routeMocks.actor,'acme',body,'maintenance-command-1');
 });

 it('forwards a complete transition on the native path with the exact key and body',async()=>{
  routeMocks.nativeCommand.mockResolvedValue({id:'work',version:2,status:'completed'});
  const body={id:'work',action:'complete',expectedVersion:1,resultingCondition:'good',evidence:'Repaired',reason:'Completed repairs'};
  const response=await maintenanceRoute.POST(workRequest(body,'maintenance-command-2'),routeContext);
  expect(response.status).toBe(200);
  expect(routeMocks.nativeCommand).toHaveBeenCalledWith(routeMocks.actor,'acme',body,'maintenance-command-2');
 });

 it('closes a native failure without a retained fallback',async()=>{
  routeMocks.nativeCommand.mockRejectedValue(Object.assign(new Error('Native Maintenance commands are not enabled yet.'),{status:503,code:'MAINTENANCE_COMMAND_UNAVAILABLE'}));
  const response=await maintenanceRoute.POST(workRequest({id:'work',action:'block',expectedVersion:1,reason:'Waiting for parts'}),routeContext);
  expect(response.status).toBe(503);
  expect((await response.json()).error.code).toBe('MAINTENANCE_COMMAND_UNAVAILABLE');
 });

 it('closes a native command that returns no acknowledgement',async()=>{
  routeMocks.nativeCommand.mockResolvedValue(undefined);
  const response=await maintenanceRoute.POST(workRequest({id:'work',action:'resume',expectedVersion:1,reason:'Parts arrived'}),routeContext);
  expect(response.status).toBe(503);
 });

 it('forwards creation to the native owner command',async()=>{
  routeMocks.nativeCommand.mockResolvedValue({id:'maintenance-maintenance-command-3',version:1,status:'open'});
  const body={assetId:'asset',assetVersion:1,title:'Synthetic work',priority:'normal',ownerId:'user',scheduledAt:null,reason:'Synthetic reason'};
  const response=await maintenanceRoute.POST(workRequest(body,'maintenance-command-3'),routeContext);
  expect(response.status).toBe(200);
  expect(routeMocks.nativeCommand).toHaveBeenCalledWith(routeMocks.actor,'acme',body,'maintenance-command-3');
 });

 it('closes creation when the native owner command returns no acknowledgement',async()=>{
  routeMocks.nativeCommand.mockResolvedValue(undefined);
  const body={assetId:'asset',assetVersion:1,title:'Synthetic work',priority:'normal',ownerId:'user',scheduledAt:null,reason:'Synthetic reason'};
  const response=await maintenanceRoute.POST(workRequest(body,'maintenance-command-4'),routeContext);
  expect(response.status).toBe(503);
  expect(routeMocks.nativeCommand).toHaveBeenCalledWith(routeMocks.actor,'acme',body,'maintenance-command-4');
 });

 it('maps retries of the same transition to the same idempotency key',async()=>{
  routeMocks.nativeCommand.mockResolvedValue({id:'work',version:2,status:'in_progress'});
  const body={id:'work',action:'start',expectedVersion:1,reason:'Start work'};
  await maintenanceRoute.POST(workRequest(body,'replay-command-key'),routeContext);
  await maintenanceRoute.POST(workRequest(body,'replay-command-key'),routeContext);
  expect(routeMocks.nativeCommand).toHaveBeenNthCalledWith(1,routeMocks.actor,'acme',body,'replay-command-key');
  expect(routeMocks.nativeCommand).toHaveBeenNthCalledWith(2,routeMocks.actor,'acme',body,'replay-command-key');
 });

 it('rejects a missing idempotency key before the native owner command',async()=>{
  const response=await maintenanceRoute.POST(workRequest({id:'work',action:'cancel',expectedVersion:1,reason:'No longer required'},null),routeContext);
  expect(response.status).toBe(422);
  expect(routeMocks.nativeCommand).not.toHaveBeenCalled();
 });
});

/** Records source cutover oracle: Maintenance plans and work are read only
 * through the native record_source port; no owner SQL remains in TypeScript. */
describe('Maintenance source-record policy is native-only',()=>{
 const key='ERPX_NATIVE_MAINTENANCE_CONFIG';
 const config=()=>JSON.stringify({privateBaseUrl:'http://127.0.0.1:44992',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'holding'}]});
 const actor={
  userId:'owner',holdingId:'holding',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,
  assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['maintenance'],iam:{permissions:['records.read']},
 } as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const context={userId:'owner',holdingId:'holding',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',revision:7,assignmentVersion:3,
  role:'owner',assignmentScopeKind:'holding',modules:['maintenance'],permissions:['records.read']};
 const row=(version=3)=>({id:'source',company_id:'company',branch_id:null,department_id:null,version,title:'Synthetic maintenance source',status:'active'});
 const payload=(kind:'maintenance_plan'|'maintenance_work',value:unknown)=>new Response(JSON.stringify({data:kind==='maintenance_plan'?{plan:value}:{work:value},context}),{status:200,headers:{'content-type':'application/json'}});
 const policy=async()=>await import('../../../packages/server-records/src/adapters/source-record-policy');
 let fetchSpy:ReturnType<typeof vi.spyOn>;
 beforeEach(()=>{vi.stubEnv(key,config());});
 it('reads plan and work sources from the native owner with no TypeScript SQL',async()=>{
  fetchSpy=vi.spyOn(globalThis,'fetch').mockImplementation(async(input)=>{
   const url=String(input);
   return payload(url.includes('/maintenance/plans')?'maintenance_plan':'maintenance_work',row());
  });
  try{
   const p=await policy();
   const plan=await p.readSourceRecord(actor,{kind:'maintenance_plan',id:'source',version:3});
   expect(plan).toMatchObject({id:'source',version:3,title:'Synthetic maintenance source',status:'active'});
   const work=await p.readSourceRecord(actor,{kind:'maintenance_work',id:'source'});
   expect(work).toMatchObject({id:'source',version:3});
   expect(fetchSpy).toHaveBeenCalledTimes(2);
   expect(String(fetchSpy.mock.calls[0][0])).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance/plans?id=source');
   expect(String(fetchSpy.mock.calls[1][0])).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance?id=source');
  }finally{fetchSpy.mockRestore();vi.unstubAllEnvs();}
 });
 it('fails closed for same-unit callers and source guards',async()=>{
  fetchSpy=vi.spyOn(globalThis,'fetch');
  try{
   const p=await policy(),db={prepare:vi.fn()} as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
   for(const kind of ['maintenance_plan','maintenance_work'] as const){
    await expect(p.readSourceRecord(actor,{kind,id:'source'},db)).rejects.toMatchObject({status:503,code:'MAINTENANCE_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
    await expect(p.sourceRecordGuards(db,actor,'key',{kind,id:'source',version:3,href:'/maintenance/'} as never)).rejects.toMatchObject({status:503,code:'MAINTENANCE_RECORD_SOURCE_NATIVE_UNIT_REQUIRED'});
   }
   expect(fetchSpy).not.toHaveBeenCalled();
  }finally{fetchSpy.mockRestore();vi.unstubAllEnvs();}
 });
 it('masks 403/404, preserves version conflicts and fails closed without configuration',async()=>{
  const p=await policy();
  try{
   for(const status of [403,404]){
    fetchSpy=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response('{}',{status}));
    expect(await p.visibleSourceRecord(actor,{kind:'maintenance_plan',id:'source'})).toBeNull();
    fetchSpy.mockRestore();
   }
   fetchSpy=vi.spyOn(globalThis,'fetch').mockResolvedValue(payload('maintenance_plan',row(2)));
   await expect(p.readSourceRecord(actor,{kind:'maintenance_plan',id:'source',version:3})).rejects.toMatchObject({status:409,code:'SOURCE_RECORD_CHANGED'});
   fetchSpy.mockRestore();
   vi.unstubAllEnvs();delete process.env[key];
   await expect(p.readSourceRecord(actor,{kind:'maintenance_plan',id:'source'})).rejects.toMatchObject({status:503,code:'MAINTENANCE_RECORD_SOURCE_NATIVE_UNAVAILABLE'});
  }finally{fetchSpy?.mockRestore();vi.unstubAllEnvs();}
 });
});

/** Preventive read-tail oracle. The eight TypeScript SELECTs were removed;
 * plan list/detail now come from the mounted native `/maintenance/plans` read.
 * These cases own the work-item derivation and closed failure policy that the
 * retired repository SQL exercised, with no owner SQL available to fall back
 * to (a `prepare` that throws proves no SELECT is built). */
describe('native preventive-plan read transport',()=>{
 const key='ERPX_NATIVE_MAINTENANCE_CONFIG';
 const cfg=()=>JSON.stringify({privateBaseUrl:'http://127.0.0.1:44992',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'holding'}]});
 const actor={
  userId:'user',holdingId:'holding',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,
  assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3,sessionId:'session'},modules:['maintenance'],
  iam:{permissions:['records.read','records.write','records.review']},
 } as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const context={companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3,permissions:['records.read','records.write','records.review']};
 const duePlan={...plan,status:'active',version:2,next_due_at:Date.now()-1000,owner_id:'user',owner_eligible:1};
 const ineligible={...plan,id:'ineligible',status:'active',version:1,next_due_at:Date.now()-1000,owner_id:'other',owner_eligible:0};
 const cancelled={...plan,id:'cancelled',status:'cancelled',version:3,owner_id:'user',owner_eligible:1};
 const detailPayload={plan:duePlan,actorId:'user',canWrite:true,canReview:true,assetVersion:1,people:[{id:'user',name:'Owner'}],history:[],occurrences:[]};
 const okJson=(data:unknown)=>new Response(JSON.stringify({data,context}),{status:200,headers:{'content-type':'application/json'}});
 const load=()=>import('../../../packages/server-maintenance/src/public/native-preventive-read');
 let spy:ReturnType<typeof vi.spyOn>;
 beforeEach(()=>{vi.stubEnv(key,cfg());});
 afterEach(()=>{vi.unstubAllEnvs();vi.restoreAllMocks();});

 it('derives work items from the mounted plan list and never builds owner SQL',async()=>{
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(okJson([duePlan,ineligible,cancelled]));
  const {preventivePlanWork}=await load();
  const db={prepare:vi.fn(()=>{throw Error('preventive read must not build SQL');})} as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
  const items=await preventivePlanWork(actor);
  expect(items.find(i=>i.id==='plan')).toMatchObject({queue:'todo',canAct:true,type:'maintenance_plan',blocker:null});
  expect(items.find(i=>i.id==='ineligible')).toMatchObject({blocker:expect.stringContaining('no longer eligible')});
  expect(items.find(i=>i.id==='cancelled')).toMatchObject({queue:'completed',canAct:false});
  expect(String(spy.mock.calls[0][0])).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance/plans');
  expect(db.prepare).not.toHaveBeenCalled();
 });

 it('reads plan detail by id from the native route for notification existence checks',async()=>{
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(okJson(detailPayload));
  const {preventivePlanDetail}=await load();
  const detail=await preventivePlanDetail(actor,'plan');
  expect(detail.plan).toMatchObject({id:'plan',status:'active',version:2});
  const url=new URL(String(spy.mock.calls[0][0]));
  expect(url.pathname).toBe('/api/v1/holdings/acme/maintenance/plans');
  expect(url.searchParams.get('id')).toBe('plan');
 });

 it('fails closed for missing config, 403/404 and a changed context',async()=>{
  const {preventivePlanDetail,preventivePlanWork}=await load();
  vi.unstubAllEnvs();delete process.env[key];
  await expect(preventivePlanDetail(actor,'plan')).rejects.toMatchObject({status:503,code:'NATIVE_MAINTENANCE_UNAVAILABLE'});
  expect(await preventivePlanWork(actor)).toEqual([]);
  vi.stubEnv(key,cfg());
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response('{}',{status:404}));
  await expect(preventivePlanDetail(actor,'plan')).rejects.toMatchObject({status:404,code:'MAINTENANCE_PLAN_NOT_FOUND'});
  spy.mockResolvedValue(new Response(JSON.stringify({data:detailPayload,context:{...context,revision:8}}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(preventivePlanDetail(actor,'plan')).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
 });
});

/** The downtime-evidence transport is a strict projection of the native GET:
 * it forwards only bounded query facts, rechecks the current context and never
 * builds owner SQL or falls back to the retained TypeScript reader. */
describe('native downtime-evidence read transport',()=>{
 const key='ERPX_NATIVE_MAINTENANCE_CONFIG';
 const cfg=()=>JSON.stringify({privateBaseUrl:'http://127.0.0.1:44992',timeoutMs:1000,holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'holding'}]});
 const actor={
  userId:'user',holdingId:'holding',holdingSlug:'acme',companyId:'company',branchId:null,departmentId:null,
  assignmentId:'assignment',role:'owner',assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['maintenance'],
  iam:{permissions:['records.read','records.write','records.review']},
 } as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
 const evidence={id:'work',version:'2',status:'completed',title:'Bearing replacement',asset_code:'A-1',
  asset_name:'Asset One',downtime_started_at:'1000',downtime_ended_at:'2000'};
 const context={companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3,permissions:['records.read','records.write','records.review']};
 const input={from:0,to:1800000000000,query:'Bearing',limit:2,cursor:{endedAt:1500,id:'previous'},recordId:'work'};
 const okJson=(data:unknown)=>new Response(JSON.stringify({data,context}),{status:200,headers:{'content-type':'application/json'}});
 const load=()=>import('../../../packages/server-maintenance/src/public/native-downtime-evidence');
 let spy:ReturnType<typeof vi.spyOn>;
 beforeEach(()=>{vi.stubEnv(key,cfg());});
 afterEach(()=>{vi.unstubAllEnvs();vi.restoreAllMocks();});

 it('forwards bounded query facts and the current context without building owner SQL',async()=>{
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(okJson([evidence]));
  const {nativeDowntimeEvidenceRows,nativeDowntimeReadConfigured}=await load();
  expect(nativeDowntimeReadConfigured()).toBe(true);
  const db={prepare:vi.fn(()=>{throw Error('downtime evidence read must not build SQL');})} as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
  const rows=await nativeDowntimeEvidenceRows(actor,input,db);
  expect(rows).toEqual([evidence]);
  const url=new URL(String(spy.mock.calls[0][0]));
  expect(url.pathname).toBe('/api/v1/holdings/acme/maintenance/downtime-evidence');
  expect(url.searchParams.get('from')).toBe('0');
  expect(url.searchParams.get('to')).toBe('1800000000000');
  expect(url.searchParams.get('query')).toBe('Bearing');
  expect(url.searchParams.get('limit')).toBe('2');
  expect(url.searchParams.get('cursorEndedAt')).toBe('1500');
  expect(url.searchParams.get('cursorId')).toBe('previous');
  expect(url.searchParams.get('recordId')).toBe('work');
  expect(spy.mock.calls[0][1]).toMatchObject({method:'GET',cache:'no-store',redirect:'error'});
  expect(new Headers((spy.mock.calls[0][1] as RequestInit).headers as HeadersInit).get('x-erpx-context-revision')).toBe('7');
  expect(db.prepare).not.toHaveBeenCalled();
 });

 it('fails closed for missing config, forbidden, changed context and malformed rows',async()=>{
  const {nativeDowntimeEvidenceRows}=await load();
  vi.unstubAllEnvs();delete process.env[key];
  await expect(nativeDowntimeEvidenceRows(actor,input)).rejects.toMatchObject({status:503,code:'NATIVE_MAINTENANCE_UNAVAILABLE'});
  vi.stubEnv(key,cfg());
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response('{}',{status:403}));
  await expect(nativeDowntimeEvidenceRows(actor,input)).rejects.toMatchObject({status:403,code:'MAINTENANCE_DOWNTIME_EVIDENCE_FORBIDDEN'});
  spy.mockResolvedValue(new Response(JSON.stringify({data:[evidence],context:{...context,revision:8}}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(nativeDowntimeEvidenceRows(actor,input)).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  spy.mockResolvedValue(new Response(JSON.stringify({data:[{...evidence,extra:true}],context}),{status:200,headers:{'content-type':'application/json'}}));
  await expect(nativeDowntimeEvidenceRows(actor,input)).rejects.toMatchObject({status:503,code:'NATIVE_MAINTENANCE_RESPONSE_INVALID'});
  spy.mockResolvedValue(new Response(JSON.stringify({data:[evidence],context}),{status:200,headers:{'content-type':'text/plain'}}));
  await expect(nativeDowntimeEvidenceRows(actor,input)).rejects.toMatchObject({status:503,code:'NATIVE_MAINTENANCE_READ_UNAVAILABLE'});
 });

 it('rejects unbounded queries before any fetch and refuses a missing holding',async()=>{
  const {nativeDowntimeEvidenceRows}=await load();
  spy=vi.spyOn(globalThis,'fetch').mockResolvedValue(okJson([evidence]));
  for(const bad of [{...input,limit:51},{...input,query:'x'.repeat(201)},{...input,cursor:{endedAt:1,id:''}},{...input,recordId:''}]){
   await expect(nativeDowntimeEvidenceRows(actor,bad)).rejects.toMatchObject({status:422,code:'MAINTENANCE_DOWNTIME_QUERY_INVALID'});
  }
  expect(spy).not.toHaveBeenCalled();
  const foreign={...actor,holdingId:'other'} as typeof actor;
  await expect(nativeDowntimeEvidenceRows(foreign,input)).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
 });
});
