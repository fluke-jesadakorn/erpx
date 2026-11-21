import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeCrmPipelineClient,
  nativeCrmPipelineAvailable,
  nativeCrmPipelineConfiguration,
  NativeCrmPipelineError,
  type NativeCrmPipelinePageContext,
} from '../../../packages/feature-sales/src/public/native-crm-pipeline';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44995',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>= {}) => new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeCrmPipelinePageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['crm'],iam:{permissions:['commerce.read','commerce.sales']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['crm'],permissions:['commerce.read','commerce.sales'],
  expiresAt:9_007_199_254_740_991};
const lead={
  id:'lead-1',company_id:'company',branch_id:'branch',department_id:null,
  title:'Office installation inquiry',contact_name:'Synthetic contact',status:'open',
  stage:null,opportunity_id:null,expected_minor:null,currency:null,close_date:null,
  due_date:'2026-11-20',owner_id:'owner',owner_name:'Staff owner',customer_id:null,
  customer_name:null,version:1,opportunity_version:null,updated_at:10,
};
const opportunity={...lead,id:'lead-2',status:'qualified',stage:'discovery',
  opportunity_id:'opportunity-2',expected_minor:1500,currency:'THB',
  close_date:'2026-12-01',customer_id:'customer',customer_name:'Acme Customer',
  version:2,opportunity_version:1,updated_at:20};
const purchase={newWork:true,settleExisting:true,history:true,revision:'7'};
const page=(items:unknown[],overrides:Record<string,unknown>={})=>({
  data:{items,forecast:{THB:{unweightedMinor:'1500',weightedMinor:'375',wonMinor:'0'}},
    canCreate:true,purchase,nextCursor:null,hasMore:false,...overrides},
  context,
});
const input={holdingSlug:'acme',search:'office',stage:'open' as const,
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native CRM pipeline transport',()=>{
 it('forwards the configured Holding, session, CAS and pipeline query',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page([lead])));
  const client=createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config()),transport);
  const result=await client.pipeline(incoming(),actor,input);
  expect(result).toEqual({items:[lead],
    forecast:{THB:{unweightedMinor:'1500',weightedMinor:'375',wonMinor:'0'}},
    canCreate:true,purchase,nextCursor:null,previousCursor:null,hasMore:false});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads?q=office&stage=open');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug and opaque cursor',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{
    items:[{...lead,branch_id:'branch'}],
    forecast:{},canCreate:false,purchase:null,nextCursor:'next',hasMore:true},
  context:branchContext}));
  const client=createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config()),transport);
  const result=await client.pipeline(incoming(),branchActor,{...input,branchSlug:'north',cursor:'opaque'});
  expect(result.nextCursor).toBe('next');
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44995/api/v1/holdings/acme/crm/leads?q=office&stage=open&cursor=opaque');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PACKAGE_REQUIRED',message:'Sales CRM history is required.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config()),transport);
  await expect(client.pipeline(incoming(),actor,input)).rejects.toMatchObject({
    code:'PACKAGE_REQUIRED',status:403,message:'Sales CRM history is required.',
  });
 });

 it('rejects scope, row, forecast, purchase and page-shape mismatches before returning data',async()=>{
  const client=(value:unknown)=>createNativeCrmPipelineClient(
    nativeCrmPipelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  await expect(client(page([{...lead,company_id:'foreign'}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([{...lead,stage:'discovery'}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([{...lead,status:'qualified'}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([{...lead,customer_name:'Unlinked customer'}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([lead,lead])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([{...lead,updated_at:20},{...lead,id:'lead-0',updated_at:20}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([lead],{forecast:{THB:{unweightedMinor:'01',weightedMinor:'0',wonMinor:'0'}}}))
    .pipeline(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([lead],{purchase:{newWork:true,settleExisting:true,history:true}}))
    .pipeline(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client(page([lead],{nextCursor:'next',hasMore:false}))
    .pipeline(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  await expect(client({data:page([lead]).data,context:{...context,branchId:'branch',branchSlug:'north'}})
    .pipeline(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_CONTEXT_CHANGED'});
  await expect(client({data:page([lead]).data,context:{...context,revision:8}})
    .pipeline(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_CONTEXT_CHANGED'});
  await expect(client(page([{...lead,extra:'field'}])).pipeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
 });

 it('accepts an opportunity row and rejects incoherent opportunity fields',async()=>{
  const client=(items:unknown[])=>createNativeCrmPipelineClient(
    nativeCrmPipelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(page(items))),
  );
  await expect(client([opportunity]).pipeline(incoming(),actor,{...input,stage:'discovery'}))
    .resolves.toMatchObject({items:[opportunity]});
  for(const invalid of [
    {...opportunity,opportunity_id:null},
    {...opportunity,opportunity_id:''},
    {...opportunity,customer_id:null},
    {...opportunity,expected_minor:-1},
    {...opportunity,currency:'thb'},
    {...opportunity,close_date:'2026-02-30'},
    {...opportunity,opportunity_version:0},
    {...opportunity,status:'open'},
  ]) {
    await expect(client([invalid]).pipeline(incoming(),actor,{...input,stage:'discovery'}))
      .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  }
 });

 it('preserves valid ISO year-zero and upper-bound dates and rejects invalid leap days',async()=>{
  const client=(item:unknown)=>createNativeCrmPipelineClient(
    nativeCrmPipelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(page([item]))),
  );
  for(const date of ['0000-01-01','0000-02-29','2000-02-29','9999-12-31']) {
    const item={...opportunity,due_date:date,close_date:date};
    await expect(client(item).pipeline(incoming(),actor,{...input,stage:'discovery'}))
      .resolves.toMatchObject({items:[item]});
  }
  for(const date of ['0000-02-30','1900-02-29']) {
    await expect(client({...opportunity,close_date:date}).pipeline(incoming(),actor,{...input,stage:'discovery'}))
      .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_RESPONSE_INVALID'});
  }
 });

 it('rejects a host or request that is not the configured exact authority',async()=>{
  const client=createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config()),vi.fn());
  await expect(client.pipeline(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_AUTHORITY_INVALID'});
  await expect(client.pipeline(incoming(),actor,{...input,stage:'paid' as never}))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_REQUEST_INVALID'});
  await expect(client.pipeline(incoming(),actor,{...input,search:'x'.repeat(201)}))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_REQUEST_INVALID'});
  await expect(client.pipeline(incoming(),actor,{...input,cursor:''}))
    .rejects.toMatchObject({code:'NATIVE_CRM_PIPELINE_REQUEST_INVALID'});
 });

 it('fails closed when the native CRM configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG;
  delete process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG;
  expect(nativeCrmPipelineAvailable()).toBe(false);
  const {nativeCrmPipeline}=await import('../../../packages/feature-sales/src/public/native-crm-pipeline-page');
  await expect(nativeCrmPipeline(actor as never,{})).resolves.toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG=previous;
 });

 it('does not fall back when a configured native CRM selector is malformed',async()=>{
  const previous=process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG;
  process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG='{not-json';
  const {nativeCrmPipeline}=await import('../../../packages/feature-sales/src/public/native-crm-pipeline-page');
  await expect(nativeCrmPipeline(actor as never,{})).rejects.toMatchObject({
    code:'NATIVE_CRM_PIPELINE_CONFIGURATION_INVALID',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG;
  else process.env.ERPX_NATIVE_CRM_PIPELINE_CONFIG=previous;
 });

 it('accepts an explicit development port on a registered Holding origin',()=>{
  expect(()=>createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config({
    holdings:[{origin:'https://acme.example.com:3443',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).not.toThrow();
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config({privateBaseUrl:'http://10.0.0.1:44995'})),vi.fn())).toThrow();
  expect(()=>createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeCrmPipelineConfiguration('{not-json')).toThrow();
  expect(()=>nativeCrmPipelineConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeCrmPipelineClient(nativeCrmPipelineConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeCrmPipelineFailure}=await import('../../../packages/feature-sales/src/public/native-crm-pipeline-page');
  expect(nativeCrmPipelineFailure(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeCrmPipelineFailure(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'CRM_PIPELINE_REQUEST_INVALID'});
  expect(nativeCrmPipelineFailure(new NativeCrmPipelineError('NATIVE_CRM_PIPELINE_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_CRM_PIPELINE_UNAVAILABLE'});
  expect(nativeCrmPipelineFailure(new NativeCrmPipelineError('PACKAGE_REQUIRED',403,'History required.')))
    .toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  expect(nativeCrmPipelineFailure(new NativeCrmPipelineError('CRM_PIPELINE_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'CRM_PIPELINE_CHANGED'});
  expect(nativeCrmPipelineFailure(new Error('other'))).toBeUndefined();
 });
});
