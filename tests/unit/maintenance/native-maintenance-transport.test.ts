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

 it('keeps both TS transport modules free of owner SQL so the native GET is the sole read authority',async()=>{
  const {readFileSync}=await import('node:fs');
  const adapters=new URL('../../../packages/server-maintenance/src/adapters/',import.meta.url);
  const pub=new URL('../../../packages/server-maintenance/src/public/',import.meta.url);
  for(const file of [
   new URL('native-downtime-evidence.ts',adapters),
   new URL('native-downtime-evidence.ts',pub),
  ]){
   const source=readFileSync(file,'utf8');
   expect(source).not.toMatch(/\b(?:SELECT|INSERT|UPDATE|DELETE)\b/i);
  }
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance finance lineage source.
 * The debit and credit journal-line balance witnesses are one owner-local fixed
 * statement: both public consumers receive the identical composed SQL and no
 * duplicate TS query literal for the second sum remains. */
describe('Maintenance finance record source fixed SQL',()=>{
 const file='../../../packages/server-maintenance/src/adapters/finance-record-sources.ts';
 it('single-sources both journal-line balance witnesses and keeps composed SQL intact',async()=>{
  const {readFile}=await import('node:fs/promises');
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  expect(source.match(/const journalLineSum=/g)).toHaveLength(1);
  expect(source).not.toContain('(SELECT sum(debit_minor) FROM finance_journal_lines');
  expect(source).not.toContain('(SELECT sum(credit_minor) FROM finance_journal_lines');
  expect(source.match(/SELECT/g)).toHaveLength(2);
  const {maintenanceFinanceRecordSources}=await import('../../../packages/server-maintenance/src/adapters/finance-record-sources');
  const journal=maintenanceFinanceRecordSources.journal;
  expect(journal.startsWith("SELECT consumption.company_id,consumption.journal_id id")).toBe(true);
  expect(journal).toContain("WHERE consumption.source_kind='maintenance' AND consumption.source_version<=work.version AND consumption.quantity_milli>0 AND consumption.value_minor>0");
  expect(journal).toContain('(SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor');
  expect(journal).toContain('(SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance package-settlement source
 * (r3). Work and Plan source images are one owner-local fixed statement; both
 * public consumers receive byte-identical composed SQL and no duplicate TS
 * query literal for the second image remains. */
describe('Maintenance package settlement source fixed SQL',()=>{
 const file='../../../packages/server-maintenance/src/adapters/package-obligations.ts';
 it('single-sources the Work/Plan images and keeps composed SQL intact',async()=>{
  const {readFile}=await import('node:fs/promises');
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  const count=(text:string,needle:string)=>text.split(needle).length-1;
  expect(source.match(/const ownerSourceSql = \(table:string,alias:string\) =>/g)).toHaveLength(1);
  // r3 removed one raw statement literal (13 -> 12); r9 shared the occurrence
  // and work-event full-row literals through settlementEventRows (12 -> 11);
  // r10 shared the two upstream-source EXISTS witnesses (11 -> 10); r15 shared
  // the two MATERIALIZED scoped CTEs through scopedSourceCte (10 -> 9); r16
  // moved the asset-version lookup into projectionLookup (9 -> 8); r20 shared
  // the three facts UNION rows through factRow (8 -> 6); r22 folded the EXISTS
  // head into existsGuardPrefix (6 -> 5); r26 delegated settlementEventRows to
  // settlementRefSource (5 -> 4); r28 delegated factRow to settlementRefSource
  // (4 -> 3).
  expect(source.match(/SELECT/g)).toHaveLength(1);
  expect(count(source,'SELECT work.*,COALESCE')).toBe(0);
  expect(count(source,'SELECT plan.*,COALESCE')).toBe(0);
  expect(source).toContain('ownerSourceSql("maintenance_work_orders","work")');
  expect(source).toContain('ownerSourceSql("maintenance_preventive_plans","plan")');
  const {maintenancePackageSettlementSources}=await import('../../../packages/server-maintenance/src/adapters/package-obligations');
  const {work,plans}=maintenancePackageSettlementSources;
  expect(work.startsWith("SELECT work.*,COALESCE(asset.source_valid AND asset.branch_id=work.branch_id AND asset.department_id IS NOT DISTINCT FROM work.department_id,false) source_valid FROM maintenance_work_orders work LEFT JOIN")).toBe(true);
  expect(plans.startsWith("SELECT plan.*,COALESCE(asset.source_valid AND asset.branch_id=plan.branch_id AND asset.department_id IS NOT DISTINCT FROM plan.department_id,false) source_valid FROM maintenance_preventive_plans plan LEFT JOIN")).toBe(true);
  expect(work).toContain('ON asset.id=work.asset_id AND asset.company_id=work.company_id');
  expect(plans).toContain('ON asset.id=plan.asset_id AND asset.company_id=plan.company_id');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance settlement event row family
 * (r9). Preventive occurrences and work events share one owner-local full-row
 * statement; both exported sources stay byte-identical to their pre-dedupe
 * literals, no duplicate TS query literal remains and locks are untouched. */
describe('Maintenance settlement event row fixed SQL',()=>{
 const file='../../../packages/server-maintenance/src/adapters/package-obligations.ts';
 it('single-sources the full-row read and keeps both composed strings intact',async()=>{
  const {readFile}=await import('node:fs/promises');
  const source=await readFile(new URL(file,import.meta.url),'utf8');
  expect(source.match(/const settlementEventRows=\(table:"maintenance_preventive_occurrences"\|"maintenance_work_events"\)=>settlementRefSource\("\*",table\)/g)).toHaveLength(1);
  expect(source).toContain('import {settlementRefSource} from "./asset-settlement-refs"');
  expect(source.match(/SELECT \* FROM/g)??[]).toHaveLength(0);
  // r10 shared the two upstream-source EXISTS witnesses (11 -> 10); r15 shared
  // the two MATERIALIZED scoped CTEs through scopedSourceCte (10 -> 9).; r16 moved the asset-version lookup into projectionLookup (9 -> 8); r20 shared the three facts UNION rows through factRow (8 -> 6); r22 folded the EXISTS head into existsGuardPrefix (6 -> 5); r26 delegated settlementEventRows to settlementRefSource (5 -> 4); r28 delegated factRow to settlementRefSource (4 -> 3).
  expect(source.match(/SELECT/g)).toHaveLength(1);
  expect(source).not.toContain('occurrences:"SELECT * FROM maintenance_preventive_occurrences"');
  expect(source).not.toContain('workEvents:"SELECT * FROM maintenance_work_events"');
  expect(source).toContain('settlementEventRows("maintenance_preventive_occurrences")');
  expect(source).toContain('settlementEventRows("maintenance_work_events")');
  expect(source).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
  const {maintenancePackageSettlementSources}=await import('../../../packages/server-maintenance/src/adapters/package-obligations');
  expect(maintenancePackageSettlementSources.occurrences).toBe('SELECT * FROM maintenance_preventive_occurrences');
  expect(maintenancePackageSettlementSources.workEvents).toBe('SELECT * FROM maintenance_work_events');
  const {settlementRefSource}=await import('../../../packages/server-maintenance/src/adapters/asset-settlement-refs');
  expect(settlementRefSource("*","maintenance_preventive_occurrences")).toBe(maintenancePackageSettlementSources.occurrences);
  expect(settlementRefSource("*","maintenance_work_events")).toBe(maintenancePackageSettlementSources.workEvents);
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance upstream-source witness family
 * (r10). The preventive-occurrence and part-consumption package scopes share one
 * owner-local existence witness; both composed predicates stay byte-identical to
 * their pre-dedupe literals, no duplicate TS query literal remains and the
 * package settlement lock is untouched. */
describe('Maintenance upstream source witness fixed SQL',()=>{
  const file='../../../packages/server-maintenance/src/adapters/package-obligations.ts';
  it('single-sources the upstream-source EXISTS witness and keeps both composed predicates intact',async()=>{
   const {readFile}=await import('node:fs/promises');
   const source=await readFile(new URL(file,import.meta.url),'utf8');
   expect(source).toContain('const workSourceExists=(source:string,alias:string,column:string)=>`${existsGuardPrefix(`${source} ${alias}`)}${alias}.${column}=work.id)`;');
   expect(source.match(/EXISTS\(SELECT 1 FROM/g)??[]).toHaveLength(0);
   // r16 moved the asset-version lookup into projectionLookup (9 -> 8); r20 shared the three facts UNION rows through factRow (8 -> 6);
   // r22 folded this EXISTS head into the shared existsGuardPrefix (6 -> 5); r26 delegated settlementEventRows to settlementRefSource (5 -> 4); r28 delegated factRow to settlementRefSource (4 -> 3).
   expect(source.match(/SELECT/g)).toHaveLength(1);
   expect(source).toContain('`${existsGuardPrefix(`${source} ${alias}`)}${alias}.${column}=work.id)`');
   expect(source).not.toContain('EXISTS(SELECT 1 FROM maintenance_preventive_occurrences occurrence');
   expect(source).not.toContain('EXISTS(SELECT 1 FROM (${maintenanceSettlementConsumptions}) part');
   expect(source).toContain('scope.package_id=\'assets-preventive\' AND ${workSourceExists("maintenance_preventive_occurrences","occurrence","work_order_id")}');
   expect(source).toContain('scope.package_id=\'assets-parts-costs\' AND ${workSourceExists(`(${maintenanceSettlementConsumptions})`,"part","source_id")}');
   expect(source).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
  });
});

/** Fixed-SQL read-tail oracle for the Maintenance scoped-source CTE family
 * (r15). The Work and Plan MATERIALIZED branches of the package-obligations
 * facts read share one owner-local CTE skeleton. Both callers pass only an
 * owner-fixed alias, a closed source relation and a fixed scope predicate; each
 * composed CTE stays byte-identical to its pre-dedupe literal, no duplicate TS
 * query literal remains and every owner lock is untouched. r3-r14 families stay
 * unchanged. */
describe('Maintenance scoped source CTE fixed SQL',()=>{
  const file='../../../packages/server-maintenance/src/adapters/package-obligations.ts';
  it('single-sources the MATERIALIZED CTEs and keeps both composed CTEs byte-identical',async()=>{
   const {readFile}=await import('node:fs/promises');
   const source=await readFile(new URL(file,import.meta.url),'utf8');
   expect(source.match(/export const scopedSourceCte=/g)).toHaveLength(1);
   // r30 folds the alias-qualified row head into the shared aliasRowsSql renderer.
   expect(source).toContain('export const scopedSourceCte=(alias:"work"|"plan",source:string,extra:string)=>aliasRowsSql(alias,` FROM (${source}) ${alias},scope WHERE ${alias}.company_id=scope.company_id AND ${extra}`);');
   expect(source.match(/MATERIALIZED\(\$\{scopedSourceCte\(/g)).toHaveLength(2);
   expect(source).not.toContain('MATERIALIZED(SELECT work.* FROM (${work}) work,scope');
   expect(source).not.toContain('MATERIALIZED(SELECT plan.* FROM (${plans}) plan,scope');
   expect(source).toContain('work AS MATERIALIZED(${scopedSourceCte("work",work,workScopeExtra)})');
   // Owner-fixed extra predicates, unchanged from the pre-dedupe literal.
   expect(source).toContain('const workScopeExtra=');
   expect(source).toContain("scope.package_id='assets' OR scope.package_id='assets-preventive' AND ${workSourceExists(");
   expect(source).toContain("scope.package_id='assets-parts-costs' AND ${workSourceExists(");
   expect(source).toContain("plans AS MATERIALIZED(${scopedSourceCte(\"plan\",plans,\"scope.package_id<>'assets-parts-costs'\")})");
   // Locks untouched.
   expect(source).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
   const {scopedSourceCte}=await import('../../../packages/server-maintenance/src/adapters/package-obligations');
   expect(scopedSourceCte("plan","<PLANS>","scope.package_id<>'assets-parts-costs'")).toBe(
     "SELECT plan.* FROM (<PLANS>) plan,scope WHERE plan.company_id=scope.company_id AND scope.package_id<>'assets-parts-costs'");
   expect(scopedSourceCte("work","<WORK>","<EXTRA>")).toBe(
     "SELECT work.* FROM (<WORK>) work,scope WHERE work.company_id=scope.company_id AND <EXTRA>");
  });
});

/** Fixed-SQL read-tail oracle for the Maintenance breakdown projection family
 * (r11). The open-work/blocked count and the asset-downtime sum share one
 * owner-local statement skeleton; both callers pass only fixed owner metric
 * fragments and a closed row alias, both composed projections keep their
 * pre-dedupe tokens and no duplicate TS query literal remains. */
describe('Maintenance breakdown projection fixed SQL',()=>{
  const file='../../../packages/server-maintenance/src/adapters/executive-metric-projections.ts';
  it('single-sources the breakdown projection and keeps both metric callers intact',async()=>{
   const {readFile}=await import('node:fs/promises');
   const source=await readFile(new URL(file,import.meta.url),'utf8');
   expect(source.match(/const breakdownProjection = \(alias: string, label: string, value: string, from: string, where: string\) =>/g)).toHaveLength(1);
   expect(source.match(/`SELECT \$\{label\} label,\$\{value\} value FROM \$\{from\} \$\{alias\} WHERE \$\{where\} GROUP BY 1 ORDER BY 1`/g)).toHaveLength(1);
   // r2 left one orgLabel image; r11 shared the two outer breakdown SELECTs
   // (8 -> 7); r16 moved the orgLabel lookup into projectionLookup (7 -> 6);
   // r17 shared the two windowed interval CTEs (6 -> 5).
   expect(source.match(/SELECT/g)).toHaveLength(5);
   expect(source).toContain('breakdownProjection("w", label, "count(*)::text", `(${source.sql})`');
   expect(source).toContain('breakdownProjection("s", label, "sum(segment_end-segment_start)::text", "monthly_segments", "segment_end>segment_start")');
   expect(source).not.toContain('label,count(*)::text value FROM (${source.sql}) w WHERE');
   expect(source).not.toContain('label,sum(segment_end-segment_start)::text value FROM monthly_segments s');
   expect(source).toContain('COALESCE(${projectionLookup("n.display_name",organizationProjectionSources.organization_nodes,"n",`n.id=w.${col} AND n.company_id=w.company_id`)}');
   expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
  });
 });

/** Fixed-SQL read-tail oracle for the Maintenance record-image family (r12).
 * The attachment target image and the Work/Plan purchase-source image share one
 * owner-local jsonb record-image projection; both composed strings stay
 * byte-identical to their pre-dedupe literals, no duplicate TS query literal
 * remains in either consumer and the purchase-source lock is untouched. */
describe('Maintenance record-image fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the record-image projection and keeps both composed images intact',async()=>{
  const {readFileSync}=await import('node:fs');
  const helper=readFileSync(new URL(`${dir}record-image-sources.ts`,import.meta.url),'utf8');
  const attachment=readFileSync(new URL(`${dir}attachment-read-sources.ts`,import.meta.url),'utf8');
  const purchase=readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8');
  expect(helper.match(/export const recordImageSql = \(fields: string, source: string\): string =>/g)).toHaveLength(1);
  expect(helper.match(/SELECT/g)).toHaveLength(1);
  expect(attachment.match(/SELECT/g)).toBeNull();
  // r13 moved the purchase eligibility EXISTS witness into the shared IAM guard.
  expect(purchase.match(/SELECT/g)).toBeNull();
  expect(attachment).toContain('recordImageSql(');
  expect(purchase).toContain('recordImageSql(');
  expect(purchase).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
  expect(attachment).not.toContain("jsonb_build_object('work',to_jsonb(work),'asset',to_jsonb(asset))::text image\n");
  expect(purchase).not.toContain('::text image FROM ${kind===');

  const {finalRootOwnerProjectionSources:proj}=await import('../../../packages/server-assets/src/public/final-root-owner-projection-sources');
  const {recordImageSql}=await import('../../../packages/server-maintenance/src/adapters/record-image-sources');
  const {maintenanceAttachmentReadSourceImage}=await import('../../../packages/server-maintenance/src/adapters/attachment-read-sources');
  const {maintenancePurchaseSourceQuery}=await import('../../../packages/server-maintenance/src/adapters/purchase-sources');
  expect(recordImageSql('x',' FROM t')).toBe('SELECT jsonb_build_object(x)::text image FROM t');
  let captured='';
  const db={prepare(sql:string){captured=sql;return {bind(){return this;},first(){return null;}};}} as unknown as import('@erpx/server-runtime/public/runtime/types').SqlDatabase;
  await maintenanceAttachmentReadSourceImage(db,'company','work');
  expect(captured).toBe(`SELECT jsonb_build_object('work',to_jsonb(work),'asset',to_jsonb(asset))::text image
 FROM maintenance_work_orders work LEFT JOIN (${proj.operational_assets}) asset ON asset.id=work.asset_id AND asset.company_id=work.company_id
 WHERE work.id=? AND work.company_id=?`);
  for(const kind of ['work','plan'] as const){
   for(const file of [false,true]){
    const sql=maintenancePurchaseSourceQuery(kind,file);
    expect(sql.startsWith(`SELECT jsonb_build_object('record',${file?"to_jsonb(r)-'version'-'updated_at'":"to_jsonb(r)"},'asset',to_jsonb(a),'ownerEligible',EXISTS(SELECT 1 FROM assignments assignment`)).toBe(true);
    expect(sql).toContain('::text image FROM maintenance_');
    expect(sql.endsWith('WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND (?::text IS NULL OR r.department_id=?)')).toBe(true);
   }
  }
 });

 it('retirement:ts-statements stays strictly below the r31 BEFORE=18 baseline',async()=>{
  const {readdirSync,readFileSync}=await import('node:fs');
  const {resolve}=await import('node:path');
  let total=0;
  const srcRoot=resolve(__dirname,'../../../packages/server-maintenance/src');
  const walk=(d:string)=>{
   for(const entry of readdirSync(d,{withFileTypes:true})){
    const p=resolve(d,entry.name);
    if(entry.isDirectory())walk(p);
    else if(entry.name.endsWith('.ts'))total+=(readFileSync(p,'utf8').match(/SELECT/g)??[]).length;
   }
  };
  walk(srcRoot);
  expect(total).toBe(17);
  expect(total).toBeLessThan(18);
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance IAM scope-guard family (r13).
 * The Work owner-eligibility/can-review images and the Work/Plan purchase-source
 * eligibility witness share one owner-local EXISTS guard. All composed strings
 * stay byte-identical to their pre-dedupe literals, no duplicate TS query literal
 * remains in either consumer and both owner locks are untouched. r3-r12 families
 * and all locks are unchanged. */
describe('Maintenance IAM scope guard fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the IAM guard and keeps every composed guard byte-identical',async()=>{
   const {readFileSync}=await import('node:fs');
   const helper=readFileSync(new URL(`${dir}iam-scope-guard.ts`,import.meta.url),'utf8');
   const work=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
   const purchase=readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8');
   expect(helper.match(/SELECT/g)??[]).toHaveLength(0);
   expect(helper).toContain('${existsGuardPrefix(from)}');
   // r7's work-local literal and the pre-r13 purchase eligibility literal are gone.
   expect(work).not.toContain('EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments})');
   expect(purchase).not.toContain('EXISTS(SELECT 1 FROM assignments assignment');
   expect(work).toContain('iamScopeGuardSql(workIamFrom');
   expect(purchase).toContain('iamScopeGuardSql(');
   // Locks untouched.
   expect(purchase).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');

   const {finalRootOwnerProjectionSources:ia}=await import('../../../packages/server-identity_access/src/public/final-root-owner-projection-sources');
   const {finalRootOwnerProjectionSources:pp}=await import('../../../packages/server-people_payroll/src/public/final-root-owner-projection-sources');
   const {iamScopeGuardSql}=await import('../../../packages/server-maintenance/src/adapters/iam-scope-guard');
   const wf=`(${ia.assignments}) x JOIN (${pp.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${pp.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id`;
   // Pre-r13 work literal rebuilt verbatim from the r7 definition.
   const preWork=(alias:string,user:string,perm:string)=>`EXISTS(SELECT 1 FROM ${wf} WHERE x.company_id=${alias}.company_id AND x.user_id=${user} AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'${perm}') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
   const wg=(alias:string,user:string,perm:'records.write'|'records.review')=>iamScopeGuardSql(wf,'x','p','e',alias,user,perm,`(x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id))`);
   expect(wg('w','w.owner_id','records.write')).toBe(preWork('w','w.owner_id','records.write'));
   expect(wg('w','?','records.review')).toBe(preWork('w','?','records.review'));
   expect(wg('a','?','records.review')).toBe(preWork('a','?','records.review'));
   // Pre-r13 purchase eligibility literal rebuilt verbatim.
   const pf='assignments assignment JOIN hr_people person ON person.user_id=assignment.user_id AND person.company_id=assignment.company_id JOIN hr_engagements engagement ON engagement.person_id=person.id AND engagement.company_id=person.company_id';
   const ps=`(assignment.scope_kind='holding' OR assignment.scope_kind='branch' AND assignment.scope_node_id=r.branch_id OR assignment.scope_kind='department' AND assignment.scope_node_id=r.department_id)`;
   const prePurchase=`EXISTS(SELECT 1 FROM ${pf} WHERE assignment.company_id=r.company_id AND assignment.user_id=r.owner_id AND iam_assignment_active(assignment.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(assignment.user_id,assignment.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(assignment.role,assignment.company_id,'records.write') AND ${ps} AND person.status='active' AND person.merged_into_id IS NULL AND engagement.status='active' AND engagement.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (engagement.end_date IS NULL OR engagement.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
   expect(iamScopeGuardSql(pf,'assignment','person','engagement','r','r.owner_id','records.write',ps)).toBe(prePurchase);
  });
 });

/** Fixed-SQL read-tail oracle for the Maintenance scheduled-source family (r14).
 * The preventive-plan and work-order branches of the calendar `scheduled` CTE
 * share one owner-local projection helper. Both composed branches stay
 * byte-identical to their pre-dedupe literals, no duplicate TS query literal
 * remains and every owner lock is untouched. r3-r13 families stay unchanged. */
describe('Maintenance scheduled-source fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the scheduled projection and keeps both composed branches byte-identical',async()=>{
   const {readFileSync}=await import('node:fs');
   const helper=readFileSync(new URL(`${dir}calendar-maintenance-facts.ts`,import.meta.url),'utf8');
   expect(helper.match(/SELECT/g)).toHaveLength(2);
   // r14 removed the two inline scheduled-branch literals; r31 routed the outer
   // alias-qualified row head through aliasRowsSql (3 -> 2); the inList read remains.
   expect(helper).not.toContain("SELECT p.id,'maintenance_plan' source_kind");
   expect(helper).not.toContain("UNION ALL SELECT w.id,'maintenance_work'");
   expect(helper.match(/scheduledSourceSql\(/g)).toHaveLength(2);
   // Locks untouched: the calendar read owns none and the other owner locks remain.
   expect(helper).not.toContain('LOCK TABLE');
   expect(readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
   expect(readFileSync(new URL(`${dir}finance-record-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
   expect(readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');

   const {scheduledSourceSql}=await import('../../../packages/server-maintenance/src/adapters/calendar-maintenance-facts');
   // Pre-r14 plan branch rebuilt verbatim.
   const prePlan=`SELECT p.id,'maintenance_plan' source_kind,p.company_id,p.branch_id,p.department_id,p.title,p.status,p.version,p.next_due_at due_at,p.owner_id,p.created_by
  FROM maintenance_preventive_plans p WHERE p.status='active'`;
   const preWork=`SELECT w.id,'maintenance_work',w.company_id,w.branch_id,w.department_id,w.title,w.status,w.version,w.scheduled_at,w.owner_id,w.created_by
  FROM maintenance_work_orders w WHERE w.status NOT IN ('completed','cancelled') AND w.scheduled_at IS NOT NULL`;
   expect(scheduledSourceSql('p','maintenance_plan','p.next_due_at due_at','maintenance_preventive_plans',"p.status='active'")).toBe(prePlan);
   expect(scheduledSourceSql('w','maintenance_work','w.scheduled_at','maintenance_work_orders',"w.status NOT IN ('completed','cancelled') AND w.scheduled_at IS NOT NULL")).toBe(preWork);
  });
 });


/** Fixed-SQL read-tail oracle for the Maintenance scalar projection lookup
 * family (r16). The Calendar owner-membership witness, the executive metric
 * label and the package-obligation asset-version read share one owner-local
 * `(SELECT <select> FROM (<source>) <alias> WHERE <where>)` skeleton. Every
 * composed string stays byte-identical to its pre-dedupe literal, no duplicate
 * TS query literal remains and every owner lock is untouched. r3-r15 families
 * stay unchanged. */
describe('Maintenance projection lookup fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the scalar projection lookup and keeps all three composed lookups byte-identical',async()=>{
   const {readFileSync}=await import('node:fs');
   const helper=readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8');
   const calendar=readFileSync(new URL(`${dir}calendar-maintenance-facts.ts`,import.meta.url),'utf8');
   const executive=readFileSync(new URL(`${dir}executive-metric-projections.ts`,import.meta.url),'utf8');
   const packageSources=readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8');
   expect(helper.match(/export const projectionLookup = \(select: string, source: string, alias: string, where: string\): string =>/g)).toHaveLength(1);
   expect(helper.match(/SELECT/g)).toHaveLength(2);
   expect(helper).toContain('export const existsGuardPrefix = (source: string): string => `EXISTS(SELECT 1 FROM ${source} WHERE `;');
   expect(helper).not.toMatch(/LOCK\s+TABLE/);
   expect(calendar).toContain('projectionLookup("user_id",people_payrollProjectionSources.hr_people,"hr_people","id=? AND company_id=s.company_id")');
   expect(executive).toContain('projectionLookup("n.display_name",organizationProjectionSources.organization_nodes,"n",');
   expect(packageSources).toContain('projectionLookup("version",assets.assets,"actual","actual.id=plan.asset_id AND actual.company_id=plan.company_id")');
   expect(calendar).not.toContain('s.owner_id IN (SELECT user_id FROM (${people_payrollProjectionSources.hr_people})');
   expect(executive).not.toContain('COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n');
   expect(packageSources).not.toContain('(SELECT version FROM (${assets.assets}) actual');
   // Locks untouched.
   expect(calendar).not.toContain('LOCK TABLE');
   expect(readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
   expect(readFileSync(new URL(`${dir}finance-record-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
   expect(readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
   expect(packageSources).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');

   const {projectionLookup}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
   const {finalRootOwnerProjectionSources:pp}=await import('../../../packages/server-people_payroll/src/public/final-root-owner-projection-sources');
   const {finalRootOwnerProjectionSources:org}=await import('../../../packages/server-organization/src/public/final-root-owner-projection-sources');
   const {assetPackageSettlementSources:as}=await import('../../../packages/server-assets/src/public/package-obligations');
   // Pre-r16 literals rebuilt verbatim.
   expect(projectionLookup('user_id',pp.hr_people,'hr_people','id=? AND company_id=s.company_id')).toBe(
     `(SELECT user_id FROM (${pp.hr_people}) hr_people WHERE id=? AND company_id=s.company_id)`);
   expect(`COALESCE(${projectionLookup('n.display_name',org.organization_nodes,'n','n.id=w.branch_id AND n.company_id=w.company_id')},'Company-wide')`).toBe(
     `COALESCE((SELECT n.display_name FROM (${org.organization_nodes}) n WHERE n.id=w.branch_id AND n.company_id=w.company_id),'Company-wide')`);
   expect(`occurrence.source_asset_version>${projectionLookup('version',as.assets,'actual','actual.id=plan.asset_id AND actual.company_id=plan.company_id')}`).toBe(
     `occurrence.source_asset_version>(SELECT version FROM (${as.assets}) actual WHERE actual.id=plan.asset_id AND actual.company_id=plan.company_id)`);
   expect(projectionLookup('x','t','a','a.id=?')).toBe('(SELECT x FROM (t) a WHERE a.id=?)');
  });
 });

/** Fixed-SQL read-tail oracle for the Maintenance windowed interval CTE family
 * (r17). The asset-downtime `ordered` and `marked` stages share one owner-local
 * `OVER(PARTITION BY asset_id ... ROWS BETWEEN UNBOUNDED PRECEDING AND <frame>)`
 * skeleton. Both composed CTEs keep their pre-dedupe SQL tokens (whitespace-only
 * reflow), no duplicate TS query literal remains and every owner lock is
 * untouched. r3-r16 families stay unchanged. */
describe('Maintenance windowed interval CTE fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the windowed interval CTE and keeps both composed stages byte-equivalent',async()=>{
   const {readFileSync}=await import('node:fs');
   const source=readFileSync(new URL(`${dir}executive-metric-projections.ts`,import.meta.url),'utf8');
   expect(source.match(/export const windowedIntervalCte = \(aggregate: string, frameEnd: "1 PRECEDING" \| "CURRENT ROW", alias: string, source: string\): string =>/g)).toHaveLength(1);
   expect(source.match(/`SELECT \*,\$\{aggregate\} OVER\(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND \$\{frameEnd\}\) \$\{alias\} FROM \$\{source\}`/g)).toHaveLength(1);
   // r11 left the breakdown projection, r16 the orgLabel lookup; r17 folds the
   // two inner windowed CTEs (6 -> 5).
   expect(source.match(/SELECT/g)).toHaveLength(5);
   expect(source.match(/windowedIntervalCte\(/g)).toHaveLength(2); // 2 callers
   expect(source).not.toContain('SELECT *,max(end_at) OVER(');
   expect(source).not.toContain('SELECT *,sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END)\n');
   expect(source).toContain('windowedIntervalCte("max(end_at)","1 PRECEDING","prior_end","raw_intervals")');
   expect(source).toContain('windowedIntervalCte("sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END)","CURRENT ROW","interval_group","ordered")');
   expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
   // Locks untouched in every owner that owns one.
   expect(readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
   expect(readFileSync(new URL(`${dir}finance-record-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
   expect(readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
   expect(readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');

   const {windowedIntervalCte}=await import('../../../packages/server-maintenance/src/adapters/executive-metric-projections');
   // Pre-r17 literals rebuilt verbatim; composed CTEs differ only in whitespace.
   const tokens=(s:string)=>s.replace(/\s+/g,' ').trim();
   expect(tokens(windowedIntervalCte('max(end_at)','1 PRECEDING','prior_end','raw_intervals'))).toBe(
     tokens('SELECT *,max(end_at) OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) prior_end FROM raw_intervals'));
   expect(tokens(windowedIntervalCte('sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END)','CURRENT ROW','interval_group','ordered'))).toBe(
     tokens('SELECT *,sum(CASE WHEN prior_end IS NULL OR start_at>prior_end THEN 1 ELSE 0 END) OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) interval_group FROM ordered'));
   expect(windowedIntervalCte('x','CURRENT ROW','a','t')).toBe('SELECT *,x OVER(PARTITION BY asset_id ORDER BY start_at,end_at,work_order_id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) a FROM t');
  });
 });

/** Fixed-SQL read-tail oracle for the Maintenance Work/Asset source family
 * (r18). The workspace items and downtime-evidence reads share one owner-local
 * Work join to the operational Asset projection. Each composed read keeps its
 * pre-dedupe SQL tokens (whitespace-only reflow for the downtime read), no
 * duplicate TS query literal remains, every r3-r17 family stays unchanged and
 * every owner lock is untouched. */
describe('Maintenance work/asset source fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the Work/Asset projection and keeps both composed reads intact',async()=>{
  const {readFileSync}=await import('node:fs');
  const source=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
  const tokens=(s:string)=>s.replace(/\s+/g,' ').trim();
  expect(source.match(/export const workAssetSource = \(fields: string, assetJoin: string, where: string\) =>/g)).toHaveLength(1);
  expect(source.match(/`SELECT \$\{fields\} FROM maintenance_work_orders w JOIN \(\$\{assetsProjectionSources\.operational_assets\}\) a ON a\.id=w\.asset_id\$\{assetJoin\} WHERE \$\{where\}`/g)).toHaveLength(1);
  // r17 left 8 work.ts SELECTs; r18 folds the items/downtime Work/Asset source (8 -> 7);
  // r21 folds the assets/nodes/consumptions projection source scans (7 -> 5);
  // r24 reuses the existsGuardPrefix head for the Work assets absent-source guard (5 -> 4);
  // r25 reuses the shared row-projection head for the row scan (4 -> 3);
  // r27 routes the Work event-history read through the shared settlement
  // ref-source head (3 -> 2).
  expect(source.match(/SELECT/g)).toHaveLength(2);
  expect(source).not.toContain('SELECT w.*,a.code asset_code,a.name asset_name,CASE WHEN');
  expect(source).not.toContain('`SELECT w.id,w.version::text version');
  expect(source).not.toMatch(/LOCK\s+TABLE/i);
  const {workAssetSource}=await import('../../../packages/server-maintenance/src/adapters/work');
  const {finalRootOwnerProjectionSources:assets}=await import('../../../packages/server-assets/src/public/final-root-owner-projection-sources');
  // Pre-r18 literals rebuilt verbatim with closed placeholder fragments.
  const itemFields='w.*,a.code asset_code,a.name asset_name,CASE WHEN <guard> THEN 1 ELSE 0 END owner_eligible,CASE WHEN <review> THEN 1 ELSE 0 END can_review,<ownerName> owner_name';
  expect(tokens(workAssetSource(itemFields,'','<scope>'))).toBe(
   tokens(`SELECT ${itemFields} FROM maintenance_work_orders w JOIN (${assets.operational_assets}) a ON a.id=w.asset_id WHERE <scope>`));
  const downFields='w.id,w.version::text version,w.status,w.title,a.code asset_code,a.name asset_name,w.downtime_started_at::text downtime_started_at,w.downtime_ended_at::text downtime_ended_at';
  expect(tokens(workAssetSource(downFields,' AND a.company_id=w.company_id','<where>'))).toBe(
   tokens(`SELECT ${downFields}
   FROM maintenance_work_orders w JOIN (${assets.operational_assets}) a ON a.id=w.asset_id AND a.company_id=w.company_id
   WHERE <where>`));
  // r3-r17 families stay unchanged.
  expect(source).toContain('iamScopeGuardSql(workIamFrom');
  expect(source).toContain("maintenanceOwnerName('w')");
  expect(readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
  expect(readFileSync(new URL(`${dir}finance-record-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
  expect(readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
  expect(readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance Asset settlement reference
 * source family (r19). The preventive-plan and maintenance-parts refs share one
 * owner-local company/asset projection template. Both composed strings stay
 * byte-identical to their pre-dedupe literals, no duplicate TS query literal
 * remains, every r3-r18 family stays unchanged and every owner lock is intact. */
describe('Maintenance asset settlement ref source fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the settlement ref template and keeps both refs byte-identical',async()=>{
  const {readFileSync}=await import('node:fs');
  const source=readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8');
  // Only one fixed SELECT template remains in the file (pre-dedupe had two).
  expect(source.match(/SELECT/g)).toHaveLength(1);
  expect(source.match(/export const settlementRefSource = \(columns: string, source: string, tail = ""\): string =>/g)).toHaveLength(1);
  expect(source.match(/`SELECT \$\{columns\} FROM \$\{source\}\$\{tail\}`/g)).toHaveLength(1);
  expect(source).not.toContain('"SELECT company_id,asset_id FROM maintenance_preventive_plans"');
  expect(source).not.toContain('`SELECT consumption.company_id,work.asset_id FROM (');
  // The owner lock is untouched and no write/row-lock statement was introduced.
  expect(source).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans IN SHARE MODE');
  expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);

  const {settlementRefSource,maintenanceAssetSettlementRefs}=await import('../../../packages/server-maintenance/src/adapters/asset-settlement-refs');
  const {maintenanceSettlementConsumptions}=await import('../../../packages/server-inventory/src/public/maintenance-settlement-sources');
  // Pre-r19 literals rebuilt verbatim with closed column/source fragments.
  const preventive='SELECT company_id,asset_id FROM maintenance_preventive_plans';
  const parts=`SELECT consumption.company_id,work.asset_id FROM (${maintenanceSettlementConsumptions}) consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id`;
  expect(settlementRefSource('company_id,asset_id','maintenance_preventive_plans')).toBe(preventive);
  expect(settlementRefSource('consumption.company_id,work.asset_id',`(${maintenanceSettlementConsumptions}) consumption LEFT JOIN maintenance_work_orders work ON work.id=consumption.source_id AND work.company_id=consumption.company_id`)).toBe(parts);
  expect(maintenanceAssetSettlementRefs.preventive).toBe(preventive);
  expect(maintenanceAssetSettlementRefs.parts).toBe(parts);
  for(const sql of [preventive,parts]){
   expect(sql).toContain('SELECT ');
   expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  }
  // r3-r18 families and their locks stay untouched.
  const w=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
  expect(w).toContain('export const workAssetSource');
  expect(readFileSync(new URL(`${dir}attachment-owner-version-guard.ts`,import.meta.url),'utf8')).toContain('export const attachmentOwnerVersionGuardSql');
  expect(readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8')).toContain('export const projectionLookup');
  expect(readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8')).toContain('const settlementEventRows');
 });
});

/** r27: the Work event-history read shares the owner-local full-row SELECT head
 * with the settlement reference family. The composed history SQL stays
 * byte-identical to its pre-dedupe literal, no duplicate TS query literal
 * remains, and no r20-r26 family or lock/native read is touched. */
describe('Maintenance Work event-history head reuse fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the full-row head and keeps the Work history read byte-identical',async()=>{
  const {readFileSync}=await import('node:fs');
  const shared=readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8');
  const work=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
  expect(shared.split('export const settlementRefSource =').length-1).toBe(1);
  expect(shared.match(/SELECT/g)).toHaveLength(1);
  expect(work).not.toContain("'SELECT * FROM maintenance_work_events WHERE work_order_id=? ORDER BY version'");
  expect(work).toContain('settlementRefSource("*","maintenance_work_events"," WHERE work_order_id=? ORDER BY version")');
  expect(work).not.toMatch(/LOCK\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);

  const {settlementRefSource}=await import('../../../packages/server-maintenance/src/adapters/asset-settlement-refs');
  const history=settlementRefSource("*","maintenance_work_events"," WHERE work_order_id=? ORDER BY version");
  expect(history).toBe('SELECT * FROM maintenance_work_events WHERE work_order_id=? ORDER BY version');
  // Existing closed callers stay byte-identical with the default empty tail.
  expect(settlementRefSource("*","maintenance_work_events")).toBe('SELECT * FROM maintenance_work_events');
  expect(settlementRefSource('company_id,asset_id','maintenance_preventive_plans')).toBe('SELECT company_id,asset_id FROM maintenance_preventive_plans');
  // r20-r26 families stay untouched.
  expect(readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8')).toContain('export const projectionHead');
  expect(readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8')).toContain('export const factRow=');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance package-obligations facts
 * family (r20). The Work, Plan and occurrence UNION ALL branches share one
 * owner-local fact-row projection head. Every composed row stays byte-identical
 * to its pre-dedupe literal, no duplicate TS query literal remains, the total
 * read-tail drops strictly below the r20 BEFORE=32 baseline, every r3-r19 family
 * stays unchanged and every owner lock is untouched. */
describe('Maintenance facts UNION fact-row fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the fact-row head and keeps all three rows byte-identical',async()=>{
  const {readFileSync}=await import('node:fs');
  const source=readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8');
  // One fixed SELECT template for the family; the three inline row literals are gone.
  expect(source.match(/export const factRow=\(kind:string,columns:string,from:string\)=>settlementRefSource\(`\$\{kind\},\$\{columns\}`,from\);/g)).toHaveLength(1);
  expect(source.match(/factRow\(/g)).toHaveLength(3);
  expect(source).not.toContain("SELECT 'work' kind,work.id");
  expect(source).not.toContain("SELECT 'preventive-plan',plan.id");
  expect(source).not.toContain("SELECT 'occurrence',occurrence.id");
  // r20 folded three SELECT literals into one template: this file is 8 -> 6 and
  // the owner read-tail total is 32 -> 30 (strictly below BEFORE=32); r26
  // delegated settlementEventRows to settlementRefSource (this file 5 -> 4,
  // owner total 23 -> 22, strictly below BEFORE=23); r28 delegated factRow to
  // settlementRefSource (this file 4 -> 3, owner total 21 -> 20, below BEFORE=21);
  // r30 folds the Work/Plan alias-qualified row head into aliasRowsSql (this file
  // 3 -> 1, owner total 19 -> 18, below BEFORE=19); r31 routes the Calendar head
  // through that same renderer (owner total 18 -> 17, below BEFORE=18).
  expect(source.match(/SELECT/g)).toHaveLength(1);
  // The owner lock and the scope CTE stay untouched; no write/row-lock introduced.
  expect(source).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
  expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);

  const {factRow,maintenancePackageSettlementSources,scopedSourceCte}=await import('../../../packages/server-maintenance/src/adapters/package-obligations');
  const {projectionLookup}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
  const {assetPackageSettlementSources:as}=await import('../../../packages/server-assets/src/public/package-obligations');
  const {work}=maintenancePackageSettlementSources;
  // Pre-r20 literals rebuilt verbatim with closed kind/column/from fragments.
  const workRow="SELECT 'work' kind,work.id,work.version,work.status state,NOT work.source_valid OR work.status NOT IN ('completed','cancelled') OR work.downtime_started_at IS NOT NULL AND work.downtime_ended_at IS NULL pending FROM work";
  const planRow="SELECT 'preventive-plan',plan.id,plan.version,plan.status,NOT plan.source_valid OR plan.status<>'cancelled' FROM plans plan";
  const occFrom=`maintenance_preventive_occurrences occurrence JOIN plans plan ON plan.id=occurrence.plan_id LEFT JOIN (${work}) generated ON generated.id=occurrence.work_order_id AND generated.company_id=plan.company_id WHERE generated.id IS NULL OR generated.asset_id<>plan.asset_id OR occurrence.plan_version>plan.version OR NOT generated.source_valid OR occurrence.source_asset_version>${projectionLookup("version",as.assets,"actual","actual.id=plan.asset_id AND actual.company_id=plan.company_id")}`;
  const occRow=`SELECT 'occurrence',occurrence.id,occurrence.plan_version,'ambiguous-source',true FROM ${occFrom}`;
  expect(factRow("'work' kind","work.id,work.version,work.status state,NOT work.source_valid OR work.status NOT IN ('completed','cancelled') OR work.downtime_started_at IS NOT NULL AND work.downtime_ended_at IS NULL pending","work")).toBe(workRow);
  expect(factRow("'preventive-plan'","plan.id,plan.version,plan.status,NOT plan.source_valid OR plan.status<>'cancelled'","plans plan")).toBe(planRow);
  expect(factRow("'occurrence'","occurrence.id,occurrence.plan_version,'ambiguous-source',true",occFrom)).toBe(occRow);
  for(const sql of [workRow,planRow,occRow]){
   expect(sql).toContain('SELECT ');
   expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  }
  // r3-r19 families and their locks stay untouched.
  expect(source).toContain('const ownerSourceSql = (table:string,alias:string) =>');
  expect(source).toContain('const settlementEventRows=');
  expect(source).toContain('const workSourceExists=');
  expect(source).toContain('export const scopedSourceCte=');
  expect(scopedSourceCte('work','<src>','<extra>')).toBe('SELECT work.* FROM (<src>) work,scope WHERE work.company_id=scope.company_id AND <extra>');
  const read=readFileSync(new URL(`${dir}asset-settlement-refs.ts`,import.meta.url),'utf8');
  expect(read).toContain('export const settlementRefSource');
  const wsrc=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
  expect(wsrc).toContain('export const workAssetSource');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance projection row-scan family
 * (r21). The Work assets, org-nodes and consumptions reads share one owner-local
 * top-level `SELECT <fields> FROM (<source>) <alias> <tail>` skeleton. All three
 * composed strings stay byte-identical to their pre-dedupe literals, no duplicate
 * TS query literal remains, the retired count drops strictly below BEFORE=30,
 * every r3-r20 family stays unchanged and every owner lock is untouched. */
describe('Maintenance projection row-scan fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the row-scan head and keeps all three composed reads byte-identical',async()=>{
  const {readFileSync}=await import('node:fs');
  const source=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
  // The row scan reuses the shared head instead of carrying its own literal.
  expect(source.match(/export const projectionRowScan = \(fields: string, source: string, alias: string, tail: string\) =>\s*\n\s*projectionHead\(fields, source, alias, tail\)/g)).toHaveLength(1);
  expect(source.match(/projectionRowScan\(/g)).toHaveLength(3);
  expect(source).not.toContain('`SELECT a.id,a.code,a.name,a.version,a.condition,a.lifecycle_status FROM (');
  expect(source).not.toContain('`SELECT id,kind,display_name,branch_id FROM (');
  expect(source).not.toContain('`SELECT p.*,j.status journal_status,product.name product_name FROM (');
  // r18 left 7 work.ts SELECTs; r21 folds three row scans into one template (7 -> 5);
  // r24 reuses the existsGuardPrefix head for the Work assets absent-source guard (5 -> 4);
  // r25 reuses the shared row-projection head for the row scan (4 -> 3);
  // r27 routes the Work event-history read through the shared settlement
  // ref-source head (3 -> 2).
  expect(source.match(/SELECT/g)).toHaveLength(2);
  // No write, row-lock or native read is accepted or changed.
  expect(source).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

  const {projectionRowScan}=await import('../../../packages/server-maintenance/src/adapters/work');
  // Pre-r21 literals rebuilt verbatim with closed fragment placeholders.
  const scope='<assetScope>';
  const preAssets=`SELECT a.id,a.code,a.name,a.version,a.condition,a.lifecycle_status FROM (<assets>) a WHERE ${scope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND NOT EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`;
  const preNodes=`SELECT id,kind,display_name,branch_id FROM (<org>) n WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=?) ORDER BY kind,display_name`;
  const preConsumptions=`SELECT p.*,j.status journal_status,product.name product_name FROM (<inv>) p JOIN (<fin>) j ON j.id=p.journal_id JOIN (<com>) product ON product.id=p.product_id WHERE p.source_kind='maintenance' AND p.source_id=? ORDER BY p.created_at,p.id`;
  expect(projectionRowScan('a.id,a.code,a.name,a.version,a.condition,a.lifecycle_status','<assets>','a',`WHERE ${scope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND NOT EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`)).toBe(preAssets);
  expect(projectionRowScan('id,kind,display_name,branch_id','<org>','n',`WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=?) ORDER BY kind,display_name`)).toBe(preNodes);
  expect(projectionRowScan('p.*,j.status journal_status,product.name product_name','<inv>','p',`JOIN (<fin>) j ON j.id=p.journal_id JOIN (<com>) product ON product.id=p.product_id WHERE p.source_kind='maintenance' AND p.source_id=? ORDER BY p.created_at,p.id`)).toBe(preConsumptions);
  for(const sql of [preAssets,preNodes,preConsumptions]){
   expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|pg_advisory/i);
  }
  // r3-r20 families and their locks stay untouched.
  expect(source).toContain('export const workAssetSource');
  expect(source).toContain('iamScopeGuardSql(workIamFrom');
  expect(readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8')).toContain('export const projectionLookup');
  expect(readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8')).toContain('export const factRow=');
  expect(readFileSync(new URL(`${dir}purchase-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE maintenance_work_orders,maintenance_preventive_plans,maintenance_preventive_occurrences IN SHARE ROW EXCLUSIVE MODE');
  expect(readFileSync(new URL(`${dir}finance-record-sources.ts`,import.meta.url),'utf8')).toContain('LOCK TABLE operational_inventory_consumptions,maintenance_work_orders,operational_assets IN SHARE MODE');
 });
});

/** Fixed-SQL read-tail oracle for the Maintenance EXISTS head + owner-name family
 * (r22). The attachment owner-version guard, the IAM scope guard and the
 * package-settlement upstream-source witness share one owner-local EXISTS head,
 * and the owner display-name image reuses the scalar projectionLookup. All four
 * composed strings stay byte-identical to their pre-dedupe literals, no duplicate
 * TS query literal remains, the retired count drops strictly below BEFORE=28,
 * every r3-r21 family stays unchanged and every owner lock is untouched. */
describe('Maintenance EXISTS head and owner-name reuse fixed SQL',()=>{
 const dir='../../../packages/server-maintenance/src/adapters/';
 it('single-sources the EXISTS head and owner-name lookup, byte-identically',async()=>{
  const {readFileSync}=await import('node:fs');
  const shared=readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8');
  const att=readFileSync(new URL(`${dir}attachment-owner-version-guard.ts`,import.meta.url),'utf8');
  const iam=readFileSync(new URL(`${dir}iam-scope-guard.ts`,import.meta.url),'utf8');
  const pkg=readFileSync(new URL(`${dir}package-obligations.ts`,import.meta.url),'utf8');
  const owner=readFileSync(new URL(`${dir}maintenance-owner-name.ts`,import.meta.url),'utf8');
  expect(shared.split('export const existsGuardPrefix =').length-1).toBe(1);
  expect(shared).toContain('`EXISTS(SELECT 1 FROM ${source} WHERE `');
  for(const s of [att,iam,pkg]) expect(s).not.toContain('EXISTS(SELECT 1 FROM');
  expect(att).toContain('${existsGuardPrefix(table)}');
  expect(iam).toContain('${existsGuardPrefix(from)}');
  expect(pkg).toContain('${existsGuardPrefix(`${source} ${alias}`)}');
  expect(owner.match(/SELECT/g)).toBeNull();
  expect(owner).toContain('projectionLookup("legal_name", people_payrollProjectionSources.hr_people, "hr_people"');
  // Locks untouched; no native read or write was introduced.
  expect(pkg).toContain('LOCK TABLE maintenance_work_orders,maintenance_work_events,maintenance_preventive_plans,maintenance_preventive_events,maintenance_preventive_occurrences IN SHARE MODE');
  for(const s of [att,iam,shared,owner]) expect(s).not.toMatch(/LOCK\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+\s+SET|FOR\s+UPDATE/i);

  const {existsGuardPrefix}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
  const {attachmentOwnerVersionGuardSql}=await import('../../../packages/server-maintenance/src/adapters/attachment-owner-version-guard');
  const {iamScopeGuardSql}=await import('../../../packages/server-maintenance/src/adapters/iam-scope-guard');
  const {maintenanceOwnerName}=await import('../../../packages/server-maintenance/src/adapters/maintenance-owner-name');
  const {finalRootOwnerProjectionSources:pp}=await import('../../../packages/server-people_payroll/src/public/final-root-owner-projection-sources');
  // Pre-r22 literals rebuilt verbatim with closed fragments.
  expect(existsGuardPrefix('t')).toBe('EXISTS(SELECT 1 FROM t WHERE ');
  expect(attachmentOwnerVersionGuardSql('maintenance_work_orders')).toBe(
   `EXISTS(SELECT 1 FROM maintenance_work_orders WHERE id=? AND company_id=? AND version=?)`);
  expect(attachmentOwnerVersionGuardSql('maintenance_preventive_plans','extra_predicate')).toBe(
   `EXISTS(SELECT 1 FROM maintenance_preventive_plans WHERE id=? AND company_id=? AND version=? AND extra_predicate)`);
  expect(iamScopeGuardSql('SRC','a','p','e','r','?','records.write','SCOPE')).toBe(
   `EXISTS(SELECT 1 FROM SRC WHERE a.company_id=r.company_id AND a.user_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'records.write') AND SCOPE AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`);
  expect(existsGuardPrefix('maintenance_preventive_occurrences occurrence')+'occurrence.work_order_id=work.id)').toBe(
   `EXISTS(SELECT 1 FROM maintenance_preventive_occurrences occurrence WHERE occurrence.work_order_id=work.id)`);
  expect(maintenanceOwnerName('w')).toBe(
   `(SELECT legal_name FROM (${pp.hr_people}) hr_people WHERE company_id=w.company_id AND user_id=w.owner_id AND merged_into_id IS NULL LIMIT 1)`);
 });
});

/** r24: the Work assets absent-source guard reuses the single EXISTS witness
 * head through a negation wrapper, so no second query head is carried in the
 * TS read tail. The composed guard stays byte-identical to its pre-dedupe
 * literal; no lock, write or native read is introduced. */
describe('Maintenance negated EXISTS head reuse fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the NOT EXISTS head and keeps the composed assets read byte-identical',async()=>{
   const {readFileSync}=await import('node:fs');
   const shared=readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8');
   const work=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
   expect(shared.split('export const notExistsGuardPrefix =').length-1).toBe(1);
   expect(shared).toContain('`NOT ${existsGuardPrefix(source)}`');
   expect(shared.split('EXISTS(SELECT 1 FROM').length-1).toBe(1);
   expect(work).not.toContain('AND NOT EXISTS(SELECT 1 FROM');
   expect(work).toContain("AND ${notExistsGuardPrefix('maintenance_work_orders w')}w.asset_id=a.id");
   expect(work).not.toMatch(/LOCK\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+\s+SET|FOR\s+UPDATE/i);

   const {notExistsGuardPrefix}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
   // Pre-r24 literal rebuilt verbatim with a closed scope fragment.
   const scope='<assetScope>';
   const preTail=`WHERE ${scope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND NOT EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`;
   const tail=`WHERE ${scope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND ${notExistsGuardPrefix('maintenance_work_orders w')}w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`;
   expect(tail).toBe(preTail);
   expect(tail).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE|pg_advisory/i);
  });
 });

/** r25: the scalar projection lookup and the top-level multi-row scan share one
 * owner-local row-projection head, so no second query head is carried in the TS
 * read tail. Both composed strings stay byte-identical to their pre-dedupe
 * literals; no lock, write or native read is introduced. */
describe('Maintenance row-projection head reuse fixed SQL',()=>{
  const dir='../../../packages/server-maintenance/src/adapters/';
  it('single-sources the row-projection head and keeps both composed reads byte-identical',async()=>{
   const {readFileSync}=await import('node:fs');
   const shared=readFileSync(new URL(`${dir}projection-lookup.ts`,import.meta.url),'utf8');
   const work=readFileSync(new URL(`${dir}work.ts`,import.meta.url),'utf8');
   expect(shared.split('export const projectionHead =').length-1).toBe(1);
   expect(shared).toContain('`SELECT ${fields} FROM (${source}) ${alias} ${tail}`');
   expect(shared).toContain('`(${projectionHead(select, source, alias, `WHERE ${where}`)})`');
   expect(work).toContain('projectionHead(fields, source, alias, tail)');
   expect(work).not.toContain('`SELECT ${fields} FROM (${source}) ${alias} ${tail}`');
   expect(work).not.toMatch(/LOCK\s+TABLE|INSERT\s+INTO|UPDATE\s+\w+\s+SET|FOR\s+UPDATE/i);

   const {projectionHead,projectionLookup}=await import('../../../packages/server-maintenance/src/adapters/projection-lookup');
   const {projectionRowScan}=await import('../../../packages/server-maintenance/src/adapters/work');
   // Pre-r25 literals rebuilt verbatim with closed fragments.
   expect(projectionHead('f','src','a','WHERE x=?')).toBe('SELECT f FROM (src) a WHERE x=?');
   expect(projectionLookup('x','t','a','a.id=?')).toBe('(SELECT x FROM (t) a WHERE a.id=?)');
   expect(projectionRowScan('a.id,a.code','<src>','a','WHERE a.company_id=? ORDER BY a.code')).toBe('SELECT a.id,a.code FROM (<src>) a WHERE a.company_id=? ORDER BY a.code');
   for(const sql of [projectionLookup('x','t','a','a.id=?'),projectionRowScan('f','src','a','WHERE x=?')]){
    expect(sql).not.toMatch(/INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE|pg_advisory/i);
   }
  });
 });
