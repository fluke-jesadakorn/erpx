import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.example.com',cookie:'__Host-erpx-product=session'})}));
vi.mock('@erpx/server-talent/public/purchases',()=>({talentPurchaseControls:vi.fn(async()=>undefined)}));
import {
  createNativeTalentClient,
  nativeTalentAvailable,
  nativeTalentConfiguration,
  NativeTalentError,
  type NativeTalentPageContext,
} from '../../../packages/feature-talent/src/public/native-talent-requisition';
import {
  nativeTalentFailure,
  nativeTalentRequisitionCommand,
  nativeTalentWorkspace,
} from '../../../packages/feature-talent/src/public/native-talent-page';
import {nativeTalentLifecycleCommand} from '../../../packages/server-talent/src/public/native-talent-lifecycle-page';
import {talentPurchaseControls} from '../../../packages/server-talent/src/public/purchases';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=()=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
});
const actor:NativeTalentPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'people_manager',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['talent'],
  iam:{permissions:['records.read','records.write','records.review']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'people_manager',assignmentScopeKind:'branch',
  modules:['talent'],permissions:['records.read','records.write','records.review'],
  expiresAt:9_007_199_254_740_991};
const row={id:'talent-req-1',company_id:'company',branch_id:'branch',department_id:null,
  job_title:'Service specialist',openings:2,target_start_date:'2026-11-01',
  description:'Serve customer cases and complete field handoffs.',status:'draft',
  created_by:'creator',reviewed_by:null,version:1,created_at:1_799_999_999_000,
  updated_at:1_799_999_999_000,canApprove:true,canReject:true,canOpen:false,
  canClose:false,canCancel:true};
const event={version:1,action:'create',actor_id:'creator',reason:'Hiring need drafted',
  status:'draft',created_at:1_799_999_999_000};
const input={holdingSlug:'acme',branchSlug:'north',
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Talent requisition transport',()=>{
 it('forwards the exact workspace Host, session and context CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{requisitions:[row]},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const result=await talent.workspace(incoming(),actor,input);
  expect(result.requisitions).toEqual([row]);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the exact requisition detail path and history',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{requisition:row,events:[event]},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const result=await talent.detail(incoming(),actor,'talent-req-1',input);
  expect(result.events).toEqual([event]);
  const [method,url]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions/talent-req-1');
 });

 it('forwards the exact create and change bodies with one idempotency key',async()=>{
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:{id:'talent-req-1',status:'draft',version:1},context}))
    .mockResolvedValueOnce(ok({data:{id:'talent-req-1',status:'approved',version:2},context}));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  const create={jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',
    description:'Serve customer cases and complete field handoffs.'};
  expect(await talent.create(incoming(),actor,JSON.stringify(create),'talent-requisition-command-1',input))
    .toEqual({id:'talent-req-1',status:'draft',version:1});
  const [,createUrl,createHeaders,createBody]=transport.mock.calls[0];
  expect(createUrl.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions');
  expect(createHeaders.get('idempotency-key')).toBe('talent-requisition-command-1');
  expect(createHeaders.get('content-type')).toBe('application/json');
  expect(JSON.parse(createBody!)).toEqual(create);
  const change={action:'approve',expectedVersion:1,reason:'Headcount and scope reviewed'};
  expect(await talent.change(incoming(),actor,'talent-req-1',JSON.stringify(change),'talent-requisition-command-2',input))
    .toEqual({id:'talent-req-1',status:'approved',version:2});
  const [changeMethod,changeUrl,changeHeaders,changeBody]=transport.mock.calls[1];
  expect(changeMethod).toBe('POST');
  expect(changeUrl.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/talent/requisitions/talent-req-1');
  expect(changeHeaders.get('idempotency-key')).toBe('talent-requisition-command-2');
  expect(JSON.parse(changeBody!)).toEqual(change);
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'CONFLICT',message:'Reload the current requisition decision.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const talent=createNativeTalentClient(nativeTalentConfiguration(config()),transport);
  await expect(talent.change(incoming(),actor,'talent-req-1','{}','talent-requisition-command-3',input))
    .rejects.toMatchObject({code:'CONFLICT',status:409,message:'Reload the current requisition decision.'});
 });

 it('rejects foreign scope, duplicate rows, extra keys and changed context',async()=>{
  const foreign=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[{...row,company_id:'foreign'}]},context}),
  ));
  await expect(foreign.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const duplicate=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[row,row]},context}),
  ));
  await expect(duplicate.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const extra=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisition:{...row,extra:'field'},events:[event]},context}),
  ));
  await expect(extra.detail(incoming(),actor,'talent-req-1',input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_RESPONSE_INVALID'});
  const changed=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{requisitions:[]},context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_CONTEXT_CHANGED'});
  const unknownHost=createNativeTalentClient(nativeTalentConfiguration(config()),vi.fn());
  await expect(unknownHost.workspace(new Headers({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_TALENT_AUTHORITY_INVALID'});
 });

 it('fails closed when the native Talent configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  delete process.env.ERPX_NATIVE_TALENT_CONFIG;
  expect(nativeTalentAvailable()).toBe(false);
  expect(await nativeTalentWorkspace(actor as never)).toBeUndefined();
  expect(await nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',description:'Serve customer cases.'},
    'talent-requisition-command-9',
  )).toBeUndefined();
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('retains non-requisition commands and rejects ineligible requisition input when configured',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  expect(await nativeTalentRequisitionCommand(
    actor as never,
    {entity:'application',id:'talent-application-1',action:'interview',expectedVersion:1,reason:'Schedule interview'},
    'talent-requisition-command-10',
  )).toBeUndefined();
  await expect(nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',id:'talent-req-1',action:'send',expectedVersion:1,reason:'Invalid action'},
    'talent-requisition-command-11',
  )).rejects.toMatchObject({status:422,code:'TALENT_REQUISITION_INVALID'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('enforces the retained recruit.positions purchase preflight before a configured command',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  vi.mocked(talentPurchaseControls).mockResolvedValueOnce({
    'recruit.positions':{newWork:false,settleExisting:true,history:true,revision:'revision'},
  } as never);
  await expect(nativeTalentRequisitionCommand(
    actor as never,
    {entity:'requisition',jobTitle:'Service specialist',openings:2,targetStartDate:'2026-11-01',description:'Serve customer cases.'},
    'talent-requisition-command-12',
  )).rejects.toMatchObject({status:403,code:'TALENT_PURCHASE_REQUIRED'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('enforces the recruit lifecycle purchase preflight before a configured candidate create',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG=config();
  vi.mocked(talentPurchaseControls).mockResolvedValueOnce({
    'recruit.candidates':{newWork:false,settleExisting:true,history:true,revision:'revision'},
  } as never);
  await expect(nativeTalentLifecycleCommand(
    new Request('https://acme.example.com/api/v1/talent',{method:'POST',headers:incoming()}),
    actor as never,
    'acme',
    {entity:'candidate',legalName:'Candidate',email:'candidate@example.test',consentEvidence:'Consent',retentionUntil:'2027-01-01'},
    'talent-lifecycle-command-1',
  )).rejects.toMatchObject({status:403,code:'TALENT_PURCHASE_REQUIRED'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('maps a malformed configured selector to a bounded 503 without fallback',async()=>{
  const previous=process.env.ERPX_NATIVE_TALENT_CONFIG;
  process.env.ERPX_NATIVE_TALENT_CONFIG='{not-json';
  await expect(nativeTalentWorkspace(actor as never)).rejects.toMatchObject({
    status:503,code:'NATIVE_TALENT_UNAVAILABLE',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_TALENT_CONFIG;else process.env.ERPX_NATIVE_TALENT_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativeTalentClient(nativeTalentConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeTalentConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'TALENT_REQUISITION_INVALID'});
  expect(nativeTalentFailure(new NativeTalentError('NATIVE_TALENT_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_TALENT_UNAVAILABLE'});
  expect(nativeTalentFailure(new NativeTalentError('CONFLICT',409,'Reload.')))
    .toMatchObject({status:409,code:'CONFLICT'});
  expect(nativeTalentFailure(new Error('other'))).toBeUndefined();
 });
});
