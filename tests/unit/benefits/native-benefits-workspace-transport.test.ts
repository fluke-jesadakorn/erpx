import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeBenefitsClient,
  nativeBenefitsAvailable,
  nativeBenefitsConfiguration,
  NativeBenefitsError,
  type NativeBenefitsPageContext,
} from '../../../packages/feature-benefits/src/public/native-benefits-workspace';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44997',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>= {}) => new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeBenefitsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['benefits'],iam:{permissions:['records.read','records.write']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['benefits'],permissions:['records.read','records.write'],
  expiresAt:9_007_199_254_740_991};
const plan={
  id:'plan-family',company_id:'company',name:'Family medical plan',provider:'Synthetic provider',
  coverage_type:'family',currency:'THB',employee_deduction_minor:12550,
  employer_cost_minor:30000,effective_from:'2026-01-01',effective_to:'2026-12-31',
  status:'published',version:1,created_by:'owner',reviewed_by:'reviewer',created_at:10,
  updated_at:20,required_eligibility_code:'PROBATION_COMPLETE',
};
const windowRow={
  id:'window-annual',company_id:'company',name:'Annual enrollment',opens_on:'2026-11-01',
  closes_on:'2026-11-30',effective_on:'2027-01-01',status:'open',version:1,
  created_by:'owner',reviewed_by:'reviewer',created_at:10,updated_at:30,
};
const dependentRow={
  id:'dependent-one',company_id:'company',person_id:'person-owner',name:'Synthetic dependent',
  relationship:'child',birth_date:'2018-01-01',status:'verified',version:1,
  created_by:'owner',reviewed_by:'reviewer',created_at:10,updated_at:40,
};
const electionRow={
  id:'election-owner',company_id:'company',person_id:'person-owner',
  engagement_id:'engagement-owner',plan_id:'plan-family',plan_name:'Family medical plan',
  employee_name:'Synthetic Owner',plan_version:1,window_id:'window-annual',window_version:1,
  coverage_type:'family',currency:'THB',employee_deduction_minor:12550,
  employer_cost_minor:30000,effective_from:'2027-01-01',effective_to:'2027-12-31',
  status:'elected',version:1,created_at:10,updated_at:50,
};
const data=(overrides:Record<string,unknown>={})=>({
  actorId:'user',canWrite:true,canReview:false,canManageDocuments:false,
  workContext:'a'.repeat(64),
  plans:[plan],windows:[windowRow],dependents:[dependentRow],elections:[electionRow],
  purchase:null,...overrides,
});
const page=(overrides:Record<string,unknown>={},contextOverrides:Record<string,unknown>={})=>({
  data:data(overrides),context:{...context,...contextOverrides},
});
const input={holdingSlug:'acme',personal:false,
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Benefits workspace transport',()=>{
 it('preserves UTF-16 eligibility code identity and rejects invalid bounds',async()=>{
  for(const code of ['ช'.repeat(120),'😀'.repeat(60),'a\nb\u0085c']){
   const value={...plan,required_eligibility_code:code};
   const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn().mockResolvedValue(ok(page({plans:[value]}))));
   expect((await client.workspace(incoming(),actor,input)).plans[0].required_eligibility_code).toBe(code);
  }
  for(const code of [`${'😀'.repeat(60)}ก`,'ab\u0000c','\ufeffab']){
   const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn().mockResolvedValue(ok(page({plans:[{...plan,required_eligibility_code:code}]}))));
   await expect(client.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  }
 });
 it('preserves UTF-16 plan name bounds and exact JS trim identity',async()=>{
  for(const name of ['ก'.repeat(160),'😀'.repeat(80),'a\u0085b','a\u2028b','\u0085ab']){
   const value={...plan,name};
   const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn().mockResolvedValue(ok(page({plans:[value]}))));
   expect((await client.workspace(incoming(),actor,input)).plans[0].name).toBe(name);
  }
  for(const name of ['ก'.repeat(161),'😀'.repeat(81),'a\u0000b','a\u007fb','\ufeffab','ab\ufeff',' กา','กา ']){
   const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn().mockResolvedValue(ok(page({plans:[{...plan,name}]}))));
   await expect(client.workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  }
 });
 it('forwards the configured Holding, session, CAS and exact workspace route',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page()));
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),transport);
  const result=await client.workspace(incoming(),actor,input);
  expect(result).toMatchObject({
    actorId:'user',canWrite:true,canReview:false,canManageDocuments:false,
    workContext:'a'.repeat(64),plans:[plan],windows:[windowRow],
    dependents:[dependentRow],elections:[electionRow],
  });
  expect(result.purchase).toBeUndefined();
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44997/api/v1/holdings/acme/benefits');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('x-erpx-benefits-personal')).toBeNull();
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards a keyed command with exact origin, context, body and bounded receipt',async()=>{
  const receipt={id:'benefit-plan-plan-command-1',status:'draft',version:1};
  const transport=vi.fn().mockResolvedValue(ok({data:receipt,context}));
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),transport);
  const body={entity:'plan',name:'Health Plus',provider:'Synthetic Provider',coverageType:'family',currency:'THB',employeeDeduction:'125.50',employerCost:'300.00',effectiveFrom:'2026-09-01'};
  const result=await client.command(incoming({origin:'https://acme.example.com'}),actor,body,'plan-command-1');
  expect(result).toEqual(receipt);
  const [method,url,headers,sent]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44997/api/v1/holdings/acme/benefits');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('idempotency-key')).toBe('plan-command-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(JSON.parse(sent)).toEqual(body);
  const invalid=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn().mockResolvedValue(ok({data:{...receipt,status:'unknown'},context})));
  await expect(invalid.command(incoming({origin:'https://acme.example.com'}),actor,body,'plan-command-2')).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client.command(incoming(),actor,body,'plan-command-3')).rejects.toMatchObject({code:'NATIVE_BENEFITS_REQUEST_INVALID'});
  await expect(client.command(incoming({origin:'https://evil.example.com'}),actor,body,'plan-command-4')).rejects.toMatchObject({code:'NATIVE_BENEFITS_REQUEST_INVALID'});
 });

 it('forwards the personal view, selected branch and department workspace slugs',async()=>{
  const branchActor={...actor,branchId:'branch',departmentId:'department',
    scope:{kind:'department' as const,branchSlug:'north',departmentSlug:'ops'}};
  const branchContext={...context,branchId:'branch',departmentId:'department',
    branchSlug:'north',departmentSlug:'ops'};
  const transport=vi.fn().mockResolvedValue(ok({
    data:data({plans:[],windows:[],dependents:[],elections:[]}),context:branchContext,
  }));
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),transport);
  const result=await client.workspace(incoming(),branchActor,{
    ...input,personal:true,branchSlug:'north',departmentSlug:'ops',
  });
  expect(result.elections).toEqual([]);
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44997/api/v1/holdings/acme/benefits');
  expect(headers.get('x-erpx-benefits-personal')).toBe('1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('x-erpx-workspace-department')).toBe('ops');
 });

 it('returns the enabled package controls without rewiring them',async()=>{
  const purchase={newWork:true,settleExisting:false,history:true,revision:'7'};
  const transport=vi.fn().mockResolvedValue(ok(page({purchase})));
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),transport);
  const result=await client.workspace(incoming(),actor,input);
  expect(result.purchase).toEqual(purchase);
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PACKAGE_REQUIRED',message:'Benefits history is required.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),transport);
  await expect(client.workspace(incoming(),actor,input)).rejects.toMatchObject({
    code:'PACKAGE_REQUIRED',status:403,message:'Benefits history is required.',
  });
 });

 it('rejects foreign, duplicate, malformed and unordered rows before returning data',async()=>{
  const client=(value:unknown)=>createNativeBenefitsClient(
    nativeBenefitsConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  await expect(client(page({plans:[{...plan,company_id:'foreign'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[{...plan,effective_from:'2026-02-30'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[{...plan,effective_to:'2025-12-31'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[{...plan,currency:'thb'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[plan,plan]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[{...plan,id:'plan-b'},{...plan,id:'plan-a'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({plans:[{...plan,extra:'field'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({windows:[{...windowRow,opens_on:'2026-12-01'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({dependents:[{...dependentRow,status:'active'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({elections:[{...electionRow,employee_name:' '}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({elections:[{...electionRow,status:'draft'}]}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({canWrite:false}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({actorId:'other'}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({purchase:{newWork:true,settleExisting:true,history:true}}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
  await expect(client(page({}, {companyId:'foreign'}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_CONTEXT_CHANGED'});
  await expect(client(page({}, {assignmentVersion:9}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_CONTEXT_CHANGED'});
  await expect(client(page({}, {permissions:['records.read']}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_CONTEXT_CHANGED'});
  await expect(client({data:data()}).workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
 });

 it('bounds an oversized owner response and rejects a non-JSON success body',async()=>{
  const client=(response:Response)=>createNativeBenefitsClient(
    nativeBenefitsConfiguration(config()),
    vi.fn().mockResolvedValue(response),
  );
  await expect(client(new Response('x'.repeat(2*1024*1024+1),{status:200,headers:{'content-type':'application/json'}}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_LIMIT'});
  await expect(client(new Response('{}',{status:200,headers:{'content-type':'text/plain'}}))
    .workspace(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_BENEFITS_RESPONSE_INVALID'});
 });

 it('rejects a host or actor that is not the configured exact authority',async()=>{
  const client=createNativeBenefitsClient(nativeBenefitsConfiguration(config()),vi.fn());
  await expect(client.workspace(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_AUTHORITY_INVALID'});
  await expect(client.workspace(incoming(),{...actor,holdingId:'h002'},input))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_AUTHORITY_INVALID'});
  await expect(client.workspace(incoming(),{...actor,authority:undefined},input))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_CONTEXT_CHANGED'});
  await expect(client.workspace(incoming(),actor,{...input,branchSlug:'North'}))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_REQUEST_INVALID'});
  await expect(client.workspace(incoming(),actor,{...input,expected:{...input.expected,revision:-1}}))
    .rejects.toMatchObject({code:'NATIVE_BENEFITS_REQUEST_INVALID'});
 });

 it('fails closed when the native Benefits configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_BENEFITS_CONFIG;
  delete process.env.ERPX_NATIVE_BENEFITS_CONFIG;
  expect(nativeBenefitsAvailable()).toBe(false);
  const {nativeBenefitsWorkspace,nativeBenefitCommand}=await import('../../../packages/feature-benefits/src/public/native-benefits-workspace-page');
  await expect(nativeBenefitsWorkspace(actor as never)).resolves.toBeUndefined();
  await expect(nativeBenefitCommand(actor as never,{entity:'plan'},'command-key-1',incoming())).resolves.toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_BENEFITS_CONFIG=previous;
 });

 it('does not fall back when a configured native Benefits selector is malformed',async()=>{
  const previous=process.env.ERPX_NATIVE_BENEFITS_CONFIG;
  process.env.ERPX_NATIVE_BENEFITS_CONFIG='{not-json';
  const {nativeBenefitsWorkspace,nativeBenefitCommand}=await import('../../../packages/feature-benefits/src/public/native-benefits-workspace-page');
  await expect(nativeBenefitsWorkspace(actor as never)).rejects.toMatchObject({
    status:503,code:'NATIVE_BENEFITS_UNAVAILABLE',
  });
  await expect(nativeBenefitCommand(actor as never,{entity:'plan'},'command-key-2',incoming())).rejects.toMatchObject({
    status:503,code:'NATIVE_BENEFITS_UNAVAILABLE',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_BENEFITS_CONFIG;
  else process.env.ERPX_NATIVE_BENEFITS_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  for(const privateBaseUrl of [
    'http://127.0.0.1:3000','http://127.0.0.1:3001','http://127.0.0.1:3100',
    'http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:5432',
    'http://127.0.0.1:8080','http://127.0.0.1:3200','http://10.0.0.1:44997',
    'https://127.0.0.1:44997','http://127.0.0.1:44997/',
  ]) {
    expect(()=>createNativeBenefitsClient(nativeBenefitsConfiguration(config({privateBaseUrl})),vi.fn())).toThrow();
  }
  for(const holdings of [
    [{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
    [{origin:'https://acme.example.com',holdingSlug:'Acme',holdingId:'h001'}],
  ]) {
    expect(()=>createNativeBenefitsClient(nativeBenefitsConfiguration(config({holdings})),vi.fn())).toThrow();
  }
  expect(()=>createNativeBenefitsClient(nativeBenefitsConfiguration(config({
    holdings:[{origin:'https://acme.example.com:8443',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).not.toThrow();
  expect(()=>nativeBenefitsConfiguration('{not-json')).toThrow();
  expect(()=>nativeBenefitsConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeBenefitsClient(nativeBenefitsConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeBenefitsFailure}=await import('../../../packages/feature-benefits/src/public/native-benefits-workspace-page');
  expect(nativeBenefitsFailure(new NativeBenefitsError('NATIVE_BENEFITS_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeBenefitsFailure(new NativeBenefitsError('NATIVE_BENEFITS_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'BENEFITS_WORKSPACE_REQUEST_INVALID'});
  expect(nativeBenefitsFailure(new NativeBenefitsError('NATIVE_BENEFITS_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_BENEFITS_UNAVAILABLE'});
  expect(nativeBenefitsFailure(new NativeBenefitsError('PACKAGE_REQUIRED',403,'History required.')))
    .toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  expect(nativeBenefitsFailure(new NativeBenefitsError('BENEFITS_SOURCE_INVALID',409,'Reload.')))
    .toMatchObject({status:409,code:'BENEFITS_SOURCE_INVALID'});
  expect(nativeBenefitsFailure(new Error('other'))).toBeUndefined();
 });
});
