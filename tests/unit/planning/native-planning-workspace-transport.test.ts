import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers()}));
import {
  createNativePlanningClient,
  nativePlanningAvailable,
  nativePlanningConfiguration,
  NativePlanningError,
  type NativePlanningPageContext,
} from '../../../packages/feature-manufacturing/src/public/native-planning-workspace';
import { nativePlanningFailure } from '../../../packages/feature-manufacturing/src/public/native-planning-workspace-page';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativePlanningPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['planning'],
  iam:{permissions:['records.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',
  branchId:'branch',departmentId:null,branchSlug:'north',departmentSlug:null,
  assignmentId:'assignment',assignmentVersion:3,revision:7,role:'owner',
  assignmentScopeKind:'branch',modules:['planning'],permissions:['records.read'],
  expiresAt:9_007_199_254_740_991};
const shortage={productId:'material',requiredMilli:4000,availableMilli:1000,
  incomingMilli:2000,shortageMilli:1000};
const scenario={id:'scenario',company_id:'company',branch_id:'branch',
  warehouse_id:'warehouse',specification_id:'spec',specification_version:2,
  demand_quantity_milli:2000,due_date:'2026-10-04',capacity_minutes:120,
  status:'calculated',created_by:'planner',version:1,created_at:100,
  updated_at:100,shortages:[shortage],context:{incomingSupply:[],stock:[],
    allocations:[{resourceId:'resource',workDate:'2026-10-04',minutes:60,
      resourceVersion:1,calendarVersion:0,availableMinutes:120}],
    operations:[],capacityBasis:'approved_routing_resource_calendar',
    comparison:{scenarioId:'other',sourceVersion:2,shortageDeltaMilli:-500,
      requiredMinutesDelta:30,feasibleDate:'2026-10-05',
      currentFeasibleDate:'2026-10-04'}}};
const recommendation={id:'recommendation',company_id:'company',branch_id:'branch',
  scenario_id:'scenario',scenario_version:1,recommended_quantity_milli:2000,
  required_minutes:60,feasible_date:'2026-10-04',status:'proposed',
  created_by:'planner',reviewed_by:null,version:1,created_at:100,updated_at:100};
const resource={id:'resource',company_id:'company',branch_id:'branch',code:'CELL',
  name:'Assembly cell',daily_minutes:480,work_weekdays:[1,2,3,4,5],
  status:'active',version:1,created_by:'planner',created_at:10,updated_at:10};
const calendar={resource_id:'resource',work_date:'2026-10-04',
  available_minutes:60,reason:'Verified shutdown',version:1,updated_by:'planner',
  updated_at:20};
const allocation={id:'allocation',scenario_id:'scenario',resource_id:'resource',
  work_date:'2026-10-04',minutes:60,created_by:'planner',created_at:30};
const data={scenarios:[scenario],recommendations:[recommendation],
  resources:[resource],calendar:[calendar],allocations:[allocation]};
const input={holdingSlug:'acme',branchSlug:'north',
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Planning workspace transport',()=>{
 it('accepts one bounded contributor Holding port and prefers the forwarded host',async()=>{
  const contributor={...JSON.parse(config()),holdings:[{origin:'https://acme.example.com:3443',holdingSlug:'acme',holdingId:'h001'}]};
  const transport=vi.fn().mockResolvedValue(ok({data,context}));
  const client=createNativePlanningClient(nativePlanningConfiguration(JSON.stringify(contributor)),transport);
  await client.workspace(incoming({host:'127.0.0.1:3215','x-forwarded-host':'acme.example.com:3443'}),actor,input);
  expect(transport.mock.calls[0][2].get('host')).toBe('acme.example.com:3443');
  for(const origin of ['https://acme.example.com:0','https://acme.example.com:65536','https://acme.example.com:abc'])
    expect(()=>createNativePlanningClient(nativePlanningConfiguration(config({holdings:[{origin,holdingSlug:'acme',holdingId:'h001'}]})),vi.fn())).toThrow();
 });

 it('forwards the exact configured Holding, session, context and branch to the owner read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data,context}));
  const client=createNativePlanningClient(nativePlanningConfiguration(config()),transport);
  const result=await client.workspace(incoming(),actor,input);
  expect(result).toEqual(data);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/planning/workspace');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PLANNING_SOURCE_INVALID',message:'Reload the current planning workspace.'}}),
    {status:503,headers:{'content-type':'application/json'}},
  ));
  const client=createNativePlanningClient(nativePlanningConfiguration(config()),transport);
  await expect(client.workspace(incoming(),actor,input)).rejects.toMatchObject({
    code:'PLANNING_SOURCE_INVALID',status:503,
    message:'Reload the current planning workspace.',
  });
 });

 it('rejects a changed context, foreign Company, wrong branch and malformed rows before returning data',async()=>{
  const changed=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data,context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_CONTEXT_CHANGED'});
  const foreign=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,scenarios:[{...scenario,company_id:'foreign'}]},context}),
  ));
  await expect(foreign.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const wrongBranch=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,scenarios:[{...scenario,branch_id:'other'}]},context}),
  ));
  await expect(wrongBranch.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const orphan=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,recommendations:[{...recommendation,scenario_id:'missing'}]},context}),
  ));
  await expect(orphan.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const duplicateRecommendation=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,recommendations:[recommendation,{...recommendation,id:'recommendation-2'}]},context}),
  ));
  await expect(duplicateRecommendation.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const duplicate=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,scenarios:[scenario,{...scenario,version:2}]},context}),
  ));
  await expect(duplicate.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const badShortage=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,scenarios:[{...scenario,shortages:[{...shortage,shortageMilli:999}]}]},context}),
  ));
  await expect(badShortage.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const extra=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,extra:[]},context}),
  ));
  await expect(extra.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
 });

 it('accepts an owner-cancelled rejected recommendation but rejects independent self-approval',async()=>{
  const ownerCancelled={...recommendation,status:'rejected',reviewed_by:'planner',version:2};
  const accepted=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,recommendations:[ownerCancelled]},context}),
  ));
  await expect(accepted.workspace(incoming(),actor,input))
    .resolves.toMatchObject({recommendations:[{status:'rejected',reviewed_by:'planner'}]});
  const selfApproved=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,recommendations:[{...recommendation,status:'approved',reviewed_by:'planner',version:2}]},context}),
  ));
  await expect(selfApproved.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
 });

 it('rejects a self-reviewed recommendation and an unreleased allocation with an unknown resource',async()=>{
  const selfReviewed=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,recommendations:[{...recommendation,status:'approved',reviewed_by:'planner',version:2}]},context}),
  ));
  await expect(selfReviewed.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
  const orphanAllocation=createNativePlanningClient(nativePlanningConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,allocations:[{...allocation,resource_id:'missing'}]},context}),
  ));
  await expect(orphanAllocation.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PLANNING_RESPONSE_INVALID'});
 });

 it('fails closed without a selector and rejects reserved ports, insecure origins and malformed configuration',()=>{
  const previous=process.env.ERPX_NATIVE_PLANNING_CONFIG;
  delete process.env.ERPX_NATIVE_PLANNING_CONFIG;
  expect(nativePlanningAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_PLANNING_CONFIG=previous;
  expect(()=>createNativePlanningClient(nativePlanningConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativePlanningClient(nativePlanningConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativePlanningClient(nativePlanningConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativePlanningConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativePlanningFailure(new NativePlanningError('NATIVE_PLANNING_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativePlanningFailure(new NativePlanningError('NATIVE_PLANNING_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'PLANNING_REQUEST_INVALID'});
  expect(nativePlanningFailure(new NativePlanningError('NATIVE_PLANNING_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_PLANNING_UNAVAILABLE'});
  expect(nativePlanningFailure(new NativePlanningError('PLANNING_SOURCE_INVALID',503,'Reload current planning.')))
    .toMatchObject({status:503,code:'NATIVE_PLANNING_UNAVAILABLE'});
  expect(nativePlanningFailure(new Error('other'))).toBeUndefined();
 });
});
