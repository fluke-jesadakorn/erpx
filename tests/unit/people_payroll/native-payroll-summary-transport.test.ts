import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativePeoplePayrollClient,
  nativePeoplePayrollAvailable,
  nativePeoplePayrollConfiguration,
  NativePeoplePayrollError,
  type NativePeoplePayrollPageContext,
} from '../../../packages/feature-people_payroll/src/public/native-payroll-summary';
import { nativePeoplePayrollFailure } from '../../../packages/feature-people_payroll/src/public/native-payroll-summary-page';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44992',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativePeoplePayrollPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['people'],iam:{permissions:['payroll.read','payroll.payment.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['people'],permissions:['payroll.read','payroll.payment.read'],
  expiresAt:9_007_199_254_740_991};
const summary={pay_month:'2026-10',currency:'THB',run_count:1,headcount:2,
  gross_minor:1_500_000,deductions_minor:150_000,employer_cost_minor:75_000,
  net_minor:1_350_000};
const run={id:'run',pay_group_id:'group',period_start:'2026-10-01',period_end:'2026-10-31',
  pay_date:'2026-10-31',currency:'THB',status:'posted',mode:'production',
  revision:1,version:1,totals_json:'{"gross":1500000}',payment_batch_id:'batch',
  payment_status:'settled',net_payable_minor:1_350_000};
const detail={
  run:{id:'run',company_id:'company',pay_group_id:'group',country_pack_id:'pack',
    scope_engagement_id:null,period_start:'2026-10-01',period_end:'2026-10-31',
    pay_date:'2026-10-31',currency:'THB',mode:'production',status:'posted',revision:1,
    totals_json:'{"gross":1500000}',input_digest:'a'.repeat(64),error_message:null,
    version:2,created_by:'user',submitted_by:'user',submitted_digest:'b'.repeat(64),
    submitted_at:1_700_000_000_000,approved_by:'reviewer',posted_by:'poster',
    adjustment_of_run_id:null,created_at:1_700_000_000_000,updated_at:1_700_000_100_000,
    overtime_multiplier_bps:15000,proration_method:'calendar'},
  items:[{id:'item',run_id:'run',engagement_id:'engagement',legal_name:'Employee',
    person_id:'person',worker_type:'employee',input_snapshot_json:'{}',result_json:'{}',
    gross_minor:1000,deductions_minor:10,employer_cost_minor:5,net_minor:990,
    currency:'THB',status:'posted',created_at:1_700_000_000_000}],
  exports:[{id:'export',run_revision:1,checksum_sha256:'c'.repeat(64),created_at:1_700_000_000_000}],
  corrections:[],
  paymentBatch:{id:'batch',company_id:'company',status:'settled',total_minor:990},
  paymentItems:[{id:'payment',batch_id:'batch',reference:'ref',amount_minor:990,status:'settled'}],
};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native People Payroll summary transport',()=>{
 it('forwards the configured Holding, session, CAS and pay month to the summary read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:[summary],context}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  const result=await client.summary(incoming(),actor,{...input,payMonth:'2026-10'});
  expect(result).toEqual([summary]);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/summary?payMonth=2026-10');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch/workspace slug as context CAS input',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:[summary],context:branchContext}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  await client.summary(incoming(),branchActor,{...input,branchSlug:'north'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/summary');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('reads the posted run list with payment-batch facts and no totals redaction leak',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:[run],context}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  const result=await client.runs(incoming(),actor,input);
  expect(result[0]).toMatchObject({id:'run',pay_group_id:'group',payment_batch_id:'batch',payment_status:'settled'});
  expect(transport.mock.calls[0][1].toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/runs');
 });

 it('reads one native run detail with its audit columns, stored error and bounded collections',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:detail,context}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  const result=await client.detail(incoming(),actor,input,'run');
  expect(result.run).toMatchObject({
    created_by:'user',submitted_by:'user',approved_by:'reviewer',posted_by:'poster',
    submitted_digest:'b'.repeat(64),submitted_at:1_700_000_000_000,
    adjustment_of_run_id:null,country_pack_id:'pack',scope_engagement_id:null,
    created_at:1_700_000_000_000,updated_at:1_700_000_100_000,error_message:null,
  });
  expect(result.items).toHaveLength(1);
  expect(result.paymentBatch).toMatchObject({status:'settled'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/runs/run');
  expect(headers.get('idempotency-key')).toBeNull();
 });

 it('rejects a detail response that drops an owner audit column or exceeds a bounded collection',async()=>{
  const missing=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...detail,run:{...detail.run,posted_by:undefined}},context}),
  ));
  await expect(missing.detail(incoming(),actor,input,'run'))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const unbounded=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...detail,paymentItems:Array.from({length:5001},()=>detail.paymentItems[0])},context}),
  ));
  await expect(unbounded.detail(incoming(),actor,input,'run'))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn());
  await expect(client.detail(incoming(),actor,input,'run/approve'))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID'});
 });

 it('reads bounded Work payload rows in owner recency order and rejects duplicate or regressing rows',async()=>{
  const workRows=[
   {id:'run-b',pay_group_id:'group',pay_date:'2026-10-31',status:'posted',mode:'production',created_by:'user',updated_at:200},
   {id:'run-a',pay_group_id:'group',pay_date:'2026-10-31',status:'draft',mode:'preview',created_by:'other',updated_at:100},
  ];
  const transport=vi.fn().mockResolvedValue(ok({data:workRows,context}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  const result=await client.workRows(incoming(),actor,input);
  expect(result).toEqual(workRows);
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/work-rows');
  expect(headers.get('idempotency-key')).toBeNull();
  const duplicate=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
   ok({data:[workRows[0],workRows[0]],context}),
  ));
  await expect(duplicate.workRows(incoming(),actor,input))
   .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const regressing=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
   ok({data:[workRows[1],workRows[0]],context}),
  ));
  await expect(regressing.workRows(incoming(),actor,input))
   .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const extra=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
   ok({data:[{...workRows[0],company_id:'company'}],context}),
  ));
  await expect(extra.workRows(incoming(),actor,input))
   .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'COMPANY_PAYROLL_SCOPE_REQUIRED',message:'Use a Company payroll assignment.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  await expect(client.summary(incoming(),actor,input)).rejects.toMatchObject({
    code:'COMPANY_PAYROLL_SCOPE_REQUIRED',status:403,
    message:'Use a Company payroll assignment.',
  });
 });

 it('rejects an unconfigured host, a changed context and a malformed response before returning data',async()=>{
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn());
  await expect(client.summary(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_AUTHORITY_INVALID'});
  const changed=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:[summary],context:{...context,revision:8}}),
  ));
  await expect(changed.summary(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED'});
  const malformed=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:[{...summary,extra:'field'}],context}),
  ));
  await expect(malformed.summary(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const duplicate=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:[summary,summary],context}),
  ));
  await expect(duplicate.summary(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
  const invalidRun=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:[{...run,totals_json:'[]'}],context}),
  ));
  await expect(invalidRun.runs(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_RESPONSE_INVALID'});
 });

 it('fails closed when the native People Payroll configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  delete process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  expect(nativePeoplePayrollAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config({privateBaseUrl:'http://10.0.0.1:44992'})),vi.fn())).toThrow();
  expect(()=>createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativePeoplePayrollConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativePeoplePayrollFailure(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativePeoplePayrollFailure(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'PEOPLE_PAYROLL_REQUEST_INVALID'});
  expect(nativePeoplePayrollFailure(new NativePeoplePayrollError('NATIVE_PEOPLE_PAYROLL_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_PEOPLE_PAYROLL_UNAVAILABLE'});
  expect(nativePeoplePayrollFailure(new NativePeoplePayrollError('COMPANY_PAYROLL_SCOPE_REQUIRED',403,'Company payroll required')))
    .toMatchObject({status:403,code:'COMPANY_PAYROLL_SCOPE_REQUIRED'});
  expect(nativePeoplePayrollFailure(new Error('other'))).toBeUndefined();
 });
});

describe('native People Payroll command transport',()=>{
 it('posts the bounded command path, Idempotency-Key and exact origin to the configured Holding',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'run',status:'draft'}}));
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),transport);
  const result=await client.command(incoming(),actor,input,'runs',{payGroupId:'group'},'command-key-1','https://acme.example.com');
  expect(result).toEqual({id:'run',status:'draft'});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/people/payroll/runs');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('idempotency-key')).toBe('command-key-1');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('content-type')).toBe('application/json');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(JSON.parse(body!)).toEqual({payGroupId:'group'});
 });
 it('rejects a command outside the owner route shape before any transport',async()=>{
  const client=createNativePeoplePayrollClient(nativePeoplePayrollConfiguration(config()),vi.fn());
  await expect(client.command(incoming(),actor,input,'runs/run/approve',{},'command-key-1','https://acme.example.com'))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_REQUEST_INVALID'});
  await expect(client.command({get:vi.fn().mockReturnValue('other.example.test')} as unknown as Pick<Headers,'get'>,actor,input,
    'runs',{},'command-key-1','https://acme.example.com'))
    .rejects.toMatchObject({code:'NATIVE_PEOPLE_PAYROLL_AUTHORITY_INVALID'});
 });
 it('fails closed without the native configuration so the route returns 503 instead of a TS writer',async()=>{
  const previous=process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  delete process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  const {nativePeoplePayrollCommand}=await import('../../../packages/feature-people_payroll/src/public/native-payroll-summary-page');
  expect(await nativePeoplePayrollCommand(incoming(),actor,'acme','runs',{},'command-key-1','https://acme.example.com'))
    .toBeUndefined();
  expect(await nativePeoplePayrollCommand(incoming(),actor,'acme','runs/run/calculate',{},'command-key-1','https://acme.example.com'))
    .toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG=previous;
 });
});

describe('native People Payroll owned reads',()=>{
 it('leaves the recorded payroll month detail native-only when the owner reader is unconfigured',async()=>{
  const previous=process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  delete process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG;
  const {ownedPeoplePayrollRuns,ownedPeoplePayrollSummary}=await import('../../../packages/feature-people_payroll/src/public/native-payroll-summary-owned');
  // The Records payroll-month drawer and the Payroll history fallback raise
  // NATIVE_PEOPLE_PAYROLL_UNAVAILABLE instead of consulting a TypeScript reader.
  expect(await ownedPeoplePayrollSummary(actor as never,'acme','2026-10')).toBeUndefined();
  expect(await ownedPeoplePayrollRuns(actor as never,'acme')).toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG=previous;
 });
});
