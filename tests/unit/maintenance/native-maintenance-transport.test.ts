import {describe,it,expect,vi,beforeEach} from 'vitest';
import {
  createNativeMaintenanceClient,
  nativeMaintenanceConfiguration,
  type NativeMaintenancePageContext,
} from '../../../packages/feature-assets/src/adapters/server/native-maintenance';
import { nativeMaintenanceAvailable } from '../../../packages/feature-assets/src/adapters/server/native-maintenance-page';
vi.mock('server-only',()=>({}));
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
