import {describe,it,expect,vi} from 'vitest';
vi.mock('next/headers',()=>({headers:async()=>new Headers()}));
import {
  createNativeOperationsClient,
  nativeOperationsConfiguration,
  NativeOperationsError,
  type NativeOperationsPageContext,
} from '../../../packages/feature-operations/src/adapters/server/native-operations';
import {nativeOperationsAvailable} from '../../../packages/workplace/src/lib/native-operations';

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
const actor:NativeOperationsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},modules:['operations'],
  iam:{permissions:['records.read','records.write','records.review']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'branch',
  modules:['operations'],permissions:['records.read','records.write','records.review'],
  expiresAt:9_007_199_254_740_991};
const source={kind:'staff_task',id:'task-open',version:1,title:'Current task',href:'/tasks/task-open',
  label:'Task · Current task',companyId:'company',branchId:'branch',departmentId:null,status:'open'};
const exception={id:'exception-open',company_id:'company',branch_id:'branch',department_id:null,
  area:'service',source_kind:'staff_task',source_id:'task-open',source_version:1,
  source_reference_json:'{"kind":"staff_task","id":"task-open"}',policy_id:'policy-service',
  policy_snapshot_json:'{"id":"policy-service"}',title:'Open task',issue:'Synthetic issue',
  priority:'normal',status:'open',owner_id:'owner',escalation_owner_id:null,resolution:null,
  resolved_by:null,reviewed_by:null,version:1,created_by:'maker',created_at:0,updated_at:10,
  due_at:100,source};
const policy={id:'policy-service',company_id:'company',area:'service',name:'Service queue',
  revision:1,source_kinds_json:'["staff_task"]',target_minutes:60,status:'published',
  created_by:'maker',reviewed_by:'reviewer',version:2,created_at:0,updated_at:1};
const workspace={exceptions:[exception],policies:[policy],
  people:[{id:'owner',name:'Synthetic Owner'}],actorId:'user',canWrite:true,canReview:true,
  summary:{active:1,awaitingReview:0,escalated:0,overdue:1,byArea:{service:1}}};
const event={id:'event-open',exception_id:'exception-open',version:1,action:'open',
  actor_id:'maker',reason:'Synthetic issue',status:'open',details_json:'{}',created_at:0};
const detail={...workspace,row:exception,history:[event]};
const readInput={holdingSlug:'acme',branchSlug:'north',expected:{companyId:'company',
  assignmentId:'assignment',revision:7,assignmentVersion:3}};
const commandKey='command-key-01';
const openBody=JSON.stringify({entity:'exception',source:{kind:'staff_task',id:'task-open',version:1},
  policyId:'policy-service',policyVersion:2,title:'Open task',issue:'Synthetic issue',
  priority:'normal',ownerId:'owner'});
const changeBody=JSON.stringify({entity:'exception',id:'exception-open',action:'acknowledge',
  expectedVersion:1,reason:'Reviewed actual source'});
const draftBody=JSON.stringify({entity:'policy',area:'service',name:'Service queue',
  targetMinutes:60,sourceKinds:['staff_task']});
const reviewBody=JSON.stringify({entity:'policy',id:'policy-service',action:'publish',
  expectedVersion:2,reason:'Independent policy review'});
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const failed=(status:number,code:string)=>new Response(JSON.stringify({error:{code,message:'Synthetic failure'}}),{status,headers:{'content-type':'application/json'}});

describe('native Operations queue transport',()=>{
 it('forwards the registered Holding, session and current CAS to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  const result=await client.workspace(incoming(),actor,readInput);
  expect(result.exceptions[0]).toMatchObject({id:'exception-open',source:{label:'Task · Current task'}});
  expect(result.summary.overdue).toBe(1);
  const [method,url,requestHeaders,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/operations');
  expect(requestHeaders.get('cookie')).toBe('__Host-erpx-product=session');
  expect(requestHeaders.get('x-erpx-workspace-branch')).toBe('north');
  expect(requestHeaders.get('x-erpx-context-company')).toBe('company');
  expect(requestHeaders.get('x-erpx-context-assignment')).toBe('assignment');
  expect(requestHeaders.get('x-erpx-context-revision')).toBe('7');
  expect(requestHeaders.get('x-erpx-context-assignment-version')).toBe('3');
  expect(body).toBeUndefined();
 });

 it('reads detail by id and preserves the exact owner event history',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:detail,context}));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  const result=await client.detail(incoming(),actor,{...readInput,id:'exception-open'});
  expect(result.row.id).toBe('exception-open');
  expect(result.history).toEqual([event]);
  const [,url]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/operations?id=exception-open');
 });

 it('fails closed when the selected authority is absent or the response context changed',async()=>{
  const absent=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativeOperationsClient(JSON.parse(config()),absent);
  await expect(client.workspace(incoming(),{...actor,authority:undefined},readInput)).rejects.toMatchObject({code:'NATIVE_OPERATIONS_CONTEXT_CHANGED'});
  expect(absent).not.toHaveBeenCalled();
  const changed=vi.fn().mockResolvedValue(ok({data:workspace,context:{...context,revision:8}}));
  const changedClient=createNativeOperationsClient(JSON.parse(config()),changed);
  await expect(changedClient.workspace(incoming(),actor,readInput)).rejects.toMatchObject({code:'NATIVE_OPERATIONS_CONTEXT_CHANGED'});
 });

 it('rejects foreign source labels, unknown contexts and malformed workspaces',async()=>{
  const foreign={...exception,source:{...source,companyId:'other'}};
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:{...workspace,exceptions:[foreign]},context}))
    .mockResolvedValueOnce(ok({data:workspace,context:{...context,companyId:'other'}}))
    .mockResolvedValueOnce(ok({data:{...workspace,exceptions:[{...exception,extra:true}]},context}));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  for(let index=0;index<3;index++)await expect(client.workspace(incoming(),actor,readInput)).rejects.toBeInstanceOf(NativeOperationsError);
 });

 it('maps bounded owner errors without falling back',async()=>{
  const transport=vi.fn().mockResolvedValue(failed(409,'OPERATIONS_STATE_CHANGED'));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  await expect(client.workspace(incoming(),actor,readInput)).rejects.toMatchObject({
    code:'OPERATIONS_STATE_CHANGED',status:409,message:'Synthetic failure',
  });
 });

 it('forwards a command POST with the idempotency key, body and current CAS headers',async()=>{
  const receipt={id:`operations-exception-${commandKey}`,status:'open',version:1};
  const transport=vi.fn().mockResolvedValue(ok({data:receipt,context}));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  const result=await client.command(incoming(),actor,readInput,openBody,commandKey);
  expect(result).toEqual(receipt);
  const [method,url,requestHeaders,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/operations');
  expect(body).toBe(openBody);
  expect(requestHeaders.get('content-type')).toBe('application/json');
  expect(requestHeaders.get('idempotency-key')).toBe(commandKey);
  expect(requestHeaders.get('cookie')).toBe('__Host-erpx-product=session');
  expect(requestHeaders.get('x-erpx-context-company')).toBe('company');
  expect(requestHeaders.get('x-erpx-context-revision')).toBe('7');
 });

 it('validates change and policy receipts against the exact action, id and next version',async()=>{
  const changeReceipt={id:'exception-open',status:'acknowledged',version:2};
  const change=vi.fn().mockResolvedValue(ok({data:changeReceipt,context}));
  const client=createNativeOperationsClient(JSON.parse(config()),change);
  await expect(client.command(incoming(),actor,readInput,changeBody,commandKey)).resolves.toEqual(changeReceipt);
  const draftReceipt={id:`queue-policy-${commandKey}`,status:'draft',version:1};
  const draft=vi.fn().mockResolvedValue(ok({data:draftReceipt,context}));
  await expect(createNativeOperationsClient(JSON.parse(config()),draft)
    .command(incoming(),actor,readInput,draftBody,commandKey)).resolves.toEqual(draftReceipt);
  const reviewReceipt={id:'policy-service',status:'published',version:3};
  const review=vi.fn().mockResolvedValue(ok({data:reviewReceipt,context}));
  await expect(createNativeOperationsClient(JSON.parse(config()),review)
    .command(incoming(),actor,readInput,reviewBody,commandKey)).resolves.toEqual(reviewReceipt);
  const wrong=vi.fn().mockResolvedValue(ok({data:{...changeReceipt,status:'closed'},context}));
  await expect(createNativeOperationsClient(JSON.parse(config()),wrong)
    .command(incoming(),actor,readInput,changeBody,commandKey)).rejects.toMatchObject({code:'NATIVE_OPERATIONS_RESPONSE_INVALID'});
  const cursor=vi.fn().mockResolvedValue(ok({data:{...changeReceipt,version:3},context}));
  await expect(createNativeOperationsClient(JSON.parse(config()),cursor)
    .command(incoming(),actor,readInput,changeBody,commandKey)).rejects.toMatchObject({code:'NATIVE_OPERATIONS_RESPONSE_INVALID'});
 });

 it('rejects invalid commands and changed context before transport',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'x',status:'open',version:1},context}));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,readInput,'{"entity":"exception"}',commandKey))
    .rejects.toMatchObject({code:'NATIVE_OPERATIONS_REQUEST_INVALID'});
  await expect(client.command(incoming(),actor,readInput,openBody,'short'))
    .rejects.toMatchObject({code:'NATIVE_OPERATIONS_REQUEST_INVALID'});
  await expect(client.command(incoming(),{...actor,authority:undefined},readInput,openBody,commandKey))
    .rejects.toMatchObject({code:'NATIVE_OPERATIONS_CONTEXT_CHANGED'});
  await expect(client.command(incoming(),{...actor,iam:undefined},readInput,openBody,commandKey))
    .rejects.toMatchObject({code:'NATIVE_OPERATIONS_CONTEXT_CHANGED'});
  expect(transport).not.toHaveBeenCalled();
 });

 it('maps command conflicts without a TypeScript fallback',async()=>{
  const transport=vi.fn().mockResolvedValue(failed(409,'OPERATIONS_STATE_CHANGED'));
  const client=createNativeOperationsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,readInput,changeBody,commandKey))
    .rejects.toMatchObject({code:'OPERATIONS_STATE_CHANGED',status:409});
 });

 it('keeps the Next reader fail-closed unless the native configuration is present',()=>{
  const previous=process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  delete process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  expect(nativeOperationsAvailable()).toBe(false);
  process.env.ERPX_NATIVE_OPERATIONS_CONFIG=config();
  expect(nativeOperationsAvailable()).toBe(true);
  if(previous===undefined)delete process.env.ERPX_NATIVE_OPERATIONS_CONFIG;
  else process.env.ERPX_NATIVE_OPERATIONS_CONFIG=previous;
 });

 it('rejects configuration that can escape the registered Holding origin',()=>{
  for(const raw of [
    config({privateBaseUrl:'http://127.0.0.1:3100'}),
    config({privateBaseUrl:'http://10.0.0.1:44993'}),
    config({holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}]}),
    config({holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'},{origin:'https://acme.example.com',holdingSlug:'other',holdingId:'h002'}]}),
    config({timeoutMs:100}),
  ]) {
    expect(()=>createNativeOperationsClient(JSON.parse(raw))).toThrowError(NativeOperationsError);
  }
  for(const raw of ['not-json',JSON.stringify({privateBaseUrl:'http://127.0.0.1:44993'})])
    expect(()=>nativeOperationsConfiguration(raw)).toThrowError(NativeOperationsError);
 });
});
